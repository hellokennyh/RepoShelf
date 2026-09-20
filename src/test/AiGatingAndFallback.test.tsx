import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SearchPage } from "../pages/SearchPage";
import { TrendingPage } from "../pages/TrendingPage";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import { I18nProvider } from "../i18n/I18nContext";
import { AiSearchReadiness, GitHubRepository } from "../types/github";
import { FavoriteItem } from "../types/library";

let mockAiReadiness: AiSearchReadiness = { ready: false, reason: "AI is disabled in settings" };
let mockInvokeCalls: { cmd: string; args?: Record<string, unknown> }[] = [];

const sampleRepo: GitHubRepository = {
  github_repo_id: 101,
  github_url: "https://github.com/tauri-apps/tauri",
  name: "tauri",
  description: "Build smaller, faster desktop apps.",
  stars: 85000,
  license: "Apache-2.0",
  topics: ["rust", "desktop"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
};

const sampleFavorite: FavoriteItem = {
  id: 1,
  github_repo_id: 101,
  github_url: "https://github.com/tauri-apps/tauri",
  name: "tauri",
  description: "Build smaller, faster desktop apps.",
  stars: 85000,
  license: "Apache-2.0",
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
  favorited_at: "2026-03-01T12:00:00Z",
  metadata_refreshed_at: "2026-03-01T12:00:00Z",
  tags: [],
  notes: "",
  releases: null,
  languages: null,
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    mockInvokeCalls.push({ cmd, args });
    switch (cmd) {
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: false, provider: null, base_url: null, model: null },
          ui: { locale: "en" },
        });
      case "get_ai_search_readiness":
        return Promise.resolve(mockAiReadiness);
      case "check_favorite_identities_batch":
      case "find_favorite_identities":
        return Promise.resolve({
          matched_repo_ids: [],
          matched_urls: [],
        });
      case "github_search_repositories":
        return Promise.resolve({
          items: [sampleRepo],
          total_count: 1,
        });
      case "github_get_repository":
        return Promise.resolve(sampleRepo);
      case "github_ai_search_repositories":
        return Promise.resolve({
          items: [sampleRepo],
          generated_queries: ["test query"],
          partial: false,
        });
      case "get_trending":
        return Promise.resolve({
          period: "daily",
          requested_limit: 10,
          returned_count: 1,
          items: [{ ...sampleRepo, trending_rank: 1, period_stars: 100 }],
          enrichment_summary: { attempted: 1, succeeded: 1, failed: 0, rate_limited: false, rate_limit_reset: null },
          is_short_source: false,
          partial_warning: null,
        });
      case "get_distinct_tags":
        return Promise.resolve([]);
      case "check_repo_exists":
        return Promise.resolve(false);
      case "translate_repository_description":
        return Promise.resolve({
          translated_text: "构建更小、更快的桌面应用。",
          target_locale: "zh-Hans",
          source_hash: "abc",
          action: "translated",
          provider: "mock",
          model: "mock-model",
        });
      default:
        return Promise.resolve(null);
    }
  }),
}));

describe("S2A AI Gating and Search Fallback", () => {
  beforeEach(() => {
    mockAiReadiness = { ready: false, reason: "AI is disabled in settings" };
    mockInvokeCalls = [];
    vi.clearAllMocks();
  });

  const renderWithProviders = (ui: React.ReactElement) => {
    return render(
      <I18nProvider>
        <DiscoverySessionProvider>{ui}</DiscoverySessionProvider>
      </I18nProvider>
    );
  };

  it("SearchPage automatically falls back to keyword mode when AI is unavailable", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled in settings" };

    renderWithProviders(<SearchPage />);

    // Wait for initial AI readiness check
    await waitFor(() => {
      const keywordRadio = screen.getByTestId("search-mode-keyword") as HTMLInputElement;
      expect(keywordRadio.checked).toBe(true);
    });

    const aiRadio = screen.getByTestId("search-mode-ai") as HTMLInputElement;
    expect(aiRadio.disabled).toBe(true);
    expect(aiRadio.checked).toBe(false);

    // Keyword form elements are displayed, AI form textarea is not
    expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    expect(screen.queryByTestId("search-ai-input")).not.toBeInTheDocument();
  });

  it("SearchPage preserves keyword query draft independently of AI fallback", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled" };

    renderWithProviders(<SearchPage />);

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const queryInput = screen.getByTestId("search-query-input") as HTMLInputElement;
    fireEvent.change(queryInput, { target: { value: "rust desktop framework" } });
    expect(queryInput.value).toBe("rust desktop framework");
  });

  it("SearchPage disables batch translation button when AI is unavailable", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled" };

    renderWithProviders(<SearchPage />);

    // Perform a keyword search to get items
    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "tauri" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("search-batch-translate-btn")).toBeInTheDocument();
    });

    const batchTranslateBtn = screen.getByTestId("search-batch-translate-btn");
    expect(batchTranslateBtn).toBeDisabled();

    // Clicking does not invoke translate_repository_description
    fireEvent.click(batchTranslateBtn);
    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(0);
  });

  it("TrendingPage disables batch translation button when AI is unavailable", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled" };

    renderWithProviders(<TrendingPage />);

    await waitFor(() => {
      expect(screen.getByTestId("trending-fetch-btn")).toBeInTheDocument();
    });

    // Fetch trending items first so that results are displayed
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-batch-translate-btn")).toBeInTheDocument();
    });

    const batchBtn = screen.getByTestId("trending-batch-translate-btn");
    expect(batchBtn).toBeDisabled();

    // Clicking does not invoke translate_repository_description
    fireEvent.click(batchBtn);
    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(0);
  });

  it("AddRepositoryPage disables batch translation button when AI is unavailable", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled" };

    renderWithProviders(<AddRepositoryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-url-input")).toBeInTheDocument();
    });

    // Fetch a repository so that results card is displayed
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://github.com/tauri-apps/tauri" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-batch-translate-btn")).toBeInTheDocument();
    });

    const batchBtn = screen.getByTestId("add-batch-translate-btn");
    expect(batchBtn).toBeDisabled();

    fireEvent.click(batchBtn);
    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(0);
  });

  it("FavoriteDetailModal disables translate button when AI is unavailable", async () => {
    mockAiReadiness = { ready: false, reason: "AI is disabled" };

    renderWithProviders(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={vi.fn()}
        onUpdated={vi.fn()}
        onDeleted={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId("detail-translate-btn")).toBeInTheDocument();
    });

    const translateBtn = screen.getByTestId("detail-translate-btn");
    expect(translateBtn).toBeDisabled();

    fireEvent.click(translateBtn);
    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(0);
  });

  it("Enabling AI unlocks AI mode radio button but requires explicit user selection to switch", async () => {
    mockAiReadiness = { ready: true, reason: null };

    renderWithProviders(<SearchPage />);

    await waitFor(() => {
      const aiRadio = screen.getByTestId("search-mode-ai") as HTMLInputElement;
      expect(aiRadio.disabled).toBe(false);
    });

    // Default remains keyword mode; does not auto-switch without user action
    const keywordRadio = screen.getByTestId("search-mode-keyword") as HTMLInputElement;
    const aiRadio = screen.getByTestId("search-mode-ai") as HTMLInputElement;
    expect(keywordRadio.checked).toBe(true);
    expect(aiRadio.checked).toBe(false);

    // User can explicitly switch to AI mode
    fireEvent.click(aiRadio);
    expect(aiRadio.checked).toBe(true);
    expect(screen.getByTestId("search-ai-input")).toBeInTheDocument();
  });
});
