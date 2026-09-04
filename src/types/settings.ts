export type TrendingPeriod = "daily" | "weekly" | "monthly";

export type AiProvider =
  | "openai_compatible"
  | "anthropic"
  | "gemini"
  | "ollama";

export interface GitHubSettings {
  default_search_limit: number;
  default_trending_period: TrendingPeriod;
}

export interface AiSettings {
  enabled: boolean;
  provider: AiProvider | null;
  base_url: string | null;
  model: string | null;
}

export interface Settings {
  schema_version: number;
  github: GitHubSettings;
  ai: AiSettings;
}

export interface SecretStatus {
  github_token_configured: boolean;
  ai_api_key_configured: boolean;
}

export interface AppErrorResponse {
  code: string;
  message: string;
}
