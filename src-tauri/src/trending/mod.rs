pub mod github;
pub mod models;
pub mod parser;
pub mod provider;

pub use github::GitHubTrendingProvider;
pub use models::*;
pub use parser::parse_trending_html;
pub use provider::TrendingProvider;
