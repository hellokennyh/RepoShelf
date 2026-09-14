import { GitHubRepository } from "./github";

export interface ArticleImportResult {
  source_url: string;
  repositories: GitHubRepository[];
  extracted_unique_count: number;
  valid_count: number;
  invalid_count: number;
  skipped_count: number;
  truncated: boolean;
  warning?: string | null;
}

export interface ArticleSummary {
  sourceUrl: string;
  extractedUniqueCount: number;
  validCount: number;
  invalidCount: number;
  skippedCount: number;
  truncated: boolean;
  warning?: string | null;
}
