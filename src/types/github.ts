export interface GitHubRepository {
  github_repo_id: number;
  github_url: string;
  name: string;
  description: string | null;
  stars: number;
  license: string | null;
  topics: string[];
  pushed_at: string | null;
  category: string;
}

export interface RateLimitBucket {
  limit: number;
  remaining: number;
  used: number;
  reset_at: string;
  resource: string;
}

export interface GitHubRateLimitStatus {
  core: RateLimitBucket;
  search: RateLimitBucket;
}

export interface GitHubConnectionStatus {
  connected: boolean;
  mode: "Anonymous" | "Authenticated" | string;
  rate_limit: GitHubRateLimitStatus;
}

export interface RepositorySearchRequest {
  query: string;
  min_stars?: number | null;
  pushed_after?: string | null;
  limit: number;
  exclude_selected?: boolean;
  exclude_favorites?: boolean;
}

export interface RepositorySearchResponse {
  items: GitHubRepository[];
  total_count: number;
}

export interface AiRepositorySearchRequest {
  input: string;
  min_stars?: number | null;
  pushed_after?: string | null;
  limit: number;
  exclude_selected?: boolean;
  exclude_favorites?: boolean;
}

export interface AiRepositorySearchResponse {
  items: GitHubRepository[];
  generated_queries: string[];
  partial: boolean;
}

export interface AiSearchReadiness {
  ready: boolean;
  reason: string | null;
}

export type LastActivePreset = "week" | "month" | "half_year" | "year" | "any";
