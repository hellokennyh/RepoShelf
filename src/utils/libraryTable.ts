import { FavoriteItem, FavoriteTagItem } from "../types/library";

export const LIBRARY_COLUMN_STORAGE_KEY = "reposhelf.library.columnWidths.v1";

export interface LibraryColumnDef {
  id: string;
  headerKey:
    | "name"
    | "category"
    | "description"
    | "userTags"
    | "notes"
    | "stars"
    | "createdAt"
    | "lastActive"
    | "rating"
    | "favoritedAt"
    | "lastRefresh"
    | "actions"
    | "";
  defaultWidth: number;
  minWidth: number;
  maxWidth: number;
  resizable: boolean;
  align?: "left" | "right" | "center";
}

export const LIBRARY_COLUMNS: LibraryColumnDef[] = [
  {
    id: "selection",
    headerKey: "",
    defaultWidth: 44,
    minWidth: 44,
    maxWidth: 44,
    resizable: false,
    align: "center",
  },
  {
    id: "name",
    headerKey: "name",
    defaultWidth: 180,
    minWidth: 140,
    maxWidth: 400,
    resizable: true,
    align: "left",
  },
  {
    id: "category",
    headerKey: "category",
    defaultWidth: 110,
    minWidth: 90,
    maxWidth: 200,
    resizable: true,
    align: "left",
  },
  {
    id: "description",
    headerKey: "description",
    defaultWidth: 280,
    minWidth: 150,
    maxWidth: 600,
    resizable: true,
    align: "left",
  },
  {
    id: "userTags",
    headerKey: "userTags",
    defaultWidth: 170,
    minWidth: 100,
    maxWidth: 350,
    resizable: true,
    align: "left",
  },
  {
    id: "notes",
    headerKey: "notes",
    defaultWidth: 220,
    minWidth: 120,
    maxWidth: 500,
    resizable: true,
    align: "left",
  },
  {
    id: "stars",
    headerKey: "stars",
    defaultWidth: 90,
    minWidth: 80,
    maxWidth: 160,
    resizable: true,
    align: "right",
  },
  {
    id: "createdAt",
    headerKey: "createdAt",
    defaultWidth: 110,
    minWidth: 95,
    maxWidth: 180,
    resizable: true,
    align: "left",
  },
  {
    id: "lastActive",
    headerKey: "lastActive",
    defaultWidth: 110,
    minWidth: 95,
    maxWidth: 180,
    resizable: true,
    align: "left",
  },
  {
    id: "rating",
    headerKey: "rating",
    defaultWidth: 90,
    minWidth: 80,
    maxWidth: 160,
    resizable: true,
    align: "left",
  },
  {
    id: "favoritedAt",
    headerKey: "favoritedAt",
    defaultWidth: 110,
    minWidth: 95,
    maxWidth: 180,
    resizable: true,
    align: "left",
  },
  {
    id: "lastRefresh",
    headerKey: "lastRefresh",
    defaultWidth: 110,
    minWidth: 95,
    maxWidth: 180,
    resizable: true,
    align: "left",
  },
  {
    id: "actions",
    headerKey: "actions",
    defaultWidth: 96,
    minWidth: 90,
    maxWidth: 160,
    resizable: true,
    align: "center",
  },
];

export function getDefaultColumnWidths(): Record<string, number> {
  const widths: Record<string, number> = {};
  for (const col of LIBRARY_COLUMNS) {
    widths[col.id] = col.defaultWidth;
  }
  return widths;
}

export function loadPersistedColumnWidths(): Record<string, number> {
  const defaults = getDefaultColumnWidths();
  try {
    if (typeof window === "undefined" || !window.localStorage) {
      return defaults;
    }
    const raw = window.localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY);
    if (!raw) return defaults;

    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return defaults;
    }

    const result: Record<string, number> = { ...defaults };
    for (const col of LIBRARY_COLUMNS) {
      if (!col.resizable) {
        result[col.id] = col.defaultWidth;
        continue;
      }
      const val = parsed[col.id];
      if (typeof val === "number" && Number.isFinite(val) && val > 0) {
        // Clamp to allowed column bounds
        result[col.id] = Math.max(col.minWidth, Math.min(col.maxWidth, Math.round(val)));
      }
    }
    return result;
  } catch (err) {
    console.warn("Failed to load column widths from localStorage, falling back to defaults:", err);
    return defaults;
  }
}

export function persistColumnWidths(widths: Record<string, number>): void {
  try {
    if (typeof window === "undefined" || !window.localStorage) {
      return;
    }
    // Only persist valid finite numbers for known resizable column IDs. Never save business content.
    const toSave: Record<string, number> = {};
    for (const col of LIBRARY_COLUMNS) {
      if (col.resizable && typeof widths[col.id] === "number" && Number.isFinite(widths[col.id])) {
        toSave[col.id] = Math.max(col.minWidth, Math.min(col.maxWidth, Math.round(widths[col.id])));
      }
    }
    window.localStorage.setItem(LIBRARY_COLUMN_STORAGE_KEY, JSON.stringify(toSave));
  } catch (err) {
    console.warn("Failed to persist column widths to localStorage:", err);
  }
}

export function resetPersistedColumnWidths(): Record<string, number> {
  const defaults = getDefaultColumnWidths();
  try {
    if (typeof window !== "undefined" && window.localStorage) {
      window.localStorage.removeItem(LIBRARY_COLUMN_STORAGE_KEY);
    }
  } catch (err) {
    console.warn("Failed to remove column widths from localStorage:", err);
  }
  return defaults;
}

export function getResolvedDescription(
  fav: FavoriteItem,
  currentLocale: string
): { text: string; isTranslation: boolean } {
  // Priority 1: Stored translation matching current UI language strictly
  if (
    fav.description_translation &&
    fav.description_translation.target_locale === currentLocale &&
    typeof fav.description_translation.text === "string" &&
    fav.description_translation.text.trim().length > 0 &&
    fav.translation_status !== "corrupt"
  ) {
    return {
      text: fav.description_translation.text.trim(),
      isTranslation: true,
    };
  }

  // Priority 2: Original GitHub description
  if (typeof fav.description === "string" && fav.description.trim().length > 0) {
    return {
      text: fav.description.trim(),
      isTranslation: false,
    };
  }

  // Priority 3: No valid data
  return {
    text: "—",
    isTranslation: false,
  };
}

export function getUserTags(fav: FavoriteItem): FavoriteTagItem[] {
  if (!fav.tags || !Array.isArray(fav.tags)) {
    return [];
  }
  return fav.tags.filter((t) => t.source === "user");
}
