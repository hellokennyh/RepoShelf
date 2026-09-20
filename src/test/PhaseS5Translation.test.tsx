import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import App from "../App";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { FavoriteItem } from "../types/library";
import { I18nProvider } from "../i18n/I18nContext";

const sampleTrendingItem1 = {
  name: "tauri",
  description: "Build smaller, faster, and more secure desktop applications with a web frontend.",
  stars: 85000,
  license: "Apache-2.0",
  topics: ["rust", "desktop"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
  trending_rank: 1,
  period_stars: 120,
  github_repo_id: 101,
  github_url: "https://github.com/tauri-apps/tauri",
};

const sampleTrendingItem2 = {
  name: "tokio",
  description: "An asynchronous runtime for the Rust programming language.",
  stars: 25000,
  license: "MIT",
  topics: ["rust", "async"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Libraries & Frameworks",
  trending_rank: 2,
  period_stars: 80,
  github_repo_id: 102,
  github_url: "https://github.com/tokio-rs/tokio",
};

const sampleTrendingResponse = {
  period: "daily",
  requested_limit: 10,
  returned_count: 2,
  items: [sampleTrendingItem1, sampleTrendingItem2],
  enrichment_summary: {
    attempted: 2,
    succeeded: 2,
    failed: 0,
    rate_limited: false,
    rate_limit_reset: null,
  },
  is_short_source: false,
  partial_warning: null,
};

let capturedFavoriteInput: unknown = null;
let capturedMetadataUpdate: unknown = null;
let mockTranslateHandler: ((desc: string, locale: string) => Promise<unknown>) | null = null;
const favoritedSet = new Set<string>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_app_info":
        return Promise.resolve({ name: "RepoShelf", phase: "v1.1.0 S5" });
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: true, provider: "ollama", base_url: "http://localhost:11434", model: "qwen2.5:7b" },
          ui: { locale: "zh-Hans" },
        });
      case "get_trending":
        return Promise.resolve(sampleTrendingResponse);
      case "github_search_repositories":
        return Promise.resolve({ items: [sampleTrendingItem1], total_count: 1 });
      case "github_get_repository":
        return Promise.resolve(sampleTrendingItem1);
      case "check_favorite_identities_batch": {
        const repoIds = (args?.repoIds || args?.repo_ids) as number[];
        const urls = args?.urls as string[];
        const matchedRepoIds: number[] = [];
        const matchedUrls: string[] = [];
        if (repoIds && Array.isArray(repoIds)) {
          for (const id of repoIds) {
            if (favoritedSet.has(String(id))) matchedRepoIds.push(id);
          }
        }
        if (urls && Array.isArray(urls)) {
          for (const url of urls) {
            if (favoritedSet.has(url)) matchedUrls.push(url);
          }
        }
        return Promise.resolve({
          matched_repo_ids: matchedRepoIds,
          matched_urls: matchedUrls,
        });
      }
      case "favorite_repository": {
        capturedFavoriteInput = args?.input;
        const input = args?.input as { github_repo_id?: number; github_url: string };
        if (input) {
          if (input.github_repo_id) favoritedSet.add(String(input.github_repo_id));
          if (input.github_url) favoritedSet.add(input.github_url);
        }
        return Promise.resolve({
          id: 99,
          github_repo_id: input?.github_repo_id ?? null,
          github_url: input?.github_url,
          name: "tauri",
          category: "Developer Tools",
          stars: 85000,
          tags: [],
          notes: "",
          favorited_at: "2026-03-01T12:00:00Z",
          metadata_refreshed_at: "2026-03-01T12:00:00Z",
        });
      }
      case "translate_repository_description": {
        const desc = (args?.text ?? args?.description) as string;
        const targetLocale = (args?.targetLocale ?? args?.target_locale) as string;
        if (mockTranslateHandler) {
          return mockTranslateHandler(desc, targetLocale);
        }
        return Promise.resolve({
          action: "translated",
          target_locale: targetLocale,
          translated_text: `[${targetLocale}] 翻译: ${desc}`,
          source_hash: "mockhash123",
          provider: "ollama",
          model: "qwen2.5:7b",
        });
      }
      case "update_favorite_user_metadata": {
        capturedMetadataUpdate = args?.input;
        return Promise.resolve({
          id: 101,
          github_repo_id: 101,
          github_url: "https://github.com/tauri-apps/tauri",
          name: "tauri",
          description: "Build smaller, faster, and more secure desktop applications with a web frontend.",
          category: "Developer Tools",
          stars: 85000,
          tags: [],
          notes: "",
          favorited_at: "2026-03-01T12:00:00Z",
          metadata_refreshed_at: "2026-03-01T12:00:00Z",
          description_translation: {
            text: "使用 Web 前端构建体积更小、速度更快、更安全的桌面应用程序。",
            target_locale: "zh-Hans",
          },
          description_translation_source_hash: "mockhash123",
          translation_status: "valid",
        });
      }
      case "get_ai_search_readiness":
        return Promise.resolve({ ready: true, reason: null });
      case "get_favorites":
        return Promise.resolve([]);
      case "get_distinct_tags":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  }),
}));

describe("Phase S5: Description Translation Integration", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    favoritedSet.clear();
    capturedFavoriteInput = null;
    capturedMetadataUpdate = null;
    mockTranslateHandler = null;
  });

  it("discovery batch translation runs across items and displays translated text", async () => {
    render(<App />);

    // Navigate to Trending
    const trendingNav = await screen.findByTestId("nav-trending");
    fireEvent.click(trendingNav);

    // Fetch trending items
    const fetchBtn = await screen.findByTestId("trending-fetch-btn");
    fireEvent.click(fetchBtn);

    // Verify trending cards appear
    expect(await screen.findByText("tauri")).toBeInTheDocument();
    expect(screen.getByText("tokio")).toBeInTheDocument();

    // Batch translate button should exist
    const batchBtn = screen.getByTestId("trending-batch-translate-btn");
    expect(batchBtn).toBeInTheDocument();

    // Click batch translate
    fireEvent.click(batchBtn);

    // Wait for translations to appear in cards
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });
    expect(screen.getByText(/\[zh-Hans\] 翻译: An asynchronous runtime/)).toBeInTheDocument();
  });

  it("batch translation preserves session state when navigating between pages", async () => {
    render(<App />);

    // Go to Trending & Fetch
    fireEvent.click(await screen.findByTestId("nav-trending"));
    fireEvent.click(await screen.findByTestId("trending-fetch-btn"));
    await screen.findByText("tauri");

    // Translate
    fireEvent.click(screen.getByTestId("trending-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });

    // Navigate to Library page
    fireEvent.click(screen.getByTestId("nav-library"));
    expect(screen.queryByText(/\[zh-Hans\] 翻译: Build smaller/)).not.toBeInTheDocument();

    // Navigate back to Trending
    fireEvent.click(screen.getByTestId("nav-trending"));

    // Translation preview should still be preserved without refetching
    expect(await screen.findByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
  });

  it("saving an uncollected repository atomically attaches the translation preview", async () => {
    render(<App />);

    fireEvent.click(await screen.findByTestId("nav-trending"));
    fireEvent.click(await screen.findByTestId("trending-fetch-btn"));
    await screen.findByText("tauri");

    // Translate
    fireEvent.click(screen.getByTestId("trending-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });

    // Find favorite button for tauri card
    const favButtons = screen.getAllByTestId("favorite-btn");
    expect(favButtons.length).toBeGreaterThan(0);
    fireEvent.click(favButtons[0]);

    // Verify favoriteRepository was called with translation payload
    await waitFor(() => {
      expect(capturedFavoriteInput).not.toBeNull();
    });

    const input = capturedFavoriteInput as Record<string, unknown>;
    expect(input.translation).toEqual({
      text: "[zh-Hans] 翻译: Build smaller, faster, and more secure desktop applications with a web frontend.",
      target_locale: "zh-Hans",
    });
  });

  it("already-favorited card displays guidance notice", async () => {
    // Mark tauri as already favorited
    favoritedSet.add("101");
    favoritedSet.add("https://github.com/tauri-apps/tauri");

    render(<App />);

    fireEvent.click(await screen.findByTestId("nav-trending"));
    fireEvent.click(await screen.findByTestId("trending-fetch-btn"));
    await screen.findByText("tauri");

    // Translate
    fireEvent.click(screen.getByTestId("trending-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });

    // Notice should be displayed on already-favorited card
    expect(
      screen.getByText(/已收藏项目请在详情中翻译并保存|Already favorited/i)
    ).toBeInTheDocument();
  });

  it("Favorite Detail Modal handles Translate, unsaved draft, and Save & Exit persistence", async () => {
    const mockFav: FavoriteItem = {
      id: 101,
      github_repo_id: 101,
      github_url: "https://github.com/tauri-apps/tauri",
      name: "tauri",
      description: "Build smaller, faster, and more secure desktop applications with a web frontend.",
      category: "Developer Tools",
      stars: 85000,
      license: "Apache-2.0",
      pushed_at: "2026-03-01T12:00:00Z",
      rating: null,
      rating_reason: null,
      rating_confidence: null,
      notes: "My notes",
      favorited_at: "2026-03-01T12:00:00Z",
      metadata_refreshed_at: "2026-03-01T12:00:00Z",
      tags: [],
      releases: 12,
      languages: { Rust: 75.5 },
      description_translation: null,
      description_translation_source_hash: null,
      translation_status: null,
    };

    const onClose = vi.fn();
    const onUpdated = vi.fn();

    render(
      <I18nProvider>
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFav}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={vi.fn()}
        />
      </I18nProvider>
    );

    // Translate button exists in modal
    await waitFor(() => {
      expect(screen.getByTestId("detail-translate-btn")).not.toBeDisabled();
    });

    // Click translate
    fireEvent.click(screen.getByTestId("detail-translate-btn"));

    // Wait for draft translation
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });

    // Draft notice should show
    expect(screen.getByText(/未保存的翻译草稿|Unsaved translation draft/i)).toBeInTheDocument();

    // DB was NOT written yet!
    expect(capturedMetadataUpdate).toBeNull();

    // Save & Exit
    const saveBtn = screen.getByTestId("detail-save-btn");
    fireEvent.click(saveBtn);

    // Modal calls update_favorite_user_metadata with translation_patch Replace
    await waitFor(() => {
      expect(capturedMetadataUpdate).not.toBeNull();
    });

    const updateInput = capturedMetadataUpdate as Record<string, unknown>;
    expect(updateInput.translation_patch).toEqual({
      action: "replace",
      text: "[zh-Hans] 翻译: Build smaller, faster, and more secure desktop applications with a web frontend.",
      target_locale: "zh-Hans",
      source_text: mockFav.description,
    });
  });

  it("Favorite Detail Modal discards draft on close/cancel", async () => {
    const mockFav: FavoriteItem = {
      id: 101,
      github_repo_id: 101,
      github_url: "https://github.com/tauri-apps/tauri",
      name: "tauri",
      description: "Build smaller, faster, and more secure desktop applications with a web frontend.",
      category: "Developer Tools",
      stars: 85000,
      license: "Apache-2.0",
      pushed_at: "2026-03-01T12:00:00Z",
      rating: null,
      rating_reason: null,
      rating_confidence: null,
      notes: "My notes",
      favorited_at: "2026-03-01T12:00:00Z",
      metadata_refreshed_at: "2026-03-01T12:00:00Z",
      tags: [],
      releases: 12,
      languages: { Rust: 75.5 },
      description_translation: null,
      description_translation_source_hash: null,
      translation_status: null,
    };

    const onClose = vi.fn();
    const onUpdated = vi.fn();

    render(
      <I18nProvider>
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFav}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={vi.fn()}
        />
      </I18nProvider>
    );

    // Click translate
    await waitFor(() => {
      expect(screen.getByTestId("detail-translate-btn")).not.toBeDisabled();
    });
    fireEvent.click(screen.getByTestId("detail-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });

    // Close without saving
    fireEvent.click(screen.getByTestId("detail-close-btn"));
    expect(onClose).toHaveBeenCalled();
    expect(capturedMetadataUpdate).toBeNull();
  });

  it("handles unchanged translation action gracefully without crashing or duplicating", async () => {
    mockTranslateHandler = vi.fn().mockResolvedValue({
      action: "unchanged",
      target_locale: "zh-Hans",
      translated_text: null,
      source_hash: "mockhash_unchanged",
      provider: "ollama",
      model: "qwen2.5:7b",
    });

    render(<App />);

    fireEvent.click(await screen.findByTestId("nav-trending"));
    fireEvent.click(await screen.findByTestId("trending-fetch-btn"));
    await screen.findByText("tauri");

    fireEvent.click(screen.getByTestId("trending-batch-translate-btn"));

    await waitFor(() => {
      expect(mockTranslateHandler).toHaveBeenCalled();
    });

    // Unchanged notice should be visible in card
    await waitFor(() => {
      const notices = screen.getAllByTestId("repo-card-translation-unchanged");
      expect(notices.length).toBeGreaterThan(0);
    });
  });

  it("skips empty description items before calling translation service", async () => {
    mockTranslateHandler = vi.fn().mockImplementation((desc, targetLocale) => {
      return Promise.resolve({
        action: "translated",
        target_locale: targetLocale,
        translated_text: `translated: ${desc}`,
        source_hash: "mockhash",
        provider: "ollama",
        model: "qwen2.5:7b",
      });
    });

    // Render FavoriteDetailModal with empty description
    const mockFavEmptyDesc: FavoriteItem = {
      id: 102,
      github_repo_id: 102,
      github_url: "https://github.com/foo/bar",
      name: "bar",
      description: "",
      category: "Developer Tools",
      stars: 10,
      license: null,
      pushed_at: null,
      rating: null,
      rating_reason: null,
      rating_confidence: null,
      notes: "",
      favorited_at: "2026-03-01T12:00:00Z",
      metadata_refreshed_at: "2026-03-01T12:00:00Z",
      tags: [],
      releases: null,
      languages: null,
      description_translation: null,
      description_translation_source_hash: null,
      translation_status: null,
    };

    render(
      <I18nProvider>
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFavEmptyDesc}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      </I18nProvider>
    );

    // Translation button should be disabled for empty description
    const transBtn = screen.getByTestId("detail-translate-btn");
    expect(transBtn).toBeDisabled();
    expect(mockTranslateHandler).not.toHaveBeenCalled();
  });

  it("Favorite Detail displays stale notice when translation status is stale", async () => {
    const mockFavStale: FavoriteItem = {
      id: 103,
      github_repo_id: 103,
      github_url: "https://github.com/foo/stale-repo",
      name: "stale-repo",
      description: "Brand new description from GitHub.",
      category: "Developer Tools",
      stars: 100,
      license: null,
      pushed_at: null,
      rating: null,
      rating_reason: null,
      rating_confidence: null,
      notes: "",
      favorited_at: "2026-03-01T12:00:00Z",
      metadata_refreshed_at: "2026-03-01T12:00:00Z",
      tags: [],
      releases: null,
      languages: null,
      description_translation: {
        text: "旧的翻译内容",
        target_locale: "zh-Hans",
      },
      description_translation_source_hash: "old_outdated_hash",
      translation_status: "stale",
    };

    render(
      <I18nProvider>
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFavStale}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      </I18nProvider>
    );

    // Stale notice should be visible
    expect(
      screen.getByText(/原文已更新，如需更新请点击翻译按钮|The original description has changed/i)
    ).toBeInTheDocument();

    // Old translation is still visible
    expect(screen.getByText("旧的翻译内容")).toBeInTheDocument();
  });

  it("Search page runs batch translation and shows translation on search cards", async () => {
    render(<App />);

    // Navigate to Search
    fireEvent.click(await screen.findByTestId("nav-search"));

    // Type in query and submit search
    const searchInput = await screen.findByTestId("search-query-input");
    fireEvent.change(searchInput, { target: { value: "tauri" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    // Wait for search results
    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Batch translate button should exist
    const batchBtn = screen.getByTestId("search-batch-translate-btn");
    expect(batchBtn).toBeInTheDocument();

    // Trigger batch translation
    fireEvent.click(batchBtn);

    // Verify translated description appears in search results
    await waitFor(() => {
      expect(screen.getByText(/\[zh-Hans\] 翻译: Build smaller/)).toBeInTheDocument();
    });
  });
});
