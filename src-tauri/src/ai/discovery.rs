use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::ai::models::{OllamaEndpointDiscovery, OllamaModelInfo};
use crate::ai::provider::{map_http_error, map_reqwest_error};
use crate::ai::url::{
    normalize_ollama_server_root, resolve_ollama_tags_endpoint, validate_url_security,
    DEFAULT_OLLAMA_BASE_URL,
};
use crate::error::AppError;

pub const OLLAMA_PROBE_TIMEOUT_SECS: u64 = 2;
pub const OLLAMA_LIST_MODELS_TIMEOUT_SECS: u64 = 5;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Option<Vec<OllamaTagModelItem>>,
}

#[derive(Deserialize)]
struct OllamaTagModelItem {
    name: Option<String>,
    model: Option<String>,
    size: Option<u64>,
    modified_at: Option<String>,
}

fn build_probe_client(timeout_secs: u64) -> Result<Client, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("RepoShelf/{}", env!("CARGO_PKG_VERSION")))
            .map_err(|e| AppError::InvalidInput(e.to_string()))?,
    );

    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::AIConnection(format!("Failed to build HTTP client: {}", e)))
}

/// Normalizes an optional OLLAMA_HOST environment variable value into a safe local URL.
/// Handles cases such as:
/// - `:11434` -> `http://127.0.0.1:11434`
/// - `0.0.0.0:11434` -> `http://127.0.0.1:11434`
/// - `127.0.0.1:11434` -> `http://127.0.0.1:11434`
/// - `http://localhost:11434` -> `http://localhost:11434`
pub fn parse_ollama_host_env(env_val: &str) -> Option<String> {
    let trimmed = env_val.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if let Some(stripped) = trimmed.strip_prefix(':') {
        format!("http://127.0.0.1:{}", stripped)
    } else if let Some(stripped) = trimmed.strip_prefix("0.0.0.0:") {
        format!("http://127.0.0.1:{}", stripped)
    } else if trimmed == "0.0.0.0" {
        "http://127.0.0.1:11434".to_string()
    } else if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        format!("http://{}", trimmed)
    } else {
        trimmed.to_string()
    };

    if let Ok(parsed) = Url::parse(&candidate) {
        if validate_url_security(&parsed).is_ok() {
            if let Ok(normalized) = normalize_ollama_server_root(Some(&candidate)) {
                return Some(normalized.as_str().trim_end_matches('/').to_string());
            }
        }
    }

    None
}

/// Gathers trusted local Ollama candidate base URLs.
/// Strictly bounded to local loopback addresses and configured URL.
/// No broad LAN scanning or port sweeps.
pub fn get_candidate_endpoints(configured_base_url: Option<&str>) -> Vec<String> {
    let mut candidates = Vec::new();

    // 1. Configured base URL if present
    if let Some(custom) = configured_base_url {
        if let Ok(root) = normalize_ollama_server_root(Some(custom)) {
            candidates.push(root.as_str().trim_end_matches('/').to_string());
        }
    }

    // 2. Default 127.0.0.1:11434
    candidates.push(DEFAULT_OLLAMA_BASE_URL.to_string());

    // 3. Default localhost:11434
    candidates.push("http://localhost:11434".to_string());

    // 4. OLLAMA_HOST environment variable if present
    if let Ok(host_env) = std::env::var("OLLAMA_HOST") {
        if let Some(parsed) = parse_ollama_host_env(&host_env) {
            candidates.push(parsed);
        }
    }

    // Deduplicate candidates preserving order
    let mut seen = std::collections::HashSet::new();
    let mut deduplicated = Vec::new();
    for candidate in candidates {
        if seen.insert(candidate.clone()) {
            deduplicated.push(candidate);
        }
    }

    deduplicated
}

/// Probes a single candidate endpoint using GET {base_url}/api/tags.
/// Returns models list if candidate is a verified Ollama instance.
pub async fn probe_ollama_endpoint(
    client: &Client,
    base_url: &str,
) -> Option<Vec<OllamaModelInfo>> {
    let tags_url = resolve_ollama_tags_endpoint(Some(base_url)).ok()?;

    let response = client.get(tags_url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }

    let text = response.text().await.ok()?;
    let parsed: OllamaTagsResponse = serde_json::from_str(&text).ok()?;
    let items = parsed.models?;

    let models = items
        .into_iter()
        .filter_map(|m| {
            let name = m.name.clone().or_else(|| m.model.clone())?;
            let model = m.model.or_else(|| m.name.clone())?;
            if name.trim().is_empty() {
                None
            } else {
                Some(OllamaModelInfo {
                    name,
                    model,
                    size: m.size,
                    modified_at: m.modified_at,
                })
            }
        })
        .collect();

    Some(models)
}

/// Safely discovers active local Ollama endpoints from trusted local candidates.
pub async fn discover_ollama_endpoints(
    configured_base_url: Option<&str>,
) -> Result<Vec<OllamaEndpointDiscovery>, AppError> {
    let client = build_probe_client(OLLAMA_PROBE_TIMEOUT_SECS)?;
    let candidates = get_candidate_endpoints(configured_base_url);

    let mut discoveries = Vec::new();

    for candidate in candidates {
        if let Some(models) = probe_ollama_endpoint(&client, &candidate).await {
            discoveries.push(OllamaEndpointDiscovery {
                base_url: candidate,
                models,
            });
        }
    }

    Ok(discoveries)
}

/// Fetches installed Ollama models from the specified base URL.
pub async fn list_ollama_models(
    custom_base: Option<&str>,
) -> Result<Vec<OllamaModelInfo>, AppError> {
    let client = build_probe_client(OLLAMA_LIST_MODELS_TIMEOUT_SECS)?;
    let tags_url = resolve_ollama_tags_endpoint(custom_base)?;

    let response = client
        .get(tags_url)
        .send()
        .await
        .map_err(|e| map_reqwest_error("Ollama", e))?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(map_http_error("Ollama", status, Some(&body_text)));
    }

    let text = response
        .text()
        .await
        .map_err(|e| map_reqwest_error("Ollama", e))?;

    let parsed: OllamaTagsResponse = serde_json::from_str(&text).map_err(|_| {
        AppError::AIInvalidResponse("Ollama: Malformed JSON response from /api/tags".to_string())
    })?;

    let items = parsed.models.unwrap_or_default();

    let models: Vec<OllamaModelInfo> = items
        .into_iter()
        .filter_map(|m| {
            let name = m.name.clone().or_else(|| m.model.clone())?;
            let model = m.model.or_else(|| m.name.clone())?;
            if name.trim().is_empty() {
                None
            } else {
                Some(OllamaModelInfo {
                    name,
                    model,
                    size: m.size,
                    modified_at: m.modified_at,
                })
            }
        })
        .collect();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ollama_host_env() {
        assert_eq!(
            parse_ollama_host_env(":11434"),
            Some("http://127.0.0.1:11434".to_string())
        );
        assert_eq!(
            parse_ollama_host_env("0.0.0.0:11434"),
            Some("http://127.0.0.1:11434".to_string())
        );
        assert_eq!(
            parse_ollama_host_env("0.0.0.0"),
            Some("http://127.0.0.1:11434".to_string())
        );
        assert_eq!(
            parse_ollama_host_env("127.0.0.1:11435"),
            Some("http://127.0.0.1:11435".to_string())
        );
        assert_eq!(
            parse_ollama_host_env("http://localhost:11434"),
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(
            parse_ollama_host_env("http://127.0.0.1:11434/v1/chat/completions"),
            Some("http://127.0.0.1:11434".to_string())
        );
        // Remote HTTP is rejected by security policy
        assert_eq!(parse_ollama_host_env("http://192.168.1.50:11434"), None);
        // Empty / whitespace
        assert_eq!(parse_ollama_host_env("   "), None);
    }

    #[test]
    fn test_get_candidate_endpoints_deduplication() {
        let candidates = get_candidate_endpoints(Some("http://127.0.0.1:11434/"));
        // 127.0.0.1:11434 appears once, localhost:11434 appears once
        assert_eq!(candidates[0], "http://127.0.0.1:11434");
        assert_eq!(candidates[1], "http://localhost:11434");
        assert_eq!(candidates.len(), 2);
    }
}
