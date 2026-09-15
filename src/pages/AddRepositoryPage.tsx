import React, { useEffect, useRef, useState } from "react";
import { DuplicateModal } from "../components/DuplicateModal";
import { RepositoryCard } from "../components/RepositoryCard";
import {
  favoriteRepository,
  getGitHubRepository,
  importRepositoriesFromArticle,
  refreshFavorite,
  translateRepositoryDescription,
} from "../services/tauri";
import { useDiscoverySession } from "../context/DiscoverySessionContext";
import { GitHubRepository } from "../types/github";
import { useI18n } from "../i18n/I18nContext";

export interface AddRepositoryPageProps {
  onNavigateToFavorite?: (favoriteId: number) => void;
}

function classifyUrl(raw: string, t: (k: string, p?: Record<string, string | number>) => string): {
  valid: boolean;
  isDirectGitHub: boolean;
  errorType?: string;
  errorMessage?: string;
} {
  const trimmed = raw.trim();
  if (!trimmed) {
    return {
      valid: false,
      isDirectGitHub: false,
      errorType: "InvalidUrl",
      errorMessage: t("add.errors.urlEmpty"),
    };
  }

  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return {
      valid: false,
      isDirectGitHub: false,
      errorType: "InvalidUrl",
      errorMessage: t("add.errors.invalidUrl"),
    };
  }

  const scheme = parsed.protocol.toLowerCase();
  if (scheme !== "http:" && scheme !== "https:") {
    return {
      valid: false,
      isDirectGitHub: false,
      errorType: "UnsupportedScheme",
      errorMessage: t("add.errors.unsupportedScheme", { scheme: parsed.protocol }),
    };
  }

  const host = parsed.hostname.toLowerCase();
  if (host === "github.com" || host === "www.github.com") {
    return { valid: true, isDirectGitHub: true };
  }

  return { valid: true, isDirectGitHub: false };
}

export const AddRepositoryPage: React.FC<AddRepositoryPageProps> = ({
  onNavigateToFavorite,
}) => {
  const { t, locale } = useI18n();
  const {
    add,
    setAdd,
    updateAddTranslation,
    nextAddGeneration,
    isFavorited: checkIsFavorited,
    markFavorited,
    reconcileIdentities,
  } = useDiscoverySession();

  const {
    urlInput,
    loading,
    mode,
    previewRepo,
    articleResults,
    articleSummary,
    errorInfo,
    actionBanner,
    translations,
  } = add;

  const [translatingBatch, setTranslatingBatch] = useState(false);
  const [batchProgress, setBatchProgress] = useState({ total: 0, completed: 0, skipped: 0, failed: 0 });
  const cancelBatchRef = useRef(false);

  const setUrlInput = (u: string) => {
    setAdd((prev) => ({ ...prev, urlInput: u }));
  };

  const [duplicateModal, setDuplicateModal] = useState<{
    isOpen: boolean;
    repoName: string;
    favoriteId: number | null;
  }>({ isOpen: false, repoName: "", favoriteId: null });

  // Reconcile favorite identities on previewRepo or articleResults changes
  useEffect(() => {
    if (mode === "article" && articleResults && articleResults.length > 0) {
      reconcileIdentities(
        articleResults.map((r) => ({
          github_repo_id: r.github_repo_id,
          github_url: r.github_url,
        }))
      );
    } else if (previewRepo) {
      reconcileIdentities([
        {
          github_repo_id: previewRepo.github_repo_id,
          github_url: previewRepo.github_url,
        },
      ]);
    }
  }, [mode, articleResults, previewRepo, reconcileIdentities]);

  const handleFetchMetadata = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = urlInput.trim();
    if (!trimmed) return;

    const classification = classifyUrl(trimmed, t);
    if (!classification.valid) {
      setAdd((prev) => ({
        ...prev,
        loading: false,
        errorInfo: {
          type: classification.errorType || "InvalidUrl",
          message: classification.errorMessage || t("add.errors.invalidUrl"),
        },
      }));
      return;
    }

    const currentGen = nextAddGeneration();
    // Failed-refresh principle: keep previous successful results while fetching
    setAdd((prev) => ({
      ...prev,
      loading: true,
      errorInfo: null,
      actionBanner: null,
      generation: currentGen,
    }));

    if (classification.isDirectGitHub) {
      try {
        const repo = await getGitHubRepository(trimmed);
        setAdd((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            mode: "direct",
            previewRepo: repo,
            articleResults: [],
            articleSummary: null,
            loading: false,
            errorInfo: null,
            actionBanner: null,
          };
        });
      } catch (err: unknown) {
        const errObj = err as { code?: string; message?: string } | undefined;
        const code = errObj?.code || "Unknown";
        const message = errObj?.message || String(err);

        let userFacingMessage = message;
        if (code === "InvalidRepositoryUrl") {
          userFacingMessage = t("add.errors.invalidUrlArticle", { message });
        } else if (code === "GitHubNotFound") {
          userFacingMessage = t("add.errors.gitHubNotFound");
        } else if (code === "PrivateRepositoryUnsupported") {
          userFacingMessage = t("add.errors.gitHubPrivate");
        } else if (code === "GitHubAuth") {
          userFacingMessage = t("add.errors.gitHubAuth");
        } else if (code === "GitHubRateLimit") {
          userFacingMessage = t("add.errors.gitHubRateLimit", { message });
        } else if (code === "GitHubTimeout") {
          userFacingMessage = t("add.errors.gitHubTimeout");
        } else if (code === "GitHubNetwork") {
          userFacingMessage = t("add.errors.gitHubNetwork", { message });
        }

        setAdd((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            loading: false,
            errorInfo: {
              type: code,
              message: userFacingMessage,
            },
          };
        });
      }
    } else {
      // Article Import flow
      try {
        const result = await importRepositoriesFromArticle(trimmed);
        setAdd((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            mode: "article",
            previewRepo: null,
            articleResults: result.repositories,
            articleSummary: {
              sourceUrl: result.source_url,
              extractedUniqueCount: result.extracted_unique_count,
              validCount: result.valid_count,
              invalidCount: result.invalid_count,
              skippedCount: result.skipped_count,
              truncated: result.truncated,
              warning: result.warning,
            },
            loading: false,
            errorInfo: null,
            actionBanner: null,
          };
        });
      } catch (err: unknown) {
        const errObj = err as { code?: string; message?: string } | undefined;
        const code = errObj?.code || "Unknown";
        const message = errObj?.message || String(err);

        let userFacingMessage = message;
        if (code === "InvalidUrl") {
          userFacingMessage = t("add.errors.invalidUrlArticle", { message });
        } else if (code === "UnsupportedScheme") {
          userFacingMessage = t("add.errors.unsupportedSchemeArticle", { message });
        } else if (code === "BlockedAddress") {
          userFacingMessage = t("add.errors.blockedAddress");
        } else if (code === "TooManyRedirects") {
          userFacingMessage = t("add.errors.tooManyRedirects");
        } else if (code === "Timeout") {
          userFacingMessage = t("add.errors.timeout");
        } else if (code === "ResponseTooLarge") {
          userFacingMessage = t("add.errors.responseTooLarge");
        } else if (code === "UnsupportedContentType") {
          userFacingMessage = t("add.errors.unsupportedContentType");
        } else if (code === "NoGitHubRepositories") {
          userFacingMessage = t("add.errors.noGitHubRepositories");
        } else if (code === "GitHubRateLimit") {
          userFacingMessage = t("add.errors.gitHubRateLimit", { message });
        } else if (code === "HttpError") {
          userFacingMessage = t("add.errors.httpError", { message });
        }

        setAdd((prev) => {
          if (prev.generation !== currentGen) return prev;
          return {
            ...prev,
            loading: false,
            errorInfo: {
              type: code,
              message: userFacingMessage,
            },
          };
        });
      }
    }
  };

  const handleBatchTranslate = async () => {
    const targetList =
      mode === "direct" && previewRepo
        ? [previewRepo]
        : mode === "article"
        ? articleResults
        : [];
    if (targetList.length === 0 || translatingBatch) return;

    setTranslatingBatch(true);
    cancelBatchRef.current = false;
    const targetLocale = locale;
    const total = targetList.length;
    let completed = 0;
    let skipped = 0;
    let failed = 0;
    setBatchProgress({ total, completed, skipped, failed });

    for (const item of targetList) {
      if (cancelBatchRef.current) break;

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
        if (cancelBatchRef.current) break;

        if (res.action === "unchanged") {
          skipped++;
          updateAddTranslation(item.github_url, {
            text: "",
            target_locale: res.target_locale,
            action: "unchanged",
            status: "valid",
            source_hash: res.source_hash,
          });
        } else if (res.action === "translated" && res.translated_text) {
          completed++;
          updateAddTranslation(item.github_url, {
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
        if (cancelBatchRef.current) break;
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

  const handleFavoriteItem = async (repo: GitHubRepository) => {
    setAdd((prev) => ({ ...prev, actionBanner: null }));
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
        releases: repo.releases,
        languages: repo.languages,
        translation: translationInput,
      });
      markFavorited(repo.github_repo_id, repo.github_url);
      setAdd((prev) => ({
        ...prev,
        actionBanner: {
          type: "success",
          message: t("add.favoriteSuccess", { name: repo.name }),
        },
      }));
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
        setAdd((prev) => ({
          ...prev,
          actionBanner: {
            type: "error",
            message: t("add.favoriteFailed", { error: errObj?.message || String(err) }),
          },
        }));
      }
    }
  };

  const handleDuplicateRefresh = async (favoriteId: number) => {
    try {
      const res = await refreshFavorite(favoriteId);
      if (res.status === "succeeded") {
        setAdd((prev) => ({
          ...prev,
          actionBanner: {
            type: "success",
            message: t("add.refreshSuccess"),
          },
        }));
      } else {
        setAdd((prev) => ({
          ...prev,
          actionBanner: {
            type: "error",
            message: t("add.refreshFailed", { error: res.message || res.status }),
          },
        }));
      }
    } catch (err) {
      setAdd((prev) => ({
        ...prev,
        actionBanner: {
          type: "error",
          message: t("add.refreshRequestError", { error: String(err) }),
        },
      }));
    } finally {
      setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    }
  };

  const handleDuplicateOpenRecord = (favoriteId: number) => {
    setDuplicateModal({ isOpen: false, repoName: "", favoriteId: null });
    onNavigateToFavorite?.(favoriteId);
  };

  const directIsFavorited = previewRepo
    ? checkIsFavorited(previewRepo.github_repo_id, previewRepo.github_url)
    : false;

  const hasArticleResults = mode === "article" && articleResults && articleResults.length > 0;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>➕</span> {t("add.title")}
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            Add GitHub Repository
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          {t("add.subtitle")}
        </p>
      </div>

      {/* URL Input Form */}
      <form
        onSubmit={handleFetchMetadata}
        className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-3"
      >
        <label
          htmlFor="add-repo-url-input"
          className="block text-xs font-medium text-slate-300"
        >
          {t("add.inputLabel")}
        </label>
        <div className="flex gap-3">
          <input
            id="add-repo-url-input"
            data-testid="add-repo-url-input"
            type="text"
            placeholder={t("add.placeholder")}
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3.5 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 font-mono"
          />
          <button
            type="submit"
            data-testid="add-repo-fetch-btn"
            disabled={loading || !urlInput.trim()}
            className="px-5 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium rounded-md transition-colors shrink-0"
          >
            {loading ? t("add.fetchingBtn") : t("add.fetchBtn")}
          </button>
        </div>
        <p className="text-xs text-slate-500">
          {t("add.hint")}
        </p>
      </form>

      {/* Loading state */}
      {loading && (
        <div
          data-testid="add-repo-loading"
          className="rounded-lg border border-slate-700 bg-slate-800/50 p-10 text-center"
        >
          <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
          <p className="text-sm text-slate-300">{t("add.loadingText")}</p>
        </div>
      )}

      {/* Error state */}
      {errorInfo && !loading && (
        <div
          data-testid="add-repo-error"
          className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-5 text-rose-300 space-y-1 text-sm"
        >
          <div className="font-semibold text-rose-200 flex items-center gap-1.5">
            <span>⚠️</span> {t("add.fetchFailed", { type: errorInfo.type })}
          </div>
          <p className="text-xs text-rose-300/90">{errorInfo.message}</p>
        </div>
      )}

      {/* Action Banner */}
      {actionBanner && (
        <div
          data-testid="add-repo-action-banner"
          className={`p-3 rounded-md text-xs border ${
            actionBanner.type === "success"
              ? "bg-emerald-950/40 border-emerald-800 text-emerald-300"
              : "bg-rose-950/40 border-rose-800 text-rose-300"
          }`}
        >
          {actionBanner.message}
        </div>
      )}

      {/* Direct GitHub Preview Card */}
      {mode === "direct" && previewRepo && !loading && (
        <div className="space-y-3">
          <div className="flex items-center justify-between text-xs text-slate-400 px-1">
            <div className="flex items-center gap-2">
              <span className="font-medium text-slate-300">{t("add.directPreviewTitle")}</span>
              <span className="text-slate-500">
                {directIsFavorited ? t("add.directInLibrary") : t("add.directNotSaved")}
              </span>
            </div>
            <div className="flex items-center gap-2">
              {translatingBatch ? (
                <button
                  type="button"
                  data-testid="add-cancel-translate-btn"
                  onClick={handleCancelBatchTranslate}
                  className="px-3 py-1.5 rounded bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium transition-colors"
                >
                  {t("translation.batchCancelBtn")}
                </button>
              ) : (
                <button
                  type="button"
                  data-testid="add-batch-translate-btn"
                  onClick={handleBatchTranslate}
                  className="px-3 py-1.5 rounded bg-slate-700 hover:bg-slate-600 text-slate-200 text-xs font-medium transition-colors"
                >
                  🌐 {t("translation.batchTranslateBtn")}
                </button>
              )}
              <button
                type="button"
                data-testid="add-repo-favorite-btn"
                onClick={() => handleFavoriteItem(previewRepo)}
                className={`px-3 py-1.5 rounded text-white text-xs font-medium transition-colors ${
                  directIsFavorited
                    ? "bg-amber-600 hover:bg-amber-500"
                    : "bg-indigo-600 hover:bg-indigo-500"
                }`}
              >
                {directIsFavorited ? t("add.savedToLibrary") : t("add.saveToLibrary")}
              </button>
            </div>
          </div>

          {translatingBatch && (
            <div
              data-testid="add-batch-progress"
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

          <div data-testid="repo-preview-container">
            <RepositoryCard
              repo={previewRepo}
              isFavorited={directIsFavorited}
              onFavorite={() => handleFavoriteItem(previewRepo)}
              translation={translations[previewRepo.github_url]}
            />
          </div>
        </div>
      )}

      {/* Article Import Results */}
      {mode === "article" && articleSummary && !loading && (
        <div className="space-y-4" data-testid="article-results-section">
          {/* Summary Banner */}
          <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-4 space-y-2 text-sm text-slate-300">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-700/60 pb-2">
              <div className="flex items-center gap-2">
                <span className="text-base">📄</span>
                <span className="font-medium text-slate-200">{t("add.articleResultsTitle")}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-xs text-slate-400 font-mono truncate max-w-md">
                  {articleSummary.sourceUrl}
                </span>
                {translatingBatch ? (
                  <button
                    type="button"
                    data-testid="add-cancel-translate-btn"
                    onClick={handleCancelBatchTranslate}
                    className="px-2.5 py-1 rounded bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium transition-colors"
                  >
                    {t("translation.batchCancelBtn")}
                  </button>
                ) : (
                  <button
                    type="button"
                    data-testid="add-batch-translate-btn"
                    onClick={handleBatchTranslate}
                    disabled={articleResults.length === 0}
                    className="px-2.5 py-1 rounded bg-slate-700 hover:bg-slate-600 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium transition-colors"
                  >
                    🌐 {t("translation.batchTranslateBtn")}
                  </button>
                )}
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-300" data-testid="article-summary-counts">
              <span>
                {t("add.foundRepos", { count: articleSummary.extractedUniqueCount })}
              </span>
              <span>
                {t("add.successfullyRead", { count: articleSummary.validCount })}
              </span>
              {articleSummary.invalidCount > 0 && (
                <span>
                  {t("add.invalidOrInaccessible", { count: articleSummary.invalidCount })}
                </span>
              )}
              {articleSummary.skippedCount > 0 && (
                <span>
                  {t("add.skipped", { count: articleSummary.skippedCount })}
                </span>
              )}
            </div>

            {articleSummary.truncated && (
              <div className="text-xs text-amber-400 bg-amber-950/30 border border-amber-800/40 rounded px-2.5 py-1.5 mt-1" data-testid="article-truncated-warning">
                {t("add.truncatedWarning")}
              </div>
            )}

            {articleSummary.warning && !articleSummary.truncated && (
              <div className="text-xs text-amber-400 bg-amber-950/30 border border-amber-800/40 rounded px-2.5 py-1.5 mt-1" data-testid="article-general-warning">
                ⚠️ {articleSummary.warning}
              </div>
            )}
          </div>

          {translatingBatch && (
            <div
              data-testid="add-batch-progress"
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

          {/* Results List */}
          {hasArticleResults ? (
            <div className="space-y-4" data-testid="article-results-container">
              {articleResults.map((repo) => {
                const itemFavorited = checkIsFavorited(
                  repo.github_repo_id,
                  repo.github_url
                );
                return (
                  <div
                    key={repo.github_repo_id || repo.github_url}
                    data-testid={`article-repo-card-${repo.github_repo_id}`}
                  >
                    <RepositoryCard
                      repo={repo}
                      isFavorited={itemFavorited}
                      onFavorite={() => handleFavoriteItem(repo)}
                      translation={translations[repo.github_url]}
                    />
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-8 text-center text-slate-400 text-sm">
              {t("add.articleEmpty")}
            </div>
          )}
        </div>
      )}

      {/* Idle state */}
      {!previewRepo && (!articleResults || articleResults.length === 0) && !articleSummary && !errorInfo && !loading && (
        <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-10 text-center text-slate-400 space-y-2">
          <div className="text-3xl">📥</div>
          <h3 className="text-sm font-medium text-slate-300">{t("add.idleTitle")}</h3>
          <p className="text-xs text-slate-500 max-w-sm mx-auto">
            {t("add.idleDesc")}
          </p>
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
    </div>
  );
};
