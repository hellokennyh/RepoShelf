import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import React, { useState } from "react";
import { RepositoryCard } from "../components/RepositoryCard";
import { TrendingPage } from "../pages/TrendingPage";
import { SearchPage } from "../pages/SearchPage";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import {
  DiscoverySessionProvider,
  useDiscoverySession,
} from "../context/DiscoverySessionContext";
import { I18nProvider, useI18n } from "../i18n/I18nContext";
import { setTestFallbackLocale } from "../i18n/localeState";
import { UiLocale } from "../types/settings";
import * as tauriService from "../services/tauri";
import { GitHubRepository } from "../types/github";
import { TrendingRepositoryItem, TrendingResponse } from "../types/trending";

vi.mock("../services/tauri", () => ({
  fetchTrending: vi.fn(),
  searchRepositories: vi.fn(),
  searchGitHubRepositories: vi.fn(),
  githubGetRepository: vi.fn(),
  getGitHubRepository: vi.fn(),
  importRepositoriesFromArticle: vi.fn(),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: true, reason: null }),
  openGitHubUrl: vi.fn().mockResolvedValue(undefined),
  favoriteRepository: vi.fn(),
  favoriteRepositoriesBatch: vi.fn(),
  checkFavoriteIdentitiesBatch: vi.fn().mockResolvedValue({
    matched_repo_ids: [],
    matched_urls: [],
  }),
  translateRepositoryDescription: vi.fn(),
  analyzeRepository: vi.fn(),
  getFavorite: vi.fn(),
  refreshFavorite: vi.fn(),
  fetchSettings: vi.fn().mockResolvedValue({
    schema_version: 1,
    github: { default_search_limit: 20, default_trending_period: "daily" },
    ai: { enabled: false, provider: null, base_url: null, model: null },
  }),
  fetchLocaleInitInfo: vi.fn().mockResolvedValue({
    locale: "zh-Hans",
    warning: null,
    detected_source_tag: "zh-CN",
  }),
  updateUiLanguage: vi.fn().mockResolvedValue({
    schema_version: 1,
    ui_language: "zh-Hans",
  }),
}));

const mockTrendingRepo: TrendingRepositoryItem = {
  name: "antigravity-app/awesome-tool",
  description: "Next generation repository management application with rich intelligence.",
  stars: 12500,
  license: "Apache-2.0",
  topics: ["rust", "react", "tauri", "desktop"],
  pushed_at: "2026-10-30T10:00:00Z",
  created_at: "2025-01-15T08:00:00Z",
  category: "Developer Tools",
  trending_rank: 1,
  period_stars: 420,
  github_repo_id: 10001,
  github_url: "https://github.com/antigravity-app/awesome-tool",
  languages: {
    Rust: 75000,
    TypeScript: 20000,
    CSS: 5000,
  },
  releases: 18,
};

const mockSearchRepo: GitHubRepository = {
  github_repo_id: 20002,
  github_url: "https://github.com/super-org/super-search-project",
  name: "super-search-project",
  description: "A comprehensive project discovered via search keywords.",
  stars: 8800,
  license: "MIT",
  topics: ["search", "indexing", "fast"],
  pushed_at: "2026-10-25T14:30:00Z",
  created_at: "2024-06-01T12:00:00Z",
  category: "Backend",
  languages: {
    Go: 85000,
    Python: 15000,
  },
  releases: 5,
};

const mockLongUrlRepo: GitHubRepository = {
  github_repo_id: 30003,
  github_url: "https://github.com/an-extremely-long-organization-name-with-many-subprojects/and-an-ultra-long-repository-name-that-could-overflow",
  name: "and-an-ultra-long-repository-name-that-could-overflow",
  description: "Repository with ultra long URL to test CSS truncation and responsive wrapping.",
  stars: 550,
  license: null,
  pushed_at: null,
  category: "AI & Agents",
  topics: ["ai", "long-url"],
  languages: { Python: 10000 },
};

function renderWithProviders(ui: React.ReactNode, initialLocale: UiLocale = "zh-Hans") {
  setTestFallbackLocale(initialLocale);
  return render(
    <I18nProvider>
      <DiscoverySessionProvider>
        {ui}
      </DiscoverySessionProvider>
    </I18nProvider>
  );
}

describe("S3 Discovery Compact Cards & GitHub URL Layout Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setTestFallbackLocale("zh-Hans");
  });

  // =========================================================================
  // 1. RepositoryCard Component Isolated Layout & Toggle
  // =========================================================================
  describe("RepositoryCard: showDetails Prop & Conditional Rendering", () => {
    it("A. when showDetails=false, hides GitHub Topics and Languages bar", () => {
      renderWithProviders(
        <RepositoryCard repo={mockTrendingRepo} showDetails={false} />
      );

      // Topics must be hidden
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByText("#rust")).not.toBeInTheDocument();
      expect(screen.queryByText("#react")).not.toBeInTheDocument();
      expect(screen.queryByTestId("empty-topics")).not.toBeInTheDocument();

      // LanguagesBar must be hidden
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();
      expect(screen.queryByText("Rust")).not.toBeInTheDocument();

      // Core fields MUST remain visible
      expect(screen.getByText("antigravity-app/awesome-tool")).toBeInTheDocument();
      expect(screen.getByText(/Next generation repository management/)).toBeInTheDocument();
      expect(screen.getByTestId("repo-category")).toHaveTextContent("Developer Tools");
      expect(screen.getByText("12,500")).toBeInTheDocument(); // Stars
      expect(screen.getByTestId("card-github-url")).toHaveTextContent(mockTrendingRepo.github_url);
      expect(screen.getByTestId("repo-created-at")).toBeInTheDocument();
      expect(screen.getByTestId("analyze-btn")).toBeInTheDocument();
    });

    it("B. when showDetails=true, restores GitHub Topics and Languages bar", () => {
      renderWithProviders(
        <RepositoryCard repo={mockTrendingRepo} showDetails={true} />
      );

      // Topics are visible
      expect(screen.getByTestId("repo-topics-container")).toBeInTheDocument();
      expect(screen.getByText("#rust")).toBeInTheDocument();
      expect(screen.getByText("#react")).toBeInTheDocument();

      // LanguagesBar is visible
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();
      expect(screen.getByText("Rust")).toBeInTheDocument();
    });

    it("C. default showDetails prop maintains backwards compatibility (true when omitted)", () => {
      renderWithProviders(
        <RepositoryCard repo={mockTrendingRepo} />
      );

      // When omitted, existing callers/tests still receive full display
      expect(screen.getByTestId("repo-topics-container")).toBeInTheDocument();
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();
    });

    it("D. missing topics or languages handle gracefully without generating fake data", () => {
      const repoNoTopicsNoLang: GitHubRepository = {
        github_repo_id: 9999,
        github_url: "https://github.com/org/no-topics-repo",
        name: "no-topics-repo",
        description: "Empty topics",
        stars: 10,
        license: null,
        pushed_at: null,
        category: "Other",
        topics: [],
        languages: null,
      };

      // When showDetails=false -> completely hidden
      const { unmount } = renderWithProviders(
        <RepositoryCard repo={repoNoTopicsNoLang} showDetails={false} />
      );
      expect(screen.queryByTestId("empty-topics")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();
      unmount();

      // When showDetails=true -> renders empty fallback without fake badges
      renderWithProviders(
        <RepositoryCard repo={repoNoTopicsNoLang} showDetails={true} />
      );
      expect(screen.getByTestId("empty-topics")).toHaveTextContent("暂无标签");
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();
    });
  });

  // =========================================================================
  // 2. GitHub URL Position, Structure, and Click Behavior
  // =========================================================================
  describe("RepositoryCard: GitHub URL in Header Line", () => {
    it("A. places GitHub URL in the top title container after period stars when present", () => {
      renderWithProviders(
        <RepositoryCard
          repo={mockTrendingRepo}
          trendingRank={1}
          periodStars={420}
          periodLabel="stars today"
          showDetails={false}
        />
      );

      const titleRow = screen.getByTestId("open-github-link").closest(".flex-wrap");
      expect(titleRow).toBeInTheDocument();

      const urlEl = screen.getByTestId("card-github-url");
      expect(titleRow).toContainElement(urlEl);

      // Verify DOM order: name -> category -> periodStars -> url
      const nameEl = screen.getByTestId("open-github-link");
      const catEl = screen.getByTestId("repo-category");
      const starsEl = screen.getByTestId("period-stars");

      expect(nameEl.compareDocumentPosition(catEl) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(catEl.compareDocumentPosition(starsEl) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(starsEl.compareDocumentPosition(urlEl) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it("B. places GitHub URL directly after category when period stars is absent (no fake badge)", () => {
      renderWithProviders(
        <RepositoryCard
          repo={mockSearchRepo}
          showDetails={false}
        />
      );

      expect(screen.queryByTestId("period-stars")).not.toBeInTheDocument();

      const catEl = screen.getByTestId("repo-category");
      const urlEl = screen.getByTestId("card-github-url");
      expect(catEl.compareDocumentPosition(urlEl) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it("C. old separate URL row is completely removed", () => {
      const { container } = renderWithProviders(
        <RepositoryCard repo={mockTrendingRepo} showDetails={false} />
      );

      // Verify there is only one link with text matching github_url
      const urlLinks = container.querySelectorAll(`a[href="${mockTrendingRepo.github_url}"]`);
      // Exactly 2 links to the repository: 1 title link (open-github-link), 1 url link (card-github-url)
      expect(urlLinks.length).toBe(2);
      expect(screen.getByTestId("open-github-link")).toHaveAttribute("href", mockTrendingRepo.github_url);
      expect(screen.getByTestId("card-github-url")).toHaveAttribute("href", mockTrendingRepo.github_url);
    });

    it("D. long URL has truncation classes and hover title attribute", () => {
      renderWithProviders(
        <RepositoryCard repo={mockLongUrlRepo} showDetails={false} />
      );

      const urlEl = screen.getByTestId("card-github-url");
      expect(urlEl).toHaveAttribute("title", mockLongUrlRepo.github_url);
      expect(urlEl.className).toContain("truncate");
      expect(urlEl.className).toContain("max-w-");
    });

    it("E. clicking GitHub URL opens external browser and stops propagation", async () => {
      const onToggleSelect = vi.fn();
      const onFavorite = vi.fn();
      const onAnalyze = vi.fn();

      renderWithProviders(
        <RepositoryCard
          repo={mockTrendingRepo}
          selectable={true}
          onToggleSelect={onToggleSelect}
          onFavorite={onFavorite}
          onAnalyze={onAnalyze}
          showDetails={false}
        />
      );

      const urlEl = screen.getByTestId("card-github-url");
      fireEvent.click(urlEl);

      await waitFor(() => {
        expect(tauriService.openGitHubUrl).toHaveBeenCalledWith(mockTrendingRepo.github_url);
      });

      // No unintended side-effects on card
      expect(onToggleSelect).not.toHaveBeenCalled();
      expect(onFavorite).not.toHaveBeenCalled();
      expect(onAnalyze).not.toHaveBeenCalled();
    });

    it("F. clicking repo name opens external browser and stops propagation", async () => {
      renderWithProviders(
        <RepositoryCard repo={mockTrendingRepo} showDetails={false} />
      );

      const titleLink = screen.getByTestId("open-github-link");
      fireEvent.click(titleLink);

      await waitFor(() => {
        expect(tauriService.openGitHubUrl).toHaveBeenCalledWith(mockTrendingRepo.github_url);
      });
    });
  });

  // =========================================================================
  // 3. Three Discovery Pages Default False & Independent Toggles
  // =========================================================================
  describe("Three Pages: Default showDetails=false and Independent Toggle", () => {
    it("A. TrendingPage defaults to unchecked and toggles compact/full view", async () => {
      const trendingResponse: TrendingResponse = {
        period: "daily",
        requested_limit: 10,
        returned_count: 1,
        items: [mockTrendingRepo],
        enrichment_summary: { attempted: 1, succeeded: 1, failed: 0, rate_limited: false, rate_limit_reset: null },
        is_short_source: false,
        partial_warning: null,
      };
      vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(trendingResponse);

      renderWithProviders(<TrendingPage />);

      // 1. Checkbox exists and is unchecked by default
      const checkbox = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      expect(checkbox).toBeInTheDocument();
      expect(checkbox.checked).toBe(false);

      // Fetch results
      fireEvent.click(screen.getByTestId("trending-fetch-btn"));
      await waitFor(() => {
        expect(screen.getByText("antigravity-app/awesome-tool")).toBeInTheDocument();
      });

      // 2. Default unchecked: topics and languages hidden
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();

      // 3. Check "详细": topics and languages appear
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(true);
      expect(screen.getByTestId("repo-topics-container")).toBeInTheDocument();
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();

      // 4. Uncheck: hidden again
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(false);
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();

      // No redundant network calls triggered by toggle
      expect(tauriService.fetchTrending).toHaveBeenCalledTimes(1);
    });

    it("B. SearchPage defaults to unchecked and toggles compact/full view", async () => {
      vi.mocked(tauriService.searchGitHubRepositories).mockResolvedValueOnce({
        items: [mockSearchRepo],
        total_count: 1,
      });

      renderWithProviders(<SearchPage />);

      // 1. Checkbox exists and is unchecked by default
      const checkbox = screen.getByTestId("search-details-checkbox") as HTMLInputElement;
      expect(checkbox).toBeInTheDocument();
      expect(checkbox.checked).toBe(false);

      // Perform keyword search
      const input = screen.getByTestId("search-query-input");
      fireEvent.change(input, { target: { value: "super-search" } });
      fireEvent.click(screen.getByTestId("search-submit-btn"));

      await waitFor(() => {
        expect(screen.getByText("super-search-project")).toBeInTheDocument();
      });

      // 2. Default unchecked: topics and languages hidden
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();

      // 3. Check "详细": topics and languages appear
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(true);
      expect(screen.getByTestId("repo-topics-container")).toBeInTheDocument();
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();

      // 4. Uncheck: hidden again
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(false);
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();

      expect(tauriService.searchGitHubRepositories).toHaveBeenCalledTimes(1);
    });

    it("C. AddRepositoryPage defaults to unchecked and toggles compact/full view for direct & article preview", async () => {
      vi.mocked(tauriService.getGitHubRepository).mockResolvedValueOnce(mockSearchRepo);

      renderWithProviders(<AddRepositoryPage />);

      // 1. Checkbox exists and is unchecked by default
      const checkbox = screen.getByTestId("add-details-checkbox") as HTMLInputElement;
      expect(checkbox).toBeInTheDocument();
      expect(checkbox.checked).toBe(false);

      // Fetch direct repo preview
      const input = screen.getByTestId("add-repo-url-input");
      fireEvent.change(input, { target: { value: mockSearchRepo.github_url } });
      fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("repo-preview-container")).toBeInTheDocument();
      });

      // 2. Default unchecked: topics and languages hidden
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();

      // 3. Check "详细": topics and languages appear
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(true);
      expect(screen.getByTestId("repo-topics-container")).toBeInTheDocument();
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();

      // 4. Uncheck: hidden again
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(false);
      expect(screen.queryByTestId("repo-topics-container")).not.toBeInTheDocument();
      expect(screen.queryByTestId("repo-languages")).not.toBeInTheDocument();
    });

    it("D. state isolation: toggling one page does NOT alter the other two pages", () => {
      const TestCrossPageHarness: React.FC = () => {
        const { trending, search, add, setTrending, setSearch, setAdd } = useDiscoverySession();

        return (
          <div>
            <div data-testid="trending-val">{String(trending.showDetails)}</div>
            <div data-testid="search-val">{String(search.showDetails)}</div>
            <div data-testid="add-val">{String(add.showDetails)}</div>

            <button
              data-testid="toggle-trending"
              onClick={() => setTrending((prev) => ({ ...prev, showDetails: !prev.showDetails }))}
            >
              Toggle Trending
            </button>
            <button
              data-testid="toggle-search"
              onClick={() => setSearch((prev) => ({ ...prev, showDetails: !prev.showDetails }))}
            >
              Toggle Search
            </button>
            <button
              data-testid="toggle-add"
              onClick={() => setAdd((prev) => ({ ...prev, showDetails: !prev.showDetails }))}
            >
              Toggle Add
            </button>
          </div>
        );
      };

      renderWithProviders(<TestCrossPageHarness />);

      // Initial: all false
      expect(screen.getByTestId("trending-val")).toHaveTextContent("false");
      expect(screen.getByTestId("search-val")).toHaveTextContent("false");
      expect(screen.getByTestId("add-val")).toHaveTextContent("false");

      // 1. Toggle Trending -> only trending turns true
      fireEvent.click(screen.getByTestId("toggle-trending"));
      expect(screen.getByTestId("trending-val")).toHaveTextContent("true");
      expect(screen.getByTestId("search-val")).toHaveTextContent("false");
      expect(screen.getByTestId("add-val")).toHaveTextContent("false");

      // 2. Toggle Search -> trending remains true, search turns true, add remains false
      fireEvent.click(screen.getByTestId("toggle-search"));
      expect(screen.getByTestId("trending-val")).toHaveTextContent("true");
      expect(screen.getByTestId("search-val")).toHaveTextContent("true");
      expect(screen.getByTestId("add-val")).toHaveTextContent("false");

      // 3. Toggle Add -> all three true independently
      fireEvent.click(screen.getByTestId("toggle-add"));
      expect(screen.getByTestId("trending-val")).toHaveTextContent("true");
      expect(screen.getByTestId("search-val")).toHaveTextContent("true");
      expect(screen.getByTestId("add-val")).toHaveTextContent("true");
    });
  });

  // =========================================================================
  // 4. Session Persistence Across Navigations & Restart Clean Reset
  // =========================================================================
  describe("Session State Lifecycle: Navigations vs App Restart", () => {
    it("A. retains showDetails state during tab navigation within the same session", () => {
      const NavigationHarness: React.FC = () => {
        const [activeTab, setActiveTab] = useState<"trending" | "search">("trending");

        return (
          <div>
            <button data-testid="nav-trending" onClick={() => setActiveTab("trending")}>
              Go Trending
            </button>
            <button data-testid="nav-search" onClick={() => setActiveTab("search")}>
              Go Search
            </button>

            {activeTab === "trending" ? <TrendingPage /> : <SearchPage />}
          </div>
        );
      };

      renderWithProviders(<NavigationHarness />);

      // 1. In Trending: turn on showDetails
      const trendingCheckbox = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      expect(trendingCheckbox.checked).toBe(false);
      fireEvent.click(trendingCheckbox);
      expect(trendingCheckbox.checked).toBe(true);

      // 2. Navigate to Search: search is still false
      fireEvent.click(screen.getByTestId("nav-search"));
      const searchCheckbox = screen.getByTestId("search-details-checkbox") as HTMLInputElement;
      expect(searchCheckbox.checked).toBe(false);

      // 3. Navigate back to Trending: showDetails is STILL TRUE!
      fireEvent.click(screen.getByTestId("nav-trending"));
      const restoredTrendingCheckbox = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      expect(restoredTrendingCheckbox.checked).toBe(true);
    });

    it("B. new application session cleanly resets all showDetails to false (zero persistence boundary)", () => {
      // Session 1: user toggles trending to true
      const { unmount } = render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <TrendingPage />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      const checkbox1 = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      fireEvent.click(checkbox1);
      expect(checkbox1.checked).toBe(true);

      unmount();

      // Session 2: simulating application restart (new provider instance)
      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <TrendingPage />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      const checkbox2 = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      // Clean reset to false
      expect(checkbox2.checked).toBe(false);
    });
  });

  // =========================================================================
  // 5. Internationalization & Accessibility Parity
  // =========================================================================
  describe("i18n & Accessibility Parity", () => {
    it("A. renders correct localized labels across zh-Hans, zh-Hant, and en", async () => {
      const CheckboxLabelTest: React.FC = () => {
        const { t, setLocale } = useI18n();
        return (
          <div>
            <button data-testid="btn-zh-hans" onClick={() => setLocale("zh-Hans")}>zh-Hans</button>
            <button data-testid="btn-zh-hant" onClick={() => setLocale("zh-Hant")}>zh-Hant</button>
            <button data-testid="btn-en" onClick={() => setLocale("en")}>en</button>
            <label>
              <input type="checkbox" data-testid="test-checkbox" />
              <span>{t("common.details")}</span>
            </label>
          </div>
        );
      };

      render(
        <I18nProvider>
          <CheckboxLabelTest />
        </I18nProvider>
      );

      // Default zh-Hans -> 详细
      expect(screen.getByLabelText("详细")).toBeInTheDocument();

      // Switch to zh-Hant -> 詳細
      fireEvent.click(screen.getByTestId("btn-zh-hant"));
      await waitFor(() => {
        expect(screen.getByLabelText("詳細")).toBeInTheDocument();
      });

      // Switch to en -> Details
      fireEvent.click(screen.getByTestId("btn-en"));
      await waitFor(() => {
        expect(screen.getByLabelText("Details")).toBeInTheDocument();
      });
    });

    it("B. switching UI language does NOT reset showDetails state", async () => {
      const LangSwitchHarness: React.FC = () => {
        const { setLocale } = useI18n();
        return (
          <div>
            <button data-testid="switch-en" onClick={() => setLocale("en")}>EN</button>
            <TrendingPage />
          </div>
        );
      };

      renderWithProviders(<LangSwitchHarness />);

      const checkbox = screen.getByTestId("trending-details-checkbox") as HTMLInputElement;
      expect(checkbox.checked).toBe(false);

      // Toggle to true
      fireEvent.click(checkbox);
      expect(checkbox.checked).toBe(true);

      // Switch language to en
      fireEvent.click(screen.getByTestId("switch-en"));

      // Checkbox is still checked, with updated English label!
      await waitFor(() => {
        expect(checkbox.checked).toBe(true);
        expect(screen.getByLabelText("Details")).toBeInTheDocument();
      });
    });
  });
});
