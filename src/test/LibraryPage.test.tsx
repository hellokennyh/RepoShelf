import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { LibraryPage } from "../pages/LibraryPage";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";

vi.mock("../services/tauri", () => ({
  listFavorites: vi.fn(),
  listDistinctTags: vi.fn(),
  openGitHubUrl: vi.fn(),
  refreshFavoritesBatch: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
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
];

const mockTags = ["rust", "desktop", "core-tool", "database", "async"];

describe("LibraryPage — Phase 4 Verification Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.listFavorites).mockResolvedValue(mockFavorites);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue(mockTags);
  });

  it("1. renders LibraryPage table with columns, search, and filters", async () => {
    render(<LibraryPage />);

    expect(screen.getByText("Local Repository Library")).toBeInTheDocument();
    expect(screen.getByTestId("library-search-input")).toBeInTheDocument();
    expect(screen.getByTestId("library-category-filter")).toBeInTheDocument();
    expect(screen.getByTestId("library-tag-filter")).toBeInTheDocument();
    expect(screen.getByTestId("library-rating-filter")).toBeInTheDocument();

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    expect(screen.getByText("tauri")).toBeInTheDocument();
    expect(screen.getByText("sqlx")).toBeInTheDocument();
    expect(screen.getAllByText("Developer Tools").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("Data & Database").length).toBeGreaterThanOrEqual(1);
  });

  it("2. renders empty state when no favorites exist", async () => {
    vi.mocked(tauriService.listFavorites).mockResolvedValueOnce([]);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValueOnce([]);

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-empty-state")).toBeInTheDocument();
      expect(screen.getByText("还没有收藏项目。")).toBeInTheDocument();
    });
  });

  it("3. handles local multi-field search", async () => {
    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    const searchInput = screen.getByTestId("library-search-input");
    fireEvent.change(searchInput, { target: { value: "sql" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          search: "sql",
        })
      );
    });
  });

  it("4. handles category and tag filters", async () => {
    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    const categorySelect = screen.getByTestId("library-category-filter");
    fireEvent.change(categorySelect, { target: { value: "Developer Tools" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          category: "Developer Tools",
        })
      );
    });

    const tagSelect = screen.getByTestId("library-tag-filter");
    fireEvent.change(tagSelect, { target: { value: "rust" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          tag: "rust",
        })
      );
    });
  });

  it("5. handles safe allowlisted sorting selection", async () => {
    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    const sortSelect = screen.getByTestId("library-sort-select");
    fireEvent.change(sortSelect, { target: { value: "stars_desc" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          sort_field: "stars",
          sort_order: "desc",
        })
      );
    });

    fireEvent.change(sortSelect, { target: { value: "name_asc" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          sort_field: "name",
          sort_order: "asc",
        })
      );
    });
  });

  it("5b. handles rating filter selection", async () => {
    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    const ratingSelect = screen.getByTestId("library-rating-filter");
    fireEvent.change(ratingSelect, { target: { value: "5" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({
          rating: "5",
        })
      );
    });
  });

  it("6. supports row selection and batch refresh selected items", async () => {
    vi.mocked(tauriService.refreshFavoritesBatch).mockResolvedValueOnce({
      total: 1,
      succeeded: 1,
      failed: 0,
      rate_limited: 0,
      not_found: 0,
      results: [
        {
          favorite_id: 1,
          status: "succeeded",
        },
      ],
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    // Select row 1
    const checkbox1 = screen.getByTestId("library-row-checkbox-1");
    fireEvent.click(checkbox1);

    const refreshSelectedBtn = screen.getByTestId("library-refresh-selected-btn");
    expect(refreshSelectedBtn).toBeEnabled();

    fireEvent.click(refreshSelectedBtn);

    await waitFor(() => {
      expect(tauriService.refreshFavoritesBatch).toHaveBeenCalledWith([1]);
    });
  });

  it("6b. supports Select All checkbox and batch refresh", async () => {
    vi.mocked(tauriService.refreshFavoritesBatch).mockResolvedValueOnce({
      total: 2,
      succeeded: 2,
      failed: 0,
      rate_limited: 0,
      not_found: 0,
      results: [
        { favorite_id: 1, status: "succeeded" },
        { favorite_id: 2, status: "succeeded" },
      ],
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    const selectAll = screen.getByTestId("library-select-all-checkbox");
    fireEvent.click(selectAll);

    const refreshSelectedBtn = screen.getByTestId("library-refresh-selected-btn");
    expect(refreshSelectedBtn).toHaveTextContent("更新选中 (2)");

    fireEvent.click(refreshSelectedBtn);

    await waitFor(() => {
      expect(tauriService.refreshFavoritesBatch).toHaveBeenCalledWith([1, 2]);
    });
  });

  it("6c. Update All triggers batch refresh for all items and displays result modal with status counts", async () => {
    vi.mocked(tauriService.refreshFavoritesBatch).mockResolvedValueOnce({
      total: 2,
      succeeded: 1,
      failed: 1,
      rate_limited: 0,
      not_found: 0,
      results: [
        { favorite_id: 1, status: "succeeded" },
        { favorite_id: 2, status: "failed", message: "Network timeout" },
      ],
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    const refreshAllBtn = screen.getByTestId("library-refresh-all-btn");
    fireEvent.click(refreshAllBtn);

    await waitFor(() => {
      expect(tauriService.refreshFavoritesBatch).toHaveBeenCalledWith([1, 2]);
      expect(screen.getByTestId("batch-result-modal")).toBeInTheDocument();
      expect(screen.getByTestId("batch-result-title")).toHaveTextContent("批量刷新元数据结果");
      expect(screen.getByTestId("batch-ref-succeeded-count")).toHaveTextContent("1");
      expect(screen.getByTestId("batch-ref-failed-count")).toHaveTextContent("1");
      expect(screen.getByTestId("batch-ref-ratelimit-count")).toHaveTextContent("0");
      expect(screen.getByTestId("batch-ref-notfound-count")).toHaveTextContent("0");
    });
  });

  it("7. opens favorite detail modal and edits user-owned metadata", async () => {
    const updatedFav: FavoriteItem = {
      ...mockFavorites[0],
      category: "Productivity",
      rating: 5,
      notes: "Updated notes",
      tags: [
        ...mockFavorites[0].tags,
        { id: 10, favorite_id: 1, tag: "superb", source: "user" },
      ],
    };

    vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValueOnce(
      updatedFav
    );

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    // Open detail modal by clicking detail button
    const detailBtn = screen.getByTestId("row-detail-btn-1");
    fireEvent.click(detailBtn);

    await waitFor(() => {
      expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
    });

    // Edit notes
    const notesInput = screen.getByTestId("detail-notes-textarea");
    fireEvent.change(notesInput, { target: { value: "Updated notes" } });

    // Add user tag
    const tagInput = screen.getByTestId("detail-add-tag-input");
    const addTagBtn = screen.getByTestId("detail-add-tag-btn");
    fireEvent.change(tagInput, { target: { value: "superb" } });
    fireEvent.click(addTagBtn);

    // Save
    const saveBtn = screen.getByTestId("detail-save-btn");
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalledWith(
        expect.objectContaining({
          favorite_id: 1,
          notes: "Updated notes",
          user_tags: expect.arrayContaining(["core-tool", "superb"]),
        })
      );
    });
  });

  it("7b. FavoriteDetailModal tag isolation: deleting user tag does not delete github topics", async () => {
    vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValueOnce({
      ...mockFavorites[0],
      tags: mockFavorites[0].tags.filter((t) => t.source === "github"),
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("row-detail-btn-1"));

    await waitFor(() => {
      expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
    });

    const modal = screen.getByTestId("favorite-detail-modal");

    // Verify GitHub read-only topics section is rendered in modal with 2 github topic badges
    expect(within(modal).getByText("GitHub Topics & 元数据标签 (只读)")).toBeInTheDocument();
    expect(within(modal).getAllByText("(github)")).toHaveLength(2);

    // Verify user tag is rendered with remove button
    expect(screen.getByTestId("user-tag-core-tool")).toBeInTheDocument();
    const removeBtn = screen.getByTestId("remove-tag-core-tool");

    // Remove user tag
    fireEvent.click(removeBtn);
    expect(screen.queryByTestId("user-tag-core-tool")).not.toBeInTheDocument();

    // GitHub topics must still be present in the modal
    expect(within(modal).getAllByText("(github)")).toHaveLength(2);

    // Save changes
    fireEvent.click(screen.getByTestId("detail-save-btn"));

    await waitFor(() => {
      expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalledWith(
        expect.objectContaining({
          favorite_id: 1,
          user_tags: [],
        })
      );
    });
  });

  it("8. executes single metadata refresh from detail modal", async () => {
    vi.mocked(tauriService.refreshFavorite).mockResolvedValueOnce({
      status: "succeeded",
      favorite_id: 1,
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("row-detail-btn-1")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("row-detail-btn-1"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-refresh-btn")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("detail-refresh-btn"));

    await waitFor(() => {
      expect(tauriService.refreshFavorite).toHaveBeenCalledWith(1);
    });
  });

  it("9. cascades deletion of favorite with confirmation", async () => {
    vi.mocked(tauriService.deleteFavorite).mockResolvedValueOnce(true);

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("row-detail-btn-1")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("row-detail-btn-1"));

    await waitFor(() => {
      expect(screen.getByTestId("detail-delete-btn")).toBeInTheDocument();
    });

    // Click delete to reveal confirmation
    fireEvent.click(screen.getByTestId("detail-delete-btn"));

    const confirmBtn = screen.getByTestId("confirm-delete-btn");
    expect(confirmBtn).toBeInTheDocument();

    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(tauriService.deleteFavorite).toHaveBeenCalledWith(1);
    });
  });

  it("10. opens GitHub URL safely", async () => {
    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    const openLink = screen.getByTestId("row-github-btn-1");
    fireEvent.click(openLink);

    expect(tauriService.openGitHubUrl).toHaveBeenCalledWith(
      "https://github.com/tauri-apps/tauri"
    );
  });
});
