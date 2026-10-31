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
import { formatDateYYYYMMDD } from "../utils/date";
import { formatCategory } from "../utils/category";
import {
  LIBRARY_COLUMNS,
  loadPersistedColumnWidths,
  persistColumnWidths,
  resetPersistedColumnWidths,
  getResolvedDescription,
  getUserTags,
} from "../utils/libraryTable";
import { CellViewerModal } from "../components/CellViewerModal";

export interface LibraryPageProps {
  initialOpenFavoriteId?: number | null;
  onClearInitialFavoriteId?: () => void;
}

export const LibraryPage: React.FC<LibraryPageProps> = ({
  initialOpenFavoriteId,
  onClearInitialFavoriteId,
}) => {
  const { t, locale } = useI18n();
  const { markUnfavorited } = useDiscoverySession();
  const [favorites, setFavorites] = useState<FavoriteItem[]>([]);

  // Column widths state and persistence
  const [columnWidths, setColumnWidths] = useState<Record<string, number>>(() =>
    loadPersistedColumnWidths()
  );
  const activeDragCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      activeDragCleanupRef.current?.();
    };
  }, []);

  // Cell viewer modal state (for full description, notes, user tags)
  const [viewerModalState, setViewerModalState] = useState<{
    isOpen: boolean;
    title: string;
    repoName: string;
    type: "description" | "notes" | "userTags";
    content?: string;
    tags?: string[];
    isTranslation?: boolean;
  }>({
    isOpen: false,
    title: "",
    repoName: "",
    type: "description",
  });

  const handlePointerDownResize = (colId: string, e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();

    const col = LIBRARY_COLUMNS.find((c) => c.id === colId);
    if (!col || !col.resizable) return;

    const startX =
      typeof e.clientX === "number" && !Number.isNaN(e.clientX)
        ? e.clientX
        : 0;
    const startWidth = columnWidths[colId] || col.defaultWidth;
    let currentW = startWidth;

    const target = e.currentTarget as HTMLElement;
    try {
      if (typeof target.setPointerCapture === "function") {
        target.setPointerCapture(e.pointerId);
      }
    } catch {
      // Ignore environments without pointer capture support
    }

    const handlePointerMove = (moveEv: PointerEvent | MouseEvent) => {
      if (typeof moveEv.clientX !== "number" || Number.isNaN(moveEv.clientX)) {
        return;
      }
      const deltaX = moveEv.clientX - startX;
      const newWidth = Math.max(
        col.minWidth,
        Math.min(col.maxWidth, Math.round(startWidth + deltaX))
      );
      currentW = newWidth;
      setColumnWidths((prev) => ({
        ...prev,
        [colId]: newWidth,
      }));
    };

    const removeListeners = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerCancel);
      document.removeEventListener("pointermove", handlePointerMove);
      document.removeEventListener("pointerup", handlePointerUp);
      document.removeEventListener("pointercancel", handlePointerCancel);
    };

    const cleanup = (upEv?: PointerEvent) => {
      try {
        if (upEv && typeof target.releasePointerCapture === "function") {
          target.releasePointerCapture(upEv.pointerId);
        }
      } catch {
        // Ignore environments without pointer capture support
      }
      removeListeners();
      activeDragCleanupRef.current = null;

      setColumnWidths((finalPrev) => {
        const updated = { ...finalPrev, [colId]: currentW };
        persistColumnWidths(updated);
        return updated;
      });
    };

    const handlePointerUp = (upEv: PointerEvent) => {
      cleanup(upEv);
    };

    const handlePointerCancel = (cancelEv: PointerEvent) => {
      cleanup(cancelEv);
    };

    activeDragCleanupRef.current = () => cleanup();
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);
    document.addEventListener("pointermove", handlePointerMove);
    document.addEventListener("pointerup", handlePointerUp);
    document.addEventListener("pointercancel", handlePointerCancel);
  };

  const handleKeyDownResize = (colId: string, e: React.KeyboardEvent) => {
    const col = LIBRARY_COLUMNS.find((c) => c.id === colId);
    if (!col || !col.resizable) return;

    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const step = 10;
      const delta = e.key === "ArrowRight" ? step : -step;
      const currentWidth = columnWidths[colId] || col.defaultWidth;
      const newWidth = Math.max(
        col.minWidth,
        Math.min(col.maxWidth, currentWidth + delta)
      );
      setColumnWidths((prev) => {
        const next = { ...prev, [colId]: newWidth };
        persistColumnWidths(next);
        return next;
      });
    }
  };

  const handleResetColumnWidths = () => {
    const defaults = resetPersistedColumnWidths();
    setColumnWidths(defaults);
  };

  const [distinctTags, setDistinctTags] = useState<string[]>([]);
  const totalCountRef = useRef<number | null>(null);
  const querySeqRef = useRef<number>(0);
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
    const seq = ++querySeqRef.current;
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

      if (seq !== querySeqRef.current) {
        return;
      }

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
      if (seq !== querySeqRef.current) {
        return;
      }
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
            data-testid="library-reset-column-widths-btn"
            onClick={handleResetColumnWidths}
            className="px-3 py-1.5 rounded-md border border-slate-700 bg-slate-800 text-slate-200 text-xs font-medium hover:bg-slate-700 transition-colors"
            title={t("library.table.resetColumnWidths")}
          >
            {t("library.table.resetColumnWidths")}
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
                  {formatCategory(t, cat)}
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
              style={{
                tableLayout: "fixed",
                minWidth: `${LIBRARY_COLUMNS.reduce(
                  (acc, col) => acc + (columnWidths[col.id] || col.defaultWidth),
                  0
                )}px`,
              }}
            >
              <colgroup>
                {LIBRARY_COLUMNS.map((col) => (
                  <col
                    key={col.id}
                    style={{
                      width: `${columnWidths[col.id] || col.defaultWidth}px`,
                    }}
                  />
                ))}
              </colgroup>
              <thead className="bg-slate-900 border-b border-slate-700 text-slate-400 uppercase tracking-wider font-semibold">
                <tr>
                  {LIBRARY_COLUMNS.map((col) => {
                    if (col.id === "selection") {
                      return (
                        <th
                          key={col.id}
                          data-testid="library-th-selection"
                          className="p-3 text-center align-middle"
                          style={{
                            width: `${columnWidths[col.id] || col.defaultWidth}px`,
                          }}
                        >
                          <input
                            type="checkbox"
                            data-testid="library-select-all-checkbox"
                            checked={allSelected}
                            onChange={(e) =>
                              handleSelectAllCurrent(e.target.checked)
                            }
                            className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                          />
                        </th>
                      );
                    }

                    const headerLabel = col.headerKey
                      ? t(`library.table.${col.headerKey}`)
                      : "";
                    const alignClass =
                      col.align === "right"
                        ? "text-right"
                        : col.align === "center"
                        ? "text-center"
                        : "text-left";

                    return (
                      <th
                        key={col.id}
                        data-testid={`library-th-${col.id}`}
                        className={`relative p-3 select-none ${alignClass}`}
                        style={{
                          width: `${columnWidths[col.id] || col.defaultWidth}px`,
                        }}
                      >
                        <span className="truncate block pr-2">{headerLabel}</span>
                        {col.resizable && (
                          <div
                            role="separator"
                            aria-orientation="vertical"
                            aria-label={t("library.table.resizeColumn", {
                              column: headerLabel,
                            })}
                            aria-valuemin={col.minWidth}
                            aria-valuemax={col.maxWidth}
                            aria-valuenow={
                              columnWidths[col.id] || col.defaultWidth
                            }
                            tabIndex={0}
                            data-testid={`column-resize-handle-${col.id}`}
                            className="absolute right-0 top-0 bottom-0 w-2.5 cursor-col-resize select-none flex items-center justify-center hover:bg-indigo-500/50 focus:outline-none focus:ring-2 focus:ring-indigo-400 group z-10"
                            onPointerDown={(e) =>
                              handlePointerDownResize(col.id, e)
                            }
                            onKeyDown={(e) => handleKeyDownResize(col.id, e)}
                            onClick={(e) => e.stopPropagation()}
                          >
                            <div className="w-[1px] h-3.5 bg-slate-600 group-hover:bg-indigo-300" />
                          </div>
                        )}
                      </th>
                    );
                  })}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-700/60 text-slate-200">
                {favorites.map((fav) => {
                  const isChecked = selectedIds.has(fav.id);
                  const formattedCreatedAt = formatDateYYYYMMDD(
                    fav.repo_created_at
                  );
                  const formattedDate = formatDateYYYYMMDD(fav.pushed_at);
                  const formattedFavDate = formatDateYYYYMMDD(fav.favorited_at);
                  const formattedRefreshDate = formatDateYYYYMMDD(
                    fav.metadata_refreshed_at
                  );
                  const resolvedDesc = getResolvedDescription(fav, locale);
                  const userTags = getUserTags(fav);
                  const trimmedNotes = fav.notes?.trim() || "";

                  return (
                    <tr
                      key={fav.id}
                      data-testid={`library-row-${fav.id}`}
                      className={`hover:bg-slate-700/40 transition-colors cursor-pointer ${
                        isChecked ? "bg-indigo-950/20" : ""
                      }`}
                      onClick={() => setActiveDetailFav(fav)}
                    >
                      {/* 1. selection */}
                      <td
                        className="p-3 text-center align-middle"
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          width: `${columnWidths["selection"] || 44}px`,
                        }}
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

                      {/* 2. name */}
                      <td
                        className="p-3 font-medium text-slate-100 align-top"
                        style={{
                          width: `${columnWidths["name"] || 180}px`,
                        }}
                      >
                        <div className="flex flex-col space-y-0.5">
                          <span className="hover:text-indigo-400 font-semibold text-sm">
                            {fav.name}
                          </span>
                          <span className="text-[11px] text-slate-500 font-mono truncate max-w-[200px]">
                            {fav.github_url.replace("https://github.com/", "")}
                          </span>
                          <div className="flex items-center gap-2 text-[10px] text-slate-400 pt-0.5">
                            <span data-testid={`library-releases-${fav.id}`}>
                              {fav.releases !== null &&
                              fav.releases !== undefined
                                ? `Releases ${fav.releases}`
                                : "Releases —"}
                            </span>
                            {fav.languages &&
                              Object.keys(fav.languages).length > 0 && (
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

                      {/* 3. category */}
                      <td
                        className="p-3 align-top"
                        style={{
                          width: `${columnWidths["category"] || 110}px`,
                        }}
                      >
                        <span className="px-2 py-0.5 rounded text-[11px] bg-slate-900 text-indigo-300 border border-slate-700 font-medium whitespace-nowrap">
                          {formatCategory(t, fav.category)}
                        </span>
                      </td>

                      {/* 4. description */}
                      <td
                        className="p-3 align-top"
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          width: `${columnWidths["description"] || 280}px`,
                        }}
                      >
                        <div className="flex flex-col space-y-1">
                          <div
                            data-testid={`library-desc-${fav.id}`}
                            className="text-slate-300 line-clamp-2 break-words text-[11px] leading-relaxed"
                            title={resolvedDesc.text}
                          >
                            {resolvedDesc.text}
                          </div>
                          {resolvedDesc.text !== "—" && (
                            <button
                              type="button"
                              data-testid={`library-desc-view-btn-${fav.id}`}
                              onClick={() =>
                                setViewerModalState({
                                  isOpen: true,
                                  title: t("library.table.description"),
                                  repoName: fav.name,
                                  type: "description",
                                  content: resolvedDesc.text,
                                  isTranslation: resolvedDesc.isTranslation,
                                })
                              }
                              className="text-[10px] text-indigo-400 hover:text-indigo-300 hover:underline self-start"
                            >
                              {t("library.table.viewFull")}
                            </button>
                          )}
                        </div>
                      </td>

                      {/* 5. userTags */}
                      <td
                        className="p-3 align-top"
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          width: `${columnWidths["userTags"] || 170}px`,
                        }}
                      >
                        <div
                          data-testid={`library-user-tags-${fav.id}`}
                          className="flex flex-wrap gap-1 items-center"
                        >
                          {userTags.length === 0 ? (
                            <span className="text-slate-500 italic text-[11px]">
                              —
                            </span>
                          ) : (
                            <>
                              {userTags.slice(0, 3).map((tagObj) => (
                                <span
                                  key={tagObj.id}
                                  className="px-1.5 py-0.5 rounded text-[10px] bg-indigo-950 text-indigo-300 border border-indigo-700/60 font-medium"
                                >
                                  #{tagObj.tag}
                                </span>
                              ))}
                              {userTags.length > 3 && (
                                <button
                                  type="button"
                                  data-testid={`library-user-tags-more-${fav.id}`}
                                  onClick={() =>
                                    setViewerModalState({
                                      isOpen: true,
                                      title: t("library.table.userTags"),
                                      repoName: fav.name,
                                      type: "userTags",
                                      tags: userTags.map((t) => t.tag),
                                    })
                                  }
                                  className="text-[10px] text-indigo-400 hover:text-indigo-300 px-1 py-0.5 bg-slate-900 border border-slate-700 rounded hover:border-indigo-500 transition-colors"
                                  title={t("library.table.viewFull")}
                                >
                                  +{userTags.length - 3}
                                </button>
                              )}
                            </>
                          )}
                        </div>
                      </td>

                      {/* 6. notes */}
                      <td
                        className="p-3 align-top"
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          width: `${columnWidths["notes"] || 220}px`,
                        }}
                      >
                        <div className="flex flex-col space-y-1">
                          <div
                            data-testid={`library-notes-${fav.id}`}
                            className="text-slate-300 line-clamp-2 break-words text-[11px] leading-relaxed"
                            title={trimmedNotes || undefined}
                          >
                            {trimmedNotes ? (
                              trimmedNotes
                            ) : (
                              <span className="text-slate-500 italic text-[11px]">
                                —
                              </span>
                            )}
                          </div>
                          {trimmedNotes ? (
                            <button
                              type="button"
                              data-testid={`library-notes-view-btn-${fav.id}`}
                              onClick={() =>
                                setViewerModalState({
                                  isOpen: true,
                                  title: t("library.table.notes"),
                                  repoName: fav.name,
                                  type: "notes",
                                  content: fav.notes,
                                })
                              }
                              className="text-[10px] text-indigo-400 hover:text-indigo-300 hover:underline self-start"
                            >
                              {t("library.table.viewFull")}
                            </button>
                          ) : null}
                        </div>
                      </td>

                      {/* 7. stars */}
                      <td
                        className="p-3 text-right font-mono text-amber-300 font-medium align-top"
                        style={{
                          width: `${columnWidths["stars"] || 90}px`,
                        }}
                      >
                        ★ {fav.stars.toLocaleString()}
                      </td>

                      {/* 8. createdAt */}
                      <td
                        className="p-3 text-slate-400 align-top whitespace-nowrap"
                        data-testid={`library-created-at-${fav.id}`}
                        style={{
                          width: `${columnWidths["createdAt"] || 110}px`,
                        }}
                      >
                        {formattedCreatedAt}
                      </td>

                      {/* 9. lastActive */}
                      <td
                        className="p-3 text-slate-400 align-top whitespace-nowrap"
                        data-testid={`library-last-active-${fav.id}`}
                        style={{
                          width: `${columnWidths["lastActive"] || 110}px`,
                        }}
                      >
                        {formattedDate}
                      </td>

                      {/* 10. rating */}
                      <td
                        className="p-3 align-top whitespace-nowrap"
                        style={{
                          width: `${columnWidths["rating"] || 90}px`,
                        }}
                      >
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

                      {/* 11. favoritedAt */}
                      <td
                        className="p-3 text-slate-400 align-top whitespace-nowrap"
                        data-testid={`library-favorited-at-${fav.id}`}
                        style={{
                          width: `${columnWidths["favoritedAt"] || 110}px`,
                        }}
                      >
                        {formattedFavDate}
                      </td>

                      {/* 12. lastRefresh */}
                      <td
                        className="p-3 text-slate-400 align-top whitespace-nowrap"
                        data-testid={`library-last-refresh-${fav.id}`}
                        style={{
                          width: `${columnWidths["lastRefresh"] || 110}px`,
                        }}
                      >
                        {formattedRefreshDate}
                      </td>

                      {/* 13. actions */}
                      <td
                        className="p-3 text-center align-top whitespace-nowrap"
                        onClick={(e) => e.stopPropagation()}
                        style={{
                          width: `${columnWidths["actions"] || 96}px`,
                        }}
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
      {/* Cell Full Viewer Modal */}
      <CellViewerModal
        isOpen={viewerModalState.isOpen}
        onClose={() =>
          setViewerModalState((prev) => ({ ...prev, isOpen: false }))
        }
        title={viewerModalState.title}
        repoName={viewerModalState.repoName}
        type={viewerModalState.type}
        content={viewerModalState.content}
        tags={viewerModalState.tags}
        isTranslation={viewerModalState.isTranslation}
      />
    </div>
  );
};
