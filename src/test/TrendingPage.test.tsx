import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { TrendingPage } from "../pages/TrendingPage";
import * as tauriService from "../services/tauri";
import { TrendingResponse } from "../types/trending";

// Mock tauri service
vi.mock("../services/tauri", () => ({
  fetchSettings: vi.fn(),
  fetchTrending: vi.fn(),
  openGitHubUrl: vi.fn(),
}));

const mockSuccessfulTrendingResponse: TrendingResponse = {
  period: "daily",
  requested_limit: 20,
  returned_count: 2,
  items: [
    {
      github_repo_id: 101,
      github_url: "https://github.com/facebook/react",
      name: "react",
      description: "The library for web and native user interfaces",
      category: "Libraries & Frameworks",
      stars: 220000,
      license: "MIT",
      topics: ["javascript", "ui", "frontend"],
      pushed_at: "2026-09-03T10:00:00Z",
      trending_rank: 1,
      period_stars: 450,
    },
    {
      github_repo_id: 102,
      github_url: "https://github.com/vuejs/core",
      name: "core",
      description: "Vue.js is a progressive JavaScript framework",
      category: "Web & SaaS",
      stars: 45000,
      license: "MIT",
      topics: ["framework", "vue"],
      pushed_at: "2026-09-02T10:00:00Z",
      trending_rank: 2,
      period_stars: 320,
    },
  ],
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

describe("TrendingPage — Requirements Verification Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.fetchSettings).mockResolvedValue({
      schema_version: 1,
      github: {
        default_search_limit: 20,
        default_trending_period: "daily",
      },
      ai: {
        enabled: false,
        provider: null,
        base_url: null,
        model: null,
      },
    });
  });

  // 1. Page render
  it("requirement 1: renders trending page layout and title correctly", () => {
    render(<TrendingPage />);
    expect(screen.getByText("热门项目")).toBeInTheDocument();
    expect(screen.getByText("GitHub Trending")).toBeInTheDocument();
    expect(screen.getByTestId("trending-fetch-btn")).toBeInTheDocument();
  });

  // 2. Period default value read from settings
  it("requirement 2: reads default trending period from settings", async () => {
    vi.mocked(tauriService.fetchSettings).mockResolvedValueOnce({
      schema_version: 1,
      github: {
        default_search_limit: 20,
        default_trending_period: "weekly",
      },
      ai: {
        enabled: false,
        provider: null,
        base_url: null,
        model: null,
      },
    });

    render(<TrendingPage />);
    const periodSelect = (await screen.findByTestId(
      "trending-period-select"
    )) as HTMLSelectElement;
    expect(periodSelect.value).toBe("weekly");
  });

  // 3. Today / Week / Month selection
  it("requirement 3: allows selecting Today, This week, and This month periods", async () => {
    render(<TrendingPage />);
    const periodSelect = (await screen.findByTestId(
      "trending-period-select"
    )) as HTMLSelectElement;

    fireEvent.change(periodSelect, { target: { value: "daily" } });
    expect(periodSelect.value).toBe("daily");

    fireEvent.change(periodSelect, { target: { value: "weekly" } });
    expect(periodSelect.value).toBe("weekly");

    fireEvent.change(periodSelect, { target: { value: "monthly" } });
    expect(periodSelect.value).toBe("monthly");
  });

  // 4. Limit options (10, 20, 30, 50)
  it("requirement 4: allows selecting limit options 10, 20, 30, 50 with default 20", async () => {
    render(<TrendingPage />);
    const limitSelect = (await screen.findByTestId(
      "trending-limit-select"
    )) as HTMLSelectElement;
    expect(limitSelect.value).toBe("20");

    fireEvent.change(limitSelect, { target: { value: "10" } });
    expect(limitSelect.value).toBe("10");

    fireEvent.change(limitSelect, { target: { value: "30" } });
    expect(limitSelect.value).toBe("30");

    fireEvent.change(limitSelect, { target: { value: "50" } });
    expect(limitSelect.value).toBe("50");
  });

  // 5. Idle state (no network fetch on mount)
  it("requirement 5: defaults to idle state and does NOT perform automatic network fetch on mount", () => {
    render(<TrendingPage />);
    expect(screen.getByTestId("trending-idle-state")).toBeInTheDocument();
    expect(tauriService.fetchTrending).not.toHaveBeenCalled();
  });

  // 6. Loading state
  it("requirement 6: displays loading indicator while fetching", async () => {
    let resolvePromise: (value: TrendingResponse) => void = () => {};
    const pendingPromise = new Promise<TrendingResponse>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(tauriService.fetchTrending).mockReturnValueOnce(pendingPromise);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    expect(screen.getByTestId("trending-loading-state")).toBeInTheDocument();
    expect(screen.getByText(/正在从 GitHub 获取热门项目/)).toBeInTheDocument();

    resolvePromise(mockSuccessfulTrendingResponse);
    await waitFor(() => {
      expect(screen.queryByTestId("trending-loading-state")).not.toBeInTheDocument();
    });
  });

  // 7. Success state
  it("requirement 7: renders results on successful fetch", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-results-list")).toBeInTheDocument();
      expect(screen.getByText("react")).toBeInTheDocument();
      expect(screen.getByText("core")).toBeInTheDocument();
    });
  });

  // 8. GitHub rank displayed
  it("requirement 8: displays trending rank prominently on each card", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const ranks = screen.getAllByTestId("trending-rank");
      expect(ranks).toHaveLength(2);
      expect(ranks[0]).toHaveTextContent("#1");
      expect(ranks[1]).toHaveTextContent("#2");
    });
  });

  // 9. API/Trending order preserved
  it("requirement 9: strictly preserves the GitHub Trending ranking order", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const cards = screen.getAllByTestId(/^repo-card-/);
      expect(cards).toHaveLength(2);
      expect(cards[0]).toHaveTextContent("react");
      expect(cards[1]).toHaveTextContent("core");
    });
  });

  // 10. Period stars displayed
  it("requirement 10: displays parsed period stars with correct period label", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const pStars = screen.getAllByTestId("period-stars");
      expect(pStars).toHaveLength(2);
      expect(pStars[0]).toHaveTextContent("+450 stars today");
      expect(pStars[1]).toHaveTextContent("+320 stars today");
    });
  });

  // 11. Metadata fields rendered
  it("requirement 11: renders all required repository metadata fields", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("The library for web and native user interfaces")).toBeInTheDocument();
      expect(screen.getByText("Libraries & Frameworks")).toBeInTheDocument();
      expect(screen.getByText("220,000")).toBeInTheDocument();
      expect(screen.getByText("#javascript")).toBeInTheDocument();
      expect(screen.getByText("#ui")).toBeInTheDocument();
    });
  });

  // 12. Unknown fallback for missing fields
  it("requirement 12: falls back to 'Unknown' when optional metadata fields are missing", async () => {
    const degradedResponse: TrendingResponse = {
      period: "daily",
      requested_limit: 20,
      returned_count: 1,
      items: [
        {
          github_repo_id: null,
          github_url: "https://github.com/degraded/repo",
          name: "repo",
          description: null,
          category: "Other",
          stars: null,
          license: null,
          topics: [],
          pushed_at: null,
          trending_rank: 1,
          period_stars: null,
        },
      ],
      enrichment_summary: {
        attempted: 1,
        succeeded: 0,
        failed: 1,
        rate_limited: false,
        rate_limit_reset: null,
      },
      is_short_source: false,
      partial_warning: "部分 GitHub 项目信息未能补充，热门排名仍来自 GitHub Trending。",
    };

    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(degradedResponse);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("repo")).toBeInTheDocument();
      // Stars missing -> Unknown
      expect(screen.getByText("★").parentElement).toHaveTextContent("Unknown");
      // License missing -> Unknown
      expect(screen.getByText("License:").nextSibling).toHaveTextContent("Unknown");
      // Last active missing -> Unknown
      expect(screen.getByText("最后活跃:").nextSibling).toHaveTextContent("Unknown");
      // Empty topics fallback
      expect(screen.getByTestId("empty-topics")).toHaveTextContent("暂无标签");
    });
  });

  // 13. Empty state
  it("requirement 13: renders friendly empty state when no repositories are returned", async () => {
    const emptyResponse: TrendingResponse = {
      period: "daily",
      requested_limit: 20,
      returned_count: 0,
      items: [],
      enrichment_summary: {
        attempted: 0,
        succeeded: 0,
        failed: 0,
        rate_limited: false,
        rate_limit_reset: null,
      },
      is_short_source: false,
      partial_warning: null,
    };

    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(emptyResponse);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-empty-state")).toBeInTheDocument();
      expect(screen.getByText("暂无热门项目数据")).toBeInTheDocument();
    });
  });

  // 14. Network/parser error state
  it("requirement 14: renders error state with retry button on network or parse failure", async () => {
    vi.mocked(tauriService.fetchTrending).mockRejectedValueOnce(
      new Error("Failed to fetch trending HTML from https://github.com/trending")
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-error-state")).toBeInTheDocument();
      expect(screen.getByText("获取热门项目失败")).toBeInTheDocument();
      expect(screen.getByText(/Failed to fetch trending HTML/)).toBeInTheDocument();
    });
  });

  // 15. Source fewer than requested notice
  it("requirement 15: displays short-source notice when GitHub returns fewer items than requested", async () => {
    const shortResponse: TrendingResponse = {
      ...mockSuccessfulTrendingResponse,
      requested_limit: 50,
      returned_count: 2,
      is_short_source: true,
    };

    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(shortResponse);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const notice = screen.getByTestId("trending-short-source-notice");
      expect(notice).toHaveTextContent("当前数据源只返回了 2 个热门项目。");
    });
  });

  // 16. Partial enrichment warning
  it("requirement 16: displays partial enrichment warning when some repository metadata failed", async () => {
    const partialResponse: TrendingResponse = {
      ...mockSuccessfulTrendingResponse,
      partial_warning: "部分 GitHub 项目信息未能补充，热门排名仍来自 GitHub Trending。",
      enrichment_summary: {
        attempted: 2,
        succeeded: 1,
        failed: 1,
        rate_limited: false,
        rate_limit_reset: null,
      },
    };

    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(partialResponse);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const warning = screen.getByTestId("trending-partial-warning");
      expect(warning).toHaveTextContent("部分 GitHub 项目信息未能补充，热门排名仍来自 GitHub Trending。");
    });
  });

  // 17. Rate-limit partial warning
  it("requirement 17: displays rate limit notice and reset time in partial warning", async () => {
    const rateLimitedResponse: TrendingResponse = {
      ...mockSuccessfulTrendingResponse,
      partial_warning: "部分 GitHub 项目信息因 GitHub API 访问限额未能补充，热门排名仍来自 GitHub Trending。",
      enrichment_summary: {
        attempted: 2,
        succeeded: 1,
        failed: 1,
        rate_limited: true,
        rate_limit_reset: 1725450000,
      },
    };

    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(rateLimitedResponse);

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      const warning = screen.getByTestId("trending-partial-warning");
      expect(warning).toHaveTextContent("GitHub API 访问限额未能补充");
      expect(warning).toHaveTextContent("GitHub API 限额重置时间:");
    });
  });

  // 18. External GitHub link opens via opener
  it("requirement 18: invokes openGitHubUrl when external link is clicked", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-results-list")).toBeInTheDocument();
    });

    const openLinks = screen.getAllByTestId("open-github-link");
    fireEvent.click(openLinks[0]);

    expect(tauriService.openGitHubUrl).toHaveBeenCalledWith(
      "https://github.com/facebook/react"
    );
  });

  // 19. No Favorite action present
  it("requirement 19: does NOT provide any Favorite button or action on the trending page", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-results-list")).toBeInTheDocument();
    });

    // Verify there are no favorite buttons or actions
    expect(screen.queryByText("收藏")).not.toBeInTheDocument();
    expect(screen.queryByText("Favorite")).not.toBeInTheDocument();
    expect(screen.queryByTestId("favorite-btn")).not.toBeInTheDocument();
  });

  // 20. No language filter
  it("requirement 20: does NOT include any programming language or spoken language filter", () => {
    render(<TrendingPage />);
    expect(screen.queryByText("Language")).not.toBeInTheDocument();
    expect(screen.queryByText("编程语言")).not.toBeInTheDocument();
    expect(screen.queryByTestId("language-filter")).not.toBeInTheDocument();
  });

  // 21. No pagination or custom sorting
  it("requirement 21: does NOT provide pagination controls or custom sorting options", async () => {
    vi.mocked(tauriService.fetchTrending).mockResolvedValueOnce(
      mockSuccessfulTrendingResponse
    );

    render(<TrendingPage />);
    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("trending-results-list")).toBeInTheDocument();
    });

    expect(screen.queryByText("Next Page")).not.toBeInTheDocument();
    expect(screen.queryByText("上一页")).not.toBeInTheDocument();
    expect(screen.queryByText("下一页")).not.toBeInTheDocument();
    expect(screen.queryByTestId("pagination")).not.toBeInTheDocument();
    expect(screen.queryByText("Sort by")).not.toBeInTheDocument();
  });
});
