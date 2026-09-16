use reposhelf_lib::star_history::parse_readme_for_star_history;

#[test]
fn test_fixture_01_markdown_inline_official_star_history_image() {
    let readme = r#"
# Sample Repo
Here is our star growth:
![Star History Chart](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}

#[test]
fn test_fixture_02_reference_style_markdown_official_star_history_image() {
    let readme = r#"
# Sample Repo
Star history below:
![Star History][star-history-ref]

[star-history-ref]: https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}

#[test]
fn test_fixture_03_html_img_official_star_history_image() {
    let readme = r#"
# Sample Repo
<div align="center">
    <img src="https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date" alt="Star History Chart" />
</div>
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}

#[test]
fn test_fixture_04_html_picture_source_supported_source() {
    let readme = r#"
# Sample Repo
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date" />
  <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date" />
  <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date" />
</picture>
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}

#[test]
fn test_fixture_05_text_star_history_with_no_image_rejected() {
    let readme = r#"
# Sample Repo
Star History: Check out our stars on github!
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_06_ordinary_hyperlink_to_star_history_rejected() {
    let readme = r#"
# Sample Repo
[Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_07_official_host_but_wrong_path_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/chart?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_08_wrong_host_or_look_alike_rejected() {
    let readme1 = r#"
![Star History](https://star-history.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme1, "test-owner", "test-repo"),
        None
    );

    let readme2 = r#"
![Star History](https://api.star-history.evil.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme2, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_09_http_instead_of_https_rejected() {
    let readme = r#"
![Star History](http://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_10_wrong_repository_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=other-owner/other-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_11_multi_repository_repos_query_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo,other/repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_12_missing_repos_query_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_13_unknown_query_parameter_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date&theme=dark)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_14_type_date_accepted() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}

#[test]
fn test_fixture_15_type_timeline_accepted() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Timeline)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some(
            "https://api.star-history.com/svg?repos=test-owner/test-repo&type=Timeline".to_string()
        )
    );
}

#[test]
fn test_fixture_16_unsupported_type_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Monthly)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_17_commented_out_image_rejected() {
    let readme = r#"
<!-- ![Star History](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date) -->
<!-- <img src="https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date" /> -->
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_18_malformed_embed_rejected() {
    let readme = r#"
![Star History](https://api.star-history.com/svg?repos=)
<img src="not-a-url" />
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        None
    );
}

#[test]
fn test_fixture_19_duplicate_supported_embeds_first_document_order() {
    let readme = r#"
# Readme

First embed:
![First](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date)

Second embed:
![Second](https://api.star-history.com/svg?repos=test-owner/test-repo&type=Timeline)
"#;
    assert_eq!(
        parse_readme_for_star_history(readme, "test-owner", "test-repo"),
        Some("https://api.star-history.com/svg?repos=test-owner/test-repo&type=Date".to_string())
    );
}
