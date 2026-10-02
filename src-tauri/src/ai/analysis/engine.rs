use std::time::{Duration, Instant};
use chrono::Utc;
use sha2::{Digest, Sha256};


use crate::ai::provider::{create_provider_client, GenerateTextOptions};
use crate::ai::translation::TranslationLimiter;
use crate::db::analysis::{
    derive_search_text, validate_and_serialize_envelope, AnalysisCoverage, AnalysisEnvelopeV1,
    AnalysisGenerator, AnalysisLocale, AnalysisSource,
};
use crate::error::AppError;
use crate::github::evidence::service::EvidencePack;
use crate::github::evidence::AcquisitionCancelToken;
use crate::secrets::{SecretStore, SECRET_KEY_AI_API_KEY};
use crate::settings::{AiProvider, Settings, SettingsStore};

use super::budget::compute_context_budget;
use super::prompt::{
    format_repair_prompt, pack_evidence_for_model, PROMPT_VERSION,
    REPOSITORY_ANALYSIS_SYSTEM_INSTRUCTION,
};
use super::schema::ollama_analysis_json_schema;
use super::validation::{validate_model_output, ValidationResult};

pub const DEFAULT_SINGLE_CALL_TIMEOUT: Duration = Duration::from_secs(600);
pub const DEFAULT_OVERALL_TASK_TIMEOUT: Duration = Duration::from_secs(900);

/// Execution tuning options for repository analysis (allows injecting tiny durations for automated tests).
#[derive(Debug, Clone)]
pub struct AnalysisExecutionOptions {
    pub single_call_timeout: Duration,
    pub overall_task_timeout: Duration,
}

impl Default for AnalysisExecutionOptions {
    fn default() -> Self {
        Self {
            single_call_timeout: DEFAULT_SINGLE_CALL_TIMEOUT,
            overall_task_timeout: DEFAULT_OVERALL_TASK_TIMEOUT,
        }
    }
}

/// Computes a deterministic configuration fingerprint for gate verification and stale-task detection.
pub fn compute_config_revision(settings: &Settings, secret_store: &dyn SecretStore) -> String {
    let mut hasher = Sha256::new();
    hasher.update(if settings.ai.enabled { b"1" } else { b"0" });
    if let Some(p) = settings.ai.provider {
        hasher.update(format!("{:?}", p).as_bytes());
    }
    if let Some(ref base) = settings.ai.base_url {
        hasher.update(base.as_bytes());
    }
    if let Some(ref model) = settings.ai.model {
        hasher.update(model.as_bytes());
    }
    if let Some(tokens) = settings.ai.repository_analysis_context_tokens {
        hasher.update(tokens.to_string().as_bytes());
    }
    // Check if cloud key exists (without hashing the secret itself)
    let has_key = match secret_store.get_secret(SECRET_KEY_AI_API_KEY) {
        Ok(Some(k)) => !k.trim().is_empty(),
        _ => false,
    };
    hasher.update(if has_key { b"key_present" as &[u8] } else { b"key_absent" as &[u8] });

    format!("{:x}", hasher.finalize())

}

/// Evaluates backend AI gate requirements before dispatching repository analysis.
pub fn verify_ai_analysis_gate(
    settings: &Settings,
    secret_store: &dyn SecretStore,
) -> Result<(AiProvider, String, Option<String>, Option<String>), AppError> {
    if !settings.ai.enabled {
        return Err(AppError::SettingsValidation(
            "AI 功能未启用，请在设置中开启 AI (AI is disabled in Settings)".to_string(),
        ));
    }

    let provider = match settings.ai.provider {
        Some(p) => p,
        None => {
            return Err(AppError::SettingsValidation(
                "未选择 AI Provider (No AI provider configured)".to_string(),
            ))
        }
    };

    let model = match settings
        .ai
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(m) => m.to_string(),
        None => {
            return Err(AppError::AIModel(
                "未配置 AI Model 名称 (No AI model configured)".to_string(),
            ))
        }
    };

    let base_url = settings.ai.base_url.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(str::to_string);

    let api_key = if provider == AiProvider::Ollama {
        None
    } else {
        match secret_store.get_secret(SECRET_KEY_AI_API_KEY)? {
            Some(k) if !k.trim().is_empty() => Some(k.trim().to_string()),
            _ => {
                return Err(AppError::AIAuth(
                    "未配置 AI API Key (AI API Key not configured)".to_string(),
                ))
            }
        }
    };

    Ok((provider, model, base_url, api_key))
}

/// Executes the full, bounded repository analysis pipeline.
///
/// Follows RepoShelf Technical Spec §13–§37:
/// 1. Pre-gate check (AI enabled, provider, model, API key)
/// 2. Config revision snapshot
/// 3. Shared concurrency permit acquisition
/// 4. Post-permit re-check & cancellation check
/// 5. Deterministic bounded packing according to context budget
/// 6. First model generation with single call deadline (<=600s) and overall deadline (<=900s)
/// 7. Output validation pipeline
/// 8. At most 1 bounded structural repair call if repairable error occurred
/// 9. Trusted metadata and canonical search_text assembly
/// 10. Final envelope validation (returns valid candidate, NOT persisted to DB).
pub async fn execute_repository_analysis(
    evidence_pack: &EvidencePack,
    target_locale: AnalysisLocale,
    settings_store: &SettingsStore,
    secret_store: &dyn SecretStore,
    ai_limiter: &TranslationLimiter,
    cancel_token: Option<AcquisitionCancelToken>,
    options: AnalysisExecutionOptions,
) -> Result<AnalysisEnvelopeV1, AppError> {
    let start_time = Instant::now();
    let overall_deadline = start_time + options.overall_task_timeout;

    // Check cancellation
    if let Some(ref ct) = cancel_token {
        if ct.is_cancelled() {
            return Err(AppError::AnalysisCancelled("Analysis task was cancelled before start".to_string()));
        }
    }

    // Step 1: Pre-gate check
    let settings_initial = settings_store.get()?;
    let (provider, _model, _base_url, _api_key) =
        verify_ai_analysis_gate(&settings_initial, secret_store)?;
    let initial_config_revision = compute_config_revision(&settings_initial, secret_store);


    // Step 2: Acquire permit from shared limiter (local <= 1, cloud <= 2)
    let _permit = ai_limiter.acquire(&provider).await?;

    // Step 3: Post-permit gate recheck (ensure AI was not disabled and config did not change while queued)
    if let Some(ref ct) = cancel_token {
        if ct.is_cancelled() {
            return Err(AppError::AnalysisCancelled("Analysis task was cancelled after acquiring permit".to_string()));
        }
    }

    let settings_post_permit = settings_store.get()?;
    let current_config_revision = compute_config_revision(&settings_post_permit, secret_store);
    if current_config_revision != initial_config_revision {
        return Err(AppError::AnalysisStaleConfig(
            "AI configuration changed or was disabled while waiting in queue".to_string(),
        ));
    }

    // Verify gate again
    let (active_provider, active_model, active_base_url, active_key) =
        verify_ai_analysis_gate(&settings_post_permit, secret_store)?;

    // Step 4: Context budgeting and bounded packing
    let budget = compute_context_budget(settings_post_permit.ai.repository_analysis_context_tokens);
    let packed = pack_evidence_for_model(evidence_pack, target_locale, budget.model_input_target);

    // Step 5: Configure provider request options
    let mut gen_options = GenerateTextOptions {
        max_tokens: Some(budget.output_reserve_tokens),
        temperature: Some(0.2),
        think: Some(false),
        num_ctx: None,
        json_schema: None,
    };

    if active_provider == AiProvider::Ollama {
        if settings_post_permit.ai.repository_analysis_context_tokens.is_some() {
            gen_options.num_ctx = settings_post_permit.ai.repository_analysis_context_tokens;
        }
        gen_options.json_schema = Some(ollama_analysis_json_schema());
    }

    // Step 6: Dispatch first model call
    let remaining_before_first = overall_deadline.saturating_duration_since(Instant::now());
    if remaining_before_first.is_zero() {
        return Err(AppError::AnalysisTimeout("Task generation overall deadline exceeded before dispatch".to_string()));
    }
    let call_timeout = std::cmp::min(options.single_call_timeout, remaining_before_first);

    let client = create_provider_client(
        active_provider,
        active_base_url.as_deref(),
        &active_model,
        active_key.as_deref(),
        Some(call_timeout),
    )?;

    if let Some(ref ct) = cancel_token {
        if ct.is_cancelled() {
            return Err(AppError::AnalysisCancelled("Analysis task was cancelled right before dispatch".to_string()));
        }
    }

    let first_response_res = client
        .generate_text_with_options(
            REPOSITORY_ANALYSIS_SYSTEM_INSTRUCTION,
            &packed.user_prompt,
            gen_options.clone(),
        )
        .await;

    let first_raw_text = match first_response_res {
        Ok(text) => text,
        Err(e) => {
            // Check if timeout or cancelled
            if let Some(ref ct) = cancel_token {
                if ct.is_cancelled() {
                    return Err(AppError::AnalysisCancelled("Analysis task was cancelled during generation".to_string()));
                }
            }
            if Instant::now() >= overall_deadline {
                return Err(AppError::AnalysisTimeout("Task generation deadline exceeded".to_string()));
            }
            return Err(e);
        }
    };

    // Step 7: Validate first response
    let validation_res = validate_model_output(&first_raw_text, &packed.sent_evidence_records);

    let final_content = match validation_res {
        ValidationResult::Success(c) => c,
        ValidationResult::NonRepairable(err) => {
            return Err(err);
        }
        ValidationResult::Repairable(repairable_reason) => {
            // Step 8: Optional max-1 structural repair
            if let Some(ref ct) = cancel_token {
                if ct.is_cancelled() {
                    return Err(AppError::AnalysisCancelled("Analysis task was cancelled before repair".to_string()));
                }
            }

            let remaining_for_repair = overall_deadline.saturating_duration_since(Instant::now());
            if remaining_for_repair.is_zero() {
                return Err(AppError::AnalysisTimeout("Task generation overall deadline exceeded before repair".to_string()));
            }

            let repair_timeout = std::cmp::min(options.single_call_timeout, remaining_for_repair);
            let repair_client = create_provider_client(
                active_provider,
                active_base_url.as_deref(),
                &active_model,
                active_key.as_deref(),
                Some(repair_timeout),
            )?;

            let allowed_ids: Vec<String> = packed
                .sent_evidence_records
                .iter()
                .map(|r| r.id.clone())
                .collect();

            let repair_prompt = format_repair_prompt(
                &repairable_reason,
                &first_raw_text,
                &allowed_ids,
                target_locale,
            );

            let repair_response_res = repair_client
                .generate_text_with_options(
                    REPOSITORY_ANALYSIS_SYSTEM_INSTRUCTION,
                    &repair_prompt,
                    gen_options,
                )
                .await;

            let repair_raw_text = match repair_response_res {
                Ok(t) => t,
                Err(e) => {
                    if let Some(ref ct) = cancel_token {
                        if ct.is_cancelled() {
                            return Err(AppError::AnalysisCancelled("Analysis task was cancelled during repair".to_string()));
                        }
                    }
                    if Instant::now() >= overall_deadline {
                        return Err(AppError::AnalysisTimeout("Task generation deadline exceeded during repair".to_string()));
                    }
                    return Err(e);
                }
            };

            // Second validation on repaired output
            match validate_model_output(&repair_raw_text, &packed.sent_evidence_records) {
                ValidationResult::Success(c) => c,
                ValidationResult::Repairable(err) => {
                    return Err(AppError::AIInvalidResponse(format!(
                        "Model output invalid after maximum of 1 repair attempt: {}",
                        err
                    )));
                }
                ValidationResult::NonRepairable(err) => {
                    return Err(err);
                }
            }
        }
    };

    // Step 9: Re-verify cancel and config before assembling envelope
    if let Some(ref ct) = cancel_token {
        if ct.is_cancelled() {
            return Err(AppError::AnalysisCancelled("Analysis task was cancelled before returning candidate".to_string()));
        }
    }

    let settings_final = settings_store.get()?;
    let final_config_revision = compute_config_revision(&settings_final, secret_store);
    if final_config_revision != initial_config_revision {
        return Err(AppError::AnalysisStaleConfig(
            "AI configuration changed while task was running".to_string(),
        ));
    }

    // Step 10: Assemble trusted AnalysisEnvelopeV1
    let provider_name = match active_provider {
        AiProvider::Ollama => "ollama",
        AiProvider::OpenaiCompatible => "openai_compatible",
        AiProvider::Anthropic => "anthropic",
        AiProvider::Gemini => "gemini",
    };

    let generator = AnalysisGenerator {
        provider: provider_name.to_string(),
        model_requested: active_model.clone(),
        model_reported: None,
        prompt_version: PROMPT_VERSION.to_string(),
        selector_version: evidence_pack.selector_version.clone(),
        context_budget_tokens: budget.effective_context_tokens,
        input_tokens: packed.estimated_input_tokens,
        input_tokens_is_estimate: true,
        output_limit_tokens: budget.output_reserve_tokens,
    };

    let source = AnalysisSource {
        github_repo_id: evidence_pack.metadata_summary.github_repo_id.to_string(),
        canonical_url: evidence_pack.metadata_summary.canonical_url.clone(),
        default_branch: evidence_pack.metadata_summary.default_branch.clone(),
        commit_sha: evidence_pack.source.commit_sha.clone(),
        metadata_observed_at: evidence_pack.source.metadata_observed_at.clone(),
        evidence_fingerprint: evidence_pack.evidence_fingerprint.clone(),
    };

    let coverage = AnalysisCoverage {
        status: evidence_pack.coverage.status.clone(),
        selected_file_count: evidence_pack.coverage.selected_file_count,
        sent_file_count: packed.sent_file_count,
        evidence_count: packed.sent_evidence_records.len() as u32,
        gaps: evidence_pack.coverage.gaps.clone(),
        executed_code: false,
        verified_deployment: false,
    };

    let derived_search_text = derive_search_text(&final_content);

    let envelope = AnalysisEnvelopeV1 {
        schema_version: 1,
        content: final_content,
        target_locale,
        generated_at: Utc::now().to_rfc3339(),
        generator,
        source,
        evidence: packed.sent_evidence_records,
        coverage,
        search_text: derived_search_text,
    };

    // Step 11: Final canonical validation using S1 validator
    validate_and_serialize_envelope(&envelope)?;

    // Return unpersisted candidate
    Ok(envelope)
}
