import React, { useEffect, useState } from "react";
import { BatchResultModal } from "../components/BatchResultModal";
import { DuplicateModal } from "../components/DuplicateModal";
import { RepositoryCard } from "../components/RepositoryCard";
import {
  favoriteRepositoriesBatch,
  favoriteRepository,
  fetchSettings,
  getAiSearchReadiness,
  refreshFavorite,
  searchAiGitHubRepositories,
  searchGitHubRepositories,
} from "../services/tauri";
import {
  AiRepositorySearchRequest,
  AiSearchReadiness,
  GitHubRepository,
  LastActivePreset,
  RepositorySearchRequest,
} from "../types/github";
import { BatchFavoriteResult } from "../types/library";
import { useDiscoverySession } from "../context/DiscoverySessionContext";

function getPushedAfterDate(preset: LastActivePreset): string | null {
  if (preset === "any") return null;
  const now = new Date();
  let days = 7;
  if (preset === "month") days = 30;
  else if (preset === "half_year") days = 180;
  else if (preset === "year") days = 365;

  const target = new Date(now.getTime() - days * 24 * 60 * 60 * 1000);
  const y = target.getUTCFullYear();
  const m = String(target.getUTCMonth() + 1).padStart(2, "0");
  const d = String(target.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

export interface SearchPageProps {
  onNavigateToFavorite?: (favoriteId: number) => void;
}

export const SearchPage: React.FC<SearchPageProps> = ({
  onNavigateToFavorite,
}) => {
  const {
    search,
    setSearch,
    nextSearchGeneration,
    isFavorited,
    markFavorited,
    reconcileIdentities,
  } = useDiscoverySession();

  const {
    searchMode,
    query,
    aiInput,
    minStars,
    lastActive,
    limit,
    excludeSelected,
    excludeFavorites,
    status,
    results,
    generatedQueries,
    isPartial,
    errorMessage,
    selectedUrls,
  } = search;

  const setSearchMode = (m: "keyword" | "ai") => {
    setSearch((prev) => ({ ...prev, searchMode: m }));
  };
  const setQuery = (q: string) => {
    setSearch((prev) => ({ ...prev, query: q }));
  };
  const setAiInput = (i: string) => {
    setSearch((prev) => ({ ...prev, aiInput: i }));
  };
  const setMinStars = (s: number | "") => {
    setSearch((prev) => ({ ...prev, minStars: s }));
  };
  const setLastActive = (a: LastActivePreset) => {
    setSearch((prev) => ({ ...prev, lastActive: a }));
  };
  const setLimit = (l: number) => {
    setSearch((prev) => ({ ...prev, limit: l }));
  };
  const setExcludeSelected = (v: boolean) => {
    setSearch((prev) => ({ ...prev, excludeSelected: v }));
  };
  const setExcludeFavorites = (v: boolean) => {
    setSearch((prev) => ({ ...prev, excludeFavorites: v }));
  };
  const setSelectedUrls = (updater: Set<string> | ((prev: Set<string>) => Set<string>)) => {
    setSearch((prev) => ({
      ...prev,
      selectedUrls: typeof updater === "function" ? updater(prev.selectedUrls) : updater,
    }));
  };

  const [aiReadiness, setAiReadiness] = useState<AiSearchReadiness>({
    ready: false,
    reason: null,
  });

  const [duplicateModal, setDuplicateModal] = useState<{
    isOpen: boolean;
    repoName: string;
    favoriteId: number | null;
  }>({ isOpen: false, repoName: "", favoriteId: null });
  const [batchResult, setBatchResult] = useState<BatchFavoriteResult | null>(
    null
  );
  const [actionBanner, setActionBanner] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  // Load default limit and AI readiness
  useEffect(() => {
    fetchSettings()
      .then((s) => {
        if (s?.github?.default_search_limit) {
          setSearch((prev) => {
            if (prev.status === "idle" && prev.results.length === 0) {
              return { ...prev, limit: s.github.default_search_limit };
            }
            return prev;
          });
        }
      })
      .catch(() => {
        // Fallback to default 20
      });

    getAiSearchReadiness()
      .then((r) => setAiReadiness(r))
      .catch(() => setAiReadiness({ ready: false, reason: "无法获取 AI 状态" }));
  }, [setSearch]);

  // Reconcile favorite identities on results change
  useEffect(() => {
    if (results && results.length > 0) {
      reconcileIdentities(
        results.map((r) => ({
          github_repo_id: r.github_repo_id,
          github_url: r.github_url,
        }))
      );
    }
  }, [results, reconcileIdentities]);

  const handleSearch = async (e: React.FormEvent) => {
    e.preventDefault();

    if (searchMode === "keyword") {
      const trimmed = query.trim();
      if (!trimmed) return;

      const currentGen = nextSearchGeneration();
      setSearch((prev) => ({
        ...prev,
        status: "loading",
        errorMessage: null,
        generation: currentGen,
      }));
      setActionBanner(null);

      const req: RepositorySearchRequest = {
        query: trimmed,
        min_stars: typeof minStars === "number" && minStars > 0 ? minStars : null,
        pushed_after: getPushedAfterDate(lastActive),
        limit,
        exclude_selected: excludeSelected,
        exclude_favorites: excludeFavorites,
      };

      try {
        const resp = await searchGitHubRepositories(req);
        setSearch((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            results: resp.items,
            selectedUrls: new Set(),
            status: resp.items.length === 0 ? "empty" : "success",
            errorMessage: null,
          };
        });
      } catch (err: unknown) {
        const errObj = err as { code?: string; message?: string } | undefined;
        const code = errObj?.code;
        const msg = errObj?.message || String(err);

        setSearch((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            errorMessage: msg,
            status: code === "GitHubRateLimit" ? "rate_limited" : "error",
          };
        });
      }
    } else {
      // AI Search Mode
      const trimmed = aiInput.trim();
      if (!trimmed) {
        setSearch((prev) => ({
          ...prev,
          errorMessage: "自然语言搜索需求不能为空",
          status: "error",
        }));
        return;
      }
      if (trimmed.length > 2000) {
        setSearch((prev) => ({
          ...prev,
          errorMessage: "自然语言搜索需求不能超过 2000 个字符",
          status: "error",
        }));
        return;
      }

      const currentGen = nextSearchGeneration();
      setSearch((prev) => ({
        ...prev,
        status: "loading",
        errorMessage: null,
        generation: currentGen,
      }));
      setActionBanner(null);

      const req: AiRepositorySearchRequest = {
        input: trimmed,
        min_stars: typeof minStars === "number" && minStars > 0 ? minStars : null,
        pushed_after: getPushedAfterDate(lastActive),
        limit,
        exclude_selected: excludeSelected,
        exclude_favorites: excludeFavorites,
      };

      try {
        const resp = await searchAiGitHubRepositories(req);
        setSearch((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            results: resp.items,
            generatedQueries: resp.generated_queries,
            isPartial: resp.partial,
            selectedUrls: new Set(),
            status: resp.items.length === 0 ? "empty" : "success",
            errorMessage: null,
          };
        });
      } catch (err: unknown) {
        const errObj = err as { code?: string; message?: string } | undefined;
        const code = errObj?.code;
        const msg = errObj?.message || String(err);

        setSearch((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            errorMessage: msg,
            status: code === "GitHubRateLimit" ? "rate_limited" : "error",
          };
        });
      }
    }
  };

  const handleSingleFavorite = async (repo: GitHubRepository) => {
    setActionBanner(null);
    try {
      await favoriteRepository({
        github_repo_id: repo.github_repo_id,
        github_url: repo.github_url,
        name: repo.name,
        description: repo.description,
        category: repo.category,
        stars: Number(repo.stars),
        license: repo.license,
        topics: repo.topics,
        pushed_at: repo.pushed_at,
        releases: repo.releases,
        languages: repo.languages,
      });
      markFavorited(repo.github_repo_id, repo.github_url);
      setActionBanner({
        type: "success",
        message: `成功将 ${repo.name} 收藏到本地库！`,
      });
    } catch (err: unknown) {
      const errObj = err as
        | { code?: string; favorite_id?: number; message?: string }
        | undefined;
      if (errObj?.code === "FavoriteAlreadyExists") {
        markFavorited(repo.github_repo_id, repo.github_url);
        setDuplicateModal({
          isOpen: true,
          repoName: repo.name,
          favoriteId: errObj.favorite_id ?? null,
        });
      } else {
        setActionBanner({
          type: "error",
          message: `收藏失败: ${errObj?.message || String(err)}`,
        });
      }
    }
  };

  const handleBatchFavorite = async () => {
    const selectedRepos = results.filter((r) =>
      selectedUrls.has(r.github_url)
    );
    if (selectedRepos.length === 0) return;

    setActionBanner(null);
    try {
      const inputs = selectedRepos.map((r) => ({
        github_repo_id: r.github_repo_id,
        github_url: r.github_url,
        name: r.name,
        description: r.description,
        category: r.category,
        stars: Number(r.stars),
        license: r.license,
        topics: r.topics,
        pushed_at: r.pushed_at,
        releases: r.releases,
        languages: r.languages,
      }));

      const res = await favoriteRepositoriesBatch(inputs);
      setBatchResult(res);

      for (const item of res.items) {
        if (item.status === "succeeded" || item.status === "already_exists") {
          const matched = selectedRepos.find((r) => r.github_url === item.github_url);
          markFavorited(matched?.github_repo_id, item.github_url);
        }
      }
      setSelectedUrls(new Set());
    } catch (err) {
      setActionBanner({
        type: "error",
        message: `批量收藏处理失败: ${String(err)}`,
      });
    }
  };


  const handleSelectAllCurrent = (checked: boolean) => {
    if (checked) {
      setSelectedUrls(new Set(results.map((r) => r.github_url)));
    } else {
      setSelectedUrls(new Set());
    }
  };

  const handleToggleSelectRow = (url: string, checked: boolean) => {
    const next = new Set(selectedUrls);
    if (checked) {
      next.add(url);
    } else {
      next.delete(url);
    }
    setSelectedUrls(next);
  };

  const handleDuplicateRefresh = async (favoriteId: number) => {
    try {
      const res = await refreshFavorite(favoriteId);
      if (res.status === "succeeded") {
        setActionBanner({
          type: "success",
          message: `成功刷新收藏项元数据！`,
        });
      } else {
        setActionBanner({
          type: "error",
          message: `刷新失败: ${res.message || res.status}`,
        });
      }
    } catch (err) {
      setActionBanner({
        type: "error",
        message: `刷新请求出错: ${String(err)}`,
      });
    } finally {
      setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    }
  };

  const handleDuplicateOpenRecord = (favoriteId: number) => {
    setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    onNavigateToFavorite?.(favoriteId);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>🔎</span> 搜索项目
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            GitHub Repository Search
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          根据关键词快速发现 GitHub 开源项目并预览详情。
        </p>
      </div>

      {/* Mode Switcher */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-4">
        <div className="text-xs font-medium text-slate-300 mb-2">搜索模式：</div>
        <div className="flex items-center gap-6 text-sm">
          <label className="flex items-center gap-2 text-slate-200 cursor-pointer">
            <input
              type="radio"
              name="search-mode"
              data-testid="search-mode-keyword"
              checked={searchMode === "keyword"}
              onChange={() => setSearchMode("keyword")}
              className="text-indigo-600 focus:ring-indigo-500 cursor-pointer"
            />
            <span className="font-medium">关键词搜索</span>
          </label>

          <label
            title={
              aiReadiness.ready
                ? "使用 AI 理解自然语言需求并生成多维度 GitHub 检索词"
                : aiReadiness.reason || "AI 未配置"
            }
            className={`flex items-center gap-2 ${
              aiReadiness.ready
                ? "text-slate-200 cursor-pointer"
                : "text-slate-500 cursor-not-allowed opacity-60"
            }`}
          >
            <input
              type="radio"
              name="search-mode"
              data-testid="search-mode-ai"
              disabled={!aiReadiness.ready}
              checked={searchMode === "ai"}
              onChange={() => aiReadiness.ready && setSearchMode("ai")}
              className={
                aiReadiness.ready
                  ? "text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                  : "cursor-not-allowed"
              }
            />
            <span className={searchMode === "ai" ? "font-medium text-indigo-300" : ""}>
              AI自然语言搜索
            </span>
            {aiReadiness.ready ? (
              <span
                data-testid="ai-ready-badge"
                className="text-[11px] px-2 py-0.5 rounded bg-emerald-950/60 text-emerald-400 border border-emerald-800"
              >
                已就绪
              </span>
            ) : (
              <span
                data-testid="ai-disabled-reason"
                className="text-[11px] px-2 py-0.5 rounded bg-slate-700 text-slate-400 border border-slate-600"
                title={aiReadiness.reason || "未配置 AI"}
              >
                {aiReadiness.reason || "未配置 AI"}
              </span>
            )}
          </label>
        </div>
      </div>

      {/* Search Form */}
      <form
        onSubmit={handleSearch}
        className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-4"
      >
        {/* Main query input or AI textarea */}
        {searchMode === "keyword" ? (
          <div>
            <label
              htmlFor="search-query-input"
              className="block text-xs font-medium text-slate-300 mb-1.5"
            >
              搜索关键词
            </label>
            <div className="flex gap-3">
              <input
                id="search-query-input"
                data-testid="search-query-input"
                type="text"
                placeholder="例如: instagram scraper, tauri, llm agent..."
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3.5 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
              />
              <button
                type="submit"
                data-testid="search-submit-btn"
                disabled={status === "loading" || !query.trim()}
                className="px-6 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium rounded-md transition-colors shrink-0"
              >
                {status === "loading" ? "搜索中..." : "搜索"}
              </button>
            </div>
          </div>
        ) : (
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <label
                htmlFor="search-ai-input"
                className="block text-xs font-medium text-indigo-300"
              >
                自然语言需求描述
              </label>
              <span className="text-[11px] text-slate-400">
                {aiInput.trim().length} / 2000 字
              </span>
            </div>
            <div className="space-y-2">
              <textarea
                id="search-ai-input"
                data-testid="search-ai-input"
                rows={3}
                maxLength={2000}
                placeholder="例如: 找可以获取 Instagram 粉丝和评论的开源项目，最好提供API或者MCP，最近仍然有人维护。"
                value={aiInput}
                onChange={(e) => setAiInput(e.target.value)}
                className="w-full bg-slate-900 border border-slate-700 rounded-md px-3.5 py-2.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 resize-y"
              />
              <div className="flex justify-end">
                <button
                  type="submit"
                  data-testid="search-submit-btn"
                  disabled={
                    status === "loading" ||
                    !aiInput.trim() ||
                    aiInput.trim().length > 2000
                  }
                  className="px-6 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium rounded-md transition-colors shrink-0"
                >
                  {status === "loading" ? "AI 搜索中..." : "AI 搜索"}
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Filters Grid */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-1">
          {/* Minimum Stars */}
          <div>
            <label
              htmlFor="search-min-stars"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              最低 Stars
            </label>
            <input
              id="search-min-stars"
              data-testid="search-min-stars"
              type="number"
              min={0}
              placeholder="不限制 (例如: 100)"
              value={minStars}
              onChange={(e) => {
                const val = e.target.value;
                setMinStars(val === "" ? "" : Math.max(0, parseInt(val, 10) || 0));
              }}
              className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            />
          </div>

          {/* Last Active */}
          <div>
            <label
              htmlFor="search-last-active"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              最后活跃时间
            </label>
            <select
              id="search-last-active"
              data-testid="search-last-active"
              value={lastActive}
              onChange={(e) => setLastActive(e.target.value as LastActivePreset)}
              className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="week">最近一周内</option>
              <option value="month">最近一个月内</option>
              <option value="half_year">最近半年内</option>
              <option value="year">最近一年内</option>
              <option value="any">不限制</option>
            </select>
          </div>

          {/* Result Limit */}
          <div>
            <label
              htmlFor="search-limit"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              返回结果数量
            </label>
            <select
              id="search-limit"
              data-testid="search-limit"
              value={limit}
              onChange={(e) => setLimit(parseInt(e.target.value, 10) || 20)}
              className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value={10}>10 项</option>
              <option value={20}>20 项</option>
              <option value={30}>30 项</option>
              <option value={50}>50 项</option>
              <option value={100}>100 项</option>
            </select>
          </div>
        </div>

        {/* Phase 5 Exclusions */}
        <div className="pt-3 border-t border-slate-700/60 flex flex-wrap items-center gap-6">
          <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
            <input
              type="checkbox"
              data-testid="search-exclude-selected"
              checked={excludeSelected}
              onChange={(e) => setExcludeSelected(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
            />
            <span>排除以前入选</span>
          </label>

          <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
            <input
              type="checkbox"
              data-testid="search-exclude-favorites"
              checked={excludeFavorites}
              onChange={(e) => setExcludeFavorites(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
            />
            <span>排除已收藏</span>
          </label>
        </div>
      </form>

      {/* Results / Status Area */}
      <div data-testid="search-results-area" className="space-y-4">
        {/* Loading State */}
        {status === "loading" && (
          <div
            data-testid="search-loading"
            className="rounded-lg border border-slate-700 bg-slate-800/50 p-12 text-center"
          >
            <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
            <p className="text-sm text-slate-300">
              {searchMode === "ai"
                ? "正在使用 AI 分析搜索意图并检索 GitHub..."
                : "正在搜索 GitHub 公开项目..."}
            </p>
          </div>
        )}

        {/* Empty State */}
        {status === "empty" && (
          <div
            data-testid="search-empty"
            className="rounded-lg border border-slate-700 bg-slate-800/50 p-12 text-center space-y-2"
          >
            <div className="text-3xl">📭</div>
            <h3 className="text-base font-medium text-slate-200">未找到符合条件的项目</h3>
            <p className="text-sm text-slate-400 max-w-md mx-auto">
              没有找到符合条件的公开项目。可以尝试降低 Star 限制或扩大最后活跃时间范围。
            </p>
          </div>
        )}

        {/* Error State (No prior results) */}
        {status === "error" && results.length === 0 && (
          <div
            data-testid="search-error"
            className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-6 text-center space-y-2 text-rose-300"
          >
            <div className="text-2xl">⚠️</div>
            <h3 className="text-base font-semibold text-rose-200">搜索请求失败</h3>
            <p className="text-xs text-rose-300 max-w-md mx-auto">{errorMessage}</p>
          </div>
        )}

        {/* Error Banner (With prior results) */}
        {status === "error" && results.length > 0 && (
          <div
            data-testid="search-error-banner"
            className="rounded-lg border border-rose-500/40 bg-rose-950/40 p-4 text-sm text-rose-200 flex items-start gap-3"
          >
            <span className="text-rose-400 text-lg shrink-0">⚠️</span>
            <div className="space-y-1">
              <p className="font-medium text-rose-200">搜索刷新失败，当前继续显示旧结果：</p>
              <p className="text-xs text-rose-300">{errorMessage}</p>
            </div>
          </div>
        )}

        {/* Rate Limited State (No prior results) */}
        {status === "rate_limited" && results.length === 0 && (
          <div
            data-testid="search-rate-limited"
            className="rounded-lg border border-amber-800/60 bg-amber-950/40 p-6 text-center space-y-2 text-amber-300"
          >
            <div className="text-2xl">⏳</div>
            <h3 className="text-base font-semibold text-amber-200">
              GitHub API 访问速率已达上限
            </h3>
            <p className="text-xs text-amber-300 max-w-md mx-auto">{errorMessage}</p>
            <p className="text-xs text-slate-400">
              提示：在“设置”中配置 GitHub Token 可以大幅提升每小时请求限额。
            </p>
          </div>
        )}

        {/* Rate Limited Banner (With prior results) */}
        {status === "rate_limited" && results.length > 0 && (
          <div
            data-testid="search-rate-limited-banner"
            className="rounded-lg border border-amber-500/40 bg-amber-950/40 p-4 text-sm text-amber-200 flex items-start gap-3"
          >
            <span className="text-amber-400 text-lg shrink-0">⏳</span>
            <div className="space-y-1">
              <p className="font-medium text-amber-200">GitHub API 访问速率已达上限，当前继续显示旧结果：</p>
              <p className="text-xs text-amber-300">{errorMessage}</p>
              <p className="text-xs text-slate-400">
                提示：在“设置”中配置 GitHub Token 可以大幅提升每小时请求限额。
              </p>
            </div>
          </div>
        )}

        {/* Action Banner */}
        {actionBanner && (
          <div
            data-testid="search-action-banner"
            className={`p-3 rounded-md text-xs border ${
              actionBanner.type === "success"
                ? "bg-emerald-950/40 border-emerald-800 text-emerald-300"
                : "bg-rose-950/40 border-rose-800 text-rose-300"
            }`}
          >
            {actionBanner.message}
          </div>
        )}

        {/* AI Partial Result Warning */}
        {isPartial && (
          <div
            data-testid="ai-partial-warning"
            className="p-3 rounded-md text-xs border bg-amber-950/40 border-amber-800 text-amber-300 flex items-center gap-2"
          >
            <span>⚠️</span>
            <span>部分查询因速率限制或网络原因未能完成，当前展示已成功检索到的项目。</span>
          </div>
        )}

        {/* AI Generated Queries Banner */}
        {generatedQueries.length > 0 && (
          <div
            data-testid="ai-generated-queries-banner"
            className="bg-slate-800/90 border border-indigo-700/50 rounded-lg p-3.5 space-y-2"
          >
            <div className="flex items-center justify-between text-xs">
              <span className="text-indigo-300 font-medium flex items-center gap-1.5">
                <span>✨</span> AI 生成的 GitHub 检索词:
              </span>
              <span className="text-[11px] text-slate-400 font-mono">
                {generatedQueries.length > 1
                  ? "已通过 RRF 融合多查询排名"
                  : "保持 GitHub Best Match 排序"}
              </span>
            </div>
            <div className="flex flex-wrap gap-2">
              {generatedQueries.map((q, idx) => (
                <span
                  key={idx}
                  data-testid="ai-query-chip"
                  className="px-2.5 py-1 rounded bg-indigo-950/70 border border-indigo-600/50 text-indigo-200 text-xs font-mono"
                >
                  {q}
                </span>
              ))}
            </div>
          </div>
        )}

        {/* Success / Retained Results State */}
        {(status === "success" || ((status === "error" || status === "rate_limited") && results.length > 0)) && (
          <div className="space-y-4">
            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-slate-800/80 p-3 rounded-lg border border-slate-700 text-xs text-slate-300">
              <div className="flex items-center gap-4">
                <label className="inline-flex items-center gap-2 cursor-pointer font-medium">
                  <input
                    type="checkbox"
                    data-testid="search-select-all-checkbox"
                    checked={
                      results.length > 0 &&
                      results.every((r) => selectedUrls.has(r.github_url))
                    }
                    onChange={(e) => handleSelectAllCurrent(e.target.checked)}
                    className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                  />
                  <span>全选当前结果</span>
                </label>
                <span className="text-slate-500">|</span>
                <span className="text-slate-400">
                  共找到 {results.length} 个公开项目
                  {selectedUrls.size > 0 ? `（已选 ${selectedUrls.size} 项）` : ""}
                </span>
              </div>

              <div className="flex items-center gap-3">
                <span className="text-[11px] text-slate-500">
                  {searchMode === "ai" && generatedQueries.length > 1
                    ? "已通过 RRF 融合多查询排名"
                    : "保持 GitHub Best Match 排序"}
                </span>
                <button
                  type="button"
                  data-testid="search-favorite-selected-btn"
                  disabled={selectedUrls.size === 0}
                  onClick={handleBatchFavorite}
                  className="px-3.5 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium transition-colors"
                >
                  收藏选中项目 ({selectedUrls.size})
                </button>
              </div>
            </div>

            <div className="space-y-3">
              {results.map((repo) => {
                const cardId = repo.github_repo_id ?? repo.name;
                const isFav = isFavorited(repo.github_repo_id, repo.github_url);
                const isSel = selectedUrls.has(repo.github_url);

                return (
                  <RepositoryCard
                    key={cardId}
                    repo={repo}
                    selectable={true}
                    isSelected={isSel}
                    onToggleSelect={(checked) =>
                      handleToggleSelectRow(repo.github_url, checked)
                    }
                    isFavorited={isFav}
                    onFavorite={() => handleSingleFavorite(repo)}
                  />
                );
              })}
            </div>
          </div>
        )}

        {/* Idle State */}
        {status === "idle" && (
          <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-12 text-center text-slate-400 space-y-2">
            <div className="text-3xl">🔎</div>
            <h3 className="text-base font-medium text-slate-300">准备搜索</h3>
            <p className="text-xs text-slate-500 max-w-sm mx-auto">
              输入关键词并设定筛选条件，点击“搜索”即可实时查询 GitHub 公开代码库。
            </p>
          </div>
        )}
      </div>

      {/* Duplicate Modal */}
      <DuplicateModal
        isOpen={duplicateModal.isOpen}
        repoName={duplicateModal.repoName}
        favoriteId={duplicateModal.favoriteId}
        onClose={() =>
          setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null })
        }
        onRefresh={handleDuplicateRefresh}
        onOpenRecord={handleDuplicateOpenRecord}
      />

      {/* Batch Result Modal */}
      <BatchResultModal
        isOpen={batchResult != null}
        onClose={() => setBatchResult(null)}
        title="批量收藏结果"
        favoriteResult={batchResult}
      />
    </div>
  );
};
