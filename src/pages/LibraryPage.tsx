import React, { useCallback, useEffect, useState } from "react";
import { BatchResultModal } from "../components/BatchResultModal";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import {
  listDistinctTags,
  listFavorites,
  openGitHubUrl,
  refreshFavoritesBatch,
} from "../services/tauri";
import {
  ALL_CATEGORIES,
  BatchRefreshResult,
  FavoriteItem,
  LibraryQuery,
  LibrarySortField,
  SortOrder,
} from "../types/library";

export interface LibraryPageProps {
  initialOpenFavoriteId?: number | null;
  onClearInitialFavoriteId?: () => void;
}

export const LibraryPage: React.FC<LibraryPageProps> = ({
  initialOpenFavoriteId,
  onClearInitialFavoriteId,
}) => {
  const [favorites, setFavorites] = useState<FavoriteItem[]>([]);
  const [distinctTags, setDistinctTags] = useState<string[]>([]);
  const [status, setStatus] = useState<
    "loading" | "success" | "empty" | "error"
  >("loading");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Filters & Query state
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState("all");
  const [selectedTag, setSelectedTag] = useState("all");
  const [selectedRating, setSelectedRating] = useState("all");
  const [sortKey, setSortKey] = useState<string>("favorited_at_desc");

  // Selection state
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());

  // Modals state
  const [activeDetailFav, setActiveDetailFav] = useState<FavoriteItem | null>(
    null
  );
  const [batchRefreshResult, setBatchRefreshResult] =
    useState<BatchRefreshResult | null>(null);
  const [batchRefreshing, setBatchRefreshing] = useState(false);
  const [refreshProgressText, setRefreshProgressText] = useState<string | null>(
    null
  );

  const parseSortKey = (
    key: string
  ): { field: LibrarySortField; order: SortOrder } => {
    switch (key) {
      case "stars_desc":
        return { field: "stars", order: "desc" };
      case "stars_asc":
        return { field: "stars", order: "asc" };
      case "rating_desc":
        return { field: "rating", order: "desc" };
      case "rating_asc":
        return { field: "rating", order: "asc" };
      case "pushed_at_desc":
        return { field: "pushed_at", order: "desc" };
      case "pushed_at_asc":
        return { field: "pushed_at", order: "asc" };
      case "name_asc":
        return { field: "name", order: "asc" };
      case "name_desc":
        return { field: "name", order: "desc" };
      case "favorited_at_asc":
        return { field: "favorited_at", order: "asc" };
      case "favorited_at_desc":
      default:
        return { field: "favorited_at", order: "desc" };
    }
  };

  const loadData = useCallback(async () => {
    setStatus("loading");
    setErrorMessage(null);

    const { field, order } = parseSortKey(sortKey);
    const query: LibraryQuery = {
      search: searchQuery.trim() || undefined,
      category: selectedCategory !== "all" ? selectedCategory : undefined,
      tag: selectedTag !== "all" ? selectedTag : undefined,
      rating: selectedRating !== "all" ? selectedRating : undefined,
      sort_field: field,
      sort_order: order,
    };

    try {
      const [favs, tags] = await Promise.all([
        listFavorites(query),
        listDistinctTags(),
      ]);

      setFavorites(favs);
      setDistinctTags(tags);
      setSelectedIds(new Set());

      if (favs.length === 0) {
        setStatus("empty");
      } else {
        setStatus("success");
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setErrorMessage(msg);
      setStatus("error");
    }
  }, [searchQuery, selectedCategory, selectedTag, selectedRating, sortKey]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Handle deep-link initialOpenFavoriteId
  useEffect(() => {
    if (initialOpenFavoriteId != null && favorites.length > 0) {
      const target = favorites.find((f) => f.id === initialOpenFavoriteId);
      if (target) {
        setActiveDetailFav(target);
        onClearInitialFavoriteId?.();
      }
    }
  }, [initialOpenFavoriteId, favorites, onClearInitialFavoriteId]);

  // Selection handlers
  const handleSelectAllCurrent = (checked: boolean) => {
    if (checked) {
      setSelectedIds(new Set(favorites.map((f) => f.id)));
    } else {
      setSelectedIds(new Set());
    }
  };

  const handleToggleSelectRow = (id: number, checked: boolean) => {
    const next = new Set(selectedIds);
    if (checked) {
      next.add(id);
    } else {
      next.delete(id);
    }
    setSelectedIds(next);
  };

  // Batch refresh actions
  const handleRefreshSelected = async () => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;

    setBatchRefreshing(true);
    setRefreshProgressText(`正在刷新 1 / ${ids.length}...`);
    try {
      const res = await refreshFavoritesBatch(ids);
      setBatchRefreshResult(res);
      await loadData();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      alert(`批量刷新失败: ${msg}`);
    } finally {
      setBatchRefreshing(false);
      setRefreshProgressText(null);
    }
  };

  const handleRefreshAll = async () => {
    if (favorites.length === 0) return;
    const allIds = favorites.map((f) => f.id);

    setBatchRefreshing(true);
    setRefreshProgressText(`正在刷新 1 / ${allIds.length}...`);
    try {
      const res = await refreshFavoritesBatch(allIds);
      setBatchRefreshResult(res);
      await loadData();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      alert(`刷新全部失败: ${msg}`);
    } finally {
      setBatchRefreshing(false);
      setRefreshProgressText(null);
    }
  };

  const handleOpenGitHub = async (e: React.MouseEvent, url: string) => {
    e.stopPropagation();
    try {
      await openGitHubUrl(url);
    } catch (err) {
      console.error("Failed to open GitHub link:", err);
    }
  };

  const allSelected =
    favorites.length > 0 &&
    favorites.every((f) => selectedIds.has(f.id));

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
            <span>📚</span> 项目浏览
            <span className="text-xs font-normal text-slate-400 font-mono ml-2">
              Local Repository Library
            </span>
          </h1>
          <p className="text-slate-400 mt-1 text-sm">
            本地收藏项目综合管理，支持多维联合搜索、过滤、安全排序与元数据刷新。
          </p>
        </div>

        {/* Batch action buttons */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-testid="library-refresh-selected-btn"
            disabled={selectedIds.size === 0 || batchRefreshing}
            onClick={handleRefreshSelected}
            className="px-3 py-1.5 rounded-md border border-slate-700 bg-slate-800 text-slate-200 text-xs font-medium hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            更新选中 ({selectedIds.size})
          </button>
          <button
            type="button"
            data-testid="library-refresh-all-btn"
            disabled={favorites.length === 0 || batchRefreshing}
            onClick={handleRefreshAll}
            className="px-3.5 py-1.5 rounded-md bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            更新全部
          </button>
        </div>
      </div>

      {/* Controls Bar: Search, Category, Tag, Rating, Sort */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-4 shadow-sm space-y-3">
        {/* Search row */}
        <div>
          <label
            htmlFor="library-search-input"
            className="block text-xs font-medium text-slate-300 mb-1"
          >
            本地联合搜索 (名称 / 分类 / 描述 / 备注 / 标签)
          </label>
          <div className="relative">
            <input
              id="library-search-input"
              type="text"
              data-testid="library-search-input"
              placeholder="输入关键词进行本地检索..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-3 py-1.5 text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-indigo-500"
            />
            {searchQuery && (
              <button
                type="button"
                data-testid="library-clear-search-btn"
                onClick={() => setSearchQuery("")}
                className="absolute right-2.5 top-2 text-slate-400 hover:text-slate-200 text-xs"
              >
                ✕
              </button>
            )}
          </div>
        </div>

        {/* Filters grid */}
        <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 text-xs">
          {/* Category filter */}
          <div>
            <label
              htmlFor="library-category-filter"
              className="block font-medium text-slate-400 mb-1"
            >
              分类 (Category)
            </label>
            <select
              id="library-category-filter"
              data-testid="library-category-filter"
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">全部分类 (All)</option>
              {ALL_CATEGORIES.map((cat) => (
                <option key={cat} value={cat}>
                  {cat}
                </option>
              ))}
            </select>
          </div>

          {/* Tag filter */}
          <div>
            <label
              htmlFor="library-tag-filter"
              className="block font-medium text-slate-400 mb-1"
            >
              标签 (Tag)
            </label>
            <select
              id="library-tag-filter"
              data-testid="library-tag-filter"
              value={selectedTag}
              onChange={(e) => setSelectedTag(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">全部标签 (All)</option>
              {distinctTags.map((t) => (
                <option key={t} value={t}>
                  #{t}
                </option>
              ))}
            </select>
          </div>

          {/* Rating filter */}
          <div>
            <label
              htmlFor="library-rating-filter"
              className="block font-medium text-slate-400 mb-1"
            >
              评级 (Rating)
            </label>
            <select
              id="library-rating-filter"
              data-testid="library-rating-filter"
              value={selectedRating}
              onChange={(e) => setSelectedRating(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">全部评级 (All)</option>
              <option value="unrated">未评级 (Unrated)</option>
              <option value="5">★★★★★ (5 星)</option>
              <option value="4">★★★★☆ (4 星)</option>
              <option value="3">★★★☆☆ (3 星)</option>
              <option value="2">★★☆☆☆ (2 星)</option>
              <option value="1">★☆☆☆☆ (1 星)</option>
            </select>
          </div>

          {/* Sort dropdown */}
          <div>
            <label
              htmlFor="library-sort-select"
              className="block font-medium text-slate-400 mb-1"
            >
              排序规则 (Sort)
            </label>
            <select
              id="library-sort-select"
              data-testid="library-sort-select"
              value={sortKey}
              onChange={(e) => setSortKey(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="favorited_at_desc">收藏时间 (最新)</option>
              <option value="favorited_at_asc">收藏时间 (最早)</option>
              <option value="stars_desc">Stars (从高到低)</option>
              <option value="stars_asc">Stars (从低到高)</option>
              <option value="rating_desc">评级 (从高到低)</option>
              <option value="rating_asc">评级 (从低到高)</option>
              <option value="pushed_at_desc">最后活跃 (最近)</option>
              <option value="pushed_at_asc">最后活跃 (较早)</option>
              <option value="name_asc">项目名称 (A-Z)</option>
              <option value="name_desc">项目名称 (Z-A)</option>
            </select>
          </div>
        </div>
      </div>

      {/* Batch Refreshing Indicator */}
      {batchRefreshing && (
        <div
          data-testid="batch-refresh-progress-indicator"
          className="flex items-center gap-3 p-3 rounded-md bg-indigo-950/40 border border-indigo-700 text-indigo-300 text-xs"
        >
          <div className="animate-spin w-4 h-4 border-2 border-indigo-400 border-t-transparent rounded-full" />
          <span>{refreshProgressText || "正在并发批量刷新..."}</span>
        </div>
      )}

      {/* Loading state */}
      {status === "loading" && !batchRefreshing && (
        <div
          data-testid="library-loading-state"
          className="rounded-lg border border-slate-700 bg-slate-800/40 p-12 text-center"
        >
          <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
          <p className="text-sm text-slate-300">正在加载本地收藏库...</p>
        </div>
      )}

      {/* Error state */}
      {status === "error" && (
        <div
          data-testid="library-error-state"
          className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-6 text-center space-y-3"
        >
          <div className="text-xl">⚠️</div>
          <h3 className="text-sm font-semibold text-rose-200">加载收藏失败</h3>
          <p className="text-xs text-rose-300/90">{errorMessage}</p>
          <button
            type="button"
            data-testid="library-retry-btn"
            onClick={() => loadData()}
            className="px-4 py-1.5 rounded-md bg-rose-700 hover:bg-rose-600 text-white text-xs font-medium transition-colors"
          >
            重试
          </button>
        </div>
      )}

      {/* Empty state */}
      {status === "empty" && (
        <div
          data-testid="library-empty-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/30 p-12 text-center space-y-3"
        >
          <div className="text-4xl">📚</div>
          <h3 className="text-base font-semibold text-slate-200">
            还没有收藏项目。
          </h3>
          <p className="text-sm text-slate-400 max-w-md mx-auto">
            可以从 Trending、Search 或 Add Repository 添加。
          </p>
        </div>
      )}

      {/* Success Table State */}
      {status === "success" && (
        <div className="rounded-lg border border-slate-700 bg-slate-800/90 shadow-sm overflow-hidden">
          <div className="overflow-x-auto">
            <table
              data-testid="library-table"
              className="w-full text-left text-xs border-collapse"
            >
              <thead className="bg-slate-900 border-b border-slate-700 text-slate-400 uppercase tracking-wider font-semibold">
                <tr>
                  <th className="p-3 w-10 text-center">
                    <input
                      type="checkbox"
                      data-testid="library-select-all-checkbox"
                      checked={allSelected}
                      onChange={(e) => handleSelectAllCurrent(e.target.checked)}
                      className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                    />
                  </th>
                  <th className="p-3 min-w-[140px]">Name</th>
                  <th className="p-3 min-w-[120px]">Category</th>
                  <th className="p-3 min-w-[160px]">Tags</th>
                  <th className="p-3 text-right min-w-[80px]">Stars</th>
                  <th className="p-3 min-w-[100px]">Last Active</th>
                  <th className="p-3 min-w-[90px]">Rating</th>
                  <th className="p-3 min-w-[120px]">Favorited At</th>
                  <th className="p-3 min-w-[120px]">Last Refresh</th>
                  <th className="p-3 text-center min-w-[100px]">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-700/60 text-slate-200">
                {favorites.map((fav) => {
                  const isChecked = selectedIds.has(fav.id);
                  const formattedDate = fav.pushed_at
                    ? new Date(fav.pushed_at).toLocaleDateString()
                    : "Unknown";
                  const formattedFavDate = fav.favorited_at
                    ? new Date(fav.favorited_at).toLocaleDateString()
                    : "Unknown";
                  const formattedRefreshDate = fav.metadata_refreshed_at
                    ? new Date(fav.metadata_refreshed_at).toLocaleDateString()
                    : "Unknown";

                  return (
                    <tr
                      key={fav.id}
                      data-testid={`library-row-${fav.id}`}
                      className={`hover:bg-slate-700/40 transition-colors cursor-pointer ${
                        isChecked ? "bg-indigo-950/20" : ""
                      }`}
                      onClick={() => setActiveDetailFav(fav)}
                    >
                      <td
                        className="p-3 text-center"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <input
                          type="checkbox"
                          data-testid={`library-row-checkbox-${fav.id}`}
                          checked={isChecked}
                          onChange={(e) =>
                            handleToggleSelectRow(fav.id, e.target.checked)
                          }
                          className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                        />
                      </td>
                      <td className="p-3 font-medium text-slate-100">
                        <div className="flex flex-col">
                          <span className="hover:text-indigo-400 font-semibold text-sm">
                            {fav.name}
                          </span>
                          <span className="text-[11px] text-slate-500 font-mono truncate max-w-[200px]">
                            {fav.github_url.replace("https://github.com/", "")}
                          </span>
                        </div>
                      </td>
                      <td className="p-3">
                        <span className="px-2 py-0.5 rounded text-[11px] bg-slate-900 text-indigo-300 border border-slate-700 font-medium">
                          {fav.category}
                        </span>
                      </td>
                      <td className="p-3">
                        <div className="flex flex-wrap gap-1 max-w-xs">
                          {fav.tags.slice(0, 3).map((t) => (
                            <span
                              key={t.id}
                              className={`px-1.5 py-0.2 rounded text-[10px] ${
                                t.source === "user"
                                  ? "bg-indigo-950 text-indigo-300 border border-indigo-700/60"
                                  : "bg-slate-900 text-slate-400 border border-slate-700"
                              }`}
                            >
                              #{t.tag}
                            </span>
                          ))}
                          {fav.tags.length > 3 && (
                            <span className="text-[10px] text-slate-500 self-center">
                              +{fav.tags.length - 3}
                            </span>
                          )}
                          {fav.tags.length === 0 && (
                            <span className="text-[10px] text-slate-600 italic">
                              -
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="p-3 text-right font-mono text-amber-300 font-medium">
                        ★ {fav.stars.toLocaleString()}
                      </td>
                      <td className="p-3 text-slate-400">{formattedDate}</td>
                      <td className="p-3">
                        {fav.rating != null ? (
                          <span className="text-amber-400 font-medium">
                            {"★".repeat(fav.rating)}
                            <span className="text-slate-600">
                              {"☆".repeat(5 - fav.rating)}
                            </span>
                          </span>
                        ) : (
                          <span className="text-slate-500 italic text-[11px]">
                            未评级
                          </span>
                        )}
                      </td>
                      <td className="p-3 text-slate-400">{formattedFavDate}</td>
                      <td className="p-3 text-slate-400">
                        {formattedRefreshDate}
                      </td>
                      <td
                        className="p-3 text-center"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <div className="flex items-center justify-center gap-2">
                          <button
                            type="button"
                            data-testid={`row-detail-btn-${fav.id}`}
                            onClick={() => setActiveDetailFav(fav)}
                            className="text-xs text-indigo-400 hover:text-indigo-300 font-medium"
                          >
                            详情
                          </button>
                          <button
                            type="button"
                            data-testid={`row-github-btn-${fav.id}`}
                            onClick={(e) => handleOpenGitHub(e, fav.github_url)}
                            className="text-xs text-slate-400 hover:text-slate-200"
                            title="在 GitHub 打开"
                          >
                            ↗
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Favorite Detail Modal */}
      <FavoriteDetailModal
        isOpen={activeDetailFav != null}
        favorite={activeDetailFav}
        onClose={() => setActiveDetailFav(null)}
        onUpdated={(updated) => {
          setActiveDetailFav(updated);
          setFavorites((prev) =>
            prev.map((item) => (item.id === updated.id ? updated : item))
          );
        }}
        onDeleted={(deletedId) => {
          setActiveDetailFav(null);
          setFavorites((prev) => prev.filter((item) => item.id !== deletedId));
          if (favorites.length <= 1) {
            setStatus("empty");
          }
        }}
      />

      {/* Batch Result Modal */}
      <BatchResultModal
        isOpen={batchRefreshResult != null}
        onClose={() => setBatchRefreshResult(null)}
        title="批量刷新元数据结果"
        refreshResult={batchRefreshResult}
      />
    </div>
  );
};
