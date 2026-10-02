pub mod budget;
pub mod fingerprint;
pub mod identity;
pub mod release;
pub mod selector;
pub mod service;
pub mod text;
pub mod transport;
pub mod tree;

pub use budget::{
    AcquisitionBudgetTracker, AcquisitionCancelToken, ResourceAccounting,
    BUDGET_MAX_CONCURRENCY, BUDGET_MAX_DIRECTORY_REQUESTS, BUDGET_MAX_GITHUB_REQUESTS,
    BUDGET_MAX_RECENT_RELEASES, BUDGET_MAX_REDIRECTS_PER_REQUEST,
    BUDGET_MAX_SELECTED_FILES, BUDGET_MAX_SINGLE_FILE_DECODED_BYTES,
    BUDGET_MAX_SINGLE_RESPONSE_BYTES, BUDGET_MAX_TOTAL_RESPONSES_BYTES,
    BUDGET_MODEL_DIRECTORY_MAX_PATHS, BUDGET_OVERALL_ACQUISITION_DEADLINE,
    BUDGET_PER_REQUEST_TIMEOUT,
};
pub use fingerprint::compute_evidence_fingerprint;
pub use identity::{validate_redirect_target, validate_repository_target, ValidatedRepoIdentity};
pub use release::ReleaseEvidenceItem;
pub use selector::{
    classify_file_role, is_excluded_path, is_sensitive_secret_path, select_evidence_files,
    FileRole, FileSelectionPlan, SelectedCandidate, SELECTOR_VERSION,
};
pub use service::{
    AcquisitionOptions, DirectorySummary, EvidencePack, RepositoryEvidenceService,
    RepositoryMetadataSummary, SelectedTextEvidence,
};
pub use text::{
    extract_heading_aware_bounded_markdown, is_binary_content, is_git_lfs_pointer,
    process_file_content, redact_secret_values, ProcessedTextResult,
};
pub use tree::{discover_bounded_tree, BoundedTreeResult, GitTreeEntry};
