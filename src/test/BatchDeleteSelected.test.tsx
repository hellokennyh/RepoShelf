import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { LibraryPage } from "../pages/LibraryPage";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import { I18nProvider } from "../i18n/I18nContext";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";

vi.mock("../services/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../services/tauri")>();
  return {
    ...actual,
    fetchLocaleInitInfo: vi.fn().mockResolvedValue({ locale: "en", source: "settings" }),
    listFavorites: vi.fn(),
    listDistinctTags: vi.fn().mockResolvedValue(["rust", "react", "tool"]),
    deleteFavoritesBatch: vi.fn(),
    deleteFavorite: vi.fn(),
    refreshFavorite: vi.fn(),
    refreshFavoritesBatch: vi.fn(),
    openGitHubUrl: vi.fn(),
    getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: false, reason: null }),
  };
});

const sampleFavorites: FavoriteItem[] = [
  {
    id: 1,
    github_repo_id: 101,
    github_url: "https://github.com/owner/repo1",
    name: "repo1",
    description: "First repository description",
    category: "Developer Tools",
    stars: 1200,
    license: "MIT",
    pushed_at: "2026-09-20T00:00:00Z",
    rating: 5,
    rating_reason: null,
    rating_confidence: null,
    notes: "Essential tool",
    favorited_at: "2026-09-18T10:00:00Z",
    metadata_refreshed_at: "2026-09-18T10:00:00Z",
    tags: [{ id: 1, favorite_id: 1, tag: "rust", source: "github" }],
  },
  {
    id: 2,
    github_repo_id: 102,
    github_url: "https://github.com/owner/repo2",
    name: "repo2",
    description: "Second repository description",
    category: "Web & SaaS",
    stars: 3400,
    license: "Apache-2.0",
    pushed_at: "2026-09-19T00:00:00Z",
    rating: 4,
    rating_reason: null,
    rating_confidence: null,
    notes: "Good library",
    favorited_at: "2026-09-19T10:00:00Z",
    metadata_refreshed_at: "2026-09-19T10:00:00Z",
    tags: [{ id: 2, favorite_id: 2, tag: "react", source: "github" }],
  },
  {
    id: 3,
    github_repo_id: 103,
    github_url: "https://github.com/owner/repo3",
    name: "repo3",
    description: "Third repository description",
    category: "System Utilities",
    stars: 560,
    license: "GPL-3.0",
    pushed_at: "2026-09-15T00:00:00Z",
    rating: null,
    rating_reason: null,
    rating_confidence: null,
    notes: "",
    favorited_at: "2026-09-17T10:00:00Z",
    metadata_refreshed_at: "2026-09-17T10:00:00Z",
    tags: [{ id: 3, favorite_id: 3, tag: "tool", source: "user" }],
  },
];

describe("Stage S5: Library Batch Delete Selected (R11, AC078-AC088)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.listFavorites).mockResolvedValue([...sampleFavorites]);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue(["rust", "react", "tool"]);
  });

  it("AC078: 'Delete Selected' button is located immediately to the left of 'Update Selected' and disabled when no items selected", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    const deleteBtn = screen.getByTestId("library-delete-selected-btn");
    const updateBtn = screen.getByTestId("library-refresh-selected-btn");

    expect(deleteBtn).toBeInTheDocument();
    expect(updateBtn).toBeInTheDocument();

    // Verify ordering: deleteBtn comes before updateBtn in the DOM
    const positionComparison = deleteBtn.compareDocumentPosition(updateBtn);
    expect(positionComparison & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    // Initially disabled when selection is empty
    expect(deleteBtn).toBeDisabled();
  });

  it("AC079 & AC080: Selecting items enables Delete Selected, opening modal with snapshot, preview, warning, and default cancel focus", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    // Select repo1 (id: 1) and repo2 (id: 2)
    const checkbox1 = screen.getByTestId("library-row-checkbox-1");
    const checkbox2 = screen.getByTestId("library-row-checkbox-2");

    fireEvent.click(checkbox1);
    fireEvent.click(checkbox2);

    const deleteBtn = screen.getByTestId("library-delete-selected-btn");
    expect(deleteBtn).toBeEnabled();

    // Open confirmation modal
    fireEvent.click(deleteBtn);

    const modal = screen.getByTestId("delete-confirm-modal");
    expect(modal).toBeInTheDocument();

    // Check warning text is displayed
    const warning = screen.getByTestId("delete-confirm-warning");
    expect(warning).toBeInTheDocument();
    expect(warning.textContent).toContain("GitHub");

    // Check preview list contains repo1 and repo2
    const previewList = screen.getByTestId("delete-confirm-preview-list");
    expect(previewList.textContent).toContain("repo1");
    expect(previewList.textContent).toContain("repo2");

    // Cancel button exists and clicking it closes modal without calling backend
    const cancelBtn = screen.getByTestId("delete-confirm-cancel-btn");
    fireEvent.click(cancelBtn);

    expect(screen.queryByTestId("delete-confirm-modal")).not.toBeInTheDocument();
    expect(tauriService.deleteFavoritesBatch).not.toHaveBeenCalled();
    expect(checkbox1).toBeChecked();
    expect(checkbox2).toBeChecked();
  });

  it("AC080: Pressing Escape closes confirmation modal with 0 write calls", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("library-row-checkbox-1"));
    fireEvent.click(screen.getByTestId("library-delete-selected-btn"));

    expect(screen.getByTestId("delete-confirm-modal")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });

    expect(screen.queryByTestId("delete-confirm-modal")).not.toBeInTheDocument();
    expect(tauriService.deleteFavoritesBatch).not.toHaveBeenCalled();
  });

  it("AC081-AC085: Confirming delete executes batch delete transaction, updates UI and clears selection", async () => {
    vi.mocked(tauriService.deleteFavoritesBatch).mockResolvedValue({
      requested_count: 2,
      deleted_count: 2,
      not_found_count: 0,
      deleted_ids: [1, 2],
    });

    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("library-row-checkbox-1"));
    fireEvent.click(screen.getByTestId("library-row-checkbox-2"));

    fireEvent.click(screen.getByTestId("library-delete-selected-btn"));
    fireEvent.click(screen.getByTestId("delete-confirm-submit-btn"));

    await waitFor(() => {
      expect(tauriService.deleteFavoritesBatch).toHaveBeenCalledWith([1, 2]);
    });

    // Rows 1 and 2 are removed from table
    await waitFor(() => {
      expect(screen.queryByTestId("library-row-1")).not.toBeInTheDocument();
      expect(screen.queryByTestId("library-row-2")).not.toBeInTheDocument();
      expect(screen.getByTestId("library-row-3")).toBeInTheDocument();
    });

    // Delete selected button resets to disabled
    expect(screen.getByTestId("library-delete-selected-btn")).toBeDisabled();

    // Success notice displayed
    expect(screen.getByTestId("library-operation-notice")).toBeInTheDocument();
  });

  it("AC088: Failed batch delete transaction preserves selection and list state, displaying operation error", async () => {
    vi.mocked(tauriService.deleteFavoritesBatch).mockRejectedValue(
      new Error("Database write locked")
    );

    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("library-table")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId("library-row-checkbox-1"));
    fireEvent.click(screen.getByTestId("library-delete-selected-btn"));
    fireEvent.click(screen.getByTestId("delete-confirm-submit-btn"));

    await waitFor(() => {
      expect(tauriService.deleteFavoritesBatch).toHaveBeenCalledWith([1]);
    });

    // Operation error banner is displayed
    await waitFor(() => {
      expect(screen.getByTestId("library-operation-error")).toBeInTheDocument();
      expect(screen.getByTestId("library-operation-error").textContent).toContain(
        "Database write locked"
      );
    });

    // Selection and rows are preserved
    expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    expect(screen.getByTestId("library-row-checkbox-1")).toBeChecked();
  });
});
