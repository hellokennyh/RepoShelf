use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrendingPeriod {
    Daily,
    Weekly,
    Monthly,
}

impl TrendingPeriod {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrendingPeriod::Daily => "daily",
            TrendingPeriod::Weekly => "weekly",
            TrendingPeriod::Monthly => "monthly",
        }
    }
}

impl fmt::Display for TrendingPeriod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TrendingPeriod {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "daily" | "today" => Ok(TrendingPeriod::Daily),
            "weekly" | "week" | "this week" => Ok(TrendingPeriod::Weekly),
            "monthly" | "month" | "this month" => Ok(TrendingPeriod::Monthly),
            other => Err(AppError::InvalidTrendingInput(format!(
                "Invalid trending period '{}'. Expected 'daily', 'weekly', or 'monthly'.",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendingRequest {
    pub period: TrendingPeriod,
    pub limit: usize,
}

impl TrendingRequest {
    pub fn new(period: TrendingPeriod, limit: usize) -> Result<Self, AppError> {
        if limit == 0 || limit > 50 {
            return Err(AppError::InvalidTrendingInput(format!(
                "Invalid limit {}. Allowed limit is 1 to 50.",
                limit
            )));
        }
        Ok(Self { period, limit })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedTrendingRepository {
    pub github_url: String,
    pub owner: String,
    pub repo: String,
    pub name: String,
    pub description: Option<String>,
    pub stars: Option<u64>,
    pub period_stars: Option<u64>,
    pub trending_rank: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EnrichmentSummary {
    pub attempted: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub rate_limited: bool,
    pub rate_limit_reset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingRepositoryItem {
    pub github_repo_id: Option<i64>,
    pub github_url: String,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub stars: Option<u64>,
    pub license: Option<String>,
    pub topics: Vec<String>,
    pub pushed_at: Option<String>,
    pub trending_rank: usize,
    pub period_stars: Option<u64>,
    #[serde(default)]
    pub releases: Option<i64>,
    #[serde(default)]
    pub languages: Option<std::collections::HashMap<String, f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendingResponse {
    pub period: TrendingPeriod,
    pub requested_limit: usize,
    pub returned_count: usize,
    pub items: Vec<TrendingRepositoryItem>,
    pub enrichment_summary: EnrichmentSummary,
    pub is_short_source: bool,
    pub partial_warning: Option<String>,
}
