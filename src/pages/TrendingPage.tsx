import React, { useContext, useEffect, useRef, useState } from "react";
import { BatchResultModal } from "../components/BatchResultModal";
import { DuplicateModal } from "../components/DuplicateModal";
import { RepositoryCard } from "../components/RepositoryCard";
import { AnalysisPreviewModal } from "../components/AnalysisPreviewModal";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import {
  checkFavoriteStatus,
  favoriteRepositoriesBatch,
  favoriteRepository,
  fetchSettings,
  fetchTrending,
  getAiSearchReadiness,
  getFavorite,
  refreshFavorite,
  translateRepositoryDescription,
} from "../services/tauri";
import { AnalysisEnvelopeV1, BatchFavoriteResult, FavoriteItem } from "../types/library";
import { AiSearchReadiness } from "../types/github";
import {
  TrendingPeriod,
  TrendingRepositoryItem,
  TrendingResponse,
} from "../types/trending";
import {
  DiscoverySessionContext,
  SessionTranslationItem,
} from "../context/DiscoverySessionContext";
import { useI18n } from "../i18n/I18nContext";

export interface TrendingPageProps {
  onNavigateToFavorite?: (favoriteId: number) => void;
}

export const TrendingPage: React.FC<TrendingPageProps> = ({
  onNavigateToFavorite,
}) => {
  const { t, locale } = useI18n();
  const sessionCtx = useContext(DiscoverySessionContext);

  // If outside DiscoverySessionProvider (e.g. isolated legacy unit test), use local state
  const [localPeriod, setLocalPeriod] = useState<TrendingPeriod>("daily");
  const [localLimit, setLocalLimit] = useState<number>(sessionCtx ? 10 : 20);
  const [localStatus, setLocalStatus] = useState<
    "idle" | "loading" | "success" | "empty" | "error"
  >("idle");
  const [localResponse, setLocalResponse] = useState<TrendingResponse | null>(null);
  const [localErrorMessage, setLocalErrorMessage] = useState<string | null>(null);
  const [localSelectedUrls, setLocalSelectedUrls] = useState<Set<string>>(new Set());
  const [localTranslations, setLocalTranslations] = useState<Record<string, SessionTranslationItem>>({});

  const [translatingBatch, setTranslatingBatch] = useState(false);
  const [batchProgress, setBatchProgress] = useState({ total: 0, completed: 0, skipped: 0, failed: 0 });
  const cancelBatchRef = useRef(false);

  const [aiReadiness, setAiReadiness] = useState<AiSearchReadiness>({
    ready: false,
    reason: null,
  });
  const [aiReadinessLoading, setAiReadinessLoading] = useState(true);

  useEffect(() => {
    setAiReadinessLoading(true);
    getAiSearchReadiness()
      .then((r) => {
        setAiReadiness(r);
        setAiReadinessLoading(false);
      })
      .catch(() => {
        setAiReadiness({ ready: false, reason: null });
        setAiReadinessLoading(false);
      });
  }, [sessionCtx?.aiRevision]);

  const period = sessionCtx ? sessionCtx.trending.period : localPeriod;
  const limit = sessionCtx ? sessionCtx.trending.limit : localLimit;
  const status = sessionCtx ? sessionCtx.trending.status : localStatus;
  const response = sessionCtx ? sessionCtx.trending.response : localResponse;
  const errorMessage = sessionCtx ? sessionCtx.trending.errorMessage : localErrorMessage;
  const selectedUrls = sessionCtx ? sessionCtx.trending.selectedUrls : localSelectedUrls;
  const translations = sessionCtx ? sessionCtx.trending.translations : localTranslations;

  const updateTrendingTranslation = sessionCtx
    ? sessionCtx.updateTrendingTranslation
    : (url: string, item: SessionTranslationItem) => {
        setLocalTranslations((prev) => ({ ...prev, [url]: item }));
      };

  const setPeriod = (p: TrendingPeriod) => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({ ...prev, period: p }));
    } else {
      setLocalPeriod(p);
    }
  };

  const setLimit = (l: number) => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({ ...prev, limit: l }));
    } else {
      setLocalLimit(l);
    }
  };

  const setSelectedUrls = (updater: Set<string> | ((prev: Set<string>) => Set<string>)) => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({
        ...prev,
        selectedUrls: typeof updater === "function" ? updater(prev.selectedUrls) : updater,
      }));
    } else {
      setLocalSelectedUrls((prev) => (typeof updater === "function" ? updater(prev) : updater));
    }
  };

  const isFavorited = sessionCtx ? sessionCtx.isFavorited : () => false;
  const markFavorited = sessionCtx ? sessionCtx.markFavorited : () => {};
  const reconcileIdentities = sessionCtx
    ? sessionCtx.reconcileIdentities
    : async () => {};
  const nextTrendingGeneration = sessionCtx
    ? sessionCtx.nextTrendingGeneration
    : () => 1;

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

  const [analysisPreviewTarget, setAnalysisPreviewTarget] = useState<TrendingRepositoryItem | null>(null);
  const [activeDetailFavorite, setActiveDetailFavorite] = useState<FavoriteItem | null>(null);

  const setStatus = (s: "idle" | "loading" | "success" | "empty" | "error") => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({ ...prev, status: s }));
    } else {
      setLocalStatus(s);
    }
  };

  const setResponse = (r: TrendingResponse | null) => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({ ...prev, response: r }));
    } else {
      setLocalResponse(r);
    }
  };

  const setErrorMessage = (msg: string | null) => {
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({ ...prev, errorMessage: msg }));
    } else {
      setLocalErrorMessage(msg);
    }
  };

  // Read default_trending_period from settings on mount if not yet modified (WITHOUT auto-fetching)
  useEffect(() => {
    fetchSettings()
      .then((s) => {
        if (s?.github?.default_trending_period) {
          const p = s.github.default_trending_period;
          if (p === "daily" || p === "weekly" || p === "monthly") {
            setPeriod(p);
          }
        }
      })
      .catch((err) => {
        console.warn("Failed to load default trending period:", err);
      });
  }, []);

  // Reconcile favorite identities on response change
  useEffect(() => {
    if (response?.items && response.items.length > 0) {
      reconcileIdentities(
        response.items.map((item) => ({
          github_repo_id: item.github_repo_id,
          github_url: item.github_url,
        }))
      );
    }
  }, [response, reconcileIdentities]);

  const handleFetch = async () => {
    const currentGen = nextTrendingGeneration();
    if (sessionCtx) {
      sessionCtx.setTrending((prev) => ({
        ...prev,
        status: "loading",
        errorMessage: null,
        generation: currentGen,
      }));
    } else {
      setStatus("loading");
      setErrorMessage(null);
    }

    try {
      const res = await fetchTrending(period, limit);
      if (sessionCtx) {
        sessionCtx.setTrending((prev) => {
          if (prev.generation !== currentGen) {
            return prev;
          }
          return {
            ...prev,
            response: res,
            selectedUrls: new Set(),
            status: res.items.length === 0 ? "empty" : "success",
            errorMessage: null,
          };
        });
      } else {
        setResponse(res);
        setSelectedUrls(new Set());
        setStatus(res.items.length === 0 ? "empty" : "success");
        setErrorMessage(null);
      }
      setActionBanner(null);
    } catch (err: unknown) {
      console.error("Failed to fetch trending repositories:", err);
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      if (sessionCtx) {
        sessionCtx.setTrending((prev) => {
          if (prev.generation !== currentGen) {
            return prev;
          }
          return {
            ...prev,
            errorMessage: msg || t("trending.fetchError"),
            status: "error",
          };
        });
      } else {
        setErrorMessage(msg || t("trending.fetchError"));
        setStatus("error");
      }
    }
  };

  const handleBatchTranslate = async () => {
    if (!response || response.items.length === 0 || translatingBatch) return;
    if (!aiReadiness.ready || aiReadinessLoading) return;
    setTranslatingBatch(true);
    cancelBatchRef.current = false;
    const targetLocale = locale;
    const total = response.items.length;
    let completed = 0;
    let skipped = 0;
    let failed = 0;
    setBatchProgress({ total, completed, skipped, failed });

    for (const item of response.items) {
      if (cancelBatchRef.current) {
        break;
      }

      if (!item.description || item.description.trim() === "") {
        skipped++;
        setBatchProgress({ total, completed, skipped, failed });
        continue;
      }

      const existing = translations[item.github_url];
      if (
        existing &&
        existing.status === "valid" &&
        existing.target_locale === targetLocale
      ) {
        skipped++;
        setBatchProgress({ total, completed, skipped, failed });
        continue;
      }

      try {
        const res = await translateRepositoryDescription(
          item.description,
          targetLocale
        );
        if (cancelBatchRef.current) {
          break;
        }
        if (res.action === "unchanged") {
          skipped++;
          updateTrendingTranslation(item.github_url, {
            text: "",
            target_locale: res.target_locale,
            action: "unchanged",
            status: "valid",
            source_hash: res.source_hash,
          });
        } else if (res.action === "translated" && res.translated_text) {
          completed++;
          updateTrendingTranslation(item.github_url, {
            text: res.translated_text,
            target_locale: res.target_locale,
            action: "translated",
            status: "valid",
            source_hash: res.source_hash,
          });
        } else {
          failed++;
        }
      } catch {
        if (cancelBatchRef.current) {
          break;
        }
        failed++;
      }
      setBatchProgress({ total, completed, skipped, failed });
    }

    setTranslatingBatch(false);
  };

  const handleCancelBatchTranslate = () => {
    cancelBatchRef.current = true;
    setTranslatingBatch(false);
  };

  const handleSingleFavorite = async (repo: TrendingRepositoryItem) => {
    setActionBanner(null);
    try {
      const trans = translations[repo.github_url];
      const translationInput =
        trans && (trans.action === "translated" || trans.status === "valid") && trans.text
          ? { text: trans.text, target_locale: trans.target_locale }
          : undefined;

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
        repo_created_at: repo.created_at,
        releases: repo.releases,
        languages: repo.languages,
        translation: translationInput,
      });
      markFavorited(repo.github_repo_id, repo.github_url);
      setActionBanner({
        type: "success",
        message: t("trending.favoriteSuccess", { name: repo.name }),
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
          message: t("trending.favoriteFailed", { error: errObj?.message || String(err) }),
        });
      }
    }
  };

  const handleBatchFavorite = async () => {
    if (!response) return;
    const selectedRepos = response.items.filter((r) =>
      selectedUrls.has(r.github_url)
    );
    if (selectedRepos.length === 0) return;

    setActionBanner(null);
    try {
      const inputs = selectedRepos.map((r) => {
        const trans = translations[r.github_url];
        const translationInput =
          trans && (trans.action === "translated" || trans.status === "valid") && trans.text
            ? { text: trans.text, target_locale: trans.target_locale }
            : undefined;

        return {
          github_repo_id: r.github_repo_id,
          github_url: r.github_url,
          name: r.name,
          description: r.description,
          category: r.category,
          stars: Number(r.stars),
          license: r.license,
          topics: r.topics,
          pushed_at: r.pushed_at,
          repo_created_at: r.created_at,
          releases: r.releases,
          languages: r.languages,
          translation: translationInput,
        };
      });

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
        message: t("trending.batchFailed", { error: String(err) }),
      });
    }
  };

  const handleSelectAllCurrent = (checked: boolean) => {
    if (!response) return;
    if (checked) {
      setSelectedUrls(new Set(response.items.map((r) => r.github_url)));
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
          message: t("trending.refreshSuccess"),
        });
      } else {
        setActionBanner({
          type: "error",
          message: t("trending.refreshFailed", { error: res.message || res.status }),
        });
      }
    } catch (err) {
      setActionBanner({
        type: "error",
        message: t("trending.refreshRequestError", { error: String(err) }),
      });
    } finally {
      setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    }
  };

  const handleDuplicateOpenRecord = (favoriteId: number) => {
    setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    onNavigateToFavorite?.(favoriteId);
  };

  const handleOpenAnalysis = async (repo: TrendingRepositoryItem) => {
    const isFav = isFavorited(repo.github_repo_id, repo.github_url);
    if (isFav) {
      try {
        const status = await checkFavoriteStatus(repo.github_url, repo.github_repo_id);
        if (status.favorite_id) {
          const fav = await getFavorite(status.favorite_id);
          setActiveDetailFavorite(fav);
          return;
        }
      } catch (err) {
        console.warn("Failed to lookup favorite details for analysis:", err);
      }
    }
    setAnalysisPreviewTarget(repo);
  };

  const handleFavoriteWithPreview = async (envelope: AnalysisEnvelopeV1) => {
    if (!analysisPreviewTarget) return;
    const repo = analysisPreviewTarget;
    try {
      const trans = translations[repo.github_url];
      const translationInput =
        trans && (trans.action === "translated" || trans.status === "valid") && trans.text
          ? { text: trans.text, target_locale: trans.target_locale }
          : undefined;

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
        repo_created_at: repo.created_at,
        releases: repo.releases,
        languages: repo.languages,
        translation: translationInput,
        ai_analysis: envelope,
      });

      markFavorited(repo.github_repo_id, repo.github_url);
      setAnalysisPreviewTarget(null);
      setActionBanner({
        type: "success",
        message: t("trending.favoriteSuccess", { name: repo.name }),
      });
    } catch (err: unknown) {
      const errObj = err as { code?: string; message?: string } | undefined;
      setActionBanner({
        type: "error",
        message: t("trending.favoriteFailed", { error: errObj?.message || String(err) }),
      });
    }
  };

  const getPeriodLabel = (p: TrendingPeriod): string => {
    switch (p) {
      case "daily":
        return t("trending.starsToday");
      case "weekly":
        return t("trending.starsThisWeek");
      case "monthly":
        return t("trending.starsThisMonth");
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>🔥</span> {t("trending.title")}
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            {t("trending.badge")}
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          {t("trending.subtitle")}
        </p>
      </div>

      {/* Controls Bar */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-4">
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 items-end">
          {/* Period */}
          <div>
            <label
              htmlFor="trending-period-select"
              className="block text-xs font-medium text-slate-300 mb-1.5"
            >
              {t("trending.periodLabel")}
            </label>
            <select
              id="trending-period-select"
              data-testid="trending-period-select"
              value={period}
              onChange={(e) => setPeriod(e.target.value as TrendingPeriod)}
              className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            >
              <option value="daily">{t("trending.periods.daily")}</option>
              <option value="weekly">{t("trending.periods.weekly")}</option>
              <option value="monthly">{t("trending.periods.monthly")}</option>
            </select>
          </div>

          {/* Limit */}
          <div>
            <label
              htmlFor="trending-limit-select"
              className="block text-xs font-medium text-slate-300 mb-1.5"
            >
              {t("trending.limitLabel")}
            </label>
            <select
              id="trending-limit-select"
              data-testid="trending-limit-select"
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            >
              <option value={10}>10</option>
              <option value={20}>20</option>
              <option value={30}>30</option>
              <option value={50}>50</option>
            </select>
          </div>

          {/* Action Button */}
          <div>
            <button
              type="button"
              onClick={handleFetch}
              disabled={status === "loading"}
              data-testid="trending-fetch-btn"
              className="w-full inline-flex justify-center items-center gap-2 rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 focus:ring-offset-slate-900 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {status === "loading" ? (
                <>
                  <span className="inline-block animate-spin">⟳</span>
                  <span>{t("trending.fetchingBtn")}</span>
                </>
              ) : (
                <>
                  <span>🔥</span>
                  <span>{t("trending.fetchBtn")}</span>
                </>
              )}
            </button>
          </div>
        </div>
      </div>

      {/* Partial Warning Banner */}
      {status === "success" && response?.partial_warning && (
        <div
          data-testid="trending-partial-warning"
          className="rounded-lg border border-amber-500/40 bg-amber-950/30 p-4 text-sm text-amber-200 flex items-start gap-3"
        >
          <span className="text-amber-400 text-lg shrink-0">⚠️</span>
          <div className="space-y-1">
            <p className="font-medium">{response.partial_warning}</p>
            {response.enrichment_summary.rate_limited &&
              response.enrichment_summary.rate_limit_reset && (
                <p className="text-xs text-amber-300/80">
                  {t("trending.rateLimitReset")}{" "}
                  {new Date(
                    response.enrichment_summary.rate_limit_reset * 1000
                  ).toLocaleTimeString()}
                </p>
              )}
          </div>
        </div>
      )}

      {/* Short Source Result Notice */}
      {status === "success" && response?.is_short_source && (
        <div
          data-testid="trending-short-source-notice"
          className="rounded-md border border-slate-700 bg-slate-800/60 px-4 py-2.5 text-xs text-slate-300 flex items-center gap-2"
        >
          <span>ℹ️</span>
          <span>{t("trending.shortSourceNotice", { count: response.returned_count })}</span>
        </div>
      )}

      {/* State Renderings */}
      {status === "idle" && (
        <div
          data-testid="trending-idle-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/30 p-12 text-center"
        >
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-800 text-slate-400 mb-3 text-2xl">
            🔥
          </div>
          <h3 className="text-base font-medium text-slate-200">
            {t("trending.idleTitle")}
          </h3>
          <p className="text-slate-400 mt-2 text-sm max-w-md mx-auto">
            {t("trending.idleDesc")}
          </p>
        </div>
      )}

      {status === "loading" && (
        <div
          data-testid="trending-loading-state"
          className="rounded-lg border border-slate-700 bg-slate-800/40 p-12 text-center space-y-3"
        >
          <div className="inline-block animate-spin text-3xl text-indigo-400">
            ⟳
          </div>
          <p className="text-slate-300 text-sm font-medium">
            {t("trending.loadingTitle")}
          </p>
          <p className="text-slate-500 text-xs">
            {t("trending.loadingSubtext")}
          </p>
        </div>
      )}

      {status === "empty" && (
        <div
          data-testid="trending-empty-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/40 p-12 text-center"
        >
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-800 text-slate-400 mb-3 text-xl">
            🔍
          </div>
          <h3 className="text-base font-medium text-slate-200">{t("trending.emptyTitle")}</h3>
          <p className="text-slate-400 mt-1 text-sm">
            {t("trending.emptyDesc")}
          </p>
        </div>
      )}

      {status === "error" && !response && (
        <div
          data-testid="trending-error-state"
          className="rounded-lg border border-rose-500/40 bg-rose-950/30 p-6 text-center space-y-3"
        >
          <div className="text-rose-400 text-2xl">⚠️</div>
          <h3 className="text-base font-medium text-rose-200">{t("trending.errorTitle")}</h3>
          <p className="text-rose-300 text-sm max-w-md mx-auto">
            {errorMessage || t("trending.fetchError")}
          </p>
          <button
            type="button"
            onClick={handleFetch}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-900/60 hover:bg-rose-900 text-rose-200 border border-rose-700 text-xs font-medium transition-colors"
          >
            <span>{t("trending.retryBtn")}</span>
          </button>
        </div>
      )}

      {status === "error" && response && (
        <div
          data-testid="trending-error-banner"
          className="rounded-lg border border-rose-500/40 bg-rose-950/40 p-4 text-sm text-rose-200 flex items-start justify-between gap-3"
        >
          <div className="flex items-start gap-3">
            <span className="text-rose-400 text-lg shrink-0">⚠️</span>
            <div className="space-y-1">
              <p className="font-medium text-rose-200">{t("trending.errorBanner")}</p>
              <p className="text-xs text-rose-300">
                {errorMessage || t("trending.fetchError")}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={handleFetch}
            className="inline-flex items-center gap-1 px-3 py-1.5 rounded bg-rose-900/60 hover:bg-rose-900 text-rose-200 border border-rose-700 text-xs font-medium transition-colors shrink-0"
          >
            <span>{t("trending.retryBtn")}</span>
          </button>
        </div>
      )}

      {/* Action Banner */}
      {actionBanner && (
        <div
          data-testid="trending-action-banner"
          className={`p-3 rounded-md text-xs border ${
            actionBanner.type === "success"
              ? "bg-emerald-950/40 border-emerald-800 text-emerald-300"
              : "bg-rose-950/40 border-rose-800 text-rose-300"
          }`}
        >
          {actionBanner.message}
        </div>
      )}

      {(status === "success" || (status === "error" && response)) && response && (
        <div className="space-y-4">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 bg-slate-800/80 p-3 rounded-lg border border-slate-700 text-xs text-slate-300">
            <div className="flex items-center gap-4">
              <label className="inline-flex items-center gap-2 cursor-pointer font-medium">
                <input
                  type="checkbox"
                  data-testid="trending-select-all-checkbox"
                  checked={
                    response.items.length > 0 &&
                    response.items.every((r) => selectedUrls.has(r.github_url))
                  }
                  onChange={(e) => handleSelectAllCurrent(e.target.checked)}
                  className="h-3.5 w-3.5 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
                />
                <span>{t("trending.selectAllCurrent")}</span>
              </label>
              <span className="text-slate-500">|</span>
              <span className="text-slate-400">
                {t("trending.resultsCount", { count: response.returned_count })}
                {selectedUrls.size > 0 ? t("trending.selectedSuffix", { selected: selectedUrls.size }) : ""}
              </span>
            </div>

            <div className="flex items-center gap-3">
              <span className="text-[11px] text-slate-500">
                {t("trending.rankNotice")}
              </span>
              {translatingBatch ? (
                <button
                  type="button"
                  data-testid="trending-cancel-translate-btn"
                  onClick={handleCancelBatchTranslate}
                  className="px-3 py-1.5 rounded bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium transition-colors"
                >
                  {t("translation.batchCancelBtn")}
                </button>
              ) : (
                <button
                  type="button"
                  data-testid="trending-batch-translate-btn"
                  onClick={handleBatchTranslate}
                  disabled={response.items.length === 0 || !aiReadiness.ready || aiReadinessLoading}
                  title={
                    !aiReadiness.ready || aiReadinessLoading
                      ? aiReadiness.reason || t("translation.aiDisabled")
                      : undefined
                  }
                  className="px-3 py-1.5 rounded bg-slate-700 hover:bg-slate-600 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium transition-colors"
                >
                  🌐 {t("translation.batchTranslateBtn")}
                </button>
              )}
              <button
                type="button"
                data-testid="trending-favorite-selected-btn"
                disabled={selectedUrls.size === 0}
                onClick={handleBatchFavorite}
                className="px-3.5 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium transition-colors"
              >
                {t("trending.favoriteSelectedBtn", { count: selectedUrls.size })}
              </button>
            </div>
          </div>

          {translatingBatch && (
            <div
              data-testid="trending-batch-progress"
              className="bg-indigo-950/40 border border-indigo-800/60 rounded-lg p-3 text-xs text-indigo-200 flex items-center justify-between"
            >
              <div className="flex items-center gap-2">
                <span className="inline-block animate-spin">⟳</span>
                <span>
                  {t("translation.batchProgress", {
                    completed: batchProgress.completed,
                    total: batchProgress.total,
                  })}
                  {batchProgress.skipped > 0 && ` (${batchProgress.skipped} skipped)`}
                  {batchProgress.failed > 0 && ` (${batchProgress.failed} failed)`}
                </span>
              </div>
            </div>
          )}

          <div
            data-testid="trending-results-list"
            className="space-y-3"
          >
            {response.items.map((repo) => {
              const isFav = isFavorited(repo.github_repo_id, repo.github_url);
              const isSel = selectedUrls.has(repo.github_url);

              return (
                <RepositoryCard
                  key={`${repo.trending_rank}-${repo.github_url}`}
                  repo={repo}
                  trendingRank={repo.trending_rank}
                  periodStars={repo.period_stars}
                  periodLabel={getPeriodLabel(response.period)}
                  selectable={true}
                  isSelected={isSel}
                  onToggleSelect={(checked) =>
                    handleToggleSelectRow(repo.github_url, checked)
                  }
                  isFavorited={isFav}
                  onFavorite={() => handleSingleFavorite(repo)}
                  onAnalyze={() => handleOpenAnalysis(repo)}
                  translation={translations[repo.github_url]}
                />
              );
            })}
          </div>
        </div>
      )}

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
        title={t("trending.batchModalTitle")}
        favoriteResult={batchResult}
      />

      {/* Analysis Preview Modal */}
      <AnalysisPreviewModal
        isOpen={analysisPreviewTarget != null}
        repo={analysisPreviewTarget}
        onClose={() => setAnalysisPreviewTarget(null)}
        onFavoriteWithPreview={async (_repo, preview) => {
          await handleFavoriteWithPreview(preview);
        }}
      />

      {/* Favorite Detail Modal */}
      <FavoriteDetailModal
        isOpen={activeDetailFavorite != null}
        favorite={activeDetailFavorite}
        onClose={() => setActiveDetailFavorite(null)}
        onUpdated={(updated) => setActiveDetailFavorite(updated)}
        onDeleted={(deletedId) => {
          sessionCtx?.markUnfavorited(deletedId);
          setActiveDetailFavorite(null);
        }}
        autoTriggerAnalysis={activeDetailFavorite ? !activeDetailFavorite.ai_analysis : false}
      />
    </div>
  );
};
