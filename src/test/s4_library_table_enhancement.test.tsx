import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { LibraryPage } from "../pages/LibraryPage";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";
import { I18nProvider } from "../i18n/I18nContext";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import { LIBRARY_COLUMNS, LIBRARY_COLUMN_STORAGE_KEY } from "../utils/libraryTable";

vi.mock("../services/tauri", () => ({
  listFavorites: vi.fn(),
  listDistinctTags: vi.fn(),
  openGitHubUrl: vi.fn(),
  refreshFavoritesBatch: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
  deleteFavoritesBatch: vi.fn(),
  fetchLocaleInitInfo: vi.fn().mockResolvedValue({
    os_locale: "zh-CN",
    saved_locale: "zh-Hans",
    effective_locale: "zh-Hans",
    requires_sync: false,
    source_tag: "test",
    warning: null,
  }),
  updateUiLanguage: vi.fn().mockResolvedValue(true),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: false, reason: "AI not configured" }),
  suggestFavoriteAiMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
}));

const mockFavorites: FavoriteItem[] = [
  {
    id: 1,
    github_repo_id: 101,
    github_url: "https://github.com/tauri-apps/tauri",
    name: "tauri",
    description: "Build smaller, faster, and more secure desktop apps with a web frontend.",
    description_translation: {
      target_locale: "zh-Hans",
      text: "使用网页前端构建更小、更快、更安全的桌面应用程序。",
    },
    category: "Developer Tools",
    stars: 85000,
    license: "Apache-2.0",
    pushed_at: "2026-03-01T12:00:00Z",
    repo_created_at: "2019-05-20T10:00:00Z",
    rating: 5,
    notes: "Top-tier framework for desktop apps.\nVery reliable and fast.",
    favorited_at: "2026-03-01T10:00:00Z",
    metadata_refreshed_at: "2026-03-01T10:00:00Z",
    tags: [
      { id: 1, favorite_id: 1, tag: "rust", source: "github" },
      { id: 2, favorite_id: 1, tag: "desktop", source: "github" },
      { id: 3, favorite_id: 1, tag: "core-tool", source: "user" },
      { id: 4, favorite_id: 1, tag: "production", source: "user" },
    ],
  },
  {
    id: 2,
    github_repo_id: 102,
    github_url: "https://github.com/sqlx/sqlx",
    name: "sqlx",
    description: "The Rust SQL Toolkit.",
    category: "Data & Database",
    stars: 15000,
    license: "MIT",
    pushed_at: "2026-02-28T10:00:00Z",
    repo_created_at: "2020-01-15T08:30:00Z",
    rating: 4,
    notes: "",
    favorited_at: "2026-03-02T10:00:00Z",
    metadata_refreshed_at: "2026-03-02T10:00:00Z",
    tags: [
      { id: 5, favorite_id: 2, tag: "database", source: "github" },
      { id: 6, favorite_id: 2, tag: "ai-suggested", source: "ai" },
    ],
  },
  {
    id: 3,
    github_repo_id: 103,
    github_url: "https://github.com/user/empty-meta",
    name: "empty-meta",
    description: null,
    category: "Other",
    stars: 10,
    license: null,
    pushed_at: null,
    repo_created_at: null,
    rating: null,
    notes: "   ",
    favorited_at: "2026-03-03T10:00:00Z",
    metadata_refreshed_at: "2026-03-03T10:00:00Z",
    tags: [],
  },
  {
    id: 4,
    github_repo_id: 104,
    github_url: "https://github.com/user/many-tags",
    name: "many-tags",
    description: "<script>alert('xss')</script>Normal desc",
    category: "Productivity",
    stars: 42,
    license: "MIT",
    pushed_at: "2026-03-04T10:00:00Z",
    repo_created_at: "2021-06-01T10:00:00Z",
    rating: 3,
    notes: "Note with <img src=x onerror=alert(1)> and newlines.\nLine two.",
    favorited_at: "2026-03-04T10:00:00Z",
    metadata_refreshed_at: "2026-03-04T10:00:00Z",
    tags: [
      { id: 10, favorite_id: 4, tag: "u1", source: "user" },
      { id: 11, favorite_id: 4, tag: "u2", source: "user" },
      { id: 12, favorite_id: 4, tag: "u3", source: "user" },
      { id: 13, favorite_id: 4, tag: "u4", source: "user" },
      { id: 14, favorite_id: 4, tag: "u5", source: "user" },
    ],
  },
];

const mockTags = ["rust", "desktop", "core-tool", "database", "u1", "u2"];

function renderWithProviders(ui: React.ReactElement, initialLocale = "zh-Hans") {
  return render(
    <I18nProvider initialLocale={initialLocale as "zh-Hans" | "zh-Hant" | "en"} initialIsInitialized={true}>
      <DiscoverySessionProvider>
        {ui}
      </DiscoverySessionProvider>
    </I18nProvider>
  );
}

describe("S4 Library Table Enhancement & Column Resizing Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(tauriService.listFavorites).mockResolvedValue(mockFavorites);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue(mockTags);
  });

  describe("11.1 Table Column Structure & Order (R05)", () => {
    it("removes TAGS column and renders 13 columns in correct order", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      // Assert TAGS column header is completely absent
      expect(screen.queryByTestId("library-th-tags")).not.toBeInTheDocument();

      // Assert the 3 new columns exist
      expect(screen.getByTestId("library-th-description")).toBeInTheDocument();
      expect(screen.getByTestId("library-th-userTags")).toBeInTheDocument();
      expect(screen.getByTestId("library-th-notes")).toBeInTheDocument();

      // Assert all 13 columns are in the exact specified order
      const ths = screen.getAllByRole("columnheader");
      expect(ths).toHaveLength(13);

      const expectedIds = [
        "library-th-selection",
        "library-th-name",
        "library-th-category",
        "library-th-description",
        "library-th-userTags",
        "library-th-notes",
        "library-th-stars",
        "library-th-createdAt",
        "library-th-lastActive",
        "library-th-rating",
        "library-th-favoritedAt",
        "library-th-lastRefresh",
        "library-th-actions",
      ];

      expectedIds.forEach((id, idx) => {
        expect(ths[idx]).toHaveAttribute("data-testid", id);
      });
    });

    it("renders each row with exactly 13 cells aligned with the headers", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const row = screen.getByTestId("library-row-1");
      const cells = within(row).getAllByRole("cell");
      expect(cells).toHaveLength(13);
    });

    it("preserves select all and individual row selection", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const selectAll = screen.getByTestId("library-select-all-checkbox") as HTMLInputElement;
      expect(selectAll.checked).toBe(false);

      // Select all
      fireEvent.click(selectAll);
      expect(selectAll.checked).toBe(true);

      const rowCheckbox1 = screen.getByTestId("library-row-checkbox-1") as HTMLInputElement;
      expect(rowCheckbox1.checked).toBe(true);

      // Deselect row 1
      fireEvent.click(rowCheckbox1);
      expect(rowCheckbox1.checked).toBe(false);
      expect(selectAll.checked).toBe(false);
    });
  });

  describe("11.2 Project Description Data Mapping & Full View (R05)", () => {
    it("displays saved translation matching current language (Priority 1)", async () => {
      renderWithProviders(<LibraryPage />, "zh-Hans");

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const descCell = screen.getByTestId("library-desc-1");
      expect(descCell).toHaveTextContent("使用网页前端构建更小、更快、更安全的桌面应用程序。");
    });

    it("falls back to GitHub original description when translation locale does NOT match (Priority 2)", async () => {
      // Current language is zh-Hant, but fav 1 translation is zh-Hans
      renderWithProviders(<LibraryPage />, "zh-Hant");

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const descCell = screen.getByTestId("library-desc-1");
      // Must fall back to original English description, NOT zh-Hans translation
      expect(descCell).toHaveTextContent("Build smaller, faster, and more secure desktop apps with a web frontend.");
    });

    it("falls back to GitHub original description when repo has no translation", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const descCell = screen.getByTestId("library-desc-2");
      expect(descCell).toHaveTextContent("The Rust SQL Toolkit.");
    });

    it("displays '—' when repo has no description and no translation (Priority 3)", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const descCell = screen.getByTestId("library-desc-3");
      expect(descCell).toHaveTextContent("—");
      expect(screen.queryByTestId("library-desc-view-btn-3")).not.toBeInTheDocument();
    });

    it("allows viewing full description in CellViewerModal and prevents HTML injection", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      // Fav 4 has HTML script tags in description
      const viewBtn = screen.getByTestId("library-desc-view-btn-4");
      fireEvent.click(viewBtn);

      const modal = screen.getByTestId("cell-viewer-modal");
      expect(modal).toBeInTheDocument();
      expect(modal).toHaveTextContent("<script>alert('xss')</script>Normal desc");

      // Close modal
      const closeBtn = screen.getByTestId("cell-viewer-close-btn");
      fireEvent.click(closeBtn);
      expect(screen.queryByTestId("cell-viewer-modal")).not.toBeInTheDocument();
    });
  });

  describe("11.3 User Tags Column (R05)", () => {
    it("strictly displays user tags and excludes GitHub topics and AI tags", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const userTagsCell1 = screen.getByTestId("library-user-tags-1");
      // Should contain core-tool and production
      expect(userTagsCell1).toHaveTextContent("#core-tool");
      expect(userTagsCell1).toHaveTextContent("#production");
      // Should NOT contain GitHub topics 'rust' or 'desktop'
      expect(userTagsCell1).not.toHaveTextContent("#rust");
      expect(userTagsCell1).not.toHaveTextContent("#desktop");

      // Fav 2 only has github topic 'database' and ai tag 'ai-suggested' -> should display '—'
      const userTagsCell2 = screen.getByTestId("library-user-tags-2");
      expect(userTagsCell2).toHaveTextContent("—");
      expect(userTagsCell2).not.toHaveTextContent("#database");
      expect(userTagsCell2).not.toHaveTextContent("#ai-suggested");
    });

    it("displays +N when user has more than 3 user tags and allows full view", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const userTagsCell4 = screen.getByTestId("library-user-tags-4");
      expect(userTagsCell4).toHaveTextContent("#u1");
      expect(userTagsCell4).toHaveTextContent("#u2");
      expect(userTagsCell4).toHaveTextContent("#u3");
      expect(userTagsCell4).toHaveTextContent("+2");

      const moreBtn = screen.getByTestId("library-user-tags-more-4");
      fireEvent.click(moreBtn);

      const modal = screen.getByTestId("cell-viewer-modal");
      expect(modal).toBeInTheDocument();
      expect(modal).toHaveTextContent("#u1");
      expect(modal).toHaveTextContent("#u5");

      // Close via Escape key
      fireEvent.keyDown(window, { key: "Escape" });
      expect(screen.queryByTestId("cell-viewer-modal")).not.toBeInTheDocument();
    });
  });

  describe("11.4 Personal Notes Column & Draft Isolation (R05)", () => {
    it("displays formally saved notes and '—' when empty", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const notesCell1 = screen.getByTestId("library-notes-1");
      expect(notesCell1).toHaveTextContent("Top-tier framework for desktop apps.");

      const notesCell2 = screen.getByTestId("library-notes-2");
      expect(notesCell2).toHaveTextContent("—");

      const notesCell3 = screen.getByTestId("library-notes-3");
      expect(notesCell3).toHaveTextContent("—");
    });

    it("allows full viewing of notes with newlines and safe text rendering", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const viewBtn = screen.getByTestId("library-notes-view-btn-1");
      fireEvent.click(viewBtn);

      const modal = screen.getByTestId("cell-viewer-modal");
      expect(modal).toBeInTheDocument();
      expect(modal).toHaveTextContent(/Top-tier framework for desktop apps/);
      expect(modal).toHaveTextContent(/Very reliable and fast/);
    });

    it("detail modal draft isolation: unsaved draft does NOT leak into table", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      // Open detail modal for fav 1
      const detailBtn = screen.getByTestId("row-detail-btn-1");
      fireEvent.click(detailBtn);

      await waitFor(() => {
        expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
      });

      // Edit notes in draft
      const notesInput = screen.getByTestId("detail-notes-textarea");
      fireEvent.change(notesInput, { target: { value: "UNSAVED_DRAFT_TEXT" } });

      // Close modal without saving (cancel/close)
      const closeBtn = screen.getByTestId("detail-close-btn");
      fireEvent.click(closeBtn);

      // Verify table row still displays original notes
      const notesCell1 = screen.getByTestId("library-notes-1");
      expect(notesCell1).not.toHaveTextContent("UNSAVED_DRAFT_TEXT");
      expect(notesCell1).toHaveTextContent("Top-tier framework for desktop apps.");
    });
  });

  describe("11.5 Excel-like Column Resizing & Reset (P1)", () => {
    it("renders resize handles with accessibility attributes on all resizable columns", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      // Selection column is NOT resizable
      expect(screen.queryByTestId("column-resize-handle-selection")).not.toBeInTheDocument();

      // All other 12 columns have resize handles
      const resizableCols = [
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
      ];

      for (const colId of resizableCols) {
        const handle = screen.getByTestId(`column-resize-handle-${colId}`);
        expect(handle).toBeInTheDocument();
        expect(handle).toHaveAttribute("role", "separator");
        expect(handle).toHaveAttribute("aria-orientation", "vertical");
        expect(handle).toHaveAttribute("aria-valuenow");
        expect(handle).toHaveAttribute("aria-valuemin");
        expect(handle).toHaveAttribute("aria-valuemax");
      }
    });

    it("resizes column via mouse pointer drag and persists to localStorage", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const handle = screen.getByTestId("column-resize-handle-description");
      expect(handle).toHaveAttribute("aria-valuenow", "280");

      // Pointer down
      fireEvent(handle, new MouseEvent("pointerdown", { bubbles: true, cancelable: true, clientX: 300 }));

      // Move pointer +50px
      fireEvent(document, new MouseEvent("pointermove", { bubbles: true, cancelable: true, clientX: 350 }));

      // Value updates in real time
      expect(screen.getByTestId("column-resize-handle-description")).toHaveAttribute("aria-valuenow", "330");

      // Pointer up
      fireEvent(document, new MouseEvent("pointerup", { bubbles: true, cancelable: true, clientX: 350 }));

      // Persisted to localStorage
      const saved = JSON.parse(localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY)!);
      expect(saved.description).toBe(330);
    });

    it("enforces min and max width constraints during drag", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const handle = screen.getByTestId("column-resize-handle-name");
      const nameCol = LIBRARY_COLUMNS.find((c) => c.id === "name")!;

      // Attempt to shrink below minWidth (140)
      fireEvent(handle, new MouseEvent("pointerdown", { bubbles: true, cancelable: true, clientX: 200 }));
      fireEvent(document, new MouseEvent("pointermove", { bubbles: true, cancelable: true, clientX: 50 })); // -150px
      expect(screen.getByTestId("column-resize-handle-name")).toHaveAttribute(
        "aria-valuenow",
        String(nameCol.minWidth)
      );
      fireEvent(document, new MouseEvent("pointerup", { bubbles: true, cancelable: true, clientX: 50 }));

      // Attempt to expand beyond maxWidth (400)
      const updatedHandle = screen.getByTestId("column-resize-handle-name");
      fireEvent(updatedHandle, new MouseEvent("pointerdown", { bubbles: true, cancelable: true, clientX: 200 }));
      fireEvent(document, new MouseEvent("pointermove", { bubbles: true, cancelable: true, clientX: 800 })); // +600px
      expect(screen.getByTestId("column-resize-handle-name")).toHaveAttribute(
        "aria-valuenow",
        String(nameCol.maxWidth)
      );
      fireEvent(document, new MouseEvent("pointerup", { bubbles: true, cancelable: true, clientX: 800 }));
    });

    it("resizes column using keyboard ArrowLeft and ArrowRight keys", async () => {
      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const handle = screen.getByTestId("column-resize-handle-category");
      expect(handle).toHaveAttribute("aria-valuenow", "110");

      // ArrowRight increases by 10px
      fireEvent.keyDown(handle, { key: "ArrowRight" });
      expect(handle).toHaveAttribute("aria-valuenow", "120");

      // ArrowLeft decreases by 10px
      fireEvent.keyDown(handle, { key: "ArrowLeft" });
      expect(handle).toHaveAttribute("aria-valuenow", "110");

      // Verify persisted
      const saved = JSON.parse(localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY)!);
      expect(saved.category).toBe(110);
    });

    it("Reset Column Widths button restores all columns to defaults", async () => {
      // Setup customized width in storage
      localStorage.setItem(
        LIBRARY_COLUMN_STORAGE_KEY,
        JSON.stringify({
          name: 300,
          description: 500,
        })
      );

      renderWithProviders(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      const descHandle = screen.getByTestId("column-resize-handle-description");
      expect(descHandle).toHaveAttribute("aria-valuenow", "500");

      // Click reset button
      const resetBtn = screen.getByTestId("library-reset-column-widths-btn");
      fireEvent.click(resetBtn);

      // Restored to default width (280)
      expect(descHandle).toHaveAttribute("aria-valuenow", "280");
      expect(localStorage.getItem(LIBRARY_COLUMN_STORAGE_KEY)).toBeNull();
    });
  });

  describe("11.6 i18n Headers & Accessibility Across Three Locales", () => {
    it("renders all table headers and reset button in zh-Hans", async () => {
      renderWithProviders(<LibraryPage />, "zh-Hans");

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      expect(screen.getByTestId("library-th-name")).toHaveTextContent("名称");
      expect(screen.getByTestId("library-th-category")).toHaveTextContent("分类");
      expect(screen.getByTestId("library-th-description")).toHaveTextContent("项目简介");
      expect(screen.getByTestId("library-th-userTags")).toHaveTextContent("用户标签");
      expect(screen.getByTestId("library-th-notes")).toHaveTextContent("个人备注");
      expect(screen.getByTestId("library-th-stars")).toHaveTextContent("星标");
      expect(screen.getByTestId("library-th-createdAt")).toHaveTextContent("创建日期");
      expect(screen.getByTestId("library-th-lastActive")).toHaveTextContent("最后活跃");
      expect(screen.getByTestId("library-th-rating")).toHaveTextContent("评级");
      expect(screen.getByTestId("library-th-favoritedAt")).toHaveTextContent("收藏日期");
      expect(screen.getByTestId("library-th-lastRefresh")).toHaveTextContent("最后刷新");
      expect(screen.getByTestId("library-th-actions")).toHaveTextContent("操作");

      expect(screen.getByTestId("library-reset-column-widths-btn")).toHaveTextContent("恢复默认列宽");
    });

    it("renders all table headers and reset button in zh-Hant", async () => {
      renderWithProviders(<LibraryPage />, "zh-Hant");

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      expect(screen.getByTestId("library-th-name")).toHaveTextContent("名稱");
      expect(screen.getByTestId("library-th-category")).toHaveTextContent("分類");
      expect(screen.getByTestId("library-th-description")).toHaveTextContent("專案簡介");
      expect(screen.getByTestId("library-th-userTags")).toHaveTextContent("使用者標籤");
      expect(screen.getByTestId("library-th-notes")).toHaveTextContent("個人備註");
      expect(screen.getByTestId("library-th-stars")).toHaveTextContent("星標");
      expect(screen.getByTestId("library-th-createdAt")).toHaveTextContent("建立日期");
      expect(screen.getByTestId("library-th-lastActive")).toHaveTextContent("最後活躍");
      expect(screen.getByTestId("library-th-rating")).toHaveTextContent("評級");
      expect(screen.getByTestId("library-th-favoritedAt")).toHaveTextContent("收藏日期");
      expect(screen.getByTestId("library-th-lastRefresh")).toHaveTextContent("最後重新整理");
      expect(screen.getByTestId("library-th-actions")).toHaveTextContent("操作");

      expect(screen.getByTestId("library-reset-column-widths-btn")).toHaveTextContent("還原預設欄寬");
    });

    it("renders all table headers and reset button in en", async () => {
      renderWithProviders(<LibraryPage />, "en");

      await waitFor(() => {
        expect(screen.getByTestId("library-table")).toBeInTheDocument();
      });

      expect(screen.getByTestId("library-th-name")).toHaveTextContent("Name");
      expect(screen.getByTestId("library-th-category")).toHaveTextContent("Category");
      expect(screen.getByTestId("library-th-description")).toHaveTextContent("Description");
      expect(screen.getByTestId("library-th-userTags")).toHaveTextContent("User Tags");
      expect(screen.getByTestId("library-th-notes")).toHaveTextContent("Notes");
      expect(screen.getByTestId("library-th-stars")).toHaveTextContent("Stars");
      expect(screen.getByTestId("library-th-createdAt")).toHaveTextContent("Created");
      expect(screen.getByTestId("library-th-lastActive")).toHaveTextContent("Last Active");
      expect(screen.getByTestId("library-th-rating")).toHaveTextContent("Rating");
      expect(screen.getByTestId("library-th-favoritedAt")).toHaveTextContent("Favorited");
      expect(screen.getByTestId("library-th-lastRefresh")).toHaveTextContent("Last Refresh");
      expect(screen.getByTestId("library-th-actions")).toHaveTextContent("Actions");

      expect(screen.getByTestId("library-reset-column-widths-btn")).toHaveTextContent("Reset Column Widths");
    });
  });
});
