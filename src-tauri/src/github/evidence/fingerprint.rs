use sha2::{Digest, Sha256};

use crate::db::analysis::EvidenceRecord;

/// Computes a deterministic SHA-256 fingerprint for an Evidence Pack.
///
/// Ensures:
/// - Same normalized evidence and repository state -> exact same fingerprint
/// - Any change in evidence content, commit SHA, or selected texts -> different fingerprint
/// - Tokens, API keys, local filesystem paths, and user notes are NEVER included
pub fn compute_evidence_fingerprint(
    canonical_url: &str,
    github_repo_id: i64,
    default_branch: Option<&str>,
    commit_sha: Option<&str>,
    selector_version: &str,
    evidence_records: &[EvidenceRecord],
    selected_texts: &[crate::github::evidence::service::SelectedTextEvidence],
) -> String {
    let mut hasher = Sha256::new();

    // Identity and baseline
    hasher.update(b"IDENTITY:");
    hasher.update(canonical_url.as_bytes());
    hasher.update(b"\nREPO_ID:");
    hasher.update(github_repo_id.to_string().as_bytes());
    hasher.update(b"\nBRANCH:");
    hasher.update(default_branch.unwrap_or("").as_bytes());
    hasher.update(b"\nCOMMIT:");
    hasher.update(commit_sha.unwrap_or("").as_bytes());
    hasher.update(b"\nSELECTOR:");
    hasher.update(selector_version.as_bytes());
    hasher.update(b"\n");

    // Evidence records in stable ID order
    let mut sorted_records = evidence_records.to_vec();
    sorted_records.sort_by(|a, b| a.id.cmp(&b.id));

    for rec in &sorted_records {
        hasher.update(b"REC:");
        hasher.update(rec.id.as_bytes());
        hasher.update(b"|");
        hasher.update(rec.kind.as_bytes());
        hasher.update(b"|");
        hasher.update(rec.path.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(rec.commit_sha.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(rec.included_content_hash.as_bytes());
        hasher.update(b"|");
        hasher.update(if rec.truncated { b"T" } else { b"F" });
        for range in &rec.included_ranges {
            hasher.update(b":");
            hasher.update(range.start_line.to_string().as_bytes());
            hasher.update(b"-");
            hasher.update(range.end_line.to_string().as_bytes());
        }
        hasher.update(b"\n");
    }

    // Selected texts in stable evidence ID order
    let mut sorted_texts = selected_texts.to_vec();
    sorted_texts.sort_by(|a, b| a.evidence_id.cmp(&b.evidence_id));

    for text_item in &sorted_texts {
        hasher.update(b"TXT:");
        hasher.update(text_item.evidence_id.as_bytes());
        hasher.update(b"|");
        hasher.update(text_item.path.as_bytes());
        hasher.update(b"|");
        hasher.update(text_item.content_hash.as_bytes());
        hasher.update(b"|");
        hasher.update(text_item.text_content.as_bytes());
        hasher.update(b"\n");
    }

    format!("{:x}", hasher.finalize())
}
