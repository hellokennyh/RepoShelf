pub mod budget;
pub mod engine;
pub mod prompt;
pub mod schema;
pub mod task;
pub mod validation;

pub use task::AnalysisTaskManager;

pub use budget::{
    compute_context_budget, estimate_tokens, RepositoryAnalysisContextBudget,
    CONSERVATIVE_FALLBACK_CONTEXT, MAX_MODEL_INPUT_TOKENS, MIN_SAFETY_MARGIN_TOKENS,
};
pub use engine::{
    compute_config_revision, execute_repository_analysis, verify_ai_analysis_gate,
    AnalysisExecutionOptions, DEFAULT_OVERALL_TASK_TIMEOUT, DEFAULT_SINGLE_CALL_TIMEOUT,
};
pub use prompt::{
    format_repair_prompt, pack_evidence_for_model, PackedModelInput, PROMPT_VERSION,
    REPOSITORY_ANALYSIS_SYSTEM_INSTRUCTION,
};
pub use schema::{ollama_analysis_json_schema, AnalysisModelOutput};
pub use validation::{validate_model_output, ValidationResult};
