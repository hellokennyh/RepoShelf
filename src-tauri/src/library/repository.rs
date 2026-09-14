use chrono::Utc;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::HashSet;
use tokio::sync::Mutex;

static DB_WRITE_LOCK: Mutex<()> = Mutex::const_new(());

use super::models::{
    FavoriteInput, FavoriteItem, FavoriteTagItem, LibraryQuery, LibrarySortField, SortOrder,
    UpdateFavoriteUserMetadataInput,
};
use crate::error::AppError;
use crate::github::classification::ALL_CATEGORIES;
use crate::github::parse_github_repo_url;

pub fn validate_category(category: &str) -> Result<&'static str, AppError> {
    for &cat in &ALL_CATEGORIES {
        if cat.eq_ignore_ascii_case(category.trim()) {
            return Ok(cat);
        }
    }
    Err(AppError::InvalidCategory(format!(
        "Category '{}' is not one of the 16 predefined categories",
        category
    )))
}

pub fn validate_rating(rating: Option<i64>) -> Result<Option<i64>, AppError> {
    if let Some(r) = rating {
        if !(1..=5).contains(&r) {
            return Err(AppError::InvalidRating(format!(
                "Rating must be between 1 and 5, got {}",
                r
            )));
        }
        Ok(Some(r))
    } else {
        Ok(None)
    }
}

pub fn validate_rating_confidence(conf: Option<&str>) -> Result<Option<String>, AppError> {
    match conf {
        None => Ok(None),
        Some(c) => {
            let trimmed = c.trim().to_lowercase();
            if trimmed.is_empty() {
                Ok(None)
            } else if trimmed == "low" || trimmed == "medium" || trimmed == "high" {
                Ok(Some(trimmed))
            } else {
                Err(AppError::InvalidFavoriteInput(format!(
                    "Invalid rating confidence '{}', expected 'low', 'medium', or 'high'",
                    c
                )))
            }
        }
    }
}

pub fn normalize_topics(topics: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for topic in topics {
        let trimmed = topic.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_lowercase()) {
            normalized.push(trimmed.to_string());
        }
    }

    normalized
}

pub async fn insert_favorite_tx(
    pool: &SqlitePool,
    input: &FavoriteInput,
) -> Result<FavoriteItem, AppError> {
    let valid_category = validate_category(&input.category)?;
    let (owner, repo, _) = parse_github_repo_url(&input.github_url)?;
    let canonical_url = format!("https://github.com/{}/{}", owner, repo);

    let now_utc = Utc::now().to_rfc3339();
    let normalized_topics = normalize_topics(&input.topics);
    let languages_json = input
        .languages
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap_or_else(|_| "{}".to_string()));

    let _lock = DB_WRITE_LOCK.lock().await;
    let mut tx = pool.begin().await?;

    let insert_result = sqlx::query(
        r#"
        INSERT INTO favorites (
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, '', ?, ?, ?, ?)
        "#,
    )
    .bind(input.github_repo_id)
    .bind(&canonical_url)
    .bind(&input.name)
    .bind(&input.description)
    .bind(valid_category)
    .bind(input.stars)
    .bind(&input.license)
    .bind(&input.pushed_at)
    .bind(&now_utc)
    .bind(&now_utc)
    .bind(input.releases)
    .bind(&languages_json)
    .execute(&mut *tx)
    .await?;

    let favorite_id = insert_result.last_insert_rowid();

    for topic in &normalized_topics {
        sqlx::query(
            r#"
            INSERT INTO favorite_tags (favorite_id, tag, source)
            VALUES (?, ?, 'github')
            "#,
        )
        .bind(favorite_id)
        .bind(topic)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_favorite_by_id(pool, favorite_id)
        .await?
        .ok_or_else(|| AppError::Database("Inserted favorite not found".to_string()))
}

pub async fn find_favorite_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<FavoriteItem>, AppError> {
    let fav_row = sqlx::query(
        r#"
        SELECT
            id,
            github_repo_id,
            github_url,
            name,
            description,
            category,
            stars,
            license,
            pushed_at,
            rating,
            rating_reason,
            rating_confidence,
            notes,
            favorited_at,
            metadata_refreshed_at,
            releases,
            languages
        FROM favorites
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let row = match fav_row {
        Some(r) => r,
        None => return Ok(None),
    };

    let tags = fetch_tags_for_favorite(pool, id).await?;

    let releases: Option<i64> = row.get("releases");
    let languages_str: Option<String> = row.get("languages");
    let languages: Option<std::collections::HashMap<String, f64>> =
        languages_str.and_then(|s| serde_json::from_str(&s).ok());

    Ok(Some(FavoriteItem {
        id: row.get("id"),
        github_repo_id: row.get("github_repo_id"),
        github_url: row.get("github_url"),
        name: row.get("name"),
        description: row.get("description"),
        category: row.get("category"),
        stars: row.get("stars"),
        license: row.get("license"),
        pushed_at: row.get("pushed_at"),
        rating: row.get("rating"),
        rating_reason: row.get("rating_reason"),
        rating_confidence: row.get("rating_confidence"),
        notes: row.get("notes"),
        favorited_at: row.get("favorited_at"),
        metadata_refreshed_at: row.get("metadata_refreshed_at"),
        tags,
        releases,
        languages,
    }))
}

pub async fn find_favorite_by_identity(
    pool: &SqlitePool,
    repo_id: Option<i64>,
    raw_url: &str,
) -> Result<Option<FavoriteItem>, AppError> {
    if let Some(id) = repo_id {
        let row = sqlx::query("SELECT id FROM favorites WHERE github_repo_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;

        if let Some(r) = row {
            let fav_id: i64 = r.get("id");
            return find_favorite_by_id(pool, fav_id).await;
        }
    }

    if let Ok((owner, repo, _)) = parse_github_repo_url(raw_url) {
        let canonical_url = format!("https://github.com/{}/{}", owner, repo);
        let row = sqlx::query("SELECT id FROM favorites WHERE github_url = ?")
            .bind(&canonical_url)
            .fetch_optional(pool)
            .await?;

        if let Some(r) = row {
            let fav_id: i64 = r.get("id");
            return find_favorite_by_id(pool, fav_id).await;
        }
    }

    Ok(None)
}

pub async fn find_favorited_identities(
    pool: &SqlitePool,
    repo_ids: &[i64],
    canonical_urls: &[String],
) -> Result<(HashSet<i64>, HashSet<String>), AppError> {
    let mut matched_repo_ids = HashSet::new();
    if !repo_ids.is_empty() {
        let mut qb: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT DISTINCT github_repo_id FROM favorites WHERE github_repo_id IN (",
        );
        let mut separated = qb.separated(", ");
        for id in repo_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
        let rows: Vec<(Option<i64>,)> = qb.build_query_as().fetch_all(pool).await?;
        for (id_opt,) in rows {
            if let Some(id) = id_opt {
                matched_repo_ids.insert(id);
            }
        }
    }

    let mut matched_urls = HashSet::new();
    if !canonical_urls.is_empty() {
        let mut qb: QueryBuilder<Sqlite> =
            QueryBuilder::new("SELECT DISTINCT github_url FROM favorites WHERE github_url IN (");
        let mut separated = qb.separated(", ");
        for url in canonical_urls {
            separated.push_bind(url);
        }
        separated.push_unseparated(")");
        let rows: Vec<(String,)> = qb.build_query_as().fetch_all(pool).await?;
        for (url,) in rows {
            matched_urls.insert(url);
        }
    }

    Ok((matched_repo_ids, matched_urls))
}

pub async fn fetch_tags_for_favorite(
    pool: &SqlitePool,
    favorite_id: i64,
) -> Result<Vec<FavoriteTagItem>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, favorite_id, tag, source
        FROM favorite_tags
        WHERE favorite_id = ?
        ORDER BY id ASC
        "#,
    )
    .bind(favorite_id)
    .fetch_all(pool)
    .await?;

    let tags = rows
        .into_iter()
        .map(|r| FavoriteTagItem {
            id: r.get("id"),
            favorite_id: r.get("favorite_id"),
            tag: r.get("tag"),
            source: r.get("source"),
        })
        .collect();

    Ok(tags)
}

pub async fn list_favorites(
    pool: &SqlitePool,
    query: &LibraryQuery,
) -> Result<Vec<FavoriteItem>, AppError> {
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        r#"
        SELECT
            f.id,
            f.github_repo_id,
            f.github_url,
            f.name,
            f.description,
            f.category,
            f.stars,
            f.license,
            f.pushed_at,
            f.rating,
            f.rating_reason,
            f.rating_confidence,
            f.notes,
            f.favorited_at,
            f.metadata_refreshed_at,
            f.releases,
            f.languages
        FROM favorites f
        WHERE 1=1
        "#,
    );

    // 1. Search across name, category, description, notes, tags
    if let Some(ref s) = query.search {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            let pattern = format!("%{}%", trimmed);
            builder.push(" AND (f.name LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR f.category LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR f.description LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR f.notes LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR EXISTS (SELECT 1 FROM favorite_tags ft WHERE ft.favorite_id = f.id AND ft.tag LIKE ");
            builder.push_bind(pattern);
            builder.push("))");
        }
    }

    // 2. Category filter
    if let Some(ref cat) = query.category {
        let trimmed = cat.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("all") {
            let valid = validate_category(trimmed)?;
            builder.push(" AND f.category = ");
            builder.push_bind(valid.to_string());
        }
    }

    // 3. Tag filter
    if let Some(ref t) = query.tag {
        let trimmed = t.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("all") {
            builder.push(" AND EXISTS (SELECT 1 FROM favorite_tags ft WHERE ft.favorite_id = f.id AND ft.tag = ");
            builder.push_bind(trimmed.to_string());
            builder.push(" COLLATE NOCASE)");
        }
    }

    // 4. Rating filter: "all" | "unrated" | "1" | "2" | "3" | "4" | "5"
    if let Some(ref r) = query.rating {
        let trimmed = r.trim().to_lowercase();
        if trimmed == "unrated" {
            builder.push(" AND f.rating IS NULL");
        } else if let Ok(val) = trimmed.parse::<i64>() {
            if (1..=5).contains(&val) {
                builder.push(" AND f.rating = ");
                builder.push_bind(val);
            } else {
                return Err(AppError::InvalidLibraryQuery(format!(
                    "Invalid rating filter value: {}",
                    r
                )));
            }
        } else if trimmed != "all" && !trimmed.is_empty() {
            return Err(AppError::InvalidLibraryQuery(format!(
                "Invalid rating filter value: {}",
                r
            )));
        }
    }

    // 5. Allowlisted Order By
    let sort_field = query.sort_field.unwrap_or(LibrarySortField::FavoritedAt);
    let sort_order = query.sort_order.unwrap_or(match sort_field {
        LibrarySortField::Name => SortOrder::Asc,
        _ => SortOrder::Desc,
    });

    let order_sql = match sort_order {
        SortOrder::Asc => "ASC",
        SortOrder::Desc => "DESC",
    };

    match sort_field {
        LibrarySortField::FavoritedAt => {
            builder.push(format!(" ORDER BY f.favorited_at {}, f.id DESC", order_sql));
        }
        LibrarySortField::Stars => {
            builder.push(format!(" ORDER BY f.stars {}, f.id DESC", order_sql));
        }
        LibrarySortField::Rating => {
            // Nulls last for predictable sorting
            match sort_order {
                SortOrder::Asc => {
                    builder.push(
                        " ORDER BY CASE WHEN f.rating IS NULL THEN 1 ELSE 0 END, f.rating ASC, f.id DESC",
                    );
                }
                SortOrder::Desc => {
                    builder.push(
                        " ORDER BY CASE WHEN f.rating IS NULL THEN 1 ELSE 0 END, f.rating DESC, f.id DESC",
                    );
                }
            }
        }
        LibrarySortField::PushedAt => match sort_order {
            SortOrder::Asc => {
                builder.push(
                        " ORDER BY CASE WHEN f.pushed_at IS NULL THEN 1 ELSE 0 END, f.pushed_at ASC, f.id DESC",
                    );
            }
            SortOrder::Desc => {
                builder.push(
                        " ORDER BY CASE WHEN f.pushed_at IS NULL THEN 1 ELSE 0 END, f.pushed_at DESC, f.id DESC",
                    );
            }
        },
        LibrarySortField::Name => {
            builder.push(format!(
                " ORDER BY f.name COLLATE NOCASE {}, f.id DESC",
                order_sql
            ));
        }
    }

    let rows = builder.build().fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut favorite_ids = Vec::with_capacity(rows.len());
    let mut favorites = Vec::with_capacity(rows.len());

    for r in rows {
        let id: i64 = r.get("id");
        favorite_ids.push(id);
        let releases: Option<i64> = r.get("releases");
        let languages_str: Option<String> = r.get("languages");
        let languages: Option<std::collections::HashMap<String, f64>> =
            languages_str.and_then(|s| serde_json::from_str(&s).ok());
        favorites.push(FavoriteItem {
            id,
            github_repo_id: r.get("github_repo_id"),
            github_url: r.get("github_url"),
            name: r.get("name"),
            description: r.get("description"),
            category: r.get("category"),
            stars: r.get("stars"),
            license: r.get("license"),
            pushed_at: r.get("pushed_at"),
            rating: r.get("rating"),
            rating_reason: r.get("rating_reason"),
            rating_confidence: r.get("rating_confidence"),
            notes: r.get("notes"),
            favorited_at: r.get("favorited_at"),
            metadata_refreshed_at: r.get("metadata_refreshed_at"),
            tags: Vec::new(),
            releases,
            languages,
        });
    }

    // Batch load tags for returned favorites
    let mut tag_builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        r#"
        SELECT id, favorite_id, tag, source
        FROM favorite_tags
        WHERE favorite_id IN (
        "#,
    );

    let mut separated = tag_builder.separated(", ");
    for id in &favorite_ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(") ORDER BY id ASC");

    let tag_rows = tag_builder.build().fetch_all(pool).await?;

    let mut tags_by_fav: std::collections::HashMap<i64, Vec<FavoriteTagItem>> =
        std::collections::HashMap::new();

    for tr in tag_rows {
        let fav_id: i64 = tr.get("favorite_id");
        tags_by_fav
            .entry(fav_id)
            .or_default()
            .push(FavoriteTagItem {
                id: tr.get("id"),
                favorite_id: fav_id,
                tag: tr.get("tag"),
                source: tr.get("source"),
            });
    }

    for fav in &mut favorites {
        if let Some(tags) = tags_by_fav.remove(&fav.id) {
            fav.tags = tags;
        }
    }

    Ok(favorites)
}

pub async fn list_distinct_tags(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows =
        sqlx::query("SELECT DISTINCT tag FROM favorite_tags ORDER BY tag COLLATE NOCASE ASC")
            .fetch_all(pool)
            .await?;

    Ok(rows
        .into_iter()
        .map(|r| r.get::<String, _>("tag"))
        .collect())
}

pub async fn update_user_metadata_tx(
    pool: &SqlitePool,
    input: &UpdateFavoriteUserMetadataInput,
) -> Result<FavoriteItem, AppError> {
    let valid_category = validate_category(&input.category)?;
    let valid_rating = validate_rating(input.rating)?;
    let valid_confidence = validate_rating_confidence(input.rating_confidence.as_deref())?;

    let _lock = DB_WRITE_LOCK.lock().await;
    let mut tx = pool.begin().await?;

    // 1. Verify existence
    let exists = sqlx::query("SELECT id FROM favorites WHERE id = ?")
        .bind(input.favorite_id)
        .fetch_optional(&mut *tx)
        .await?;

    if exists.is_none() {
        return Err(AppError::FavoriteNotFound(format!(
            "Favorite with id {} not found",
            input.favorite_id
        )));
    }

    // 2. Update user-owned metadata fields on favorites
    sqlx::query(
        "UPDATE favorites SET category = ?, rating = ?, rating_reason = ?, rating_confidence = ?, notes = ? WHERE id = ?"
    )
    .bind(valid_category)
    .bind(valid_rating)
    .bind(input.rating_reason.as_deref())
    .bind(valid_confidence.as_deref())
    .bind(&input.notes)
    .bind(input.favorite_id)
    .execute(&mut *tx)
    .await?;

    // 3. Delete ONLY user tags (leaving github and ai tags untouched)
    sqlx::query("DELETE FROM favorite_tags WHERE favorite_id = ? AND source = 'user'")
        .bind(input.favorite_id)
        .execute(&mut *tx)
        .await?;

    // 4. Insert normalized user tags
    let normalized_user_tags = normalize_topics(&input.user_tags);
    for tag in &normalized_user_tags {
        sqlx::query(
            r#"
            INSERT INTO favorite_tags (favorite_id, tag, source)
            VALUES (?, ?, 'user')
            "#,
        )
        .bind(input.favorite_id)
        .bind(tag)
        .execute(&mut *tx)
        .await?;
    }

    // 5. Update AI tags atomically if provided
    if let Some(ref raw_ai_tags) = input.ai_tags {
        let gh_tag_rows = sqlx::query(
            "SELECT tag FROM favorite_tags WHERE favorite_id = ? AND source = 'github'",
        )
        .bind(input.favorite_id)
        .fetch_all(&mut *tx)
        .await?;

        let gh_tags: Vec<String> = gh_tag_rows
            .into_iter()
            .map(|r| r.get::<String, _>("tag"))
            .collect();

        let valid_ai_tags = validate_ai_tags(raw_ai_tags)?;
        let filtered_ai = filter_supplemental_ai_tags(&valid_ai_tags, &gh_tags);

        sqlx::query("DELETE FROM favorite_tags WHERE favorite_id = ? AND source = 'ai'")
            .bind(input.favorite_id)
            .execute(&mut *tx)
            .await?;

        for tag in &filtered_ai {
            sqlx::query(
                r#"
                INSERT INTO favorite_tags (favorite_id, tag, source)
                VALUES (?, ?, 'ai')
                "#,
            )
            .bind(input.favorite_id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    find_favorite_by_id(pool, input.favorite_id)
        .await?
        .ok_or_else(|| AppError::Database("Updated favorite not found".to_string()))
}

pub fn normalize_tag_for_dedup(tag: &str) -> String {
    tag.trim().to_lowercase()
}

pub fn filter_supplemental_ai_tags(ai_tags: &[String], github_tags: &[String]) -> Vec<String> {
    let gh_set: HashSet<String> = github_tags
        .iter()
        .map(|t| normalize_tag_for_dedup(t))
        .filter(|t| !t.is_empty())
        .collect();

    let mut deduped = Vec::new();
    let mut seen_ai = HashSet::new();

    for raw_t in ai_tags {
        let t = raw_t.trim();
        if t.is_empty() {
            continue;
        }
        let norm = normalize_tag_for_dedup(t);
        if !gh_set.contains(&norm) && seen_ai.insert(norm) {
            deduped.push(t.to_string());
        }
    }

    deduped
}

pub fn validate_ai_tags(tags: &[String]) -> Result<Vec<String>, AppError> {
    if tags.len() > 5 {
        return Err(AppError::InvalidFavoriteInput(format!(
            "At most 5 AI tags allowed, got {}",
            tags.len()
        )));
    }

    let mut deduped = Vec::new();
    let mut seen_lower = HashSet::new();

    for raw_t in tags {
        let t = raw_t.trim();
        if t.is_empty() {
            return Err(AppError::InvalidFavoriteInput(
                "AI tag cannot be empty".to_string(),
            ));
        }

        if t.chars().count() > 40 {
            return Err(AppError::InvalidFavoriteInput(format!(
                "AI tag exceeds 40 characters limit ({} chars)",
                t.chars().count()
            )));
        }

        if t.contains('\n') || t.contains('\r') {
            return Err(AppError::InvalidFavoriteInput(
                "AI tag must be single-line".to_string(),
            ));
        }

        let lower = t.to_lowercase();
        if seen_lower.insert(lower) {
            deduped.push(t.to_string());
        }
    }

    if deduped.len() > 5 {
        return Err(AppError::InvalidFavoriteInput(format!(
            "At most 5 AI tags allowed after deduplication, got {}",
            deduped.len()
        )));
    }

    Ok(deduped)
}

pub async fn apply_favorite_ai_metadata_tx(
    pool: &SqlitePool,
    favorite_id: i64,
    category: &str,
    tags: &[String],
) -> Result<FavoriteItem, AppError> {
    let valid_category = validate_category(category)?;

    let _lock = DB_WRITE_LOCK.lock().await;
    let mut tx = pool.begin().await?;

    // 1. Verify existence
    let exists = sqlx::query("SELECT id FROM favorites WHERE id = ?")
        .bind(favorite_id)
        .fetch_optional(&mut *tx)
        .await?;

    if exists.is_none() {
        return Err(AppError::FavoriteNotFound(format!(
            "Favorite with id {} not found",
            favorite_id
        )));
    }

    // Query GitHub tags to filter duplicates
    let gh_tag_rows =
        sqlx::query("SELECT tag FROM favorite_tags WHERE favorite_id = ? AND source = 'github'")
            .bind(favorite_id)
            .fetch_all(&mut *tx)
            .await?;

    let gh_tags: Vec<String> = gh_tag_rows
        .into_iter()
        .map(|r| r.get::<String, _>("tag"))
        .collect();

    let valid_tags = validate_ai_tags(tags)?;
    let filtered_tags = filter_supplemental_ai_tags(&valid_tags, &gh_tags);

    // 2. Update category only
    sqlx::query("UPDATE favorites SET category = ? WHERE id = ?")
        .bind(valid_category)
        .bind(favorite_id)
        .execute(&mut *tx)
        .await?;

    // 3. Delete existing source = 'ai' tags
    sqlx::query("DELETE FROM favorite_tags WHERE favorite_id = ? AND source = 'ai'")
        .bind(favorite_id)
        .execute(&mut *tx)
        .await?;

    // 4. Insert new source = 'ai' tags
    for tag in &filtered_tags {
        sqlx::query(
            r#"
            INSERT INTO favorite_tags (favorite_id, tag, source)
            VALUES (?, ?, 'ai')
            "#,
        )
        .bind(favorite_id)
        .bind(tag)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_favorite_by_id(pool, favorite_id)
        .await?
        .ok_or_else(|| AppError::Database("Updated favorite not found".to_string()))
}

pub async fn update_github_owned_metadata_tx(
    pool: &SqlitePool,
    favorite_id: i64,
    repo: &crate::github::GitHubRepository,
    refreshed_at: &str,
    releases_patch: crate::library::models::MetadataPatch<i64>,
    languages_patch: crate::library::models::MetadataPatch<std::collections::HashMap<String, f64>>,
) -> Result<FavoriteItem, AppError> {
    let (owner, r_name, _) = parse_github_repo_url(&repo.github_url)?;
    let canonical_url = format!("https://github.com/{}/{}", owner, r_name);

    let _lock = DB_WRITE_LOCK.lock().await;
    let mut tx = pool.begin().await?;

    // 1. Verify existence
    let existing_row = sqlx::query(
        "SELECT id, github_repo_id, github_url, releases, languages FROM favorites WHERE id = ?",
    )
    .bind(favorite_id)
    .fetch_optional(&mut *tx)
    .await?;

    let existing = match existing_row {
        Some(r) => r,
        None => {
            return Err(AppError::FavoriteNotFound(format!(
                "Favorite with id {} not found",
                favorite_id
            )));
        }
    };

    // 2. Conflict check if repo renamed / transferred
    let conflict_row = sqlx::query(
        "SELECT id FROM favorites WHERE id != ? AND (github_repo_id = ? OR github_url = ?)",
    )
    .bind(favorite_id)
    .bind(repo.github_repo_id)
    .bind(&canonical_url)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(conflict) = conflict_row {
        let conflict_id: i64 = conflict.get("id");
        return Err(AppError::FavoriteIdentityConflict(format!(
            "Cannot refresh favorite {}: repository identity conflicts with existing favorite {}",
            favorite_id, conflict_id
        )));
    }

    // Determine target releases and languages according to single-field Keep/Replace semantics
    let target_releases: Option<i64> = match releases_patch {
        crate::library::models::MetadataPatch::Replace(r) => Some(r),
        crate::library::models::MetadataPatch::Keep => existing.get("releases"),
    };

    let target_languages: Option<String> = match languages_patch {
        crate::library::models::MetadataPatch::Replace(m) => {
            let json_str = serde_json::to_string(&m).unwrap_or_else(|_| "{}".to_string());
            Some(json_str)
        }
        crate::library::models::MetadataPatch::Keep => existing.get("languages"),
    };

    // 3. Update ONLY GitHub-owned metadata fields
    sqlx::query(
        r#"
        UPDATE favorites SET
            github_repo_id = ?,
            github_url = ?,
            name = ?,
            description = ?,
            stars = ?,
            license = ?,
            pushed_at = ?,
            metadata_refreshed_at = ?,
            releases = ?,
            languages = ?
        WHERE id = ?
        "#,
    )
    .bind(repo.github_repo_id)
    .bind(&canonical_url)
    .bind(&repo.name)
    .bind(&repo.description)
    .bind(repo.stars as i64)
    .bind(&repo.license)
    .bind(&repo.pushed_at)
    .bind(refreshed_at)
    .bind(target_releases)
    .bind(&target_languages)
    .bind(favorite_id)
    .execute(&mut *tx)
    .await?;

    // 4. Replace ONLY source='github' tags (preserving 'user' and 'ai' tags)
    sqlx::query("DELETE FROM favorite_tags WHERE favorite_id = ? AND source = 'github'")
        .bind(favorite_id)
        .execute(&mut *tx)
        .await?;

    let normalized_topics = normalize_topics(&repo.topics);
    for topic in &normalized_topics {
        sqlx::query(
            r#"
            INSERT INTO favorite_tags (favorite_id, tag, source)
            VALUES (?, ?, 'github')
            "#,
        )
        .bind(favorite_id)
        .bind(topic)
        .execute(&mut *tx)
        .await?;
    }

    // 5. GitHub refresh invariant: GitHub tag wins over identical AI tag
    sqlx::query(
        r#"
        DELETE FROM favorite_tags
        WHERE favorite_id = ?
          AND source = 'ai'
          AND EXISTS (
              SELECT 1 FROM favorite_tags g
              WHERE g.favorite_id = favorite_tags.favorite_id
                AND g.source = 'github'
                AND trim(g.tag) = trim(favorite_tags.tag) COLLATE NOCASE
          )
        "#,
    )
    .bind(favorite_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    find_favorite_by_id(pool, favorite_id)
        .await?
        .ok_or_else(|| AppError::Database("Refreshed favorite not found".to_string()))
}

pub async fn delete_favorite_by_id(pool: &SqlitePool, id: i64) -> Result<bool, AppError> {
    let _lock = DB_WRITE_LOCK.lock().await;
    let result = sqlx::query("DELETE FROM favorites WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::FavoriteNotFound(format!(
            "Favorite with id {} not found",
            id
        )));
    }

    Ok(true)
}
