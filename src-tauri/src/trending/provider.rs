use std::future::Future;
use std::pin::Pin;

use crate::error::AppError;
use crate::trending::models::{TrendingPeriod, TrendingResponse};

/// Minimal Trending Provider abstraction
pub trait TrendingProvider: Send + Sync {
    fn get_trending<'a>(
        &'a self,
        period: TrendingPeriod,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<TrendingResponse, AppError>> + Send + 'a>>;
}
