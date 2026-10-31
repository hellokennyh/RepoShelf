/**
 * Category localization and safe fallback utilities.
 * Conforms to RepoShelf v1.1.3 S1 specification (R02).
 */

/**
 * Safely translates category name using i18n t() function.
 * If translation key is missing or t() returns keyPath literal (e.g. "categories.Automation"),
 * it safely falls back to the original category name.
 */
export function formatCategory(
  t: (keyPath: string, params?: Record<string, string | number>) => string,
  category: string | null | undefined
): string {
  if (!category) return "";
  const trimmed = category.trim();
  if (!trimmed) return "";
  const keyPath = `categories.${trimmed}`;
  const translated = t(keyPath);
  if (!translated || translated === keyPath) {
    return trimmed;
  }
  return translated;
}
