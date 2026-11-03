use std::collections::HashMap;
use std::sync::Mutex;
use crate::error::AppError;
use crate::github::evidence::budget::AcquisitionCancelToken;

/// Manages cancellation tokens for active repository analysis tasks.
#[derive(Default)]
pub struct AnalysisTaskManager {
    tokens: Mutex<HashMap<String, AcquisitionCancelToken>>,
}

impl AnalysisTaskManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, task_id: &str) -> Result<AcquisitionCancelToken, AppError> {
        let mut map = self.tokens.lock().unwrap();
        if map.contains_key(task_id) {
            return Err(AppError::AnalysisTaskAlreadyExists(format!(
                "An analysis task with ID '{}' is already running",
                task_id
            )));
        }
        let token = AcquisitionCancelToken::new();
        map.insert(task_id.to_string(), token.clone());
        Ok(token)
    }

    pub fn unregister(&self, task_id: &str) {
        let mut map = self.tokens.lock().unwrap();
        map.remove(task_id);
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let map = self.tokens.lock().unwrap();
        if let Some(token) = map.get(task_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn active_count(&self) -> usize {
        let map = self.tokens.lock().unwrap();
        map.len()
    }
}
