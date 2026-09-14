import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import App from "../App";
import { Layout } from "../components/Layout";
import { TrendingResponse } from "../types/trending";
import { GitHubRepository, RepositorySearchResponse } from "../types/github";

const sampleTrendingItem = {
  name: "tauri",
  description: "Build smaller, faster apps",
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

const sampleTrendingResponse: TrendingResponse = {
  period: "daily",
  requested_limit: 10,
  returned_count: 1,
  items: [sampleTrendingItem],
  enrichment_summary: {
    attempted: 1,
    succeeded: 1,
    failed: 0,
    rate_limited: false,
    rate_limit_reset: null,
  },
  is_short_source: false,
  partial_warning: null,
};

const sampleSearchRepo: GitHubRepository = {
  github_repo_id: 101,
  github_url: "https://github.com/tauri-apps/tauri",
  name: "tauri",
  description: "Build smaller, faster apps",
  stars: 85000,
  license: "Apache-2.0",
  topics: ["rust", "desktop"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
};

const sampleSearchResponse: RepositorySearchResponse = {
  items: [sampleSearchRepo],
  total_count: 1,
};

let batchCheckCount = 0;
let lastBatchCheckArgs: { repo_ids?: number[]; urls?: string[] } | null = null;
const favoritedSet = new Set<string>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_app_info":
        return Promise.resolve({ name: "RepoShelf", phase: "Phase 0" });
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: false, provider: null, base_url: null, model: null },
        });
      case "get_trending":
        return Promise.resolve(sampleTrendingResponse);
      case "github_search_repositories":
        return Promise.resolve(sampleSearchResponse);
      case "github_get_repository":
        return Promise.resolve(sampleSearchRepo);
      case "check_favorite_identities_batch": {
        batchCheckCount++;
        const repoIds = (args?.repoIds || args?.repo_ids) as number[];
        const urls = (args?.urls) as string[];
        lastBatchCheckArgs = {
          repo_ids: repoIds,
          urls: urls,
        };
        const matchedRepoIds: number[] = [];
        const matchedUrls: string[] = [];
        if (repoIds && Array.isArray(repoIds)) {
          for (const id of repoIds) {
            if (favoritedSet.has(String(id))) {
              matchedRepoIds.push(id);
            }
          }
        }
        if (urls && Array.isArray(urls)) {
          for (const url of urls) {
            if (favoritedSet.has(url)) {
              matchedUrls.push(url);
            }
          }
        }
        return Promise.resolve({
          matched_repo_ids: matchedRepoIds,
          matched_urls: matchedUrls,
        });
      }
      case "favorite_repository": {
        const input = args?.input as { github_repo_id?: number; github_url: string };
        if (input) {
          if (input.github_repo_id) favoritedSet.add(String(input.github_repo_id));
          if (input.github_url) favoritedSet.add(input.github_url);
        }
        return Promise.resolve({
          status: "succeeded",
          favorite_id: 999,
          github_url: input?.github_url,
        });
      }
      case "delete_favorite": {
        favoritedSet.clear();
        return Promise.resolve();
      }
      case "list_favorites":
        return Promise.resolve({
          items: favoritedSet.size > 0 ? [
            {
              id: 999,
              github_repo_id: 101,
              github_url: "https://github.com/tauri-apps/tauri",
              name: "tauri",
              description: "Build smaller, faster apps",
              stars: 85000,
              license: "Apache-2.0",
              topics: ["rust"],
              pushed_at: "2026-03-01T12:00:00Z",
              user_tags: [],
              ai_tags: [],
              category: "Developer Tools",
              custom_note: null,
              created_at: "2026-03-01T12:00:00Z",
              updated_at: "2026-03-01T12:00:00Z",
            }
          ] : [],
          total: favoritedSet.size > 0 ? 1 : 0,
          page: 1,
          page_size: 20,
        });
      case "get_categories":
        return Promise.resolve(["Developer Tools"]);
      case "get_all_tags":
        return Promise.resolve([]);
      case "get_ai_search_readiness":
        return Promise.resolve({ ready: false, reason: "AI未配置" });
      default:
        return Promise.resolve({});
    }
  }),
}));

describe("S1B UI Session Retention & Favorite Sync Suite", () => {
  beforeEach(() => {
    batchCheckCount = 0;
    lastBatchCheckArgs = null;
    favoritedSet.clear();
  });

  it("Requirement 1: Layout expands main content with max-w-[1920px] and w-full", () => {
    const { container } = render(
      <Layout activeTab="trending" onTabChange={() => {}}>
        <div data-testid="child-content">Content</div>
      </Layout>
    );
    const mainEl = container.querySelector("main");
    expect(mainEl).toBeTruthy();
    expect(mainEl?.className).toContain("max-w-[1920px]");
    expect(mainEl?.className).toContain("w-full");
    expect(mainEl?.className).not.toContain("max-w-6xl");
  });

  it("Requirement 2: Trending default limit is 10 items (R03)", () => {
    render(<App />);
    const limitSelect = screen.getByTestId("trending-limit-select") as HTMLSelectElement;
    expect(limitSelect.value).toBe("10");
  });

  it("Requirement 3: Preserves Trending session across tab switching", async () => {
    render(<App />);

    // Trigger fetch on trending
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));
    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Switch to Search page
    fireEvent.click(screen.getByTestId("nav-search"));
    expect(screen.getByText("GitHub Repository Search")).toBeInTheDocument();
    expect(screen.queryByText("tauri")).not.toBeInTheDocument();

    // Switch back to Trending page
    fireEvent.click(screen.getByTestId("nav-trending"));
    expect(screen.getByText("GitHub Trending")).toBeInTheDocument();

    // Verify session persisted: items still present without re-fetching
    expect(screen.getByText("tauri")).toBeInTheDocument();
    const limitSelect = screen.getByTestId("trending-limit-select") as HTMLSelectElement;
    expect(limitSelect.value).toBe("10");
  });

  it("Requirement 4: Preserves Search session across tab switching", async () => {
    render(<App />);

    // Go to Search
    fireEvent.click(screen.getByTestId("nav-search"));
    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "rust desktop" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Switch to Add page
    fireEvent.click(screen.getByTestId("nav-add"));
    expect(screen.getByText("Add GitHub Repository")).toBeInTheDocument();

    // Switch back to Search page
    fireEvent.click(screen.getByTestId("nav-search"));
    expect(screen.getByText("GitHub Repository Search")).toBeInTheDocument();

    // Query and results are preserved
    expect((screen.getByTestId("search-query-input") as HTMLInputElement).value).toBe("rust desktop");
    expect(screen.getByText("tauri")).toBeInTheDocument();
  });

  it("Requirement 5: Preserves Add session input and preview across tab switching", async () => {
    render(<App />);

    // Go to Add
    fireEvent.click(screen.getByTestId("nav-add"));
    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, { target: { value: "https://github.com/tauri-apps/tauri" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Switch to Trending
    fireEvent.click(screen.getByTestId("nav-trending"));
    expect(screen.getByText("GitHub Trending")).toBeInTheDocument();

    // Switch back to Add
    fireEvent.click(screen.getByTestId("nav-add"));
    expect((screen.getByTestId("add-repo-url-input") as HTMLInputElement).value).toBe("https://github.com/tauri-apps/tauri");
    expect(screen.getByText("tauri")).toBeInTheDocument();
  });

  it("Requirement 6: Batch reconciliation identifies favorited status without N+1 per-card queries", async () => {
    favoritedSet.add("101");
    favoritedSet.add("https://github.com/tauri-apps/tauri");

    render(<App />);

    // Fetch trending
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Verify batch query called with repo_ids and urls
    expect(batchCheckCount).toBeGreaterThan(0);
    expect(lastBatchCheckArgs?.repo_ids).toContain(101);
    expect(lastBatchCheckArgs?.urls).toContain("https://github.com/tauri-apps/tauri");

    // The card should reflect "已收藏"
    await waitFor(() => {
      expect(screen.getByText("已收藏")).toBeInTheDocument();
    });
  });

  it("Requirement 7: Favoriting in Search synchronizes to Trending and Add", async () => {
    render(<App />);

    // Fetch in Trending first
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));
    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });
    expect(screen.queryByText("已收藏")).not.toBeInTheDocument();

    // Switch to Search and fetch
    fireEvent.click(screen.getByTestId("nav-search"));
    fireEvent.change(screen.getByTestId("search-query-input"), { target: { value: "tauri" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Favorite in Search
    const favButtons = screen.getAllByTestId("favorite-btn");
    expect(favButtons.length).toBeGreaterThan(0);
    fireEvent.click(favButtons[0]);

    await waitFor(() => {
      expect(screen.getByText("已收藏")).toBeInTheDocument();
    });

    // Switch to Trending: should now also show "已收藏"
    fireEvent.click(screen.getByTestId("nav-trending"));
    await waitFor(() => {
      expect(screen.getByText("已收藏")).toBeInTheDocument();
    });

    // Switch to Add: fetch metadata, preview should show favorited status
    fireEvent.click(screen.getByTestId("nav-add"));
    fireEvent.change(screen.getByTestId("add-repo-url-input"), { target: { value: "https://github.com/tauri-apps/tauri" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
      expect(screen.getByText("★ 已在本地库")).toBeInTheDocument();
    });
  });
});
