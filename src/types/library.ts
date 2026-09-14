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
  rating?: number | null;
  rating_reason?: string | null;
  rating_confidence?: "low" | "medium" | "high" | null;
  notes: string;
  favorited_at: string;
  metadata_refreshed_at: string;
  tags: FavoriteTagItem[];
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

export interface AiFavoriteMetadataSuggestion {
  category: string;
  tags: string[];
  rating: number;
  rating_reason: string;
  confidence: "low" | "medium" | "high";
}

