import React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { SearchPage } from "../pages/SearchPage";
import { TrendingPage } from "../pages/TrendingPage";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";

vi.mock("../services/tauri", () => ({
  openGitHubUrl: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
  getAiSearchReadiness: vi.fn(),
  suggestFavoriteAiMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
  getGitHubRepository: vi.fn(),
  searchGitHubRepositories: vi.fn(),
  searchAiGitHubRepositories: vi.fn(),
  fetchTrending: vi.fn(),
  checkFavoriteStatus: vi.fn(),
  favoriteRepository: vi.fn(),
  fetchSettings: vi.fn().mockResolvedValue({
    github: { default_search_limit: 20, default_trending_period: "daily" },
    ai: { enabled: false },
  }),
}));

const sampleFavorite: FavoriteItem = {
  id: 42,
  github_repo_id: 1042,
  github_url: "https://github.com/test-owner/test-repo",
  name: "test-repo",
  description: "A fast scraper tool for web data",
  category: "Other",
  stars: 1200,
  license: "MIT",
  pushed_at: "2026-03-01T00:00:00Z",
  rating: 3,
  notes: "User notes here",
  favorited_at: "2026-03-01T10:00:00Z",
  metadata_refreshed_at: "2026-03-01T10:00:00Z",
  tags: [
    { id: 1, favorite_id: 42, tag: "scraping", source: "github" },
    { id: 2, favorite_id: 42, tag: "crawler", source: "user" },
    { id: 3, favorite_id: 42, tag: "old-ai", source: "ai" },
  ],
};

describe("FavoriteDetailModal Phase 6-C Acceptance Tests", () => {
  const onUpdated = vi.fn();
  const onDeleted = vi.fn();
  const onClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("P6C-FE-01: FavoriteDetail shows AI suggestion action button", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).toBeInTheDocument();
    expect(suggestBtn).toHaveTextContent("AI 建议分类、标签和评级");
  });

  it("P6C-FE-02: Action disabled/unavailable when AI not ready", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "未配置 AI API Key (AI API Key not configured)",
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).toBeDisabled();
    expect(suggestBtn).toHaveAttribute(
      "title",
      "未配置 AI API Key (AI API Key not configured)"
    );
  });

  it("P6C-FE-03: Action enabled when AI ready", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).not.toBeDisabled();
  });

  it("P6C-FE-04, 05, 06, 07, 08, 09: Click invokes suggestion command with favorite_id only, shows loading state, and displays suggestion preview", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    let resolveSuggestion!: (
      val: import("../types/library").AiFavoriteMetadataSuggestion
    ) => void;
    const suggestionPromise =
      new Promise<import("../types/library").AiFavoriteMetadataSuggestion>(
        (resolve) => {
          resolveSuggestion = resolve;
        }
      );
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockReturnValue(
      suggestionPromise
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    // P6C-FE-04: invokes suggestion command with favorite_id only
    expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledWith(42);
    expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledTimes(1);

    // P6C-FE-05: loading state shown
    expect(suggestBtn).toHaveTextContent("AI 分析中...");
    expect(suggestBtn).toBeDisabled();

    // Resolve suggestion
    resolveSuggestion!({
      category: "Automation",
      tags: ["scraping", "bot", "crawler"],
      rating: 4,
      rating_reason: "High quality web scraping tool",
      confidence: "high",
    });

    // P6C-FE-06: Suggestion Category displayed
    const catEl = await screen.findByTestId("ai-suggested-category");
    expect(catEl).toHaveTextContent("Automation");

    // P6C-FE-07: Suggestion Tags displayed
    const tagsEl = await screen.findByTestId("ai-suggested-tags");
    expect(tagsEl).toHaveTextContent("#scraping");
    expect(tagsEl).toHaveTextContent("#bot");
    expect(tagsEl).toHaveTextContent("#crawler");

    // P6C-FE-08: Apply visible
    const applyBtn = screen.getByTestId("ai-apply-btn");
    expect(applyBtn).toBeInTheDocument();

    // P6C-FE-09: Cancel visible
    const cancelBtn = screen.getByTestId("ai-cancel-btn");
    expect(cancelBtn).toBeInTheDocument();
  });

  it("P6C-FE-10, 11: Cancel clears suggestion without calling Apply or altering DB", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scraping"],
      rating: 4,
      rating_reason: "Fast crawler tool",
      confidence: "high",
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const cancelBtn = screen.getByTestId("ai-cancel-btn");

    fireEvent.click(cancelBtn);

    // P6C-FE-10: no apply call
    expect(tauriService.applyFavoriteAiMetadata).not.toHaveBeenCalled();

    // P6C-FE-11: clears suggestion box
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();

    // Current category remains unchanged
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Automation");
  });

  it("P6C-FE-12, 13, 14, 15, 16, 17: Apply invokes apply command, updates category and AI tags, and preserves unsaved local user edits", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["bot", "fast-scrape"],
      rating: 4,
      rating_reason: "Fast crawler tool",
      confidence: "high",
    });

    const updatedAfterApply: FavoriteItem = {
      ...sampleFavorite,
      category: "Automation",
      tags: [
        { id: 1, favorite_id: 42, tag: "scraping", source: "github" },
        { id: 2, favorite_id: 42, tag: "crawler", source: "user" },
        { id: 10, favorite_id: 42, tag: "bot", source: "ai" },
        { id: 11, favorite_id: 42, tag: "fast-scrape", source: "ai" },
      ],
    };
    vi.mocked(tauriService.applyFavoriteAiMetadata).mockResolvedValue(
      updatedAfterApply
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Simulate unsaved user edits before AI suggestion
    // 1. Change notes
    const notesInput = (await screen.findByTestId(
      "detail-notes-textarea"
    )) as HTMLTextAreaElement;
    fireEvent.change(notesInput, {
      target: { value: "UNSAVED_LOCAL_NOTE_EDIT" },
    });

    // 2. Change rating to 5
    const star5 = screen.getByTestId("detail-star-5");
    fireEvent.click(star5);

    // 3. Add an unsaved user tag
    const tagInput = screen.getByTestId("detail-add-tag-input");
    fireEvent.change(tagInput, { target: { value: "unsaved-user-tag" } });
    fireEvent.submit(tagInput.closest("form")!);

    // Verify unsaved tag rendered in editable section
    expect(screen.getByTestId("user-tag-unsaved-user-tag")).toBeInTheDocument();

    // Now trigger AI suggestion
    const suggestBtn = screen.getByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const applyBtn = screen.getByTestId("ai-apply-btn");

    // Click Apply
    fireEvent.click(applyBtn);

    await waitFor(() => {
      // P6C-FE-12: Apply invokes apply command with same favorite_id
      expect(tauriService.applyFavoriteAiMetadata).toHaveBeenCalledWith(
        42,
        "Automation",
        ["bot", "fast-scrape"]
      );
    });

    // P6C-FE-13: displayed category updated to Automation
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Automation");

    // P6C-FE-14: displayed AI tags updated to new tags
    expect(screen.getByText("#bot")).toBeInTheDocument();
    expect(screen.getByText("#fast-scrape")).toBeInTheDocument();
    expect(screen.queryByText("#old-ai")).not.toBeInTheDocument();

    // P6C-FE-15: preserves unsaved local user tags
    expect(screen.getByTestId("user-tag-unsaved-user-tag")).toBeInTheDocument();

    // P6C-FE-16: AI suggestion updates rating to 4
    expect(screen.getByText("4 星")).toBeInTheDocument();

    // P6C-FE-17: preserves unsaved local notes
    expect(notesInput.value).toBe("UNSAVED_LOCAL_NOTE_EDIT");

    // Suggestion box cleared after apply
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-18: Suggestion state resets when favorite changes", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scrape"],
      rating: 4,
      rating_reason: "Helpful scraping framework",
      confidence: "medium",
    });

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");

    // Switch to a different favorite
    const differentFav: FavoriteItem = {
      ...sampleFavorite,
      id: 99,
      name: "different-repo",
      category: "Data & Database",
    };

    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={differentFav}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Suggestion box must be cleared
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-19: Suggestion state resets when modal closes and reopens", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scrape"],
      rating: 4,
      rating_reason: "Helpful scraping framework",
      confidence: "medium",
    });

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");

    // Close modal
    rerender(
      <FavoriteDetailModal
        isOpen={false}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Reopen modal
    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Suggestion box must be reset
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-20: Suggestion error does not close modal and displays concise error", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockRejectedValue(
      new Error("AI connection timed out")
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    const banner = await screen.findByTestId("detail-feedback-banner");
    expect(banner).toHaveTextContent("AI connection timed out");

    // Modal stays open
    expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("P6C-FE-21: Apply error does not falsely update UI and keeps suggestion available", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["bot"],
      rating: 4,
      rating_reason: "Automation bot",
      confidence: "high",
    });
    vi.mocked(tauriService.applyFavoriteAiMetadata).mockRejectedValue(
      new Error("Database write locked")
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const applyBtn = screen.getByTestId("ai-apply-btn");

    fireEvent.click(applyBtn);

    const banner = await screen.findByTestId("detail-feedback-banner");
    expect(banner).toHaveTextContent("应用 AI 建议失败: Database write locked");

    // Suggestion box remains available for retry or cancel
    expect(screen.getByTestId("ai-suggestion-box")).toBeInTheDocument();
  });

  it("P6C-FE-22, 23, 24: Existing Save, Refresh, and Delete remain functional", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValue({
      ...sampleFavorite,
      notes: "saved notes",
    });
    vi.mocked(tauriService.refreshFavorite).mockResolvedValue({
      favorite_id: 42,
      status: "succeeded",
      favorite: sampleFavorite,
    });
    vi.mocked(tauriService.deleteFavorite).mockResolvedValue(true);

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Save
    const saveBtn = screen.getByTestId("detail-save-btn");
    fireEvent.click(saveBtn);
    await waitFor(() => {
      expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalled();
    });

    // Refresh
    const refreshBtn = screen.getByTestId("detail-refresh-btn");
    fireEvent.click(refreshBtn);
    await waitFor(() => {
      expect(tauriService.refreshFavorite).toHaveBeenCalledWith(42);
    });

    // Delete
    const deleteBtn = screen.getByTestId("detail-delete-btn");
    fireEvent.click(deleteBtn);
    const confirmBtn = await screen.findByTestId("confirm-delete-btn");
    fireEvent.click(confirmBtn);
    await waitFor(() => {
      expect(tauriService.deleteFavorite).toHaveBeenCalledWith(42);
      expect(onDeleted).toHaveBeenCalledWith(42);
    });
  });

  it("P6C-FE-25: Existing github/ai/user tag visual distinction is preserved", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "AI disabled",
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // GitHub tag shows (github) suffix
    expect(screen.getByText("(github)")).toBeInTheDocument();
    // AI tag shows (ai) suffix
    expect(screen.getByText("(ai)")).toBeInTheDocument();
    // User tag is in editable container with remove button
    expect(screen.getByTestId("user-tag-crawler")).toBeInTheDocument();
  });

  it("P6C-FE-26: AddRepositoryPage has NO AI suggestion UI", () => {
    render(<AddRepositoryPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
    expect(screen.queryByText(/AI 建议/i)).not.toBeInTheDocument();
  });

  it("P6C-FE-27: SearchPage behavior unchanged and has no favorite AI suggestion button", () => {
    render(<SearchPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
  });

  it("P6C-FE-28: TrendingPage behavior unchanged and has no favorite AI suggestion button", () => {
    render(<TrendingPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
  });

  it("P6C-FE-REMED-01: Realistic parent re-render preserves unsaved local User Tags, Rating, and Notes after AI Apply", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Developer Tools",
      tags: ["runtime-ai", "tauri-runtime"],
      rating: 5,
      rating_reason: "High performance webview library",
      confidence: "high",
    });

    const initialFavorite: FavoriteItem = {
      id: 101,
      github_repo_id: 101,
      github_url: "https://github.com/tauri-apps/wry",
      name: "wry",
      description: "Cross-platform WebView rendering library in Rust",
      category: "Libraries & Frameworks",
      stars: 3500,
      license: "Apache-2.0",
      pushed_at: "2026-03-01T00:00:00Z",
      rating: null,
      notes: "",
      favorited_at: "2026-03-01T10:00:00Z",
      metadata_refreshed_at: "2026-03-01T10:00:00Z",
      tags: [
        { id: 1, favorite_id: 101, tag: "webview", source: "github" },
      ],
    };

    const persistedUpdatedAfterApply: FavoriteItem = {
      ...initialFavorite,
      category: "Developer Tools",
      tags: [
        { id: 1, favorite_id: 101, tag: "webview", source: "github" },
        { id: 10, favorite_id: 101, tag: "runtime-ai", source: "ai" },
        { id: 11, favorite_id: 101, tag: "tauri-runtime", source: "ai" },
      ],
    };

    vi.mocked(tauriService.applyFavoriteAiMetadata).mockResolvedValue(
      persistedUpdatedAfterApply
    );

    // Realistic Parent Component Harness mimicking LibraryPage
    const ParentHarness = () => {
      const [fav, setFav] = React.useState<FavoriteItem | null>(initialFavorite);
      return (
        <FavoriteDetailModal
          isOpen={true}
          favorite={fav}
          onClose={onClose}
          onUpdated={(updated) => setFav(updated)}
          onDeleted={onDeleted}
        />
      );
    };

    render(<ParentHarness />);

    // 1. Enter unsaved Notes
    const notesInput = (await screen.findByTestId(
      "detail-notes-textarea"
    )) as HTMLTextAreaElement;
    fireEvent.change(notesInput, {
      target: { value: "RUNTIME_UNSAVED_NOTES" },
    });

    // 2. Choose Rating 4
    const star4 = screen.getByTestId("detail-star-4");
    fireEvent.click(star4);

    // 3. Add unsaved User Tag
    const tagInput = screen.getByTestId("detail-add-tag-input");
    fireEvent.change(tagInput, { target: { value: "RUNTIME_UNSAVED_USER_TAG" } });
    fireEvent.submit(tagInput.closest("form")!);

    expect(
      await screen.findByTestId("user-tag-RUNTIME_UNSAVED_USER_TAG")
    ).toBeInTheDocument();
    expect(notesInput.value).toBe("RUNTIME_UNSAVED_NOTES");
    expect(screen.getByText("4 星")).toBeInTheDocument();

    // 4. Click AI Suggest
    const suggestBtn = screen.getByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const applyBtn = screen.getByTestId("ai-apply-btn");

    // 5. Click Apply
    fireEvent.click(applyBtn);

    await waitFor(() => {
      expect(tauriService.applyFavoriteAiMetadata).toHaveBeenCalledWith(
        101,
        "Developer Tools",
        ["runtime-ai", "tauri-runtime"]
      );
    });

    // 6. Assert visible UI after realistic parent re-render:
    // Category updated to Developer Tools
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Developer Tools");

    // AI tags updated
    expect(screen.getByText("#runtime-ai")).toBeInTheDocument();
    expect(screen.getByText("#tauri-runtime")).toBeInTheDocument();

    // CRITICAL: Unsaved local fields MUST still remain intact!
    expect(notesInput.value).toBe("RUNTIME_UNSAVED_NOTES");
    expect(screen.getByText("5 星")).toBeInTheDocument();
    expect(screen.getByTestId("user-tag-RUNTIME_UNSAVED_USER_TAG")).toBeInTheDocument();
  });

  it("P6C-FE-REMED-02: Same-ID favorite prop object update synchronizes backing favorite without clobbering unsaved local edits", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    const initialFavorite: FavoriteItem = {
      ...sampleFavorite,
      id: 202,
      name: "initial-name",
      stars: 100,
      notes: "initial persisted note",
      rating: 2,
      tags: [{ id: 1, favorite_id: 202, tag: "initial-user", source: "user" }],
    };

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={initialFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Make unsaved edits
    const notesInput = (await screen.findByTestId(
      "detail-notes-textarea"
    )) as HTMLTextAreaElement;
    fireEvent.change(notesInput, {
      target: { value: "MODIFIED_UNSAVED_NOTES" },
    });
    fireEvent.click(screen.getByTestId("detail-star-5"));

    // Parent passes updated object with SAME ID (e.g. background GitHub refresh updated stars)
    const sameIdUpdated: FavoriteItem = {
      ...initialFavorite,
      stars: 9999,
      description: "Updated description from GitHub",
    };

    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sameIdUpdated}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Backing favorite updated
    expect(screen.getByText("★ 9,999")).toBeInTheDocument();
    expect(screen.getByText("Updated description from GitHub")).toBeInTheDocument();

    // Local unsaved edits NOT clobbered
    expect(notesInput.value).toBe("MODIFIED_UNSAVED_NOTES");
    expect(screen.getByText("5 星")).toBeInTheDocument();
  });

  it("P6C-FE-REMED-03: Different-ID favorite prop reinitializes all editable fields and resets AI suggestion state", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scraping"],
      rating: 4,
      rating_reason: "Web scraper",
      confidence: "high",
    });

    const favA: FavoriteItem = {
      ...sampleFavorite,
      id: 301,
      name: "repo-A",
      notes: "notes A",
      rating: 1,
      tags: [],
    };

    const favB: FavoriteItem = {
      ...sampleFavorite,
      id: 302,
      name: "repo-B",
      notes: "notes B",
      rating: 5,
      category: "Web & SaaS",
      tags: [{ id: 50, favorite_id: 302, tag: "b-tag", source: "user" }],
    };

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={favA}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Make unsaved edits on Fav A
    const notesInput = (await screen.findByTestId(
      "detail-notes-textarea"
    )) as HTMLTextAreaElement;
    fireEvent.change(notesInput, {
      target: { value: "UNSAVED_FOR_A" },
    });

    // Generate AI suggestion for A
    fireEvent.click(screen.getByTestId("ai-suggest-btn"));
    await screen.findByTestId("ai-suggestion-box");

    // Parent switches to Favorite B
    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={favB}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // AI suggestion box MUST be reset
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();

    // Editable fields reinitialized from Fav B, no leak from Fav A
    expect(notesInput.value).toBe("notes B");
    expect(screen.getByText("5 星")).toBeInTheDocument();
    expect(screen.getByTestId("user-tag-b-tag")).toBeInTheDocument();
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Web & SaaS");
  });

  describe("Phase 6-C Enhancement: AI Rating, Ollama Structured Suggestion & Detail Modal Enlarge", () => {
    it("P6C-ENH-01: Modal desktop maximum width is at least max-w-6xl (at least 50% wider than max-w-2xl)", async () => {
      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      const modalEl = await screen.findByTestId("favorite-detail-modal");
      const innerContainer = modalEl.firstElementChild;
      expect(innerContainer).toHaveClass("max-w-6xl");
    });

    it("P6C-ENH-02: AI Suggestion populates Category, Rating stars, Rating reason, and Confidence in a single request", async () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: true,
        reason: null,
      });
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Multimedia",
        tags: ["video", "ffmpeg"],
        rating: 4,
        rating_reason: "功能丰富且持续维护的音视频工具",
        confidence: "high",
      });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={{ ...sampleFavorite, rating: null }}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      expect(screen.getByText("未评级")).toBeInTheDocument();

      const suggestBtn = await screen.findByTestId("ai-suggest-btn");
      expect(suggestBtn).toHaveTextContent("AI 建议分类、标签和评级");

      fireEvent.click(suggestBtn);

      await waitFor(() => {
        expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledWith(42);
      });

      // Category updated
      const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
      expect(select.value).toBe("Multimedia");

      // Rating stars updated
      expect(screen.getByText("4 星")).toBeInTheDocument();

      // Rating reason and confidence card displayed
      expect(screen.getByTestId("ai-rating-reason-card")).toBeInTheDocument();
      expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent("功能丰富且持续维护的音视频工具");
      expect(screen.getByTestId("ai-rating-confidence-badge")).toHaveTextContent("置信度: 高");
    });

    it("P6C-ENH-03: AI suggestions overwrite previous Rating upon clicking button again", async () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: true,
        reason: null,
      });
      vi.mocked(tauriService.suggestFavoriteAiMetadata)
        .mockResolvedValueOnce({
          category: "Multimedia",
          tags: ["video"],
          rating: 3,
          rating_reason: "初步评级",
          confidence: "medium",
        })
        .mockResolvedValueOnce({
          category: "Multimedia",
          tags: ["video", "pro"],
          rating: 5,
          rating_reason: "深度评估后升级为五星项目",
          confidence: "high",
        });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={{ ...sampleFavorite, rating: 2 }}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      expect(screen.getByText("2 星")).toBeInTheDocument();

      const suggestBtn = await screen.findByTestId("ai-suggest-btn");

      // First click: rating becomes 3
      fireEvent.click(suggestBtn);
      await screen.findByText("3 星");
      expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent("初步评级");

      // Second click: rating overwrites to 5!
      fireEvent.click(suggestBtn);
      await screen.findByText("5 星");
      expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent("深度评估后升级为五星项目");
      expect(screen.getByTestId("ai-rating-confidence-badge")).toHaveTextContent("置信度: 高");
    });

    it("P6C-ENH-04: Manual rating star editing remains active and functional after AI suggestion", async () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: true,
        reason: null,
      });
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Productivity",
        tags: ["notes"],
        rating: 4,
        rating_reason: "优秀的知识管理库",
        confidence: "high",
      });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={{ ...sampleFavorite, rating: null }}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      const suggestBtn = await screen.findByTestId("ai-suggest-btn");
      fireEvent.click(suggestBtn);

      await screen.findByText("4 星");

      // User manually clicks 2nd star
      const star2 = screen.getByTestId("detail-star-2");
      fireEvent.click(star2);

      expect(screen.getByText("2 星")).toBeInTheDocument();
      // Reasoning remains visible as reference
      expect(screen.getByTestId("ai-rating-reason-card")).toBeInTheDocument();
    });

    it("P6C-ENH-05: Bottom action row contains AI Suggestion, Refresh, and Save buttons", async () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: true,
        reason: null,
      });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      const suggestBtn = await screen.findByTestId("ai-suggest-btn");
      const refreshBtn = screen.getByTestId("detail-refresh-btn");
      const saveBtn = screen.getByTestId("detail-save-btn");

      expect(suggestBtn).toBeInTheDocument();
      expect(refreshBtn).toBeInTheDocument();
      expect(saveBtn).toBeInTheDocument();

      // All three reside together in the footer
      const parentContainer = suggestBtn.parentElement!;
      expect(parentContainer).toContainElement(suggestBtn);
      expect(parentContainer).toContainElement(refreshBtn);
      expect(parentContainer).toContainElement(saveBtn);
    });

    it("P6C-ENH-06: Saving metadata persists rating_reason and rating_confidence", async () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: true,
        reason: null,
      });
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Security & Privacy",
        tags: ["crypto"],
        rating: 5,
        rating_reason: "强加密与完备审计",
        confidence: "high",
      });
      vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValue({
        ...sampleFavorite,
        category: "Security & Privacy",
        rating: 5,
        rating_reason: "强加密与完备审计",
        rating_confidence: "high",
      });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={onDeleted}
        />
      );

      const suggestBtn = await screen.findByTestId("ai-suggest-btn");
      fireEvent.click(suggestBtn);

      await screen.findByText("5 星");

      const saveBtn = screen.getByTestId("detail-save-btn");
      fireEvent.click(saveBtn);

      await waitFor(() => {
        expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalledWith(
          expect.objectContaining({
            favorite_id: 42,
            category: "Security & Privacy",
            rating: 5,
            rating_reason: "强加密与完备审计",
            rating_confidence: "high",
          })
        );
      });
    });
  });
});
