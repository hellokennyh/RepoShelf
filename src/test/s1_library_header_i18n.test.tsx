import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { LibraryPage } from "../pages/LibraryPage";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";
import { I18nProvider } from "../i18n/I18nContext";
import { setTestFallbackLocale } from "../i18n/localeState";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";

vi.mock("../services/tauri", () => ({
  listFavorites: vi.fn(),
  listDistinctTags: vi.fn(),
  openGitHubUrl: vi.fn(),
  refreshFavoritesBatch: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: false, reason: "AI not configured" }),
  suggestFavoriteAiMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
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

const mockFavorites: FavoriteItem[] = [
  {
    id: 101,
    github_repo_id: 1001,
    github_url: "https://github.com/org/auto-tool",
    name: "auto-tool",
    description: "An automated workflow tool",
    category: "Automation",
    stars: 3200,
    license: "MIT",
    pushed_at: "2026-03-01T12:00:00Z",
    rating: 5,
    notes: "Great auto tool",
    favorited_at: "2026-03-01T10:00:00Z",
    metadata_refreshed_at: "2026-03-01T10:00:00Z",
    repo_created_at: "2024-03-15T08:00:00Z",
    tags: [
      { id: 1, favorite_id: 101, tag: "ci", source: "github" },
      { id: 2, favorite_id: 101, tag: "my-tag", source: "user" },
    ],
  },
  {
    id: 102,
    github_repo_id: 1002,
    github_url: "https://github.com/org/db-engine",
    name: "db-engine",
    description: "High performance database",
    category: "Data & Database",
    stars: 9800,
    license: "Apache-2.0",
    pushed_at: null,
    rating: null,
    notes: "",
    favorited_at: "2026-01-09T00:00:00Z",
    metadata_refreshed_at: "2026-01-09T00:00:00Z",
    repo_created_at: null,
    tags: [],
  },
];

describe("S1: Library Table Header i18n & Date Formatting (R02)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.listFavorites).mockResolvedValue(mockFavorites);
    vi.mocked(tauriService.listDistinctTags).mockResolvedValue([]);
  });

  const renderWithLocale = (loc: "zh-Hans" | "zh-Hant" | "en") => {
    setTestFallbackLocale(loc);
    vi.mocked(tauriService.fetchLocaleInitInfo).mockResolvedValue({
      locale: loc,
      warning: null,
      detected_source_tag: null,
    });
    return render(
      <I18nProvider initialLocale={loc} initialIsInitialized={true}>
        <DiscoverySessionProvider>
          <LibraryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );
  };

  it("renders all 12 table headers in Simplified Chinese (zh-Hans) with no English headers remaining", async () => {
    renderWithLocale("zh-Hans");

    // Wait for data load
    await waitFor(() => {
      expect(screen.getByTestId("library-row-101")).toBeInTheDocument();
    });

    const headers = screen.getAllByRole("columnheader").map((th) => th.textContent?.trim());

    // Expected Simplified Chinese headers (in column order)
    expect(headers).toContain("名称");
    expect(headers).toContain("分类");
    expect(headers).toContain("项目简介");
    expect(headers).toContain("用户标签");
    expect(headers).toContain("个人备注");
    expect(headers).not.toContain("标签");
    expect(headers).toContain("星标");
    expect(headers).toContain("创建日期");
    expect(headers).toContain("最后活跃");
    expect(headers).toContain("评级");
    expect(headers).toContain("收藏日期");
    expect(headers).toContain("最后刷新");
    expect(headers).toContain("操作");

    // Must NOT contain leftover English headers
    expect(headers).not.toContain("Name");
    expect(headers).not.toContain("Category");
    expect(headers).not.toContain("Description");
    expect(headers).not.toContain("User Tags");
    expect(headers).not.toContain("Notes");
    expect(headers).not.toContain("Tags");
    expect(headers).not.toContain("Stars");
    expect(headers).not.toContain("Last Active");
    expect(headers).not.toContain("Rating");
    expect(headers).not.toContain("Favorited At");
    expect(headers).not.toContain("Last Refresh");
    expect(headers).not.toContain("Actions");
  });

  it("renders all 12 table headers in Traditional Chinese (zh-Hant)", async () => {
    renderWithLocale("zh-Hant");

    await waitFor(() => {
      expect(screen.getByTestId("library-row-101")).toBeInTheDocument();
    });

    const headers = screen.getAllByRole("columnheader").map((th) => th.textContent?.trim());

    expect(headers).toContain("名稱");
    expect(headers).toContain("分類");
    expect(headers).toContain("專案簡介");
    expect(headers).toContain("使用者標籤");
    expect(headers).toContain("個人備註");
    expect(headers).not.toContain("標籤");
    expect(headers).toContain("星標");
    expect(headers).toContain("建立日期");
    expect(headers).toContain("最後活躍");
    expect(headers).toContain("評級");
    expect(headers).toContain("收藏日期");
    expect(headers).toContain("最後重新整理");
    expect(headers).toContain("操作");
  });

  it("renders all 12 table headers in English (en)", async () => {
    renderWithLocale("en");

    await waitFor(() => {
      expect(screen.getByTestId("library-row-101")).toBeInTheDocument();
    });

    const headers = screen.getAllByRole("columnheader").map((th) => th.textContent?.trim());

    expect(headers).toContain("Name");
    expect(headers).toContain("Category");
    expect(headers).toContain("Description");
    expect(headers).toContain("User Tags");
    expect(headers).toContain("Notes");
    expect(headers).not.toContain("Tags");
    expect(headers).toContain("Stars");
    expect(headers).toContain("Created");
    expect(headers).toContain("Last Active");
    expect(headers).toContain("Rating");
    expect(headers).toContain("Favorited");
    expect(headers).toContain("Last Refresh");
    expect(headers).toContain("Actions");
  });

  it("formats all four date columns in unified YYYY/MM/DD with zero padding and safe fallback", async () => {
    renderWithLocale("zh-Hans");

    await waitFor(() => {
      expect(screen.getByTestId("library-row-101")).toBeInTheDocument();
    });

    // Row 101: all valid dates
    const createdAt1 = screen.getByTestId("library-created-at-101").textContent;
    const lastActive1 = screen.getByTestId("library-last-active-101").textContent;
    const favoritedAt1 = screen.getByTestId("library-favorited-at-101").textContent;
    const lastRefresh1 = screen.getByTestId("library-last-refresh-101").textContent;

    expect(createdAt1).toBe("2024/03/15");
    expect(createdAt1).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);
    expect(lastActive1).toBe("2026/03/01");
    expect(lastActive1).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);
    expect(favoritedAt1).toBe("2026/03/01");
    expect(favoritedAt1).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);
    expect(lastRefresh1).toBe("2026/03/01");
    expect(lastRefresh1).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);

    // Row 102: null dates display "—"
    const createdAt2 = screen.getByTestId("library-created-at-102").textContent;
    const lastActive2 = screen.getByTestId("library-last-active-102").textContent;
    const favoritedAt2 = screen.getByTestId("library-favorited-at-102").textContent;
    const lastRefresh2 = screen.getByTestId("library-last-refresh-102").textContent;

    expect(createdAt2).toBe("—");
    expect(lastActive2).toBe("—");
    expect(favoritedAt2).toBe("2026/01/09");
    expect(lastRefresh2).toBe("2026/01/09");
  });

  it("translates Category badge in table row and Category filter dropdown without raw 'categories.' prefix", async () => {
    renderWithLocale("zh-Hans");

    await waitFor(() => {
      expect(screen.getByTestId("library-row-101")).toBeInTheDocument();
    });

    // Row 101 is "Automation" -> must be "自动化" in zh-Hans
    const row101 = screen.getByTestId("library-row-101");
    expect(row101).toHaveTextContent("自动化");
    expect(row101).not.toHaveTextContent("categories.Automation");

    // Row 102 is "Data & Database" -> "数据与数据库"
    const row102 = screen.getByTestId("library-row-102");
    expect(row102).toHaveTextContent("数据与数据库");

    // Category filter dropdown options
    const categorySelect = screen.getByTestId("library-category-filter");
    expect(categorySelect).toHaveTextContent("自动化");
    expect(categorySelect).toHaveTextContent("AI 与智能体");
    expect(categorySelect).toHaveTextContent("开发者工具");
    expect(categorySelect).not.toHaveTextContent("categories.");
  });
});
