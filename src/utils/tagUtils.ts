/**
 * Tag normalization and deduplication utilities for RepoShelf.
 * AI tags strictly supplement GitHub Topics:
 * If an AI tag duplicates a GitHub topic (case-insensitive, trimmed),
 * it must be filtered out.
 */

export function normalizeTagForDedup(tag: string): string {
  return tag.trim().toLowerCase();
}

export function filterSupplementalAiTags(
  aiTags: string[],
  githubTags: string[]
): string[] {
  const ghSet = new Set(
    githubTags
      .map(normalizeTagForDedup)
      .filter((t) => t.length > 0)
  );

  const result: string[] = [];
  const seenAi = new Set<string>();

  for (const rawTag of aiTags) {
    const trimmed = rawTag.trim();
    if (!trimmed) continue;

    const normalized = normalizeTagForDedup(trimmed);
    if (!ghSet.has(normalized) && !seenAi.has(normalized)) {
      seenAi.add(normalized);
      result.push(trimmed);
    }
  }

  return result.slice(0, 5);
}
