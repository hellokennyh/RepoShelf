use chrono::NaiveDate;

use crate::error::AppError;
use crate::github::models::RepositorySearchRequest;

/// Validates search request parameters and constructs the formatted GitHub search query string.
///
/// Qualifiers:
/// - `stars:>={min_stars}` when min_stars is specified and > 0
/// - `pushed:>={pushed_after}` when pushed_after date is specified (YYYY-MM-DD)
pub fn build_search_query(req: &RepositorySearchRequest) -> Result<String, AppError> {
    let trimmed_query = req.query.trim();
    if trimmed_query.is_empty() {
        return Err(AppError::InvalidSearchInput(
            "Search query cannot be empty".to_string(),
        ));
    }

    if req.limit < 1 || req.limit > 100 {
        return Err(AppError::InvalidSearchInput(format!(
            "Search limit must be between 1 and 100, got {}",
            req.limit
        )));
    }

    let mut parts: Vec<String> = vec![trimmed_query.to_string()];

    if let Some(stars) = req.min_stars {
        if stars < 0 {
            return Err(AppError::InvalidSearchInput(
                "Minimum stars cannot be negative".to_string(),
            ));
        }
        if stars > 0 {
            parts.push(format!("stars:>={}", stars));
        }
    }

    if let Some(ref date_str) = req.pushed_after {
        let trimmed_date = date_str.trim();
        if !trimmed_date.is_empty() {
            // Validate YYYY-MM-DD format
            if NaiveDate::parse_from_str(trimmed_date, "%Y-%m-%d").is_err() {
                return Err(AppError::InvalidSearchInput(format!(
                    "Invalid pushed_after date format '{}'. Expected YYYY-MM-DD",
                    trimmed_date
                )));
            }
            parts.push(format!("pushed:>={}", trimmed_date));
        }
    }

    Ok(parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_only() {
        let req = RepositorySearchRequest {
            query: "tauri desktop".to_string(),
            min_stars: None,
            pushed_after: None,
            limit: 20,
            exclude_selected: false,
            exclude_favorites: false,
        };
        let q = build_search_query(&req).unwrap();
        assert_eq!(q, "tauri desktop");
    }

    #[test]
    fn test_keyword_with_stars() {
        let req = RepositorySearchRequest {
            query: "rust sqlx".to_string(),
            min_stars: Some(500),
            pushed_after: None,
            limit: 30,
            exclude_selected: false,
            exclude_favorites: false,
        };
        let q = build_search_query(&req).unwrap();
        assert_eq!(q, "rust sqlx stars:>=500");
    }

    #[test]
    fn test_keyword_with_pushed_after() {
        let req = RepositorySearchRequest {
            query: "ai agents".to_string(),
            min_stars: None,
            pushed_after: Some("2026-01-01".to_string()),
            limit: 10,
            exclude_selected: false,
            exclude_favorites: false,
        };
        let q = build_search_query(&req).unwrap();
        assert_eq!(q, "ai agents pushed:>=2026-01-01");
    }

    #[test]
    fn test_combined_qualifiers() {
        let req = RepositorySearchRequest {
            query: "instagram scraper".to_string(),
            min_stars: Some(100),
            pushed_after: Some("2026-03-04".to_string()),
            limit: 50,
            exclude_selected: false,
            exclude_favorites: false,
        };
        let q = build_search_query(&req).unwrap();
        assert_eq!(q, "instagram scraper stars:>=100 pushed:>=2026-03-04");
    }

    #[test]
    fn test_invalid_requests() {
        // Empty query
        let req = RepositorySearchRequest {
            query: "   ".to_string(),
            min_stars: None,
            pushed_after: None,
            limit: 20,
            exclude_selected: false,
            exclude_favorites: false,
        };
        assert!(build_search_query(&req).is_err());

        // Limit 0
        let req = RepositorySearchRequest {
            query: "test".to_string(),
            min_stars: None,
            pushed_after: None,
            limit: 0,
            exclude_selected: false,
            exclude_favorites: false,
        };
        assert!(build_search_query(&req).is_err());

        // Limit > 100
        let req = RepositorySearchRequest {
            query: "test".to_string(),
            min_stars: None,
            pushed_after: None,
            limit: 101,
            exclude_selected: false,
            exclude_favorites: false,
        };
        assert!(build_search_query(&req).is_err());

        // Negative stars
        let req = RepositorySearchRequest {
            query: "test".to_string(),
            min_stars: Some(-10),
            pushed_after: None,
            limit: 20,
            exclude_selected: false,
            exclude_favorites: false,
        };
        assert!(build_search_query(&req).is_err());

        // Invalid date format
        let req = RepositorySearchRequest {
            query: "test".to_string(),
            min_stars: None,
            pushed_after: Some("2026/03/04".to_string()),
            limit: 20,
            exclude_selected: false,
            exclude_favorites: false,
        };
        assert!(build_search_query(&req).is_err());
    }
}
