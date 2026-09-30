import React, { useCallback, useEffect, useRef, useState } from "react";
import { BatchResultModal } from "../components/BatchResultModal";
import { DeleteConfirmModal } from "../components/DeleteConfirmModal";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import {
  deleteFavoritesBatch,
  listDistinctTags,
  listFavorites,
  openGitHubUrl,
  refreshFavoritesBatch,
} from "../services/tauri";
import {
  ALL_CATEGORIES,
  BatchDeleteResult,
  BatchRefreshProgress,
  BatchRefreshResult,
  FavoriteItem,
  LibraryQuery,
  LibrarySortField,
  SortOrder,
} from "../types/library";
import { getErrorMessage } from "../utils/error";
import { useDiscoverySession } from "../context/DiscoverySessionContext";
import { useI18n } from "../i18n/I18nContext";
import { formatRepositoryDate } from "../utils/date";

export interface LibraryPageProps {
  initialOpenFavoriteId?: number | null;
  onClearInitialFavoriteId?: () => void;
}

export const LibraryPage: React.FC<LibraryPageProps> = ({
  initialOpenFavoriteId,
  onClearInitialFavoriteId,
}) => {
  const { t } = useI18n();
  const { markUnfavorited } = useDiscoverySession();
  const [favorites, setFavorites] = useState<FavoriteItem[]>([]);

  const [distinctTags, setDistinctTags] = useState<string[]>([]);
  const totalCountRef = useRef<number | null>(null);
  const [status, setStatus] = useState<
    "loading" | "success" | "empty" | "filtered_empty" | "error"
  >("loading");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [operationNotice, setOperationNotice] = useState<string | null>(null);

  // Batch delete state
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  const [deleteSnapshot, setDeleteSnapshot] = useState<
    Array<{ id: number; name: string; repoId?: number | null; url?: string }>
  >([]);
  const [isDeleting, setIsDeleting] = useState(false);

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
  const [, setRefreshProgress] =
    useState<BatchRefreshProgress | null>(null);
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

  const hasActiveFilters = Boolean(
    searchQuery.trim() ||
      selectedCategory !== "all" ||
      selectedTag !== "all" ||
      selectedRating !== "all"
  );

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

      if (!hasActiveFilters) {
        totalCountRef.current = favs.length;
      }

      if (favs.length === 0) {
        if (hasActiveFilters && (totalCountRef.current === null || totalCountRef.current > 0)) {
          setStatus("filtered_empty");
        } else {
          setStatus("empty");
        }
      } else {
        setStatus("success");
      }
    } catch (err) {
      const msg = getErrorMessage(err);
      setErrorMessage(msg);
      setStatus("error");
    }
  }, [searchQuery, selectedCategory, selectedTag, selectedRating, sortKey, hasActiveFilters]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleClearFilters = () => {
    setSearchQuery("");
    setSelectedCategory("all");
    setSelectedTag("all");
    setSelectedRating("all");
  };

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

    setOperationError(null);
    setBatchRefreshing(true);
    setRefreshProgress({ completed: 0, total: ids.length });
    setRefreshProgressText(t("library.refreshProgressText", { current: 0, total: ids.length }));
    try {
      const idsWithProgress = [...ids];
      Object.defineProperty(idsWithProgress, "onProgress", {
        value: (prog: BatchRefreshProgress) => {
          setRefreshProgress(prog);
          setRefreshProgressText(t("library.refreshProgressText", { current: prog.completed, total: prog.total }));
        },
        enumerable: false,
      });
      const res = await refreshFavoritesBatch(idsWithProgress);
      setRefreshProgress({ completed: ids.length, total: ids.length });
      setRefreshProgressText(t("library.refreshProgressText", { current: ids.length, total: ids.length }));
      setBatchRefreshResult(res);
      await loadData();
    } catch (err) {
      const msg = getErrorMessage(err);
      setOperationError(msg);
    } finally {
      setBatchRefreshing(false);
      setRefreshProgress(null);
      setRefreshProgressText(null);
    }
  };

  const handleRefreshAll = async () => {
    if (favorites.length === 0) return;
    const allIds = favorites.map((f) => f.id);

    setOperationError(null);
    setBatchRefreshing(true);
    setRefreshProgress({ completed: 0, total: allIds.length });
    setRefreshProgressText(t("library.refreshProgressText", { current: 0, total: allIds.length }));
    try {
      const allIdsWithProgress = [...allIds];
      Object.defineProperty(allIdsWithProgress, "onProgress", {
        value: (prog: BatchRefreshProgress) => {
          setRefreshProgress(prog);
          setRefreshProgressText(t("library.refreshProgressText", { current: prog.completed, total: prog.total }));
        },
        enumerable: false,
      });
      const res = await refreshFavoritesBatch(allIdsWithProgress);
      setRefreshProgress({ completed: allIds.length, total: allIds.length });
      setRefreshProgressText(t("library.refreshProgressText", { current: allIds.length, total: allIds.length }));
      setBatchRefreshResult(res);
      await loadData();
    } catch (err) {
      const msg = getErrorMessage(err);
      setOperationError(msg);
    } finally {
      setBatchRefreshing(false);
      setRefreshProgress(null);
      setRefreshProgressText(null);
    }
  };

  const handleOpenDeleteModal = () => {
    if (selectedIds.size === 0 || batchRefreshing || isDeleting) return;
    const snapshot = favorites
      .filter((f) => selectedIds.has(f.id))
      .map((f) => ({
        id: f.id,
        name: f.name,
        repoId: f.github_repo_id,
        url: f.github_url,
      }));
    setDeleteSnapshot(snapshot);
    setIsDeleteModalOpen(true);
  };

  const handleConfirmDelete = async () => {
    if (deleteSnapshot.length === 0 || isDeleting) return;
    setIsDeleting(true);
    setOperationError(null);
    setOperationNotice(null);

    const targetIds = deleteSnapshot.map((item) => item.id);

    try {
      const res: BatchDeleteResult = await deleteFavoritesBatch(targetIds);
      const deletedSet = new Set(res.deleted_ids);

      for (const item of deleteSnapshot) {
        if (deletedSet.has(item.id)) {
          markUnfavorited(item.repoId, item.url);
        }
      }

      setFavorites((prev) => prev.filter((f) => !deletedSet.has(f.id)));

      setSelectedIds((prev) => {
        const next = new Set(prev);
        for (const id of res.deleted_ids) {
          next.delete(id);
        }
        return next;
      });

      if (totalCountRef.current != null) {
        totalCountRef.current = Math.max(0, totalCountRef.current - res.deleted_count);
      }

      const remainingVisible = favorites.filter((f) => !deletedSet.has(f.id)).length;
      const remainingTotal = totalCountRef.current ?? 0;
      if (remainingTotal === 0) {
        setStatus("empty");
      } else if (remainingVisible === 0) {
        setStatus(hasActiveFilters ? "filtered_empty" : "empty");
      } else {
        setStatus("success");
      }

      if (activeDetailFav && deletedSet.has(activeDetailFav.id)) {
        setActiveDetailFav(null);
      }

      if (res.deleted_count > 0) {
        setOperationNotice(t("library.deleteSuccess", { count: res.deleted_count }));
      } else if (res.not_found_count > 0) {
        setOperationNotice(t("library.deleteNotFound", { count: res.not_found_count }));
      }

      setIsDeleteModalOpen(false);
      setDeleteSnapshot([]);
    } catch (err) {
      const msg = getErrorMessage(err);
      setOperationError(t("library.deleteFailed", { message: msg }));
      setIsDeleteModalOpen(false);
    } finally {
      setIsDeleting(false);
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
            <span>📚</span> {t("library.title")}
            <span className="text-xs font-normal text-slate-400 font-mono ml-2">
              Local Repository Library
            </span>
          </h1>
          <p className="text-slate-400 mt-1 text-sm">
            {t("library.subtitle")}
          </p>
        </div>

        {/* Batch action buttons */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-testid="library-delete-selected-btn"
            disabled={selectedIds.size === 0 || batchRefreshing || isDeleting}
            onClick={handleOpenDeleteModal}
            className="px-3 py-1.5 rounded-md border border-rose-800/80 bg-rose-950/50 text-rose-200 text-xs font-medium hover:bg-rose-900/60 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {t("library.deleteSelectedBtn", { count: selectedIds.size })}
          </button>
          <button
            type="button"
            data-testid="library-refresh-selected-btn"
            disabled={selectedIds.size === 0 || batchRefreshing || isDeleting}
            onClick={handleRefreshSelected}
            className="px-3 py-1.5 rounded-md border border-slate-700 bg-slate-800 text-slate-200 text-xs font-medium hover:bg-slate-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {t("library.refreshSelectedBtn", { count: selectedIds.size })}
          </button>
          <button
            type="button"
            data-testid="library-refresh-all-btn"
            disabled={favorites.length === 0 || batchRefreshing || isDeleting}
            onClick={handleRefreshAll}
            className="px-3.5 py-1.5 rounded-md bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {t("library.refreshAllBtn")}
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
            {t("library.searchLabel")}
          </label>
          <div className="relative">
            <input
              id="library-search-input"
              type="text"
              data-testid="library-search-input"
              placeholder={t("library.searchPlaceholder")}
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
              {t("library.filterCategory")}
            </label>
            <select
              id="library-category-filter"
              data-testid="library-category-filter"
              value={selectedCategory}
              onChange={(e) => setSelectedCategory(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">{t("library.allCategories")}</option>
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
              {t("library.filterTag")}
            </label>
            <select
              id="library-tag-filter"
              data-testid="library-tag-filter"
              value={selectedTag}
              onChange={(e) => setSelectedTag(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">{t("library.allTags")}</option>
              {distinctTags.map((tag) => (
                <option key={tag} value={tag}>
                  #{tag}
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
              {t("library.filterRating")}
            </label>
            <select
              id="library-rating-filter"
              data-testid="library-rating-filter"
              value={selectedRating}
              onChange={(e) => setSelectedRating(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="all">{t("library.allRatings")}</option>
              <option value="unrated">{t("library.ratingUnrated")}</option>
              <option value="5">{t("library.ratingStars", { stars: "★★★★★", count: 5 })}</option>
              <option value="4">{t("library.ratingStars", { stars: "★★★★☆", count: 4 })}</option>
              <option value="3">{t("library.ratingStars", { stars: "★★★☆☆", count: 3 })}</option>
              <option value="2">{t("library.ratingStars", { stars: "★★☆☆☆", count: 2 })}</option>
              <option value="1">{t("library.ratingStars", { stars: "★☆☆☆☆", count: 1 })}</option>
            </select>
          </div>

          {/* Sort dropdown */}
          <div>
            <label
              htmlFor="library-sort-select"
              className="block font-medium text-slate-400 mb-1"
            >
              {t("library.sortBy")}
            </label>
            <select
              id="library-sort-select"
              data-testid="library-sort-select"
              value={sortKey}
              onChange={(e) => setSortKey(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="favorited_at_desc">{t("library.sortOptions.favoritedAtDesc")}</option>
              <option value="favorited_at_asc">{t("library.sortOptions.favoritedAtAsc")}</option>
              <option value="stars_desc">{t("library.sortOptions.starsDesc")}</option>
              <option value="stars_asc">{t("library.sortOptions.starsAsc")}</option>
              <option value="rating_desc">{t("library.sortOptions.ratingDesc")}</option>
              <option value="rating_asc">{t("library.sortOptions.ratingAsc")}</option>
              <option value="pushed_at_desc">{t("library.sortOptions.pushedAtDesc")}</option>
              <option value="pushed_at_asc">{t("library.sortOptions.pushedAtAsc")}</option>
              <option value="name_asc">{t("library.sortOptions.nameAsc")}</option>
              <option value="name_desc">{t("library.sortOptions.nameDesc")}</option>
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
          <span>{refreshProgressText || t("library.refreshProgress")}</span>
        </div>
      )}

      {/* Loading state */}
      {status === "loading" && !batchRefreshing && (
        <div
          data-testid="library-loading-state"
          className="rounded-lg border border-slate-700 bg-slate-800/40 p-12 text-center"
        >
          <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
          <p className="text-sm text-slate-300">{t("library.loadingText")}</p>
        </div>
      )}

      {/* Error state */}
      {status === "error" && (
        <div
          data-testid="library-error-state"
          className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-6 text-center space-y-3"
        >
          <div className="text-xl">⚠️</div>
          <h3 className="text-sm font-semibold text-rose-200">{t("library.loadFailed")}</h3>
          <p className="text-xs text-rose-300/90">{errorMessage}</p>
          <button
            type="button"
            data-testid="library-retry-btn"
            onClick={() => loadData()}
            className="px-4 py-1.5 rounded-md bg-rose-700 hover:bg-rose-600 text-white text-xs font-medium transition-colors"
          >
            {t("library.retry")}
          </button>
        </div>
      )}

      {/* Operation Notice Banner (Recoverable Success Notice) */}
      {operationNotice && (
        <div
          data-testid="library-operation-notice"
          className="flex items-center justify-between p-3 rounded-md bg-emerald-950/40 border border-emerald-800 text-emerald-300 text-xs"
        >
          <div className="flex items-center gap-2">
            <span>✓</span>
            <span>{operationNotice}</span>
          </div>
          <button
            type="button"
            data-testid="library-operation-notice-close"
            onClick={() => setOperationNotice(null)}
            className="text-emerald-400 hover:text-emerald-200 text-xs font-semibold px-2 py-0.5"
            title={t("common.close")}
          >
            ✕
          </button>
        </div>
      )}

      {/* Operation Error Banner (Recoverable Batch Error) */}
      {operationError && (
        <div
          data-testid="library-operation-error"
          className="flex items-center justify-between p-3 rounded-md bg-rose-950/40 border border-rose-800 text-rose-300 text-xs"
        >
          <div className="flex items-center gap-2">
            <span>⚠️</span>
            <span>{operationError}</span>
          </div>
          <button
            type="button"
            data-testid="library-operation-error-close"
            onClick={() => setOperationError(null)}
            className="text-rose-400 hover:text-rose-200 text-xs font-semibold px-2 py-0.5"
            title={t("common.close")}
          >
            ✕
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
            {t("library.emptyTitle")}
          </h3>
          <p className="text-sm text-slate-400 max-w-md mx-auto">
            {t("library.emptySubtitle")}
          </p>
        </div>
      )}

      {/* Filtered empty state */}
      {status === "filtered_empty" && (
        <div
          data-testid="library-filter-empty-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/30 p-12 text-center space-y-3"
        >
          <div className="text-4xl">🔍</div>
          <h3 className="text-base font-semibold text-slate-200">
            {t("library.filteredEmptyTitle")}
          </h3>
          <p className="text-sm text-slate-400 max-w-md mx-auto">
            {t("library.filteredEmptySubtitle")}
          </p>
          <button
            type="button"
            data-testid="library-clear-filters-btn"
            onClick={handleClearFilters}
            className="px-4 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium transition-colors"
          >
            {t("library.clearFiltersBtn")}
          </button>
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
                  <th className="p-3 min-w-[140px]">{t("library.table.name")}</th>
                  <th className="p-3 min-w-[120px]">{t("library.table.category")}</th>
                  <th className="p-3 min-w-[160px]">{t("library.table.tags")}</th>
                  <th className="p-3 text-right min-w-[80px]">{t("library.table.stars")}</th>
                  <th className="p-3 min-w-[100px]">{t("library.table.createdAt")}</th>
                  <th className="p-3 min-w-[100px]">{t("library.table.lastActive")}</th>
                  <th className="p-3 min-w-[90px]">{t("library.table.rating")}</th>
                  <th className="p-3 min-w-[120px]">{t("library.table.favoritedAt")}</th>
                  <th className="p-3 min-w-[120px]">{t("library.table.lastRefresh")}</th>
                  <th className="p-3 text-center min-w-[100px]">{t("library.table.actions")}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-700/60 text-slate-200">
                {favorites.map((fav) => {
                  const isChecked = selectedIds.has(fav.id);
                  const formattedCreatedAt = formatRepositoryDate(
                    fav.repo_created_at,
                    undefined,
                    undefined,
                    t("common.unknown")
                  );
                  const formattedDate = fav.pushed_at
                    ? new Date(fav.pushed_at).toLocaleDateString()
                    : t("common.unknown");
                  const formattedFavDate = fav.favorited_at
                    ? new Date(fav.favorited_at).toLocaleDateString()
                    : t("common.unknown");
                  const formattedRefreshDate = fav.metadata_refreshed_at
                    ? new Date(fav.metadata_refreshed_at).toLocaleDateString()
                    : t("common.unknown");

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
                        <div className="flex flex-col space-y-0.5">
                          <span className="hover:text-indigo-400 font-semibold text-sm">
                            {fav.name}
                          </span>
                          <span className="text-[11px] text-slate-500 font-mono truncate max-w-[200px]">
                            {fav.github_url.replace("https://github.com/", "")}
                          </span>
                          <div className="flex items-center gap-2 text-[10px] text-slate-400 pt-0.5">
                            <span data-testid={`library-releases-${fav.id}`}>
                              {fav.releases !== null && fav.releases !== undefined
                                ? `Releases ${fav.releases}`
                                : "Releases —"}
                            </span>
                            {fav.languages && Object.keys(fav.languages).length > 0 && (
                              <span
                                data-testid={`library-languages-${fav.id}`}
                                className="text-slate-500 font-mono"
                              >
                                {Object.entries(fav.languages)
                                  .sort((a, b) => b[1] - a[1])
                                  .slice(0, 2)
                                  .map(([l, p]) => `${l} ${p.toFixed(0)}%`)
                                  .join(", ")}
                              </span>
                            )}
                          </div>
                        </div>
                      </td>
                      <td className="p-3">
                        <span className="px-2 py-0.5 rounded text-[11px] bg-slate-900 text-indigo-300 border border-slate-700 font-medium">
                          {fav.category}
                        </span>
                      </td>
                      <td className="p-3">
                        <div className="flex flex-wrap gap-1 max-w-xs">
                          {fav.tags.slice(0, 3).map((tagObj) => (
                            <span
                              key={tagObj.id}
                              className={`px-1.5 py-0.2 rounded text-[10px] ${
                                tagObj.source === "user"
                                  ? "bg-indigo-950 text-indigo-300 border border-indigo-700/60"
                                  : "bg-slate-900 text-slate-400 border border-slate-700"
                              }`}
                            >
                              #{tagObj.tag}
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
                      <td className="p-3 text-slate-400" data-testid={`library-created-at-${fav.id}`}>{formattedCreatedAt}</td>
                      <td className="p-3 text-slate-400" data-testid={`library-last-active-${fav.id}`}>{formattedDate}</td>
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
                            {t("library.ratingScoreNone")}
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
                            {t("library.table.openDetail")}
                          </button>
                          <button
                            type="button"
                            data-testid={`row-github-btn-${fav.id}`}
                            onClick={(e) => handleOpenGitHub(e, fav.github_url)}
                            className="text-xs text-slate-400 hover:text-slate-200"
                            title={t("library.table.openGitHub")}
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
          const toDelete = favorites.find((item) => item.id === deletedId);
          if (toDelete) {
            markUnfavorited(toDelete.github_repo_id, toDelete.github_url);
          }
          setActiveDetailFav(null);
          setFavorites((prev) => prev.filter((item) => item.id !== deletedId));
          if (totalCountRef.current != null) {
            totalCountRef.current = Math.max(0, totalCountRef.current - 1);
          }
          const remainingTotal = totalCountRef.current ?? 0;
          const remainingVisible = favorites.filter((item) => item.id !== deletedId).length;

          if (remainingTotal === 0) {
            setStatus("empty");
          } else if (remainingVisible === 0) {
            setStatus(hasActiveFilters ? "filtered_empty" : "empty");
          } else {
            setStatus("success");
          }
        }}
      />

      {/* Batch Result Modal */}
      <BatchResultModal
        isOpen={batchRefreshResult != null}
        onClose={() => setBatchRefreshResult(null)}
        title={t("library.batchModalTitle")}
        refreshResult={batchRefreshResult}
      />

      {/* Delete Confirmation Modal */}
      <DeleteConfirmModal
        isOpen={isDeleteModalOpen}
        targetCount={deleteSnapshot.length}
        targetNames={deleteSnapshot.map((s) => s.name)}
        isDeleting={isDeleting}
        onConfirm={handleConfirmDelete}
        onCancel={() => {
          if (!isDeleting) {
            setIsDeleteModalOpen(false);
            setDeleteSnapshot([]);
          }
        }}
      />
    </div>
  );
};
