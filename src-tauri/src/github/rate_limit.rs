use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;

use crate::error::AppError;
use crate::github::models::{GitHubRateLimitStatus, RateLimitBucket};

pub fn format_reset_time(epoch_secs: i64) -> String {
    DateTime::from_timestamp(epoch_secs, 0)
        .map(|dt: DateTime<Utc>| dt.to_rfc3339())
        .unwrap_or_else(|| epoch_secs.to_string())
}

/// Parses rate limit headers from a GitHub REST API response.
pub fn parse_rate_limit_headers(headers: &HeaderMap) -> Option<RateLimitBucket> {
    let limit = headers
        .get("x-ratelimit-limit")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())?;

    let remaining = headers
        .get("x-ratelimit-remaining")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())?;

    let used = headers
        .get("x-ratelimit-used")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| limit.saturating_sub(remaining));

    let reset_epoch = headers
        .get("x-ratelimit-reset")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let resource = headers
        .get("x-ratelimit-resource")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("core")
        .to_string();

    let reset_at = if reset_epoch > 0 {
        format_reset_time(reset_epoch)
    } else {
        String::new()
    };

    Some(RateLimitBucket {
        limit,
        remaining,
        used,
        reset_at,
        resource,
    })
}

/// Updates the shared in-memory rate limit status with a parsed bucket.
pub fn update_rate_limit_status(status: &mut GitHubRateLimitStatus, bucket: RateLimitBucket) {
    if bucket.resource == "search" {
        status.search = bucket;
    } else {
        status.core = bucket;
    }
}

/// Checks if an HTTP 403 or 429 response indicates rate limit exhaustion or secondary limit.
pub fn handle_rate_limit_error(status_code: u16, headers: &HeaderMap) -> AppError {
    let bucket = parse_rate_limit_headers(headers);
    let retry_after = headers
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok());

    let (remaining, reset_epoch, resource) = match &bucket {
        Some(b) => {
            let reset = headers
                .get("x-ratelimit-reset")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok());
            (Some(b.remaining), reset, b.resource.as_str())
        }
        None => (None, None, "core"),
    };

    if remaining == Some(0) {
        // Primary rate limit exhausted
        let reset_str = reset_epoch
            .map(format_reset_time)
            .unwrap_or_else(|| "soon".to_string());
        AppError::GitHubRateLimit {
            reset_at: reset_epoch,
            message: format!(
                "GitHub API rate limit exhausted for resource '{}'. Resets at {}",
                resource, reset_str
            ),
        }
    } else if status_code == 429 || status_code == 403 {
        // Secondary rate limit or abuse detection
        let wait_msg = if let Some(secs) = retry_after {
            format!(" Retry after {} seconds.", secs)
        } else if let Some(reset) = reset_epoch {
            format!(" Resets at {}.", format_reset_time(reset))
        } else {
            String::new()
        };

        AppError::GitHubRateLimit {
            reset_at: retry_after.or(reset_epoch),
            message: format!(
                "GitHub secondary rate limit or abuse detection triggered.{}",
                wait_msg
            ),
        }
    } else {
        AppError::GitHubRateLimit {
            reset_at: reset_epoch,
            message: "GitHub rate limit exceeded.".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn test_parse_rate_limit_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-limit", HeaderValue::from_static("60"));
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("55"));
        headers.insert("x-ratelimit-used", HeaderValue::from_static("5"));
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("1773000000"));
        headers.insert("x-ratelimit-resource", HeaderValue::from_static("core"));

        let bucket = parse_rate_limit_headers(&headers).expect("Failed to parse headers");
        assert_eq!(bucket.limit, 60);
        assert_eq!(bucket.remaining, 55);
        assert_eq!(bucket.used, 5);
        assert_eq!(bucket.resource, "core");
        assert!(!bucket.reset_at.is_empty());
    }

    #[test]
    fn test_handle_rate_limit_primary_exhaustion() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-limit", HeaderValue::from_static("60"));
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("1773000000"));
        headers.insert("x-ratelimit-resource", HeaderValue::from_static("search"));

        let err = handle_rate_limit_error(403, &headers);
        assert_eq!(err.code(), "GitHubRateLimit");
        let msg = err.to_string();
        assert!(msg.contains("exhausted"));
        assert!(msg.contains("search"));
    }

    #[test]
    fn test_handle_rate_limit_secondary() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("60"));

        let err = handle_rate_limit_error(429, &headers);
        assert_eq!(err.code(), "GitHubRateLimit");
        assert!(err.to_string().contains("secondary rate limit"));
    }
}
