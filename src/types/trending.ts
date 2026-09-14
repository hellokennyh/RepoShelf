export type TrendingPeriod = "daily" | "weekly" | "monthly";

export interface EnrichmentSummary {
  attempted: number;
  succeeded: number;
  failed: number;
  rate_limited: boolean;
  rate_limit_reset: number | null;
}

export interface TrendingRepositoryItem {
  github_repo_id: number | null;
  github_url: string;
  name: string;
  description: string | null;
  category: string;
  stars: number | null;
  license: string | null;
  topics: string[];
  pushed_at: string | null;
  trending_rank: number;
  period_stars: number | null;
  releases?: number | null;
  languages?: Record<string, number> | null;
}

export interface TrendingResponse {
  period: TrendingPeriod;
  requested_limit: number;
  returned_count: number;
  items: TrendingRepositoryItem[];
  enrichment_summary: EnrichmentSummary;
  is_short_source: boolean;
  partial_warning: string | null;
}
