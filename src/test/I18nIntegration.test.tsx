import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsPage } from "../pages/SettingsPage";
import { Sidebar } from "../components/Sidebar";
import { Header } from "../components/Header";
import { RepositoryCard } from "../components/RepositoryCard";
import { I18nProvider, useI18n } from "../i18n/I18nContext";
import * as tauriService from "../services/tauri";
import { DiscoverySessionProvider, useDiscoverySession } from "../context/DiscoverySessionContext";

// Mock Tauri services
vi.mock("../services/tauri", () => ({
  fetchSettings: vi.fn(),
  saveSettings: vi.fn(),
  updateUiLanguage: vi.fn(),
  fetchLocaleInitInfo: vi.fn(),
  fetchSecretStatus: vi.fn(),
  testGitHubConnection: vi.fn(),
  saveGitHubToken: vi.fn(),
  clearGitHubToken: vi.fn(),
  testAiConnection: vi.fn(),
  saveAiApiKey: vi.fn(),
  clearAiApiKey: vi.fn(),
  discoverOllamaEndpoints: vi.fn(),
  listOllamaModels: vi.fn(),
  fetchTrending: vi.fn(),
  searchGitHubRepositories: vi.fn(),
  searchAiGitHubRepositories: vi.fn(),
  getAiSearchReadiness: vi.fn(),
  favoriteRepository: vi.fn(),
  favoriteRepositoriesBatch: vi.fn(),
  refreshFavorite: vi.fn(),
  checkFavoriteIdentitiesBatch: vi.fn(),
  importRepositoriesFromArticle: vi.fn(),
  openGitHubUrl: vi.fn(),
}));

// Mock @tauri-apps/api/core invoke
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_locale_initialization_info") {
      return Promise.resolve({
        locale: "zh-Hans",
        warning: null,
        detected_source_tag: "zh-CN",
      });
    }
    if (cmd === "get_settings") {
      return Promise.resolve({
        schema_version: 1,
        ui_language: "zh-Hans",
        github: { default_search_limit: 20, default_trending_period: "daily" },
        ai: { enabled: false, provider: null, base_url: null, model: null },
      });
    }
    if (cmd === "update_ui_language") {
      return Promise.resolve({
        schema_version: 1,
        ui_language: "en",
        github: { default_search_limit: 20, default_trending_period: "daily" },
        ai: { enabled: false, provider: null, base_url: null, model: null },
      });
    }
    return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
  }),
}));

const sampleTrendingItem = {
  name: "sample-repo",
  description: "An open source project",
  stars: 1234,
  license: "MIT",
  topics: ["rust", "tauri"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
  trending_rank: 1,
  period_stars: 42,
  github_repo_id: 101,
  github_url: "https://github.com/test-org/sample-repo",
  releases: 5,
  languages: { Rust: 85.5, TypeScript: 14.5 },
};

describe("S4 I18N Integration and UI Verification Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();

    vi.mocked(tauriService.fetchLocaleInitInfo).mockResolvedValue({
      locale: "zh-Hans",
      warning: null,
      detected_source_tag: "zh-CN",
    });

    vi.mocked(tauriService.fetchSettings).mockResolvedValue({
      schema_version: 1,
      ui_language: "zh-Hans",
      github: { default_search_limit: 20, default_trending_period: "daily" },
      ai: { enabled: false, provider: null, base_url: null, model: null },
    });

    vi.mocked(tauriService.updateUiLanguage).mockResolvedValue({
      schema_version: 1,
      ui_language: "en",
      github: { default_search_limit: 20, default_trending_period: "daily" },
      ai: { enabled: false, provider: null, base_url: null, model: null },
    });

    vi.mocked(tauriService.fetchSecretStatus).mockResolvedValue({
      github_token_configured: false,
      ai_api_key_configured: false,
    });

    vi.mocked(tauriService.checkFavoriteIdentitiesBatch).mockResolvedValue({
      matched_urls: [],
      matched_repo_ids: [],
    });

    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "AI is not configured",
    });
  });

  // Requirement B: Settings page options & no language selector in primary chrome
  it("Requirement B: Settings provides exactly English, 简体中文, 繁體中文, and no switcher in Sidebar/Header", async () => {
    render(
      <I18nProvider>
        <SettingsPage />
      </I18nProvider>
    );

    const select = await screen.findByTestId("ui-language-select");
    expect(select).toBeInTheDocument();

    const options = Array.from(select.querySelectorAll("option")).map((o) => ({
      value: o.value,
      text: o.textContent,
    }));

    expect(options).toEqual([
      { value: "en", text: "English" },
      { value: "zh-Hans", text: "简体中文" },
      { value: "zh-Hant", text: "繁體中文" },
    ]);

    // Ensure Sidebar has no language switcher
    const { container: sidebarContainer } = render(
      <I18nProvider>
        <Sidebar activeTab="settings" onTabChange={() => {}} />
      </I18nProvider>
    );
    expect(sidebarContainer.querySelector("select")).toBeNull();
    expect(sidebarContainer.querySelector("[data-testid='ui-language-select']")).toBeNull();

    // Ensure Header has no language switcher
    const { container: headerContainer } = render(
      <I18nProvider>
        <Header activeTab="settings" />
      </I18nProvider>
    );
    expect(headerContainer.querySelector("select")).toBeNull();
    expect(headerContainer.querySelector("[data-testid='ui-language-select']")).toBeNull();
  });

  // Requirement C: Immediate UI Update without restart
  it("Requirement C: Switching language in Settings immediately updates UI without restart", async () => {
    const TestAppShell = () => {
      const { locale } = useI18n();
      return (
        <div>
          <span data-testid="current-locale">{locale}</span>
          <Sidebar activeTab="trending" onTabChange={() => {}} />
          <SettingsPage />
        </div>
      );
    };

    render(
      <I18nProvider>
        <TestAppShell />
      </I18nProvider>
    );

    // Initial state: zh-Hans
    expect(screen.getByTestId("nav-trending")).toHaveTextContent("热门项目");
    expect(screen.getByTestId("nav-settings")).toHaveTextContent("设置");

    // Switch to English
    const select = await screen.findByTestId("ui-language-select");
    fireEvent.change(select, { target: { value: "en" } });

    await waitFor(() => {
      expect(tauriService.updateUiLanguage).toHaveBeenCalledWith("en");
      expect(screen.getByTestId("nav-trending")).toHaveTextContent("Trending");
      expect(screen.getByTestId("nav-settings")).toHaveTextContent("Settings");
    });

    // Switch to Traditional Chinese
    fireEvent.change(select, { target: { value: "zh-Hant" } });

    await waitFor(() => {
      expect(tauriService.updateUiLanguage).toHaveBeenCalledWith("zh-Hant");
      expect(screen.getByTestId("nav-trending")).toHaveTextContent("熱門專案");
      expect(screen.getByTestId("nav-settings")).toHaveTextContent("設定");
    });
  });

  // Requirement D & E: Session preservation & zero hidden network/AI requests
  it("Requirement D & E: Preserves discovery sessions across language switch with zero background fetches", async () => {
    // Session state seeder component
    const SessionTester = () => {
      const session = useDiscoverySession();
      const { locale } = useI18n();

      const seedAll = () => {
        session.setTrending((prev) => ({
          ...prev,
          status: "success",
          period: "weekly",
          limit: 30,
          response: {
            period: "weekly",
            requested_limit: 30,
            returned_count: 1,
            items: [sampleTrendingItem],
            enrichment_summary: { attempted: 1, succeeded: 1, failed: 0, rate_limited: false, rate_limit_reset: null },
            is_short_source: false,
            partial_warning: null,
          },
        }));

        session.setSearch((prev) => ({
          ...prev,
          status: "success",
          query: "awesome-rust",
          response: {
            total_count: 1,
            items: [sampleTrendingItem],
          },
        }));

        session.setAdd((prev) => ({
          ...prev,
          urlInput: "https://example.com/blog",
          mode: "article",
          articleResult: {
            article_url: "https://example.com/blog",
            discovered_count: 1,
            processed_count: 1,
            succeeded_count: 1,
            failed_count: 0,
            skipped_count: 0,
            is_truncated: false,
            items: [{
              github_repo_id: 101,
              github_url: "https://github.com/test-org/sample-repo",
              name: "sample-repo",
              description: "An open source project",
              stars: 1234,
              license: "MIT",
              topics: ["rust"],
              pushed_at: "2026-03-01T12:00:00Z",
              category: "Developer Tools",
              releases: 5,
              languages: { Rust: 100 },
            }],
          },
        }));
      };

      return (
        <div>
          <button data-testid="seed-btn" onClick={seedAll}>
            Seed Sessions
          </button>
          <span data-testid="trending-repo-name">
            {session.trending.response?.items[0]?.name || "none"}
          </span>
          <span data-testid="trending-period">{session.trending.period}</span>
          <span data-testid="search-query">{session.search.query}</span>
          <span data-testid="add-url">{session.add.urlInput}</span>
          <span data-testid="add-mode">{session.add.mode}</span>
          <span data-testid="active-locale">{locale}</span>
          <SettingsPage />
        </div>
      );
    };

    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SessionTester />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    // Seed session data
    fireEvent.click(screen.getByTestId("seed-btn"));
    expect(screen.getByTestId("trending-repo-name")).toHaveTextContent("sample-repo");
    expect(screen.getByTestId("trending-period")).toHaveTextContent("weekly");
    expect(screen.getByTestId("search-query")).toHaveTextContent("awesome-rust");
    expect(screen.getByTestId("add-url")).toHaveTextContent("https://example.com/blog");
    expect(screen.getByTestId("add-mode")).toHaveTextContent("article");

    // Clear call counts prior to switching locale
    vi.clearAllMocks();
    vi.mocked(tauriService.updateUiLanguage).mockResolvedValue({
      schema_version: 1,
      ui_language: "en",
      github: { default_search_limit: 20, default_trending_period: "daily" },
      ai: { enabled: false, provider: null, base_url: null, model: null },
    });

    // Change language in Settings
    const select = await screen.findByTestId("ui-language-select");
    fireEvent.change(select, { target: { value: "en" } });

    await waitFor(() => {
      expect(screen.getByTestId("active-locale")).toHaveTextContent("en");
    });

    // Verify session contents remain completely intact!
    expect(screen.getByTestId("trending-repo-name")).toHaveTextContent("sample-repo");
    expect(screen.getByTestId("trending-period")).toHaveTextContent("weekly");
    expect(screen.getByTestId("search-query")).toHaveTextContent("awesome-rust");
    expect(screen.getByTestId("add-url")).toHaveTextContent("https://example.com/blog");
    expect(screen.getByTestId("add-mode")).toHaveTextContent("article");

    // REQUIREMENT E: Verify ZERO hidden network or AI requests were triggered
    expect(tauriService.fetchTrending).not.toHaveBeenCalled();
    expect(tauriService.searchGitHubRepositories).not.toHaveBeenCalled();
    expect(tauriService.searchAiGitHubRepositories).not.toHaveBeenCalled();
    expect(tauriService.importRepositoriesFromArticle).not.toHaveBeenCalled();
    expect(tauriService.refreshFavorite).not.toHaveBeenCalled();
  });

  // Requirement F: Repository data immutability
  it("Requirement F: Repository content is never translated or altered by locale change", () => {
    const TestCards = () => {
      return (
        <div>
          <RepositoryCard repo={sampleTrendingItem} isFavorited={false} />
        </div>
      );
    };

    render(
      <I18nProvider>
        <TestCards />
      </I18nProvider>
    );

    // Chinese: Repo name, description, URL, programming language stay as-is
    expect(screen.getByText("sample-repo")).toBeInTheDocument();
    expect(screen.getByText("An open source project")).toBeInTheDocument();
    expect(screen.getByText("https://github.com/test-org/sample-repo")).toBeInTheDocument();
    expect(screen.getByText("Rust")).toBeInTheDocument();
    expect(screen.getByText("TypeScript")).toBeInTheDocument();
  });

  // Requirement G: Representative UI Coverage across all application domains
  it("Requirement G: Representative UI coverage resolves properly in en, zh-Hans, and zh-Hant", async () => {
    const RepresentativeTester = () => {
      const { t } = useI18n();
      return (
        <div>
          {/* Navigation */}
          <span data-testid="nav-label">{t("nav.trending")}</span>
          {/* Trending */}
          <span data-testid="trending-title">{t("trending.title")}</span>
          {/* Search */}
          <span data-testid="search-title">{t("search.title")}</span>
          {/* Add */}
          <span data-testid="add-title">{t("add.title")}</span>
          {/* S3 Article Import */}
          <span data-testid="article-summary">{t("add.articleResultsTitle")}</span>
          <span data-testid="article-truncated">{t("add.truncatedWarning")}</span>
          {/* Library */}
          <span data-testid="library-title">{t("library.title")}</span>
          {/* Favorite Detail */}
          <span data-testid="detail-title">{t("detail.title")}</span>
          <span data-testid="detail-save">{t("detail.saveAndExitBtn")}</span>
          {/* Settings */}
          <span data-testid="settings-title">{t("settings.title")}</span>
          {/* Common */}
          <span data-testid="common-save">{t("common.save")}</span>
          {/* S2 Metadata */}
          <span data-testid="s2-releases">{t("card.releasesUnknown")}</span>
          <span data-testid="s2-languages">{t("card.languagesUnknown")}</span>
        </div>
      );
    };

    const Harness = () => {
      const { setLocale } = useI18n();
      return (
        <div>
          <button data-testid="to-en" onClick={() => setLocale("en")}>To EN</button>
          <button data-testid="to-zh-hant" onClick={() => setLocale("zh-Hant")}>To ZH-HANT</button>
          <button data-testid="to-zh-hans" onClick={() => setLocale("zh-Hans")}>To ZH-HANS</button>
          <RepresentativeTester />
        </div>
      );
    };

    render(
      <I18nProvider>
        <Harness />
      </I18nProvider>
    );

    // Initial zh-Hans checks
    expect(screen.getByTestId("nav-label")).toHaveTextContent("热门项目");
    expect(screen.getByTestId("trending-title")).toHaveTextContent("热门项目");
    expect(screen.getByTestId("search-title")).toHaveTextContent("搜索项目");
    expect(screen.getByTestId("add-title")).toHaveTextContent("添加项目");
    expect(screen.getByTestId("article-summary")).toHaveTextContent("文章提取结果");
    expect(screen.getByTestId("article-truncated")).toHaveTextContent("页面中包含较多链接");
    expect(screen.getByTestId("library-title")).toHaveTextContent("项目浏览");
    expect(screen.getByTestId("detail-title")).toHaveTextContent("项目详情");
    expect(screen.getByTestId("detail-save")).toHaveTextContent("保存退出");
    expect(screen.getByTestId("settings-title")).toHaveTextContent("设置");
    expect(screen.getByTestId("s2-releases")).toHaveTextContent("Releases —");
    expect(screen.getByTestId("s2-languages")).toHaveTextContent("Languages —");

    // Switch to English
    fireEvent.click(screen.getByTestId("to-en"));
    await waitFor(() => {
      expect(screen.getByTestId("nav-label")).toHaveTextContent("Trending");
      expect(screen.getByTestId("trending-title")).toHaveTextContent("Trending Repositories");
      expect(screen.getByTestId("search-title")).toHaveTextContent("Search Repositories");
      expect(screen.getByTestId("add-title")).toHaveTextContent("Add Repository");
      expect(screen.getByTestId("article-summary")).toHaveTextContent("Article Extraction Results");
      expect(screen.getByTestId("article-truncated")).toHaveTextContent("Page contains many links");
      expect(screen.getByTestId("library-title")).toHaveTextContent("Repository Library");
      expect(screen.getByTestId("detail-title")).toHaveTextContent("Repository Details");
      expect(screen.getByTestId("detail-save")).toHaveTextContent("Save & Exit");
      expect(screen.getByTestId("settings-title")).toHaveTextContent("Settings");
    });

    // Switch to Traditional Chinese
    fireEvent.click(screen.getByTestId("to-zh-hant"));
    await waitFor(() => {
      expect(screen.getByTestId("nav-label")).toHaveTextContent("熱門專案");
      expect(screen.getByTestId("trending-title")).toHaveTextContent("熱門專案");
      expect(screen.getByTestId("search-title")).toHaveTextContent("搜尋專案");
      expect(screen.getByTestId("add-title")).toHaveTextContent("新增專案");
      expect(screen.getByTestId("article-summary")).toHaveTextContent("文章擷取結果");
      expect(screen.getByTestId("library-title")).toHaveTextContent("專案瀏覽");
      expect(screen.getByTestId("detail-title")).toHaveTextContent("專案詳情");
      expect(screen.getByTestId("detail-save")).toHaveTextContent("儲存退出");
      expect(screen.getByTestId("settings-title")).toHaveTextContent("設定");
    });
  });

  // Requirement H: Fallback behavior
  it("Requirement H: Missing key safely falls back to English canonical value or raw key path", () => {
    const FallbackTester = () => {
      const { t } = useI18n();
      return (
        <div>
          <span data-testid="valid-key">{t("common.save")}</span>
          <span data-testid="missing-key">{t("nonexistent.nested.key")}</span>
          <span data-testid="interpolated-key">{t("trending.selectCount", { count: 5 })}</span>
        </div>
      );
    };

    render(
      <I18nProvider>
        <FallbackTester />
      </I18nProvider>
    );

    expect(screen.getByTestId("valid-key")).toHaveTextContent("保存");
    expect(screen.getByTestId("missing-key")).toHaveTextContent("nonexistent.nested.key");
    expect(screen.getByTestId("interpolated-key")).toHaveTextContent("已选中 5 项");
  });
});
