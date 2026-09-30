/**
 * Date and time formatting utility with explicit timezone and locale support.
 * Conforms to RepoShelf v1.1.2 Technical Spec §6 & §18.
 */

export function formatRepositoryDate(
  isoString: string | null | undefined,
  locale?: string,
  timeZone?: string,
  fallback = "—"
): string {
  if (!isoString || typeof isoString !== "string") {
    return fallback;
  }
  const trimmed = isoString.trim();
  if (trimmed === "") {
    return fallback;
  }

  const date = new Date(trimmed);
  if (isNaN(date.getTime())) {
    return fallback;
  }

  try {
    const options: Intl.DateTimeFormatOptions = {
      year: "numeric",
      month: "short",
      day: "numeric",
    };
    if (timeZone) {
      options.timeZone = timeZone;
    }
    return new Intl.DateTimeFormat(locale || undefined, options).format(date);
  } catch {
    return fallback;
  }
}

export function formatRepositoryDateTime(
  isoString: string | null | undefined,
  locale?: string,
  timeZone?: string,
  fallback = "—"
): string {
  if (!isoString || typeof isoString !== "string") {
    return fallback;
  }
  const trimmed = isoString.trim();
  if (trimmed === "") {
    return fallback;
  }

  const date = new Date(trimmed);
  if (isNaN(date.getTime())) {
    return fallback;
  }

  try {
    const options: Intl.DateTimeFormatOptions = {
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    };
    if (timeZone) {
      options.timeZone = timeZone;
    }
    return new Intl.DateTimeFormat(locale || undefined, options).format(date);
  } catch {
    return fallback;
  }
}
