pub mod anthropic;
pub mod gemini;
pub mod models;
pub mod ollama;
pub mod openai;
pub mod provider;
pub mod search;
pub mod service;
pub mod suggestion;
pub mod url;

pub use models::*;
pub use provider::{create_provider_client, AiProviderClient};
pub use search::{
    check_ai_search_readiness, generate_search_queries, parse_and_validate_ai_queries,
    strip_outer_code_fence, validate_ai_search_input, AiRepositorySearchRequest,
    AiRepositorySearchResponse, AiSearchReadiness, AI_SEARCH_SYSTEM_INSTRUCTION,
    FORBIDDEN_QUALIFIERS,
};
pub use service::execute_test_ai_connection;
pub use suggestion::{
    format_suggestion_user_prompt, generate_favorite_suggestion, parse_and_validate_suggestion,
    AiFavoriteMetadataSuggestion, AI_METADATA_SYSTEM_INSTRUCTION,
};
pub use url::{
    default_base_url_for_provider, resolve_provider_endpoint, validate_url_security,
    DEFAULT_ANTHROPIC_BASE_URL, DEFAULT_GEMINI_BASE_URL, DEFAULT_OLLAMA_BASE_URL,
    DEFAULT_OPENAI_BASE_URL,
};
