import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { LibraryPage } from "../pages/LibraryPage";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import * as tauriService from "../services/tauri";
import { BatchRefreshProgress, BatchRefreshResult, FavoriteItem } from "../types/library";

vi.mock("../services/tauri", () => ({
  listFavorites: vi.fn(),
  listDistinctTags: vi.fn(),
  openGitHubUrl: vi.fn(),
  refreshFavoritesBatch: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
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
    description: "Build smaller, faster, and more secure desktop apps.",
    category: "Developer Tools",
    stars: 85000,
    license: "Apache-2.0",
    pushed_at: "2026-03-01T12:00:00Z",
    rating: 5,
    notes: "Essential framework for desktop.",
    favorited_at: "2026-03-01T10:00:00Z",
    metadata_refreshed_at: "2026-03-01T10:00:00Z",
    tags: [
      { id: 1, favorite_id: 1, tag: "rust", source: "github" },
      { id: 2, favorite_id: 1, tag: "desktop", source: "github" },
      { id: 3, favorite_id: 1, tag: "core-tool", source: "user" },
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
    rating: 4,
    notes: "Pure Rust SQL toolkit.",
    favorited_at: "2026-03-02T10:00:00Z",
    metadata_refreshed_at: "2026-03-02T10:00:00Z",
    tags: [
      { id: 4, favorite_id: 2, tag: "database", source: "github" },
      { id: 5, favorite_id: 2, tag: "async", source: "user" },
    ],
  },
  {
    id: 3,
    github_repo_id: 103,
    github_url: "https://github.com/tokio-rs/tokio",
    name: "tokio",
    description: "A runtime for writing reliable asynchronous applications with Rust.",
    category: "Libraries & Frameworks",
    stars: 28000,
    license: "MIT",
    pushed_at: "2026-03-03T10:00:00Z",
    rating: 5,
    notes: "Async runtime.",
    favorited_at: "2026-03-03T10:00:00Z",
    metadata_refreshed_at: "2026-03-03T10:00:00Z",
    tags: [
      { id: 6, favorite_id: 3, tag: "async", source: "github" },
      { id: 7, favorite_id: 3, tag: "network", source: "github" },
    ],
  },
];

const mockTags = ["rust", "desktop", "core-tool", "database", "async", "network"];

describe("Phase 7 Frontend Acceptance Test Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.listFavorites).mockResolvedValue(mockFavorites);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue(mockTags);
  });

  // ==========================================
  // Section 1: Dynamic Batch Refresh Progress (P7-GAP-01)
  // ==========================================
  describe("P7-GAP-01: Dynamic Batch Refresh Progress", () => {
    it("P7-FE-PROGRESS-01 to 06: progress displays 0/3, increments dynamically to 1/3, 2/3, 3/3, shows summary modal, and resets", async () => {
      let progressCallback: ((prog: BatchRefreshProgress) => void) | null = null;
      let resolveBatch: ((res: BatchRefreshResult) => void) | null = null;

      vi.mocked(tauriService.refreshFavoritesBatch).mockImplementation(
        (idsWithProgress) => {
          progressCallback =
            (idsWithProgress as number[] & { onProgress?: (prog: BatchRefreshProgress) => void }).onProgress ?? null;
          return new Promise((resolve) => {
            resolveBatch = resolve;
          });
        }
      );

      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-3")).toBeInTheDocument();
      });

      // Trigger Refresh All (total 3 items)
      const refreshAllBtn = screen.getByTestId("library-refresh-all-btn");
      fireEvent.click(refreshAllBtn);

      // P7-FE-PROGRESS-01: Starting a three-item batch displays 0 / 3
      await waitFor(() => {
        expect(screen.getByTestId("batch-refresh-progress-indicator")).toBeInTheDocument();
        expect(screen.getByTestId("batch-refresh-progress-indicator")).toHaveTextContent("正在刷新 0 / 3...");
      });

      // P7-FE-PROGRESS-02: After first progress callback: 1 / 3
      expect(progressCallback).not.toBeNull();
      act(() => {
        progressCallback!({ completed: 1, total: 3 });
      });
      expect(screen.getByTestId("batch-refresh-progress-indicator")).toHaveTextContent("正在刷新 1 / 3...");

      // P7-FE-PROGRESS-03: Then 2 / 3
      act(() => {
        progressCallback!({ completed: 2, total: 3 });
      });
      expect(screen.getByTestId("batch-refresh-progress-indicator")).toHaveTextContent("正在刷新 2 / 3...");

      // P7-FE-PROGRESS-04: Finally 3 / 3
      act(() => {
        progressCallback!({ completed: 3, total: 3 });
      });
      expect(screen.getByTestId("batch-refresh-progress-indicator")).toHaveTextContent("正在刷新 3 / 3...");

      // Resolve the batch operation
      act(() => {
        resolveBatch!({
          total: 3,
          succeeded: 2,
          failed: 0,
          rate_limited: 1,
          not_found: 0,
          results: [
            { favorite_id: 1, status: "succeeded" },
            { favorite_id: 2, status: "succeeded" },
            { favorite_id: 3, status: "rate_limited", message: "Rate limit reached" },
          ],
        });
      });

      // P7-FE-PROGRESS-05: Final BatchResultModal still shows existing summary
      await waitFor(() => {
        expect(screen.getByTestId("batch-result-modal")).toBeInTheDocument();
        expect(screen.getByTestId("batch-ref-succeeded-count")).toHaveTextContent("2");
        expect(screen.getByTestId("batch-ref-failed-count")).toHaveTextContent("0");
        expect(screen.getByTestId("batch-ref-ratelimit-count")).toHaveTextContent("1");
        expect(screen.getByTestId("batch-ref-notfound-count")).toHaveTextContent("0");
      });

      // P7-FE-PROGRESS-06: Progress indicator is reset after completion
      expect(screen.queryByTestId("batch-refresh-progress-indicator")).not.toBeInTheDocument();
    });

    it("P7-FE-PROGRESS-07: a failure does not leave batch buttons permanently disabled", async () => {
      vi.mocked(tauriService.refreshFavoritesBatch).mockRejectedValueOnce({
        code: "IPC_ERROR",
        message: "Failed to communicate with GitHub backend",
      });

      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      const refreshAllBtn = screen.getByTestId("library-refresh-all-btn");
      expect(refreshAllBtn).toBeEnabled();

      fireEvent.click(refreshAllBtn);

      await waitFor(() => {
        expect(screen.getByTestId("library-operation-error")).toBeInTheDocument();
      });

      // Buttons are not permanently disabled
      expect(refreshAllBtn).toBeEnabled();
      expect(screen.queryByTestId("batch-refresh-progress-indicator")).not.toBeInTheDocument();
    });
  });

  // ==========================================
  // Section 2: Safe IPC Error Extraction & In-App Banners (P7-GAP-02)
  // ==========================================
  describe("P7-GAP-02: Safe IPC Error Extraction & Alert Removal", () => {
    it("P7-FE-ERR-01 & P7-FE-ERR-02: batch failure uses in-app banner, does not use alert, and extracts .message from plain Tauri error object", async () => {
      const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});

      vi.mocked(tauriService.refreshFavoritesBatch).mockRejectedValueOnce({
        code: "NETWORK_TIMEOUT",
        message: "Network connection to GitHub timed out",
      });

      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      const refreshAllBtn = screen.getByTestId("library-refresh-all-btn");
      fireEvent.click(refreshAllBtn);

      // In-app error banner appears with the extracted message
      await waitFor(() => {
        const errorBanner = screen.getByTestId("library-operation-error");
        expect(errorBanner).toBeInTheDocument();
        expect(errorBanner).toHaveTextContent("Network connection to GitHub timed out");
        expect(errorBanner).not.toHaveTextContent("[object Object]");
      });

      // Window alert was NEVER called
      expect(alertSpy).not.toHaveBeenCalled();
      alertSpy.mockRestore();
    });

    it("P7-FE-ERR-03: Library remains usable after recoverable batch operation error", async () => {
      vi.mocked(tauriService.refreshFavoritesBatch).mockRejectedValueOnce({
        code: "INTERNAL",
        message: "Temporary error",
      });

      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("library-refresh-all-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("library-operation-error")).toBeInTheDocument();
      });

      // User retains access to table rows, filters, search, and can dismiss the error
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
      expect(screen.getByTestId("library-search-input")).toBeEnabled();
      expect(screen.getByTestId("library-category-filter")).toBeEnabled();

      const closeErrorBtn = screen.getByTestId("library-operation-error-close");
      fireEvent.click(closeErrorBtn);

      expect(screen.queryByTestId("library-operation-error")).not.toBeInTheDocument();
    });

    it("P7-FE-ERR-04 & P7-FE-ERR-05: FavoriteDetailModal safely formats plain object error and preserves unsaved edits and modal state", async () => {
      vi.mocked(tauriService.updateFavoriteUserMetadata).mockRejectedValueOnce({
        code: "DB_LOCK",
        message: "Database busy, please retry",
      });

      const onUpdated = vi.fn();
      const onDeleted = vi.fn();
      const onClose = vi.fn();

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFavorites[0]}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      // Make unsaved edits to category, notes, and user tags
      const categorySelect = screen.getByTestId("detail-category-select");
      fireEvent.change(categorySelect, { target: { value: "Productivity" } });

      const notesTextarea = screen.getByTestId("detail-notes-textarea");
      fireEvent.change(notesTextarea, { target: { value: "Unsaved drafted notes" } });

      const tagInput = screen.getByTestId("detail-add-tag-input");
      fireEvent.change(tagInput, { target: { value: "new-tag" } });
      fireEvent.click(screen.getByTestId("detail-add-tag-btn"));

      // Trigger Save
      const saveBtn = screen.getByTestId("detail-save-btn");
      fireEvent.click(saveBtn);

      // Verify safe message displayed in banner without [object Object]
      await waitFor(() => {
        const feedback = screen.getByTestId("detail-feedback-banner");
        expect(feedback).toBeInTheDocument();
        expect(feedback).toHaveTextContent("保存失败: Database busy, please retry");
        expect(feedback).not.toHaveTextContent("[object Object]");
      });

      // Modal remains open
      expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
      expect(onClose).not.toHaveBeenCalled();

      // Unsaved edits remain intact
      expect(categorySelect).toHaveValue("Productivity");
      expect(notesTextarea).toHaveValue("Unsaved drafted notes");
      expect(screen.getByTestId("user-tag-new-tag")).toBeInTheDocument();
    });
  });

  // ==========================================
  // Section 3: Library Filtered-Empty State (P7-GAP-03)
  // ==========================================
  describe("P7-GAP-03: Library Filtered Empty State Distinction", () => {
    it("P7-FE-EMPTY-01: Database truly empty shows true empty-library state", async () => {
      vi.mocked(tauriService.listFavorites).mockResolvedValue([]);
      vi.mocked(tauriService.listDistinctTags).mockResolvedValue([]);

      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-empty-state")).toBeInTheDocument();
        expect(screen.getByText("还没有收藏项目。")).toBeInTheDocument();
        expect(screen.getByText("可以从 Trending、Search 或 Add Repository 添加。")).toBeInTheDocument();
      });

      expect(screen.queryByTestId("library-filter-empty-state")).not.toBeInTheDocument();
    });

    it("P7-FE-EMPTY-02 & P7-FE-EMPTY-03: Library has Favorites but active search yields zero shows filtered-empty state and NOT true empty", async () => {
      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      // Mock search query returning zero results
      vi.mocked(tauriService.listFavorites).mockResolvedValueOnce([]);

      const searchInput = screen.getByTestId("library-search-input");
      fireEvent.change(searchInput, { target: { value: "nonexistent-query-xyz" } });

      await waitFor(() => {
        expect(screen.getByTestId("library-filter-empty-state")).toBeInTheDocument();
      });

      // P7-FE-EMPTY-03: Filtered-empty state does NOT say "还没有收藏项目。"
      expect(screen.getByText("未找到符合筛选条件的收藏项目。")).toBeInTheDocument();
      expect(screen.getByText("请尝试清除搜索词或调整筛选条件。")).toBeInTheDocument();
      expect(screen.queryByText("还没有收藏项目。")).not.toBeInTheDocument();
      expect(screen.queryByTestId("library-empty-state")).not.toBeInTheDocument();
    });

    it("P7-FE-EMPTY-04 & P7-FE-EMPTY-05: Clear Filters button resets active criteria and rows become visible again", async () => {
      render(<LibraryPage />);

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      // Set category and search filter yielding 0 results
      vi.mocked(tauriService.listFavorites).mockResolvedValue([]);

      const searchInput = screen.getByTestId("library-search-input");
      fireEvent.change(searchInput, { target: { value: "xyz" } });

      const categorySelect = screen.getByTestId("library-category-filter");
      fireEvent.change(categorySelect, { target: { value: "Games" } });

      await waitFor(() => {
        expect(screen.getByTestId("library-filter-empty-state")).toBeInTheDocument();
      });

      const clearBtn = screen.getByTestId("library-clear-filters-btn");
      expect(clearBtn).toBeInTheDocument();

      // Restore favorites on clear
      vi.mocked(tauriService.listFavorites).mockResolvedValue(mockFavorites);
      fireEvent.click(clearBtn);

      // P7-FE-EMPTY-04: Criteria reset
      await waitFor(() => {
        expect(searchInput).toHaveValue("");
        expect(categorySelect).toHaveValue("all");
      });

      // P7-FE-EMPTY-05: Rows become visible again
      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-3")).toBeInTheDocument();
      });

      expect(screen.queryByTestId("library-filter-empty-state")).not.toBeInTheDocument();
    });

    it("P7-REMED-01 (Scenario C): deleting the last favorite while an active filter is applied renders true-empty state", async () => {
      // 1 favorite initially
      const singleFav = [mockFavorites[0]];
      vi.mocked(tauriService.listFavorites).mockResolvedValue(singleFav);
      vi.mocked(tauriService.deleteFavorite).mockResolvedValue(true);

      render(<LibraryPage />);

      // Initial load: 1 favorite, totalCountRef = 1
      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      // Apply a filter that matches this single favorite (search "tauri")
      const searchInput = screen.getByTestId("library-search-input");
      fireEvent.change(searchInput, { target: { value: "tauri" } });

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      });

      // Open detail modal and delete
      fireEvent.click(screen.getByTestId("row-detail-btn-1"));

      await waitFor(() => {
        expect(screen.getByTestId("detail-delete-btn")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("detail-delete-btn"));
      const confirmBtn = screen.getByTestId("confirm-delete-btn");
      fireEvent.click(confirmBtn);

      // Successfully deleted: total is now 0
      await waitFor(() => {
        expect(tauriService.deleteFavorite).toHaveBeenCalledWith(1);
        expect(screen.getByTestId("library-empty-state")).toBeInTheDocument();
      });

      expect(screen.getByText("还没有收藏项目。")).toBeInTheDocument();
      expect(screen.queryByTestId("library-filter-empty-state")).not.toBeInTheDocument();
    });

    it("P7-REMED-02 (Scenario D): deleting last visible favorite under active filter when other favorites remain renders filtered-empty state and clears properly", async () => {
      // 2 favorites: Repo A (id 1, Developer Tools) and Repo B (id 2, Design & UI)
      const repoA = { ...mockFavorites[0], id: 1, name: "repo-a", category: "Developer Tools" };
      const repoB = { ...mockFavorites[1], id: 2, name: "repo-b", category: "Design & UI" };

      vi.mocked(tauriService.listFavorites).mockResolvedValue([repoA, repoB]);
      vi.mocked(tauriService.deleteFavorite).mockResolvedValue(true);

      render(<LibraryPage />);

      // Initial load: 2 favorites, totalCountRef = 2
      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
      });

      // Activate category filter "Developer Tools", which matches only Repo A
      vi.mocked(tauriService.listFavorites).mockResolvedValue([repoA]);
      const categorySelect = screen.getByTestId("library-category-filter");
      fireEvent.change(categorySelect, { target: { value: "Developer Tools" } });

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.queryByTestId("library-row-2")).not.toBeInTheDocument();
      });

      // Delete Repo A
      fireEvent.click(screen.getByTestId("row-detail-btn-1"));

      await waitFor(() => {
        expect(screen.getByTestId("detail-delete-btn")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("detail-delete-btn"));
      fireEvent.click(screen.getByTestId("confirm-delete-btn"));

      // Defect verification: remainingTotal is 1 (Repo B), visible is 0, hasActiveFilters is true
      // Must render filtered_empty, NOT empty!
      await waitFor(() => {
        expect(tauriService.deleteFavorite).toHaveBeenCalledWith(1);
        expect(screen.getByTestId("library-filter-empty-state")).toBeInTheDocument();
      });

      expect(screen.getByText("未找到符合筛选条件的收藏项目。")).toBeInTheDocument();
      expect(screen.queryByTestId("library-empty-state")).not.toBeInTheDocument();
      expect(screen.getByTestId("library-clear-filters-btn")).toBeInTheDocument();

      // Clear Filters continuation:
      vi.mocked(tauriService.listFavorites).mockResolvedValue([repoB]);
      fireEvent.click(screen.getByTestId("library-clear-filters-btn"));

      await waitFor(() => {
        expect(categorySelect).toHaveValue("all");
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
        expect(screen.queryByTestId("library-filter-empty-state")).not.toBeInTheDocument();
      });
    });

    it("P7-REMED-03: deleting one of multiple visible favorites under active filter keeps remaining visible rows and success status", async () => {
      // 3 favorites: Repo A, Repo B, Repo C
      const repoA = { ...mockFavorites[0], id: 1, name: "repo-a", category: "Developer Tools" };
      const repoB = { ...mockFavorites[1], id: 2, name: "repo-b", category: "Developer Tools" };
      const repoC = { ...mockFavorites[2], id: 3, name: "repo-c", category: "Design & UI" };

      vi.mocked(tauriService.listFavorites).mockResolvedValue([repoA, repoB, repoC]);
      vi.mocked(tauriService.deleteFavorite).mockResolvedValue(true);

      render(<LibraryPage />);

      // Initial load: 3 favorites, totalCountRef = 3
      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-3")).toBeInTheDocument();
      });

      // Filter by "Developer Tools" -> Repo A and Repo B
      vi.mocked(tauriService.listFavorites).mockResolvedValue([repoA, repoB]);
      const categorySelect = screen.getByTestId("library-category-filter");
      fireEvent.change(categorySelect, { target: { value: "Developer Tools" } });

      await waitFor(() => {
        expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
        expect(screen.queryByTestId("library-row-3")).not.toBeInTheDocument();
      });

      // Delete Repo A
      fireEvent.click(screen.getByTestId("row-detail-btn-1"));

      await waitFor(() => {
        expect(screen.getByTestId("detail-delete-btn")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("detail-delete-btn"));
      fireEvent.click(screen.getByTestId("confirm-delete-btn"));

      // Status remains success; Repo B remains visible
      await waitFor(() => {
        expect(tauriService.deleteFavorite).toHaveBeenCalledWith(1);
        expect(screen.queryByTestId("library-row-1")).not.toBeInTheDocument();
        expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
      });

      expect(screen.queryByTestId("library-empty-state")).not.toBeInTheDocument();
      expect(screen.queryByTestId("library-filter-empty-state")).not.toBeInTheDocument();
    });
  });
});
