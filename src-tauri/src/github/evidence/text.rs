use base64::Engine;
use sha2::{Digest, Sha256};

use crate::db::analysis::EvidenceRange;
use crate::error::AppError;
use crate::github::evidence::budget::BUDGET_MAX_SINGLE_FILE_DECODED_BYTES;

/// Result of safely decoding and bounding text file content.
#[derive(Debug, Clone)]
pub struct ProcessedTextResult {
    pub text_content: String,
    pub content_hash: String,
    pub truncated: bool,
    pub is_lfs_pointer: bool,
    pub included_ranges: Vec<EvidenceRange>,
    pub original_byte_size: usize,
}

/// Decodes base64 content or raw bytes into safe, bounded, redacted text.
pub fn process_file_content(
    raw_content: &str,
    encoding: Option<&str>,
    is_markdown: bool,
) -> Result<ProcessedTextResult, AppError> {
    // 1. Decode raw bytes
    let decoded_bytes = if encoding.map(str::trim).map(str::to_lowercase).as_deref() == Some("base64") {
        let cleaned: String = raw_content.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(&cleaned)
            .map_err(|e| {
                AppError::GitHubInvalidResponse(format!("Invalid base64 encoding in content: {}", e))
            })?
    } else {
        raw_content.as_bytes().to_vec()
    };

    let original_size = decoded_bytes.len();

    // 2. Reject obvious binary content (null bytes or excessive non-printable control chars)
    if is_binary_content(&decoded_bytes) {
        return Err(AppError::GitHubInvalidResponse(
            "Binary file content detected; non-text files cannot be processed as evidence"
                .to_string(),
        ));
    }

    // 3. UTF-8 decode
    let utf8_text = String::from_utf8_lossy(&decoded_bytes).to_string();

    // 4. Git LFS pointer detection
    if is_git_lfs_pointer(&utf8_text) {
        let hash = compute_sha256(&utf8_text);
        return Ok(ProcessedTextResult {
            text_content: utf8_text,
            content_hash: hash,
            truncated: false,
            is_lfs_pointer: true,
            included_ranges: vec![EvidenceRange {
                start_line: 1,
                end_line: 3,
            }],
            original_byte_size: original_size,
        });
    }

    // 5. Secret redaction on the decoded text before any further processing
    let redacted_text = redact_secret_values(&utf8_text);

    // 6. Bounded extraction
    // If the text is larger than BUDGET_MAX_SINGLE_FILE_DECODED_BYTES (256 KiB)
    // or if it's a long markdown document (> 64 KiB), perform heading-aware extraction
    let bounded = if is_markdown && redacted_text.len() > 64 * 1024 {
        extract_heading_aware_bounded_markdown(&redacted_text, BUDGET_MAX_SINGLE_FILE_DECODED_BYTES)
    } else if redacted_text.len() > BUDGET_MAX_SINGLE_FILE_DECODED_BYTES {
        truncate_plain_text(&redacted_text, BUDGET_MAX_SINGLE_FILE_DECODED_BYTES)
    } else {
        let total_lines = redacted_text.lines().count().max(1);
        (
            redacted_text,
            false,
            vec![EvidenceRange {
                start_line: 1,
                end_line: total_lines,
            }],
        )
    };

    let (final_text, truncated, ranges) = bounded;
    let content_hash = compute_sha256(&final_text);

    Ok(ProcessedTextResult {
        text_content: final_text,
        content_hash,
        truncated,
        is_lfs_pointer: false,
        included_ranges: ranges,
        original_byte_size: original_size,
    })
}

/// Checks if bytes contain null bytes or an unacceptable ratio of binary control characters.
pub fn is_binary_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    // Check first 8000 bytes sample
    let sample = if bytes.len() > 8000 {
        &bytes[..8000]
    } else {
        bytes
    };

    if sample.contains(&0) {
        return true;
    }

    let mut non_text_controls = 0;
    for &b in sample {
        // Control chars other than tab, newline, carriage return
        if b < 0x20 && b != 0x09 && b != 0x0a && b != 0x0d {
            non_text_controls += 1;
        }
    }

    // If more than 5% non-text control characters, consider binary
    (non_text_controls as f64) / (sample.len() as f64) > 0.05
}

/// Detects whether text is a Git LFS pointer file.
pub fn is_git_lfs_pointer(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("version https://git-lfs.github.com/spec/v1")
}

/// Redacts sensitive API keys, tokens, and private keys.
pub fn redact_secret_values(input: &str) -> String {
    let mut out = input.to_string();

    // Redact standard GitHub Personal Access Tokens (ghp_...)
    let ghp_pattern = "ghp_";
    while let Some(pos) = out.find(ghp_pattern) {
        let end = pos + 40; // ghp_ + 36 chars
        if end <= out.len() {
            out.replace_range(pos..end, "[REDACTED_SECRET]");
        } else {
            out.replace_range(pos.., "[REDACTED_SECRET]");
            break;
        }
    }

    // Redact fine-grained PAT (github_pat_...)
    let pat_pattern = "github_pat_";
    while let Some(pos) = out.find(pat_pattern) {
        let end = pos + 93;
        if end <= out.len() {
            out.replace_range(pos..end, "[REDACTED_SECRET]");
        } else {
            out.replace_range(pos.., "[REDACTED_SECRET]");
            break;
        }
    }

    // Redact OpenAI / Anthropic-like keys (sk-...)
    let sk_pattern = "sk-";
    while let Some(pos) = out.find(sk_pattern) {
        // Find end of token (whitespace, quote, or punctuation)
        let token_slice = &out[pos..];
        let token_len = token_slice
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
            .unwrap_or(token_slice.len());

        if token_len >= 20 {
            out.replace_range(pos..pos + token_len, "[REDACTED_SECRET]");
        } else {
            // Not a real key, advance past this instance
            break;
        }
    }

    // Redact Private Key blocks
    let pk_header = "-----BEGIN";
    let pk_end_marker = "-----END";
    while let Some(start_pos) = out.find(pk_header) {
        if let Some(rel_end) = out[start_pos..].find(pk_end_marker) {
            let end_section = &out[start_pos + rel_end..];
            // Find the dashes closing the END line: "-----END ... -----"
            let closing_dashes = end_section.find("-----").map(|first_five| {
                if let Some(second_five) = end_section[first_five + 5..].find("-----") {
                    first_five + 5 + second_five + 5
                } else {
                    first_five + 5
                }
            }).unwrap_or(rel_end + 8);

            let end_pos = start_pos + rel_end + closing_dashes;
            out.replace_range(start_pos..end_pos.min(out.len()), "[REDACTED_PRIVATE_KEY]");
        } else {
            // No END marker, redact from BEGIN to end or next 2000 chars
            let end_pos = (start_pos + 2000).min(out.len());
            out.replace_range(start_pos..end_pos, "[REDACTED_PRIVATE_KEY]");
            break;
        }
    }

    out
}

/// Performs heading-aware bounded selection for long Markdown documents.
/// Prioritizes:
/// - Overview, Features, Usage, Quick Start, Installation, Architecture, Configuration, Limitations, Status
/// - Lines containing negative/qualifier terms (experimental, deprecated, unsupported, planned, not recommended)
pub fn extract_heading_aware_bounded_markdown(
    full_text: &str,
    max_bytes: usize,
) -> (String, bool, Vec<EvidenceRange>) {
    let lines: Vec<&str> = full_text.lines().collect();
    if lines.is_empty() {
        return (String::new(), false, vec![]);
    }

    // Parse sections by headings (# Heading, ## Heading, etc.)
    #[allow(dead_code)]
    struct Section<'a> {
        heading_title: String,
        start_line_1based: usize,
        end_line_1based: usize,
        lines: Vec<&'a str>,
        priority: u32,
    }

    let mut sections = Vec::new();
    let mut current_title = "preamble".to_string();
    let mut current_lines = Vec::new();
    let mut current_start = 1;

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            if !current_lines.is_empty() {
                let priority = score_heading_priority(&current_title);
                sections.push(Section {
                    heading_title: current_title,
                    start_line_1based: current_start,
                    end_line_1based: line_num - 1,
                    lines: current_lines,
                    priority,
                });
            }
            current_title = trimmed.trim_start_matches('#').trim().to_string();
            current_lines = vec![*line];
            current_start = line_num;
        } else {
            current_lines.push(*line);
        }
    }

    if !current_lines.is_empty() {
        let priority = score_heading_priority(&current_title);
        sections.push(Section {
            heading_title: current_title,
            start_line_1based: current_start,
            end_line_1based: lines.len(),
            lines: current_lines,
            priority,
        });
    }

    // Always keep preamble / top section
    let mut selected_indices = std::collections::BTreeSet::new();
    let mut accumulated_bytes = 0;

    if !sections.is_empty() {
        let preamble_bytes: usize = sections[0].lines.iter().map(|l| l.len() + 1).sum();
        selected_indices.insert(0);
        accumulated_bytes += preamble_bytes;
    }

    // Also identify sections containing critical negative / qualification keywords
    for (idx, sec) in sections.iter().enumerate() {
        if idx == 0 {
            continue;
        }
        if contains_critical_qualifier(&sec.lines) {
            let sec_bytes: usize = sec.lines.iter().map(|l| l.len() + 1).sum();
            if accumulated_bytes + sec_bytes <= max_bytes {
                selected_indices.insert(idx);
                accumulated_bytes += sec_bytes;
            }
        }
    }

    // Sort remaining sections by priority score descending
    let mut section_order: Vec<(usize, u32)> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.priority))
        .collect();
    section_order.sort_by(|a, b| b.1.cmp(&a.1));

    for (idx, _) in section_order {
        if selected_indices.contains(&idx) {
            continue;
        }

        let sec_bytes: usize = sections[idx].lines.iter().map(|l| l.len() + 1).sum();
        if accumulated_bytes + sec_bytes <= max_bytes {
            selected_indices.insert(idx);
            accumulated_bytes += sec_bytes;
        }
    }

    // Assemble in original line order
    let mut output_lines = Vec::new();
    let mut ranges = Vec::new();
    let mut last_idx: Option<usize> = None;

    for &idx in &selected_indices {
        let sec = &sections[idx];

        if let Some(prev) = last_idx {
            if idx > prev + 1 {
                output_lines.push("\n[... content truncated for brevity ...]\n");
            }
        }

        output_lines.extend_from_slice(&sec.lines);
        ranges.push(EvidenceRange {
            start_line: sec.start_line_1based,
            end_line: sec.end_line_1based,
        });

        last_idx = Some(idx);
    }

    let is_truncated = selected_indices.len() < sections.len();
    (output_lines.join("\n"), is_truncated, ranges)
}

fn score_heading_priority(title: &str) -> u32 {
    let lower = title.to_lowercase();

    if lower.contains("overview")
        || lower.contains("about")
        || lower.contains("introduction")
        || lower.contains("what is")
    {
        return 100;
    }
    if lower.contains("feature") || lower.contains("capabilities") || lower.contains("highlights") {
        return 90;
    }
    if lower.contains("quick start")
        || lower.contains("getting started")
        || lower.contains("usage")
        || lower.contains("how to use")
    {
        return 85;
    }
    if lower.contains("installation") || lower.contains("install") || lower.contains("setup") {
        return 80;
    }
    if lower.contains("architecture") || lower.contains("design") || lower.contains("how it works") {
        return 75;
    }
    if lower.contains("limitation")
        || lower.contains("known issue")
        || lower.contains("status")
        || lower.contains("caveat")
        || lower.contains("warning")
    {
        return 70;
    }
    if lower.contains("configuration") || lower.contains("config") || lower.contains("options") {
        return 65;
    }

    10
}

fn contains_critical_qualifier(lines: &[&str]) -> bool {
    for line in lines {
        let lower = line.to_lowercase();
        if lower.contains("experimental")
            || lower.contains("deprecated")
            || lower.contains("unsupported")
            || lower.contains("planned")
            || lower.contains("not recommended")
            || lower.contains("do not use")
            || lower.contains("work in progress")
            || lower.contains("wip")
            || lower.contains("caution")
        {
            return true;
        }
    }
    false
}

fn truncate_plain_text(
    input: &str,
    max_bytes: usize,
) -> (String, bool, Vec<EvidenceRange>) {
    let mut accumulated = 0;
    let mut selected_lines = Vec::new();

    for line in input.lines() {
        let line_len = line.len() + 1;
        if accumulated + line_len > max_bytes {
            break;
        }
        selected_lines.push(line);
        accumulated += line_len;
    }

    let truncated = selected_lines.len() < input.lines().count();
    let total_lines = selected_lines.len().max(1);

    (
        selected_lines.join("\n"),
        truncated,
        vec![EvidenceRange {
            start_line: 1,
            end_line: total_lines,
        }],
    )
}

fn compute_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}
