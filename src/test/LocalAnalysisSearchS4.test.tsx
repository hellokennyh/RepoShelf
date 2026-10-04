import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
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
  deleteFavoritesBatch: vi.fn(),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: false, reason: "AI not configured" }),
  suggestFavoriteAiMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
}));

const sampleFavoriteA: FavoriteItem = {
  id: 1,
  github_repo_id: 101,
  github_url: "https://github.com/org/repo-a",
  name: "repo-a",
  description: "Repository A",
  category: "Developer Tools",
  stars: 100,
  license: "MIT",
  pushed_at: "2026-10-01T12:00:00Z",
  rating: 5,
  notes: "Notes for repo A",
  favorited_at: "2026-10-01T10:00:00Z",
  metadata_refreshed_at: "2026-10-01T10:00:00Z",
  tags: [{ id: 1, favorite_id: 1, tag: "rust", source: "github" }],
  ai_analysis: {
    schema_version: 1,
    content: {
      overview: { status: "supported", items: [{ text: "Analysis keyword alpha", basis: "documented", evidence_ids: [] }], missing_information: [] },
      features: { status: "supported", items: [], missing_information: [] },
      use_cases: { status: "supported", items: [], missing_information: [] },
      tech_stack: { status: "supported", items: [], missing_information: [] },
      setup: { status: "supported", items: [], missing_information: [] },
      target_users: { status: "supported", items: [], missing_information: [] },
      maturity: { status: "supported", items: [], missing_information: [] },
      limitations: { status: "supported", items: [], missing_information: [] },
    },
    target_locale: "en",
    generated_at: "2026-10-04T12:00:00Z",
    generator: {
      provider: "ollama",
      model_requested: "qwen",
      model_reported: null,
      prompt_version: "1.0",
      selector_version: "1.0",
      context_budget_tokens: 4000,
      input_tokens: 1000,
      input_tokens_is_estimate: false,
      output_limit_tokens: 2000,
    },
    source: {
      github_repo_id: "101",
      canonical_url: "https://github.com/org/repo-a",
      default_branch: "main",
      commit_sha: "abc",
      metadata_observed_at: "2026-10-04T12:00:00Z",
      evidence_fingerprint: "fp",
    },
    evidence: [],
    coverage: {
      status: "bounded",
      selected_file_count: 1,
      sent_file_count: 1,
      evidence_count: 0,
      gaps: [],
      executed_code: false,
      verified_deployment: false,
    },
    search_text: "Analysis keyword alpha",
  },
  analysis_status: { status: "valid" },
  analysis_revision: "rev-a",
};

const sampleFavoriteB: FavoriteItem = {
  id: 2,
  github_repo_id: 102,
  github_url: "https://github.com/org/repo-b",
  name: "repo-b",
  description: "Repository B",
  category: "AI & Agents",
  stars: 200,
  license: "Apache-2.0",
  pushed_at: "2026-10-02T12:00:00Z",
  rating: 4,
  notes: "Notes for repo B",
  favorited_at: "2026-10-02T10:00:00Z",
  metadata_refreshed_at: "2026-10-02T10:00:00Z",
  tags: [{ id: 2, favorite_id: 2, tag: "python", source: "github" }],
  ai_analysis: {
    schema_version: 1,
    content: {
      overview: { status: "supported", items: [{ text: "Analysis keyword beta", basis: "documented", evidence_ids: [] }], missing_information: [] },
      features: { status: "supported", items: [], missing_information: [] },
      use_cases: { status: "supported", items: [], missing_information: [] },
      tech_stack: { status: "supported", items: [], missing_information: [] },
      setup: { status: "supported", items: [], missing_information: [] },
      target_users: { status: "supported", items: [], missing_information: [] },
      maturity: { status: "supported", items: [], missing_information: [] },
      limitations: { status: "supported", items: [], missing_information: [] },
    },
    target_locale: "en",
    generated_at: "2026-10-04T12:00:00Z",
    generator: {
      provider: "ollama",
      model_requested: "qwen",
      model_reported: null,
      prompt_version: "1.0",
      selector_version: "1.0",
      context_budget_tokens: 4000,
      input_tokens: 1000,
      input_tokens_is_estimate: false,
      output_limit_tokens: 2000,
    },
    source: {
      github_repo_id: "102",
      canonical_url: "https://github.com/org/repo-b",
      default_branch: "main",
      commit_sha: "def",
      metadata_observed_at: "2026-10-04T12:00:00Z",
      evidence_fingerprint: "fp2",
    },
    evidence: [],
    coverage: {
      status: "bounded",
      selected_file_count: 1,
      sent_file_count: 1,
      evidence_count: 0,
      gaps: [],
      executed_code: false,
      verified_deployment: false,
    },
    search_text: "Analysis keyword beta",
  },
  analysis_status: { status: "valid" },
  analysis_revision: "rev-b",
};

describe("S4 Local Favorites Search — Frontend Lifecycle & Stale Guard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue(["rust", "python"]);
  });

  it("AC103: late-arriving query A results must not overwrite faster query B results", async () => {
    let resolveSlowQueryA: ((favs: FavoriteItem[]) => void) | null = null;
    const slowPromiseA = new Promise<FavoriteItem[]>((resolve) => {
      resolveSlowQueryA = resolve;
    });

    vi.mocked(tauriService.listFavorites).mockImplementation(async (query) => {
      if (query?.search === "slow_query") {
        return slowPromiseA;
      }
      if (query?.search === "fast_query") {
        return [sampleFavoriteB];
      }
      return [sampleFavoriteA, sampleFavoriteB];
    });

    render(<LibraryPage />);

    // Initial load displays both
    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
    });

    const searchInput = screen.getByTestId("library-search-input");

    // 1. Trigger Query A (slow)
    fireEvent.change(searchInput, { target: { value: "slow_query" } });

    // 2. Trigger Query B (fast) before Query A resolves
    fireEvent.change(searchInput, { target: { value: "fast_query" } });

    // Query B should complete fast and display repo-b
    await waitFor(() => {
      expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
      expect(screen.queryByTestId("library-row-1")).not.toBeInTheDocument();
    });

    // 3. Now let Query A resolve late with [sampleFavoriteA]
    resolveSlowQueryA!([sampleFavoriteA]);

    // Give microtasks time to execute
    await new Promise((r) => setTimeout(r, 50));

    // UI MUST STILL display Query B results (repo-b), NOT Query A (repo-a)!
    expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
    expect(screen.queryByTestId("library-row-1")).not.toBeInTheDocument();
  });

  it("AC095 & AC101: local search works offline without AI provider calls", async () => {
    vi.mocked(tauriService.listFavorites).mockResolvedValue([sampleFavoriteA]);

    render(<LibraryPage />);

    const searchInput = screen.getByTestId("library-search-input");
    fireEvent.change(searchInput, { target: { value: "alpha" } });

    await waitFor(() => {
      expect(tauriService.listFavorites).toHaveBeenCalledWith(
        expect.objectContaining({ search: "alpha" })
      );
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
    });

    // Confirm zero AI generation calls
    expect(tauriService.suggestFavoriteAiMetadata).not.toHaveBeenCalled();
    expect(tauriService.applyFavoriteAiMetadata).not.toHaveBeenCalled();
  });

  it("Section 32: deleting a favorite removes it from the search view immediately", async () => {
    vi.mocked(tauriService.listFavorites).mockResolvedValue([sampleFavoriteA, sampleFavoriteB]);
    vi.mocked(tauriService.deleteFavoritesBatch).mockResolvedValue({
      deleted_ids: [1],
      deleted_count: 1,
      not_found_count: 0,
      requested_count: 1,
    });

    render(<LibraryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("library-row-1")).toBeInTheDocument();
      expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
    });

    // Select row 1
    const checkbox1 = screen.getByTestId("library-row-checkbox-1");
    fireEvent.click(checkbox1);

    // Open delete modal
    const deleteBtn = screen.getByTestId("library-delete-selected-btn");
    fireEvent.click(deleteBtn);

    // Confirm deletion
    const confirmBtn = screen.getByTestId("delete-confirm-submit-btn");
    fireEvent.click(confirmBtn);

    // Row 1 should immediately disappear from view
    await waitFor(() => {
      expect(screen.queryByTestId("library-row-1")).not.toBeInTheDocument();
      expect(screen.getByTestId("library-row-2")).toBeInTheDocument();
    });
  });
});
