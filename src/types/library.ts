export const ALL_CATEGORIES = [
  "AI & Agents",
  "Developer Tools",
  "Web & SaaS",
  "Data & Database",
  "DevOps & Cloud",
  "Security & Privacy",
  "Networking",
  "Multimedia",
  "Automation",
  "Productivity",
  "Design & UI",
  "Mobile",
  "Games",
  "System Utilities",
  "Libraries & Frameworks",
  "Other",
] as const;

export type Category = (typeof ALL_CATEGORIES)[number];

export interface DescriptionTranslation {
  text: string;
  target_locale: string;
}

export type TranslationStatus = "valid" | "stale" | "corrupt";

export interface DescriptionTranslationResult {
  translated_text: string | null;
  target_locale: string;
  source_hash: string;
  action: "translated" | "unchanged";
  provider: string;
  model: string;
}

export type TranslationPatchInput =
  | { action: "keep" }
  | { action: "replace"; text: string; target_locale: string; source_text: string }
  | { action: "clear" };

export type AnalysisLocale = "en" | "zh-Hans" | "zh-Hant";
export type SectionId =
  | "overview"
  | "features"
  | "use_cases"
  | "tech_stack"
  | "setup"
  | "target_users"
  | "maturity"
  | "limitations";

export type SectionStatus = "supported" | "partial" | "insufficient";
export type ClaimBasis = "documented" | "code_observed" | "inferred" | "uncertain";

export interface AnalysisItem {
  text: string;
  basis: ClaimBasis;
  evidence_ids: string[];
}

export interface AnalysisSection {
  status: SectionStatus;
  items: AnalysisItem[];
  missing_information: string[];
}

export type AnalysisContent = Record<SectionId, AnalysisSection>;

export interface AnalysisGenerator {
  provider: string;
  model_requested: string;
  model_reported: string | null;
  prompt_version: string;
  selector_version: string;
  context_budget_tokens: number;
  input_tokens: number;
  input_tokens_is_estimate: boolean;
  output_limit_tokens: number;
}

export interface AnalysisSource {
  github_repo_id: string;
  canonical_url: string;
  default_branch: string | null;
  commit_sha: string | null;
  metadata_observed_at: string;
  evidence_fingerprint: string;
}

export interface EvidenceRange {
  start_line: number;
  end_line: number;
}

export interface EvidenceRecord {
  id: string;
  kind: "metadata" | "tree" | "file" | "release";
  path: string | null;
  commit_sha: string | null;
  blob_sha: string | null;
  observed_at: string;
  included_ranges: EvidenceRange[];
  included_content_hash: string;
  truncated: boolean;
  public_fact_summary: string | null;
}

export interface AnalysisGap {
  kind: "missing" | "failed" | "skipped" | "truncated" | "unverified";
  subject: string;
  reason_code: string;
}

export interface AnalysisCoverage {
  status: "bounded" | "partial";
  selected_file_count: number;
  sent_file_count: number;
  evidence_count: number;
  gaps: AnalysisGap[];
  executed_code: false;
  verified_deployment: false;
}

export interface AnalysisEnvelopeV1 {
  schema_version: 1;
  content: AnalysisContent;
  target_locale: AnalysisLocale;
  generated_at: string;
  generator: AnalysisGenerator;
  source: AnalysisSource;
  evidence: EvidenceRecord[];
  coverage: AnalysisCoverage;
  search_text: string;
}

export type AnalysisPatchInput =
  | { action: "keep" }
  | {
      action: "replace";
      envelope: AnalysisEnvelopeV1;
      expected_revision: string;
      expected_repo_id: number;
    };

export type AnalysisReadStatus =
  | { status: "none" }
  | { status: "valid" }
  | { status: "corrupt"; reason: string }
  | { status: "unsupported"; schema_version?: number; reason: string };

export interface AnalyzeRepositoryRequest {
  repository: string;
  expected_repo_id?: number | null;
  target_locale: AnalysisLocale;
  client_session_id?: string | null;
  generation?: number | null;
}

export type AnalysisProgressStage =
  | "idle"
  | "queued"
  | "processing"
  | "generated_unsaved"
  | "error"
  | "cancelled";

export interface FavoriteInput {
  github_repo_id?: number | null;
  github_url: string;
  name: string;
  description?: string | null;
  category: string;
  stars: number;
  license?: string | null;
  topics: string[];
  pushed_at?: string | null;
  repo_created_at?: string | null;
  releases?: number | null;
  languages?: Record<string, number> | null;
  translation?: {
    text: string;
    target_locale: string;
  } | null;
  ai_analysis?: AnalysisEnvelopeV1 | null;
}

export interface FavoriteTagItem {
  id: number;
  favorite_id: number;
  tag: string;
  source: "github" | "ai" | "user";
}

export interface FavoriteItem {
  id: number;
  github_repo_id?: number | null;
  github_url: string;
  name: string;
  description?: string | null;
  category: string;
  stars: number;
  license?: string | null;
  pushed_at?: string | null;
  repo_created_at?: string | null;
  rating?: number | null;
  rating_reason?: string | null;
  rating_confidence?: "low" | "medium" | "high" | null;
  notes: string;
  favorited_at: string;
  metadata_refreshed_at: string;
  tags: FavoriteTagItem[];
  releases?: number | null;
  languages?: Record<string, number> | null;
  description_translation?: DescriptionTranslation | null;
  description_translation_source_hash?: string | null;
  translation_status?: TranslationStatus | null;
  ai_analysis?: AnalysisEnvelopeV1 | null;
  analysis_status?: AnalysisReadStatus | null;
  raw_ai_analysis?: string | null;
  analysis_revision?: string | null;
}

export interface FavoriteStatus {
  is_favorited: boolean;
  favorite_id?: number | null;
}

export interface BatchFavoriteItemResult {
  github_url: string;
  status: "succeeded" | "already_exists" | "failed";
  favorite_id?: number | null;
  error?: string | null;
}

export interface BatchFavoriteResult {
  succeeded: number;
  already_exists: number;
  failed: number;
  items: BatchFavoriteItemResult[];
}

export interface BatchFavoriteIdentitiesResult {
  matched_repo_ids: number[];
  matched_urls: string[];
}


export interface UpdateFavoriteUserMetadataInput {
  favorite_id: number;
  category: string;
  rating?: number | null;
  rating_reason?: string | null;
  rating_confidence?: "low" | "medium" | "high" | null;
  notes: string;
  user_tags: string[];
  ai_tags?: string[];
  translation_patch?: TranslationPatchInput | null;
  analysis_patch?: AnalysisPatchInput | null;
}

export type LibrarySortField =
  | "favorited_at"
  | "stars"
  | "rating"
  | "pushed_at"
  | "name";

export type SortOrder = "asc" | "desc";

export interface LibraryQuery {
  search?: string;
  category?: string;
  tag?: string;
  rating?: string; // "all" | "unrated" | "1" | "2" | "3" | "4" | "5"
  sort_field?: LibrarySortField;
  sort_order?: SortOrder;
}

export interface RefreshResult {
  favorite_id: number;
  status: "succeeded" | "not_found" | "rate_limited" | "conflict" | "failed";
  message?: string | null;
  favorite?: FavoriteItem | null;
}

export interface BatchRefreshResult {
  total: number;
  succeeded: number;
  failed: number;
  rate_limited: number;
  not_found: number;
  results: RefreshResult[];
}

export interface BatchRefreshProgress {
  completed: number;
  total: number;
}

export interface BatchDeleteResult {
  requested_count: number;
  deleted_count: number;
  not_found_count: number;
  deleted_ids: number[];
}

export interface AiFavoriteMetadataSuggestion {
  category: string;
  tags: string[];
  rating: number;
  rating_reason: string;
  confidence: "low" | "medium" | "high";
}

