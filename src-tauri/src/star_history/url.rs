use url::Url;

/// Validates a candidate Star History embed URL against the strict S6 contract:
/// - Scheme must be HTTPS
/// - Host must be exact `api.star-history.com` (case-insensitive)
/// - Path must be exact `/svg`
/// - Port must be default (no explicit port)
/// - Username / password must be absent
/// - Fragment must be absent
/// - Allowed query keys: `repos` (required exactly once), `type` (optional)
/// - `repos` value must match `owner/repo` belonging to `current_owner`/`current_repo` (case-insensitive)
/// - `type` value, if present, must be `Date` or `Timeline`
/// - Unknown query parameters, multiple repos, or look-alike hosts are rejected.
pub fn validate_star_history_url(
    candidate: &str,
    current_owner: &str,
    current_repo: &str,
) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = Url::parse(trimmed).ok()?;

    // 1. HTTPS only
    if parsed.scheme() != "https" {
        return None;
    }

    // 2. Exact host api.star-history.com
    let host = parsed.host_str()?;
    if !host.eq_ignore_ascii_case("api.star-history.com") {
        return None;
    }

    // 3. No explicit non-default port
    if parsed.port().is_some() {
        return None;
    }

    // 4. No username or password
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }

    // 5. Exact path /svg
    if parsed.path() != "/svg" {
        return None;
    }

    // 6. Fragment rejected for canonical matching
    if parsed.fragment().is_some() {
        return None;
    }

    // 7. Inspect query parameters
    let mut repos_val: Option<String> = None;
    let mut type_val: Option<String> = None;

    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "repos" => {
                if repos_val.is_some() {
                    // Duplicate repos query key is rejected
                    return None;
                }
                repos_val = Some(v.into_owned());
            }
            "type" => {
                if type_val.is_some() {
                    // Duplicate type query key is rejected
                    return None;
                }
                type_val = Some(v.into_owned());
            }
            _ => {
                // Unknown query keys are strictly rejected
                return None;
            }
        }
    }

    // repos is required exactly once
    let repos_str = repos_val?;

    // Must not contain comma (no multiple repositories / comparisons)
    if repos_str.contains(',') {
        return None;
    }

    // Must resolve to owner/repo
    let parts: Vec<&str> = repos_str.split('/').collect();
    if parts.len() != 2 {
        return None;
    }
    let embed_owner = parts[0].trim();
    let embed_repo = parts[1].trim();
    if embed_owner.is_empty() || embed_repo.is_empty() {
        return None;
    }

    // Case-insensitive comparison with current canonical owner/repo
    if !embed_owner.eq_ignore_ascii_case(current_owner)
        || !embed_repo.eq_ignore_ascii_case(current_repo)
    {
        return None;
    }

    // type: optional, if present must be Date or Timeline
    if let Some(ref t) = type_val {
        if t != "Date" && t != "Timeline" {
            return None;
        }
    }

    // Construct canonical URL
    let canonical = match type_val {
        Some(t) => format!(
            "https://api.star-history.com/svg?repos={}/{}&type={}",
            current_owner, current_repo, t
        ),
        None => format!(
            "https://api.star-history.com/svg?repos={}/{}",
            current_owner, current_repo
        ),
    };

    Some(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_urls() {
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );

        // Case insensitivity
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=Owner/Repo&type=Date",
                "owner",
                "repo"
            ),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Date".to_string())
        );

        // Timeline type
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo&type=Timeline",
                "owner",
                "repo"
            ),
            Some("https://api.star-history.com/svg?repos=owner/repo&type=Timeline".to_string())
        );

        // Without type
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo",
                "owner",
                "repo"
            ),
            Some("https://api.star-history.com/svg?repos=owner/repo".to_string())
        );
    }

    #[test]
    fn test_invalid_urls() {
        // http instead of https
        assert_eq!(
            validate_star_history_url(
                "http://api.star-history.com/svg?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // wrong host / lookalike
        assert_eq!(
            validate_star_history_url(
                "https://star-history.com/svg?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );
        assert_eq!(
            validate_star_history_url(
                "https://evil.api.star-history.com/svg?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // wrong path
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/generate?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // explicit non-default port
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com:8080/svg?repos=owner/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // wrong repo
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/other&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // multi repo
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo,other/repo&type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // missing repos
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?type=Date",
                "owner",
                "repo"
            ),
            None
        );

        // unknown query param
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo&type=Date&theme=dark",
                "owner",
                "repo"
            ),
            None
        );

        // unsupported type
        assert_eq!(
            validate_star_history_url(
                "https://api.star-history.com/svg?repos=owner/repo&type=Monthly",
                "owner",
                "repo"
            ),
            None
        );
    }
}
