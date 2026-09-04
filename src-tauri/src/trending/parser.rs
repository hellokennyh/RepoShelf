use scraper::{Html, Selector};
use std::sync::LazyLock;

use crate::error::AppError;
use crate::github::parse_github_repo_url;
use crate::trending::models::ParsedTrendingRepository;

// Centralized CSS selectors
static ARTICLE_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article.Box-row").expect("valid article selector"));
static REPO_LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h2 a[href]").expect("valid repo link selector"));
static DESCRIPTION_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("p").expect("valid description selector"));
static STARGAZERS_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href*='/stargazers']").expect("valid stargazers selector"));
static EMPTY_STATE_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".blankslate, div.blankslate").expect("valid empty state selector")
});

/// Parse period stars from text, looking for e.g. "963 stars today", "24,227 stars this week", "13,597 stars this month"
fn parse_period_stars_from_text(text: &str) -> Option<u64> {
    // Search for number preceding stars (today|this week|this month)
    let lower = text.to_lowercase();
    for period_marker in &[
        "stars today",
        "star today",
        "stars this week",
        "star this week",
        "stars this month",
        "star this month",
    ] {
        if let Some(pos) = lower.find(period_marker) {
            let prefix = &text[..pos];
            // Look backward for digits and commas
            let mut num_str = String::new();
            for ch in prefix.chars().rev() {
                if ch.is_ascii_digit() || ch == ',' {
                    num_str.push(ch);
                } else if !num_str.is_empty() && ch.is_whitespace() {
                    // Leading whitespace before the number is fine, but stop if we already have digits
                    break;
                } else if !num_str.is_empty() {
                    break;
                }
            }
            if !num_str.is_empty() {
                let clean: String = num_str
                    .chars()
                    .rev()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(val) = clean.parse::<u64>() {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Parse total stars from stargazers link text
fn parse_total_stars(text: &str) -> Option<u64> {
    let clean: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if clean.is_empty() {
        None
    } else {
        clean.parse::<u64>().ok()
    }
}

/// Clean and collapse whitespace in text
fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Independent, pure, deterministic HTML parser for GitHub Trending pages.
pub fn parse_trending_html(html: &str) -> Result<Vec<ParsedTrendingRepository>, AppError> {
    let document = Html::parse_document(html);

    // 1. Check for recognized empty state
    if document.select(&EMPTY_STATE_SELECTOR).next().is_some() {
        return Ok(Vec::new());
    }

    let mut parsed_items = Vec::new();
    let mut rank = 1usize;
    let mut encountered_articles = 0usize;

    for article in document.select(&ARTICLE_SELECTOR) {
        encountered_articles += 1;

        // Extract repository link
        let repo_link_elem = match article.select(&REPO_LINK_SELECTOR).next() {
            Some(elem) => elem,
            None => {
                // Malformed card missing repo link
                continue;
            }
        };

        let href = match repo_link_elem.value().attr("href") {
            Some(h) if !h.trim().is_empty() => h.trim(),
            _ => continue,
        };

        let full_url = if href.starts_with("http://") || href.starts_with("https://") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("https://github.com{}", href)
        } else {
            format!("https://github.com/{}", href)
        };

        // Canonical validation using Phase 2 URL normalizer
        let (owner, repo, canonical_url) = match parse_github_repo_url(&full_url) {
            Ok(res) => res,
            Err(_) => {
                // Malformed or invalid repository root URL, skip this card
                continue;
            }
        };

        // Extract description
        let description = article
            .select(&DESCRIPTION_SELECTOR)
            .next()
            .map(|elem| clean_text(&elem.text().collect::<String>()))
            .filter(|s| !s.is_empty());

        // Extract total stars
        let stars = article
            .select(&STARGAZERS_SELECTOR)
            .next()
            .and_then(|elem| parse_total_stars(&elem.text().collect::<String>()));

        // Extract period stars
        let article_full_text = article.text().collect::<String>();
        let period_stars = parse_period_stars_from_text(&article_full_text);

        parsed_items.push(ParsedTrendingRepository {
            github_url: canonical_url,
            owner,
            repo: repo.clone(),
            name: repo,
            description,
            stars,
            period_stars,
            trending_rank: rank,
        });

        rank += 1;
    }

    if parsed_items.is_empty() {
        if encountered_articles == 0 {
            return Err(AppError::TrendingParse(
                "No trending repository articles found in HTML (DOM structure changed or non-trending page)"
                    .to_string(),
            ));
        } else {
            return Err(AppError::TrendingParse(
                "Articles were present but failed validation (no valid repository links recognized)"
                    .to_string(),
            ));
        }
    }

    Ok(parsed_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_period_stars() {
        assert_eq!(parse_period_stars_from_text("963 stars today"), Some(963));
        assert_eq!(
            parse_period_stars_from_text("24,227 stars this week"),
            Some(24227)
        );
        assert_eq!(
            parse_period_stars_from_text("13,597 stars this month"),
            Some(13597)
        );
        assert_eq!(parse_period_stars_from_text("1 star today"), Some(1));
        assert_eq!(
            parse_period_stars_from_text("random text without stars"),
            None
        );
    }

    #[test]
    fn test_parse_total_stars() {
        assert_eq!(parse_total_stars("25,308"), Some(25308));
        assert_eq!(parse_total_stars("  123,456  "), Some(123456));
        assert_eq!(parse_total_stars("no numbers"), None);
    }
}
