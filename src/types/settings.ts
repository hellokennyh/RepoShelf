export type TrendingPeriod = "daily" | "weekly" | "monthly";

export type UiLocale = "en" | "zh-Hans" | "zh-Hant";

export interface LocaleInitInfo {
  locale: UiLocale;
  detected_source_tag?: string | null;
  warning?: string | null;
}

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
  ui_language?: UiLocale | null;
}

export interface SecretStatus {
  github_token_configured: boolean;
  ai_api_key_configured: boolean;
}

export interface AiConnectionResult {
  provider: string;
  model: string;
  success: boolean;
  message: string;
}

export interface AppErrorResponse {
  code: string;
  message: string;
}

export interface OllamaModelInfo {
  name: string;
  model: string;
  size?: number | null;
  modified_at?: string | null;
}

export interface OllamaEndpointDiscovery {
  base_url: string;
  models: OllamaModelInfo[];
}

