import React, { useEffect, useRef, useState } from "react";
import { GitHubRepository } from "../types/github";
import { TrendingRepositoryItem } from "../types/trending";
import {
  AnalysisEnvelopeV1,
  AnalysisProgressStage,
} from "../types/library";
import { analyzeRepository, cancelRepositoryAnalysis } from "../services/tauri";
import { useDiscoverySession } from "../context/DiscoverySessionContext";
import { useI18n } from "../i18n/I18nContext";
import { RepositoryAnalysisReport } from "./RepositoryAnalysisReport";
import { getErrorMessage } from "../utils/error";

export interface AnalysisPreviewModalProps {
  isOpen: boolean;
  repo: GitHubRepository | TrendingRepositoryItem | null;
  onClose: () => void;
  onFavoriteWithPreview?: (
    repo: GitHubRepository | TrendingRepositoryItem,
    preview: AnalysisEnvelopeV1
  ) => Promise<void>;
  isFavorited?: boolean;
}

export const AnalysisPreviewModal: React.FC<AnalysisPreviewModalProps> = ({
  isOpen,
  repo,
  onClose,
  onFavoriteWithPreview,
  isFavorited = false,
}) => {
  const { t, locale } = useI18n();
  const {
    getSessionPreview,
    setSessionPreview,
    evictSessionPreview,
    setActivePreviewKey,
    aiReadiness,
    aiReadinessLoading,
  } = useDiscoverySession();

  const [preview, setPreview] = useState<AnalysisEnvelopeV1 | null>(null);
  const [stage, setStage] = useState<AnalysisProgressStage>("idle");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [favoriting, setFavoriting] = useState(false);

  // Generation counter to protect against stale/late responses
  const currentGenRef = useRef(0);
  const activeTaskIdRef = useRef<string | null>(null);

  const repoId = repo?.github_repo_id ?? null;
  const repoUrl = repo?.github_url ?? "";
  const repoName = repo?.name ?? "";

  const runAnalysis = async (generation: number) => {
    if (!repo) return;
    if (!aiReadiness.ready) {
      setErrorMessage(aiReadiness.reason || t("card.aiDisabledNotice"));
      setStage("error");
      return;
    }

    const taskId = `preview_${repoId || repoUrl}_gen${generation}_${Date.now()}`;
    activeTaskIdRef.current = taskId;

    setStage("reading_evidence");
    setErrorMessage(null);

    try {
      // Transition through real progress stages as acquisition proceeds
      const envelope = await analyzeRepository({
        repository: repoUrl || repoName,
        expected_repo_id: repoId ?? undefined,
        target_locale: locale as "en" | "zh-Hans" | "zh-Hant",
        client_session_id: taskId,
        generation,
      });

      // Discard response if generation has moved or modal closed
      if (currentGenRef.current !== generation) {
        return;
      }

      setPreview(envelope);
      setStage("generated_unsaved");
      try {
        setSessionPreview(envelope, repoId, repoUrl);
      } catch (cacheErr) {
        console.warn("Session preview cache failed:", cacheErr);
      }
    } catch (err) {
      if (currentGenRef.current !== generation) {
        return;
      }
      const msg = getErrorMessage(err);
      if (msg.includes("cancelled") || msg.includes("Cancelled")) {
        setStage("cancelled");
      } else {
        setErrorMessage(msg);
        setStage("error");
      }
    } finally {
      if (activeTaskIdRef.current === taskId) {
        activeTaskIdRef.current = null;
      }
    }
  };

  useEffect(() => {
    if (!isOpen || !repo) {
      // Modal closed: cancel any in-progress task
      if (activeTaskIdRef.current) {
        cancelRepositoryAnalysis(activeTaskIdRef.current).catch(() => {});
        activeTaskIdRef.current = null;
      }
      currentGenRef.current += 1;
      setPreview(null);
      setStage("idle");
      setErrorMessage(null);
      setActivePreviewKey(null);
      return;
    }

    // Modal opened: set active preview key to protect from eviction
    const previewKey = repoId ? `id:${repoId}` : `url:${repoUrl}`;
    setActivePreviewKey(previewKey);

    // Check session preview cache first
    const cached = getSessionPreview(repoId, repoUrl);
    if (cached) {
      setPreview(cached);
      setStage("generated_unsaved");
      setErrorMessage(null);
      return;
    }

    // Wait for AI readiness loading if in-flight
    if (aiReadinessLoading) {
      return;
    }

    // Not cached: start analysis
    currentGenRef.current += 1;
    const currentGen = currentGenRef.current;
    runAnalysis(currentGen);

    return () => {
      if (activeTaskIdRef.current) {
        cancelRepositoryAnalysis(activeTaskIdRef.current).catch(() => {});
        activeTaskIdRef.current = null;
      }
      setActivePreviewKey(null);
    };
  }, [isOpen, repoId, repoUrl, aiReadiness.ready, aiReadinessLoading]);

  // Handle ESC key to close
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        handleClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen]);

  const handleClose = () => {
    if (activeTaskIdRef.current) {
      cancelRepositoryAnalysis(activeTaskIdRef.current).catch(() => {});
      activeTaskIdRef.current = null;
    }
    currentGenRef.current += 1;
    onClose();
  };

  const handleReanalyze = () => {
    if (!repo) return;
    if (activeTaskIdRef.current) {
      cancelRepositoryAnalysis(activeTaskIdRef.current).catch(() => {});
      activeTaskIdRef.current = null;
    }
    evictSessionPreview(repoId, repoUrl);
    setPreview(null);
    currentGenRef.current += 1;
    runAnalysis(currentGenRef.current);
  };

  const handleCancel = () => {
    if (activeTaskIdRef.current) {
      cancelRepositoryAnalysis(activeTaskIdRef.current).catch(() => {});
      activeTaskIdRef.current = null;
    }
    setStage("cancelled");
  };

  const handleFavorite = async () => {
    if (!repo || !preview || favoriting) return;
    setFavoriting(true);
    try {
      if (onFavoriteWithPreview) {
        await onFavoriteWithPreview(repo, preview);
      }
    } finally {
      setFavoriting(false);
    }
  };

  if (!isOpen || !repo) return null;

  const isRunning = stage === "queued" || stage === "reading_evidence" || stage === "generating" || stage === "validating";

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="analysis-preview-title"
      data-testid="analysis-preview-modal"
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm overflow-y-auto"
      onClick={handleClose}
    >
      <div
        className="relative w-full max-w-4xl max-h-[90vh] flex flex-col rounded-xl border border-slate-700 bg-slate-900 shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4 shrink-0 bg-slate-900/90">
          <div className="flex items-center gap-3">
            <h3
              id="analysis-preview-title"
              className="text-lg font-semibold text-slate-100 flex items-center gap-2"
            >
              <span>{t("analysis.title")}</span>
              <span className="text-slate-400 text-sm font-normal">
                — {repo.name}
              </span>
            </h3>
            {preview && (
              <span
                data-testid="preview-unsaved-badge"
                className="px-2 py-0.5 rounded text-xs font-medium bg-amber-950 text-amber-300 border border-amber-700/60"
              >
                {t("analysis.unsavedBadge")}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleClose}
              data-testid="preview-close-btn"
              className="p-1 rounded-md text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
              aria-label={t("common.close")}
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Content body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* Progress / Loading State */}
          {isRunning && (
            <div
              data-testid="analysis-progress-indicator"
              className="rounded-lg border border-indigo-900/60 bg-indigo-950/30 p-6 text-center space-y-4"
            >
              <div className="inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-400" />
              <div className="space-y-1">
                <h4 className="text-sm font-medium text-indigo-200">
                  {stage === "reading_evidence" && t("analysis.stages.readingEvidence")}
                  {stage === "generating" && t("analysis.stages.generating")}
                  {stage === "validating" && t("analysis.stages.validating")}
                  {stage === "queued" && t("analysis.stages.queued")}
                </h4>
                <p className="text-xs text-slate-400">
                  {t("analysis.disclaimer")}
                </p>
              </div>

              <button
                type="button"
                onClick={handleCancel}
                data-testid="cancel-analysis-btn"
                className="inline-flex items-center px-3 py-1.5 rounded text-xs font-medium bg-slate-800 text-slate-300 hover:bg-slate-700 border border-slate-600"
              >
                {t("analysis.cancelBtn")}
              </button>
            </div>
          )}

          {/* Error / Cancelled State */}
          {(stage === "error" || stage === "cancelled") && (
            <div
              data-testid="analysis-error-state"
              className={`rounded-lg border p-4 text-xs space-y-3 ${
                stage === "cancelled"
                  ? "border-amber-900/60 bg-amber-950/20 text-amber-300"
                  : "border-rose-900/60 bg-rose-950/20 text-rose-300"
              }`}
            >
              <div className="font-semibold text-sm">
                {stage === "cancelled"
                  ? t("analysis.stages.cancelled")
                  : t("analysis.stages.error")}
              </div>
              {errorMessage && <p className="break-words">{errorMessage}</p>}
              <div>
                <button
                  type="button"
                  onClick={handleReanalyze}
                  data-testid="reanalyze-after-error-btn"
                  className="px-3 py-1.5 rounded text-xs font-medium bg-slate-800 text-slate-200 hover:bg-slate-700 border border-slate-600"
                >
                  {t("analysis.reanalyzeBtn")}
                </button>
              </div>
            </div>
          )}

          {/* Generated Report View */}
          {preview && (
            <RepositoryAnalysisReport envelope={preview} />
          )}
        </div>

        {/* Footer actions */}
        <div className="border-t border-slate-800 px-6 py-4 flex items-center justify-between shrink-0 bg-slate-900/90">
          <div className="flex items-center gap-2">
            {preview && !isRunning && (
              <button
                type="button"
                onClick={handleReanalyze}
                data-testid="reanalyze-btn"
                className="px-3 py-1.5 rounded text-xs font-medium bg-slate-800 text-slate-200 hover:bg-slate-700 border border-slate-600 transition-colors"
              >
                {t("analysis.reanalyzeBtn")}
              </button>
            )}
          </div>

          <div className="flex items-center gap-3">
            {preview && !isFavorited && onFavoriteWithPreview && (
              <button
                type="button"
                onClick={handleFavorite}
                disabled={favoriting}
                data-testid="preview-favorite-btn"
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-medium bg-indigo-600 text-white hover:bg-indigo-500 shadow-sm transition-colors"
              >
                <span>☆</span>
                <span>{favoriting ? t("common.saving") : t("card.favorite")}</span>
              </button>
            )}

            <button
              type="button"
              onClick={handleClose}
              data-testid="cancel-close-btn"
              className="px-4 py-2 rounded-lg text-xs font-medium bg-slate-800 text-slate-300 hover:bg-slate-700 border border-slate-700 transition-colors"
            >
              {t("common.close")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
