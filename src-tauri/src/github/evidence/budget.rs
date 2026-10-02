use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::AppError;

pub const BUDGET_MAX_GITHUB_REQUESTS: usize = 40;
pub const BUDGET_MAX_DIRECTORY_REQUESTS: usize = 8;
pub const BUDGET_MAX_SELECTED_FILES: usize = 20;
pub const BUDGET_MAX_RECENT_RELEASES: usize = 5;
pub const BUDGET_MAX_SINGLE_RESPONSE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB
pub const BUDGET_MAX_TOTAL_RESPONSES_BYTES: usize = 8 * 1024 * 1024; // 8 MiB
pub const BUDGET_MAX_SINGLE_FILE_DECODED_BYTES: usize = 256 * 1024; // 256 KiB
pub const BUDGET_MAX_REDIRECTS_PER_REQUEST: usize = 3;
pub const BUDGET_MAX_CONCURRENCY: usize = 3;
pub const BUDGET_PER_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
pub const BUDGET_OVERALL_ACQUISITION_DEADLINE: Duration = Duration::from_secs(120);
pub const BUDGET_MODEL_DIRECTORY_MAX_PATHS: usize = 500;

/// Thread-safe cancellation token for an acquisition session.
#[derive(Debug, Clone)]
pub struct AcquisitionCancelToken {
    cancelled: Arc<AtomicBool>,
}

impl Default for AcquisitionCancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl AcquisitionCancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Resource accounting snapshot for finished or in-progress acquisition.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct ResourceAccounting {
    pub total_github_requests: usize,
    pub directory_requests: usize,
    pub selected_files_count: usize,
    pub release_count: usize,
    pub total_bytes_decompressed: usize,
    pub duration_ms: u64,
}

/// Budget tracker enforcing all strict resource and time limits for evidence acquisition.
pub struct AcquisitionBudgetTracker {
    requests_count: AtomicUsize,
    directory_requests_count: AtomicUsize,
    selected_files_count: AtomicUsize,
    releases_count: AtomicUsize,
    total_bytes_read: AtomicUsize,
    start_time: Instant,
    deadline: Instant,
    cancel_token: AcquisitionCancelToken,
}

impl AcquisitionBudgetTracker {
    pub fn new(cancel_token: Option<AcquisitionCancelToken>) -> Self {
        let now = Instant::now();
        Self {
            requests_count: AtomicUsize::new(0),
            directory_requests_count: AtomicUsize::new(0),
            selected_files_count: AtomicUsize::new(0),
            releases_count: AtomicUsize::new(0),
            total_bytes_read: AtomicUsize::new(0),
            start_time: now,
            deadline: now + BUDGET_OVERALL_ACQUISITION_DEADLINE,
            cancel_token: cancel_token.unwrap_or_default(),
        }
    }

    pub fn with_custom_deadline(
        deadline_duration: Duration,
        cancel_token: Option<AcquisitionCancelToken>,
    ) -> Self {
        let now = Instant::now();
        Self {
            requests_count: AtomicUsize::new(0),
            directory_requests_count: AtomicUsize::new(0),
            selected_files_count: AtomicUsize::new(0),
            releases_count: AtomicUsize::new(0),
            total_bytes_read: AtomicUsize::new(0),
            start_time: now,
            deadline: now + deadline_duration,
            cancel_token: cancel_token.unwrap_or_default(),
        }
    }

    pub fn cancel_token(&self) -> AcquisitionCancelToken {
        self.cancel_token.clone()
    }

    /// Verifies that the task has not been cancelled and the overall deadline has not expired.
    pub fn check_cancel_and_deadline(&self) -> Result<(), AppError> {
        if self.cancel_token.is_cancelled() {
            return Err(AppError::AcquisitionCancelled(
                "Evidence acquisition cancelled".to_string(),
            ));
        }

        if Instant::now() >= self.deadline {
            return Err(AppError::GitHubTimeout(format!(
                "Overall evidence acquisition deadline ({}s) exceeded",
                BUDGET_OVERALL_ACQUISITION_DEADLINE.as_secs()
            )));
        }

        Ok(())
    }

    /// Calculates the maximum allowed timeout for the next HTTP request, taking into account
    /// the 20s per-request limit and the remaining acquisition deadline.
    pub fn remaining_request_timeout(&self) -> Result<Duration, AppError> {
        self.check_cancel_and_deadline()?;
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AppError::GitHubTimeout(
                "Acquisition deadline has elapsed".to_string(),
            ));
        }
        Ok(std::cmp::min(BUDGET_PER_REQUEST_TIMEOUT, remaining))
    }

    /// Records an outbound GitHub request (including retry and redirect hops).
    pub fn record_request(&self) -> Result<usize, AppError> {
        self.check_cancel_and_deadline()?;
        let count = self.requests_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count > BUDGET_MAX_GITHUB_REQUESTS {
            return Err(AppError::AcquisitionBudgetExceeded(format!(
                "GitHub request limit exceeded: attempted {} requests (max {})",
                count, BUDGET_MAX_GITHUB_REQUESTS
            )));
        }
        Ok(count)
    }

    /// Records a directory listing request.
    pub fn record_directory_request(&self) -> Result<usize, AppError> {
        self.check_cancel_and_deadline()?;
        let count = self.directory_requests_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count > BUDGET_MAX_DIRECTORY_REQUESTS {
            return Err(AppError::AcquisitionBudgetExceeded(format!(
                "Directory request limit exceeded: attempted {} requests (max {})",
                count, BUDGET_MAX_DIRECTORY_REQUESTS
            )));
        }
        Ok(count)
    }

    /// Records additional decompressed bytes read across the acquisition session.
    pub fn record_bytes(&self, additional_bytes: usize) -> Result<usize, AppError> {
        self.check_cancel_and_deadline()?;
        if additional_bytes > BUDGET_MAX_SINGLE_RESPONSE_BYTES {
            return Err(AppError::GitHubInvalidResponse(format!(
                "Single response exceeded {} bytes limit (got {} bytes)",
                BUDGET_MAX_SINGLE_RESPONSE_BYTES, additional_bytes
            )));
        }

        let total = self.total_bytes_read.fetch_add(additional_bytes, Ordering::SeqCst) + additional_bytes;
        if total > BUDGET_MAX_TOTAL_RESPONSES_BYTES {
            return Err(AppError::AcquisitionBudgetExceeded(format!(
                "Total decompressed responses exceeded {} bytes limit (accumulated {} bytes)",
                BUDGET_MAX_TOTAL_RESPONSES_BYTES, total
            )));
        }
        Ok(total)
    }

    /// Records a selected file.
    pub fn record_selected_file(&self) -> Result<usize, AppError> {
        self.check_cancel_and_deadline()?;
        let count = self.selected_files_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count > BUDGET_MAX_SELECTED_FILES {
            return Err(AppError::AcquisitionBudgetExceeded(format!(
                "Selected files limit exceeded: attempted {} files (max {})",
                count, BUDGET_MAX_SELECTED_FILES
            )));
        }
        Ok(count)
    }

    /// Records a fetched release.
    pub fn record_release(&self) -> Result<usize, AppError> {
        self.check_cancel_and_deadline()?;
        let count = self.releases_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count > BUDGET_MAX_RECENT_RELEASES {
            return Err(AppError::AcquisitionBudgetExceeded(format!(
                "Releases limit exceeded: attempted {} releases (max {})",
                count, BUDGET_MAX_RECENT_RELEASES
            )));
        }
        Ok(count)
    }

    /// Returns the current resource accounting summary.
    pub fn snapshot(&self) -> ResourceAccounting {
        ResourceAccounting {
            total_github_requests: self.requests_count.load(Ordering::SeqCst),
            directory_requests: self.directory_requests_count.load(Ordering::SeqCst),
            selected_files_count: self.selected_files_count.load(Ordering::SeqCst),
            release_count: self.releases_count.load(Ordering::SeqCst),
            total_bytes_decompressed: self.total_bytes_read.load(Ordering::SeqCst),
            duration_ms: self.start_time.elapsed().as_millis() as u64,
        }
    }
}
