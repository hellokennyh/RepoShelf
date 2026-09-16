use pulldown_cmark::{Event, Parser, Tag};
use scraper::{Html, Selector};

use crate::star_history::url::validate_star_history_url;

/// Parses a repository default README to discover any supported Star History embed.
///
/// Supports:
/// 1. Markdown inline image: `![alt](url)`
/// 2. Markdown reference-style image: `![alt][ref]` with `[ref]: url`
/// 3. Embedded HTML `<img src="...">`
/// 4. Embedded HTML `<picture><source srcset="...">`
///
/// Rejects:
/// - plain text ("Star History")
/// - plain hyperlinks `[Star History](url)`
/// - commented-out embeds `<!-- ![...](...) -->` or `<!-- <img ...> -->`
/// - non-matching repositories or unsupported URLs
///
/// Returns the first valid canonical Star History URL in document order.
pub fn parse_readme_for_star_history(readme: &str, owner: &str, repo: &str) -> Option<String> {
    if readme.trim().is_empty() {
        return None;
    }

    let parser = Parser::new(readme);
    let img_selector = Selector::parse("img").ok()?;
    let source_selector = Selector::parse("source").ok()?;

    for event in parser {
        match event {
            // 1 & 2: Markdown inline & reference-style images
            Event::Start(Tag::Image { dest_url, .. }) => {
                if let Some(canonical) = validate_star_history_url(&dest_url, owner, repo) {
                    return Some(canonical);
                }
            }

            // 3 & 4: Embedded HTML fragments
            Event::Html(raw_html) | Event::InlineHtml(raw_html) => {
                let trimmed_html = raw_html.trim();

                // Skip HTML comments
                if trimmed_html.starts_with("<!--") {
                    continue;
                }

                // Parse HTML fragment
                let fragment = Html::parse_fragment(&raw_html);

                // Check in document order: picture sources first, then img
                for element in fragment.select(&source_selector) {
                    if let Some(srcset) = element.value().attr("srcset") {
                        // srcset can be comma-separated candidates with optional density/width descriptors
                        for candidate in srcset.split(',') {
                            let url_part = candidate.trim().split_whitespace().next().unwrap_or("");
                            if let Some(canonical) =
                                validate_star_history_url(url_part, owner, repo)
                            {
                                return Some(canonical);
                            }
                        }
                    }
                }

                for element in fragment.select(&img_selector) {
                    if let Some(src) = element.value().attr("src") {
                        if let Some(canonical) = validate_star_history_url(src.trim(), owner, repo)
                        {
                            return Some(canonical);
                        }
                    }
                }
            }

            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_inline_image() {
        let md = "# Project\n\n![Star History Chart](https://api.star-history.com/svg?repos=owner/repo&type=Date)\n";
        assert_eq!(
            parse_readme_for_star_history(md, "owner", "repo"),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );
    }

    #[test]
    fn test_markdown_reference_style_image() {
        let md = "# Project\n\n![Star History][star-history-ref]\n\n[star-history-ref]: https://api.star-history.com/svg?repos=owner/repo&type=Date\n";
        assert_eq!(
            parse_readme_for_star_history(md, "owner", "repo"),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );
    }

    #[test]
    fn test_html_img_embed() {
        let md = "# Project\n\n<img src=\"https://api.star-history.com/svg?repos=owner/repo&type=Date\" alt=\"Star History Chart\" />\n";
        assert_eq!(
            parse_readme_for_star_history(md, "owner", "repo"),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );
    }

    #[test]
    fn test_html_picture_source_embed() {
        let md = r#"
# Project

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=owner/repo&type=Date" />
  <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=owner/repo&type=Date" />
  <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=owner/repo&type=Date" />
</picture>
"#;
        assert_eq!(
            parse_readme_for_star_history(md, "owner", "repo"),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );
    }

    #[test]
    fn test_commented_out_embeds_rejected() {
        let md =
            "<!-- ![Star History](https://api.star-history.com/svg?repos=owner/repo&type=Date) -->";
        assert_eq!(parse_readme_for_star_history(md, "owner", "repo"), None);

        let md_html =
            "<!-- <img src=\"https://api.star-history.com/svg?repos=owner/repo&type=Date\"> -->";
        assert_eq!(
            parse_readme_for_star_history(md_html, "owner", "repo"),
            None
        );
    }

    #[test]
    fn test_plain_text_and_hyperlink_rejected() {
        // Plain text
        let md = "# Project\n\nStar History is awesome!";
        assert_eq!(parse_readme_for_star_history(md, "owner", "repo"), None);

        // Plain hyperlink
        let md_link = "[Star History](https://api.star-history.com/svg?repos=owner/repo&type=Date)";
        assert_eq!(
            parse_readme_for_star_history(md_link, "owner", "repo"),
            None
        );
    }

    #[test]
    fn test_duplicate_embeds_first_document_order() {
        let md = r#"
![First](https://api.star-history.com/svg?repos=owner/repo&type=Date)
![Second](https://api.star-history.com/svg?repos=owner/repo&type=Timeline)
"#;
        assert_eq!(
            parse_readme_for_star_history(md, "owner", "repo"),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );
    }
}
