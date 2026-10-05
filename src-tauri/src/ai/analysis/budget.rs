use serde::{Deserialize, Serialize};

/// Context budget model for Repository Analysis.
///
/// Follows RepoShelf Technical Spec §10:
/// - Prompts + schema instructions + EvidencePack content + wrapper + output reserve + safety margin <= effective context C.
/// - Model input target is strictly capped at <= 30,000 tokens.
/// - When `repository_analysis_context_tokens` is None, falls back to a conservative, unverified baseline (14,336 tokens).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryAnalysisContextBudget {
    /// The effective context window C in tokens.
    pub effective_context_tokens: u32,
    /// Whether this budget is an unverified conservative fallback (when no explicit setting is provided).
    pub is_conservative_fallback: bool,
    /// Maximum allowed input tokens (system + evidence + wrapper), strictly <= 30,000.
    pub model_input_target: u32,
    /// Reserved output tokens for model generation and reasoning.
    pub output_reserve_tokens: u32,
    /// Safety margin reserved to avoid context boundary truncation (at least 2,048 tokens).
    pub safety_margin_tokens: u32,
}

pub const CONSERVATIVE_FALLBACK_CONTEXT: u32 = 14_336;
pub const MAX_MODEL_INPUT_TOKENS: u32 = 30_000;
pub const MIN_SAFETY_MARGIN_TOKENS: u32 = 2_048;
pub const REQUEST_WRAPPER_OVERHEAD_TOKENS: u32 = 128;

/// Computes the context budget from the optional user setting.
///
/// If `setting` is `None`:
/// - Uses conservative 14,336 token baseline
/// - Marks `is_conservative_fallback = true`
/// - input_target = 8,192, reserve = 4,096, safety = 2,048
///
/// If `setting` is `Some(C)`:
/// - Marks `is_conservative_fallback = false`
/// - Dynamically allocates input target, output reserve, and safety margin (>= 2,048).
pub fn compute_context_budget(setting: Option<u32>) -> RepositoryAnalysisContextBudget {
    let (c, is_fallback) = match setting {
        Some(val) if val >= 4_096 => (val, false),
        _ => (CONSERVATIVE_FALLBACK_CONTEXT, true),
    };

    let output_reserve = if c <= 16_384 {
        4_096
    } else if c <= 32_768 {
        6_144
    } else {
        8_192
    };

    let input_target = if c <= 16_384 {
        std::cmp::min(8_192, c.saturating_sub(output_reserve + MIN_SAFETY_MARGIN_TOKENS))
    } else if c <= 32_768 {
        std::cmp::min(20_000, c.saturating_sub(output_reserve + MIN_SAFETY_MARGIN_TOKENS))
    } else {
        std::cmp::min(MAX_MODEL_INPUT_TOKENS, c.saturating_sub(output_reserve + MIN_SAFETY_MARGIN_TOKENS))
    };

    let safety_margin = c.saturating_sub(input_target + output_reserve);

    RepositoryAnalysisContextBudget {
        effective_context_tokens: c,
        is_conservative_fallback: is_fallback,
        model_input_target: input_target,
        output_reserve_tokens: output_reserve,
        safety_margin_tokens: std::cmp::max(MIN_SAFETY_MARGIN_TOKENS, safety_margin),
    }
}

/// Conservative, deterministic token estimator for repository analysis text.
///
/// Does NOT assume "1 character = 1 token".
/// Deterministically distinguishes:
/// - CJK characters (Chinese, Japanese, Korean): ~1.5 tokens per character (conservative ceil).
/// - ASCII words / whitespace / punctuation / code symbols: ~1 token per 3.5 characters.
/// - Base request wrapper overhead: 128 tokens.
///
/// Always returns estimated token count.
pub fn estimate_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }

    let mut cjk_count: u32 = 0;
    let mut non_cjk_count: u32 = 0;

    for ch in text.chars() {
        if is_cjk(ch) {
            cjk_count += 1;
        } else {
            non_cjk_count += 1;
        }
    }

    // CJK: 1.5 tokens each -> (count * 3 + 1) / 2
    let cjk_tokens = (cjk_count * 3 + 1) / 2;
    // Non-CJK: 1 token per ~3.5 chars -> (count * 2 + 6) / 7
    let non_cjk_tokens = (non_cjk_count * 2 + 6) / 7;

    cjk_tokens + non_cjk_tokens
}

fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Extension A
        '\u{20000}'..='\u{2A6DF}' | // CJK Extension B
        '\u{F900}'..='\u{FAFF}' |   // CJK Compatibility Ideographs
        '\u{3040}'..='\u{309F}' |   // Hiragana
        '\u{30A0}'..='\u{30FF}' |   // Katakana
        '\u{AC00}'..='\u{D7AF}'     // Hangul Syllables
    )
}

/// Estimates the fixed base request overhead:
/// - Authoritative system instruction
/// - Formal JSON Schema / format contract
/// - Conservative request wrapper overhead (128 tokens)
pub fn base_request_overhead_tokens() -> u32 {
    let sys_tokens = estimate_tokens(super::prompt::REPOSITORY_ANALYSIS_SYSTEM_INSTRUCTION);
    let schema_json = serde_json::to_string(&super::schema::ollama_analysis_json_schema()).unwrap_or_default();
    let schema_tokens = estimate_tokens(&schema_json);
    sys_tokens + schema_tokens + REQUEST_WRAPPER_OVERHEAD_TOKENS
}

/// Estimates the total input tokens for a complete analysis request:
/// `base_request_overhead_tokens() + estimate_tokens(user_prompt)`.
pub fn estimate_total_analysis_request_tokens(user_prompt: &str) -> u32 {
    base_request_overhead_tokens() + estimate_tokens(user_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_budget_fallback() {
        let budget = compute_context_budget(None);
        assert_eq!(budget.effective_context_tokens, 14_336);
        assert!(budget.is_conservative_fallback);
        assert_eq!(budget.model_input_target, 8_192);
        assert_eq!(budget.output_reserve_tokens, 4_096);
        assert_eq!(budget.safety_margin_tokens, 2_048);
        assert!(budget.model_input_target + budget.output_reserve_tokens + budget.safety_margin_tokens <= 14_336);
    }

    #[test]
    fn test_context_budget_32k() {
        let budget = compute_context_budget(Some(32_768));
        assert_eq!(budget.effective_context_tokens, 32_768);
        assert!(!budget.is_conservative_fallback);
        assert_eq!(budget.model_input_target, 20_000);
        assert_eq!(budget.output_reserve_tokens, 6_144);
        assert!(budget.safety_margin_tokens >= 2_048);
        assert!(budget.model_input_target + budget.output_reserve_tokens + budget.safety_margin_tokens <= 32_768);
    }

    #[test]
    fn test_context_budget_51k() {
        let budget = compute_context_budget(Some(51_200));
        assert_eq!(budget.effective_context_tokens, 51_200);
        assert!(!budget.is_conservative_fallback);
        assert_eq!(budget.model_input_target, 30_000);
        assert_eq!(budget.output_reserve_tokens, 8_192);
        assert!(budget.safety_margin_tokens >= 2_048);
        assert!(budget.model_input_target + budget.output_reserve_tokens + budget.safety_margin_tokens <= 51_200);
    }

    #[test]
    fn test_token_estimator() {
        assert_eq!(estimate_tokens(""), 0);
        // ASCII word
        let t1 = estimate_tokens("hello world");
        assert!(t1 >= 3 && t1 <= 5);
        // Chinese
        let t2 = estimate_tokens("仓库分析");
        assert_eq!(t2, 6); // 4 chars * 1.5 = 6
    }

    #[test]
    fn test_base_request_overhead_tokens() {
        let overhead = base_request_overhead_tokens();
        assert!(overhead > 2000 && overhead < 3500, "Base overhead should be ~2500-3000 tokens, got {}", overhead);
    }
}
