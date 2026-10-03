use std::collections::HashMap;
use std::sync::Mutex;
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

    pub fn register(&self, task_id: &str) -> AcquisitionCancelToken {
        let token = AcquisitionCancelToken::new();
        let mut map = self.tokens.lock().unwrap();
        map.insert(task_id.to_string(), token.clone());
        token
    }

    pub fn unregister(&self, task_id: &str) {
        let mut map = self.tokens.lock().unwrap();
        map.remove(task_id);
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let mut map = self.tokens.lock().unwrap();
        if let Some(token) = map.remove(task_id) {
            token.cancel();
            true
        } else {
            false
        }
    }
}
