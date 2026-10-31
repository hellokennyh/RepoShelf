import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  LIBRARY_COLUMNS,
  LIBRARY_COLUMN_STORAGE_KEY,
  getDefaultColumnWidths,
  loadPersistedColumnWidths,
  persistColumnWidths,
  resetPersistedColumnWidths,
  getResolvedDescription,
  getUserTags,
} from "../utils/libraryTable";
import { FavoriteItem } from "../types/library";

describe("S4 libraryTable Utility Suite", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  describe("Column Definitions & Defaults", () => {
    it("defines 13 stable columns and excludes TAGS", () => {
      expect(LIBRARY_COLUMNS).toHaveLength(13);
      const colIds = LIBRARY_COLUMNS.map((c) => c.id);
      expect(colIds).toEqual([
        "selection",
        "name",
        "category",
        "description",
        "userTags",
        "notes",
        "stars",
        "createdAt",
        "lastActive",
        "rating",
        "favoritedAt",
        "lastRefresh",
        "actions",
      ]);
      expect(colIds).not.toContain("tags");
    });

    it("has valid minWidth, maxWidth and defaultWidth constraints for every column", () => {
      for (const col of LIBRARY_COLUMNS) {
        expect(col.defaultWidth).toBeGreaterThanOrEqual(col.minWidth);
        expect(col.defaultWidth).toBeLessThanOrEqual(col.maxWidth);
        if (col.id === "selection") {
          expect(col.resizable).toBe(false);
        } else {
          expect(col.resizable).toBe(true);
        }
      }
    });

    it("getDefaultColumnWidths returns defaults matching LIBRARY_COLUMNS", () => {
      const defaults = getDefaultColumnWidths();
      expect(Object.keys(defaults)).toHaveLength(13);
      expect(defaults.selection).toBe(44);
      expect(defaults.name).toBe(180);
      expect(defaults.category).toBe(110);
      expect(defaults.description).toBe(280);
      expect(defaults.userTags).toBe(170);
      expect(defaults.notes).toBe(220);
      expect(defaults.stars).toBe(90);
    });
  });

  describe("Persistence & Validation", () => {
    it("returns defaults when localStorage is empty", () => {
      const widths = loadPersistedColumnWidths();
      expect(widths).toEqual(getDefaultColumnWidths());
    });

    it("persists and loads valid custom column widths", () => {
      persistColumnWidths({
        name: 240,
        description: 350,
        notes: 300,
      });

      const loaded = loadPersistedColumnWidths();
      expect(loaded.name).toBe(240);
      expect(loaded.description).toBe(350);
      expect(loaded.notes).toBe(300);
      expect(loaded.category).toBe(110); // remains default
      expect(loaded.selection).toBe(44); // selection always default
    });

    it("clamps widths that are smaller than minWidth or larger than maxWidth", () => {
      const nameCol = LIBRARY_COLUMNS.find((c) => c.id === "name")!;
      persistColumnWidths({
        name: 10, // below minWidth (140)
        description: 2000, // above maxWidth (600)
      });

      const loaded = loadPersistedColumnWidths();
      expect(loaded.name).toBe(nameCol.minWidth);
      expect(loaded.description).toBe(600);
    });

    it("handles corrupt JSON gracefully by falling back to defaults", () => {
      localStorage.setItem(LIBRARY_COLUMN_STORAGE_KEY, "invalid-json{not_an_obj");
      const loaded = loadPersistedColumnWidths();
      expect(loaded).toEqual(getDefaultColumnWidths());
    });

    it("handles non-object JSON values (arrays, strings, numbers, null)", () => {
      localStorage.setItem(LIBRARY_COLUMN_STORAGE_KEY, JSON.stringify([1, 2, 3]));
      expect(loadPersistedColumnWidths()).toEqual(getDefaultColumnWidths());

      localStorage.setItem(LIBRARY_COLUMN_STORAGE_KEY, JSON.stringify("hello"));
      expect(loadPersistedColumnWidths()).toEqual(getDefaultColumnWidths());

      localStorage.setItem(LIBRARY_COLUMN_STORAGE_KEY, JSON.stringify(null));
      expect(loadPersistedColumnWidths()).toEqual(getDefaultColumnWidths());
    });

    it("discards obsolete 'tags' column and unknown IDs", () => {
      localStorage.setItem(
        LIBRARY_COLUMN_STORAGE_KEY,
        JSON.stringify({
          tags: 200,
          unknownCol: 500,
          name: 210,
        })
      );
      const loaded = loadPersistedColumnWidths();
      expect(loaded.name).toBe(210);
      expect(loaded).not.toHaveProperty("tags");
      expect(loaded).not.toHaveProperty("unknownCol");
    });

    it("handles negative numbers, NaN, and Infinity safely", () => {
      localStorage.setItem(
        LIBRARY_COLUMN_STORAGE_KEY,
        JSON.stringify({
          name: -50,
          description: "NaN",
          notes: null,
          category: 999999,
        })
      );
      const loaded = loadPersistedColumnWidths();
      expect(loaded.name).toBe(LIBRARY_COLUMNS.find((c) => c.id === "name")!.defaultWidth);
      expect(loaded.description).toBe(280);
      expect(loaded.notes).toBe(220);
      expect(loaded.category).toBe(200); // clamped to maxWidth
    });

    it("resetPersistedColumnWidths clears storage and returns defaults", () => {
      persistColumnWidths({ name: 250 });
      expect(localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY)).not.toBeNull();

      const reset = resetPersistedColumnWidths();
      expect(reset).toEqual(getDefaultColumnWidths());
      expect(localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY)).toBeNull();
    });

    it("never persists non-numeric or business data to localStorage", () => {
      persistColumnWidths({
        name: 200,
        notes: 250,
        someSecret: "apiKey123" as unknown as number,
      });

      const raw = localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY);
      expect(raw).not.toBeNull();
      const parsed = JSON.parse(raw!);
      expect(parsed).toEqual({ name: 200, notes: 250 });
      expect(parsed).not.toHaveProperty("someSecret");
    });

    it("handles localStorage throwing security/quota error safely", () => {
      vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
        throw new Error("SecurityError: localStorage is disabled");
      });
      const loaded = loadPersistedColumnWidths();
      expect(loaded).toEqual(getDefaultColumnWidths());

      vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
        throw new Error("QuotaExceededError");
      });
      expect(() => persistColumnWidths({ name: 200 })).not.toThrow();
    });
  });

  describe("Description Mapping Rules (R05)", () => {
    const baseFav: FavoriteItem = {
      id: 1,
      name: "awesome-tool",
      category: "Developer Tools",
      stars: 1000,
      notes: "My notes",
      favorited_at: "2026-03-01T00:00:00Z",
      metadata_refreshed_at: "2026-03-01T00:00:00Z",
      tags: [],
      github_url: "https://github.com/org/awesome-tool",
    };

    it("Priority 1: returns saved translation when target_locale matches current UI locale", () => {
      const fav: FavoriteItem = {
        ...baseFav,
        description: "Original English description",
        description_translation: {
          target_locale: "zh-Hans",
          text: "简体中文翻译",
        },
      };

      const res = getResolvedDescription(fav, "zh-Hans");
      expect(res.text).toBe("简体中文翻译");
      expect(res.isTranslation).toBe(true);
    });

    it("Priority 2: falls back to original description when translation locale does not match current locale", () => {
      const fav: FavoriteItem = {
        ...baseFav,
        description: "Original English description",
        description_translation: {
          target_locale: "zh-Hans",
          text: "简体中文翻译",
        },
      };

      // Current locale is zh-Hant, must NOT use zh-Hans translation
      const resHant = getResolvedDescription(fav, "zh-Hant");
      expect(resHant.text).toBe("Original English description");
      expect(resHant.isTranslation).toBe(false);

      // Current locale is en, must NOT use zh-Hans translation
      const resEn = getResolvedDescription(fav, "en");
      expect(resEn.text).toBe("Original English description");
      expect(resEn.isTranslation).toBe(false);
    });

    it("Priority 2: falls back to original description when translation is corrupt or empty", () => {
      const favCorrupt: FavoriteItem = {
        ...baseFav,
        description: "Original English description",
        description_translation: {
          target_locale: "zh-Hans",
          text: "   ",
        },
        translation_status: "corrupt",
      };

      const res = getResolvedDescription(favCorrupt, "zh-Hans");
      expect(res.text).toBe("Original English description");
      expect(res.isTranslation).toBe(false);
    });

    it("Priority 3: returns '—' when neither translation nor description is available", () => {
      const favEmpty: FavoriteItem = {
        ...baseFav,
        description: null,
        description_translation: null,
      };

      const res = getResolvedDescription(favEmpty, "zh-Hans");
      expect(res.text).toBe("—");
      expect(res.isTranslation).toBe(false);
    });
  });

  describe("User Tags Mapping Rules (R05)", () => {
    it("filters and only returns tags with source === 'user'", () => {
      const fav: FavoriteItem = {
        id: 1,
        name: "tool",
        category: "Other",
        stars: 10,
        notes: "",
        favorited_at: "2026-03-01T00:00:00Z",
        metadata_refreshed_at: "2026-03-01T00:00:00Z",
        github_url: "https://github.com/org/tool",
        tags: [
          { id: 1, favorite_id: 1, tag: "topic-1", source: "github" },
          { id: 2, favorite_id: 1, tag: "topic-2", source: "github" },
          { id: 3, favorite_id: 1, tag: "my-custom-tag", source: "user" },
          { id: 4, favorite_id: 1, tag: "ai-suggested-tag", source: "ai" },
          { id: 5, favorite_id: 1, tag: "second-user-tag", source: "user" },
        ],
      };

      const userTags = getUserTags(fav);
      expect(userTags).toHaveLength(2);
      expect(userTags.map((t) => t.tag)).toEqual(["my-custom-tag", "second-user-tag"]);
    });

    it("returns empty array when favorite has no tags or null tags", () => {
      const fav: FavoriteItem = {
        id: 1,
        name: "tool",
        category: "Other",
        stars: 10,
        notes: "",
        favorited_at: "2026-03-01T00:00:00Z",
        metadata_refreshed_at: "2026-03-01T00:00:00Z",
        github_url: "https://github.com/org/tool",
        tags: [],
      };
      expect(getUserTags(fav)).toEqual([]);
    });
  });
});
