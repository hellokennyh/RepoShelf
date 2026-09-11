use url::{Host, Url};

use crate::error::AppError;
use crate::settings::AiProvider;

pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434";

/// Validates URL security:
/// 1. Reject embedded user credentials.
/// 2. Reject query parameters and fragments.
/// 3. Require HTTPS for all remote hosts.
/// 4. Permit HTTP ONLY for loopback addresses (localhost, 127.0.0.1, ::1).
pub fn validate_url_security(url: &Url) -> Result<(), AppError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::SettingsValidation(
            "AI Base URL must not contain embedded user credentials".to_string(),
        ));
    }
    if url.query().is_some() {
        return Err(AppError::SettingsValidation(
            "AI Base URL must not contain query parameters".to_string(),
        ));
    }
    if url.fragment().is_some() {
        return Err(AppError::SettingsValidation(
            "AI Base URL must not contain URL fragments".to_string(),
        ));
    }

    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let is_loopback = match url.host() {
                Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
                Some(Host::Ipv4(ip)) => ip.is_loopback(),
                Some(Host::Ipv6(ip)) => ip.is_loopback(),
                None => false,
            };

            if is_loopback {
                Ok(())
            } else {
                Err(AppError::SettingsValidation(
                    "HTTP is only permitted for loopback addresses (localhost, 127.0.0.1, ::1). HTTPS is required for remote hosts.".to_string(),
                ))
            }
        }
        scheme => Err(AppError::SettingsValidation(format!(
            "Unsupported URL scheme '{}': only http and https are permitted",
            scheme
        ))),
    }
}

/// Resolves the default base URL for the specified AI provider.
pub fn default_base_url_for_provider(provider: AiProvider) -> &'static str {
    match provider {
        AiProvider::OpenaiCompatible => DEFAULT_OPENAI_BASE_URL,
        AiProvider::Anthropic => DEFAULT_ANTHROPIC_BASE_URL,
        AiProvider::Gemini => DEFAULT_GEMINI_BASE_URL,
        AiProvider::Ollama => DEFAULT_OLLAMA_BASE_URL,
    }
}

/// Normalizes an Ollama base URL to the Ollama server root (e.g. `http://127.0.0.1:11434`).
/// Strips trailing slashes and common endpoint suffixes such as `/v1/chat/completions`,
/// `/chat/completions`, `/v1`, `/api/chat`, `/api/tags`, `/api`, etc., preventing duplicate
/// segments and malformed URLs.
pub fn normalize_ollama_server_root(custom_base: Option<&str>) -> Result<Url, AppError> {
    let raw_base = match custom_base.map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b,
        None => DEFAULT_OLLAMA_BASE_URL,
    };

    let base_parsed = Url::parse(raw_base)
        .map_err(|e| AppError::SettingsValidation(format!("Invalid AI Base URL format: {}", e)))?;

    validate_url_security(&base_parsed)?;

    let mut clean_base = raw_base.trim_end_matches('/');

    const OLLAMA_SUFFIXES: &[&str] = &[
        "/v1/chat/completions",
        "/chat/completions",
        "/v1/models",
        "/v1",
        "/api/chat",
        "/api/tags",
        "/api/show",
        "/api/generate",
        "/api",
    ];

    for suffix in OLLAMA_SUFFIXES {
        if clean_base.ends_with(suffix) {
            clean_base = clean_base[..clean_base.len() - suffix.len()].trim_end_matches('/');
            break;
        }
    }

    let root_url = Url::parse(clean_base).map_err(|e| {
        AppError::SettingsValidation(format!("Failed to parse normalized Ollama root URL: {}", e))
    })?;

    validate_url_security(&root_url)?;

    Ok(root_url)
}

/// Resolves the native Ollama `/api/tags` endpoint URL.
pub fn resolve_ollama_tags_endpoint(custom_base: Option<&str>) -> Result<Url, AppError> {
    let root = normalize_ollama_server_root(custom_base)?;
    let root_str = root.as_str().trim_end_matches('/');
    let endpoint_str = format!("{}/api/tags", root_str);
    let endpoint_url = Url::parse(&endpoint_str).map_err(|e| {
        AppError::SettingsValidation(format!("Failed to parse resolved Ollama tags URL: {}", e))
    })?;
    validate_url_security(&endpoint_url)?;
    Ok(endpoint_url)
}

/// Resolves and validates the complete endpoint URL for an AI provider request.
/// Safely trims trailing slashes and prevents duplicate path segments such as `/v1/v1/...`
/// or `/api/api/...`.
pub fn resolve_provider_endpoint(
    provider: AiProvider,
    custom_base: Option<&str>,
) -> Result<Url, AppError> {
    if provider == AiProvider::Ollama {
        let root = normalize_ollama_server_root(custom_base)?;
        let root_str = root.as_str().trim_end_matches('/');
        let endpoint_str = format!("{}/api/chat", root_str);
        let endpoint_url = Url::parse(&endpoint_str).map_err(|e| {
            AppError::SettingsValidation(format!("Failed to parse resolved endpoint URL: {}", e))
        })?;
        validate_url_security(&endpoint_url)?;
        return Ok(endpoint_url);
    }

    let raw_base = match custom_base.map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b,
        None => default_base_url_for_provider(provider),
    };

    let base_parsed = Url::parse(raw_base)
        .map_err(|e| AppError::SettingsValidation(format!("Invalid AI Base URL format: {}", e)))?;

    // Security check on the base URL
    validate_url_security(&base_parsed)?;

    let clean_base = raw_base.trim_end_matches('/');

    let endpoint_str = match provider {
        AiProvider::OpenaiCompatible => {
            if clean_base.ends_with("/chat/completions") {
                clean_base.to_string()
            } else {
                format!("{}/chat/completions", clean_base)
            }
        }
        AiProvider::Anthropic => {
            if clean_base.ends_with("/v1/messages") {
                clean_base.to_string()
            } else if clean_base.ends_with("/v1") {
                format!("{}/messages", clean_base)
            } else {
                format!("{}/v1/messages", clean_base)
            }
        }
        AiProvider::Gemini => {
            if clean_base.ends_with("/interactions") {
                clean_base.to_string()
            } else {
                format!("{}/interactions", clean_base)
            }
        }
        AiProvider::Ollama => unreachable!(),
    };

    let endpoint_url = Url::parse(&endpoint_str).map_err(|e| {
        AppError::SettingsValidation(format!("Failed to parse resolved endpoint URL: {}", e))
    })?;

    validate_url_security(&endpoint_url)?;

    Ok(endpoint_url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback_http_allowed() {
        assert!(Url::parse("http://localhost:11434")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .is_ok());
        assert!(Url::parse("http://127.0.0.1:11434")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .is_ok());
        assert!(Url::parse("http://[::1]:11434")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .is_ok());
    }

    #[test]
    fn test_remote_http_rejected() {
        let err = Url::parse("http://api.openai.com/v1")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("HTTP is only permitted for loopback"));
    }

    #[test]
    fn test_https_allowed_for_any_host() {
        assert!(Url::parse("https://api.openai.com/v1")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .is_ok());
        assert!(Url::parse("https://my-custom-proxy.internal/v1")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .is_ok());
    }

    #[test]
    fn test_credentials_rejected() {
        let err = Url::parse("https://user:pass@api.openai.com/v1")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("must not contain embedded user credentials"));
    }

    #[test]
    fn test_query_and_fragment_rejected() {
        let err_q = Url::parse("https://api.openai.com/v1?test=1")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .unwrap_err();
        assert!(err_q
            .to_string()
            .contains("must not contain query parameters"));

        let err_f = Url::parse("https://api.openai.com/v1#section")
            .map(|u| validate_url_security(&u))
            .unwrap()
            .unwrap_err();
        assert!(err_f.to_string().contains("must not contain URL fragments"));
    }

    #[test]
    fn test_openai_endpoint_resolution() {
        let u1 = resolve_provider_endpoint(AiProvider::OpenaiCompatible, None).unwrap();
        assert_eq!(u1.as_str(), "https://api.openai.com/v1/chat/completions");

        let u2 = resolve_provider_endpoint(
            AiProvider::OpenaiCompatible,
            Some("https://proxy.example.com/v1/"),
        )
        .unwrap();
        assert_eq!(u2.as_str(), "https://proxy.example.com/v1/chat/completions");

        let u3 = resolve_provider_endpoint(
            AiProvider::OpenaiCompatible,
            Some("https://proxy.example.com/v1/chat/completions"),
        )
        .unwrap();
        assert_eq!(u3.as_str(), "https://proxy.example.com/v1/chat/completions");
    }

    #[test]
    fn test_anthropic_duplicate_v1_prevented() {
        // Default base
        let u1 = resolve_provider_endpoint(AiProvider::Anthropic, None).unwrap();
        assert_eq!(u1.as_str(), "https://api.anthropic.com/v1/messages");

        // Custom base without /v1
        let u2 =
            resolve_provider_endpoint(AiProvider::Anthropic, Some("https://my-anthropic.internal"))
                .unwrap();
        assert_eq!(u2.as_str(), "https://my-anthropic.internal/v1/messages");

        // Custom base with /v1 - MUST NOT become /v1/v1/messages
        let u3 = resolve_provider_endpoint(
            AiProvider::Anthropic,
            Some("https://my-anthropic.internal/v1"),
        )
        .unwrap();
        assert_eq!(u3.as_str(), "https://my-anthropic.internal/v1/messages");

        let u4 = resolve_provider_endpoint(
            AiProvider::Anthropic,
            Some("https://my-anthropic.internal/v1/"),
        )
        .unwrap();
        assert_eq!(u4.as_str(), "https://my-anthropic.internal/v1/messages");

        let u5 = resolve_provider_endpoint(
            AiProvider::Anthropic,
            Some("https://my-anthropic.internal/v1/messages"),
        )
        .unwrap();
        assert_eq!(u5.as_str(), "https://my-anthropic.internal/v1/messages");
    }

    #[test]
    fn test_gemini_endpoint_resolution() {
        let u1 = resolve_provider_endpoint(AiProvider::Gemini, None).unwrap();
        assert_eq!(
            u1.as_str(),
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );

        let u2 = resolve_provider_endpoint(
            AiProvider::Gemini,
            Some("https://custom-gemini.internal/v1beta/"),
        )
        .unwrap();
        assert_eq!(
            u2.as_str(),
            "https://custom-gemini.internal/v1beta/interactions"
        );
    }

    #[test]
    fn test_ollama_endpoint_resolution() {
        let u1 = resolve_provider_endpoint(AiProvider::Ollama, None).unwrap();
        assert_eq!(u1.as_str(), "http://127.0.0.1:11434/api/chat");

        let u2 = resolve_provider_endpoint(AiProvider::Ollama, Some("http://localhost:11434/api"))
            .unwrap();
        assert_eq!(u2.as_str(), "http://localhost:11434/api/chat");

        let u3 =
            resolve_provider_endpoint(AiProvider::Ollama, Some("http://localhost:11434/api/chat"))
                .unwrap();
        assert_eq!(u3.as_str(), "http://localhost:11434/api/chat");

        // Trailing slashes
        let u4 = resolve_provider_endpoint(AiProvider::Ollama, Some("http://127.0.0.1:11434/"))
            .unwrap();
        assert_eq!(u4.as_str(), "http://127.0.0.1:11434/api/chat");

        // Strip /v1 and /v1/chat/completions (malformed input prevention)
        let u5 = resolve_provider_endpoint(
            AiProvider::Ollama,
            Some("http://localhost:11434/v1/chat/completions"),
        )
        .unwrap();
        assert_eq!(u5.as_str(), "http://localhost:11434/api/chat");

        let u6 = resolve_provider_endpoint(
            AiProvider::Ollama,
            Some("http://localhost:11434/v1/chat/completions/"),
        )
        .unwrap();
        assert_eq!(u6.as_str(), "http://localhost:11434/api/chat");

        let u7 = resolve_provider_endpoint(AiProvider::Ollama, Some("http://localhost:11434/v1"))
            .unwrap();
        assert_eq!(u7.as_str(), "http://localhost:11434/api/chat");

        let u8 = resolve_provider_endpoint(
            AiProvider::Ollama,
            Some("http://localhost:11434/chat/completions"),
        )
        .unwrap();
        assert_eq!(u8.as_str(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn test_normalize_ollama_server_root() {
        assert_eq!(
            normalize_ollama_server_root(None).unwrap().as_str(),
            "http://127.0.0.1:11434/"
        );
        assert_eq!(
            normalize_ollama_server_root(Some("http://127.0.0.1:11434"))
                .unwrap()
                .as_str(),
            "http://127.0.0.1:11434/"
        );
        assert_eq!(
            normalize_ollama_server_root(Some("http://127.0.0.1:11434/"))
                .unwrap()
                .as_str(),
            "http://127.0.0.1:11434/"
        );
        assert_eq!(
            normalize_ollama_server_root(Some("http://127.0.0.1:11434/api/chat"))
                .unwrap()
                .as_str(),
            "http://127.0.0.1:11434/"
        );
        assert_eq!(
            normalize_ollama_server_root(Some("http://127.0.0.1:11434/v1/chat/completions"))
                .unwrap()
                .as_str(),
            "http://127.0.0.1:11434/"
        );
    }

    #[test]
    fn test_resolve_ollama_tags_endpoint() {
        assert_eq!(
            resolve_ollama_tags_endpoint(None).unwrap().as_str(),
            "http://127.0.0.1:11434/api/tags"
        );
        assert_eq!(
            resolve_ollama_tags_endpoint(Some("http://localhost:11434/"))
                .unwrap()
                .as_str(),
            "http://localhost:11434/api/tags"
        );
        assert_eq!(
            resolve_ollama_tags_endpoint(Some("http://localhost:11434/v1/chat/completions"))
                .unwrap()
                .as_str(),
            "http://localhost:11434/api/tags"
        );
    }
}
