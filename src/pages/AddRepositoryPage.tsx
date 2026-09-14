import React, { useEffect, useState } from "react";
import { DuplicateModal } from "../components/DuplicateModal";
import { RepositoryCard } from "../components/RepositoryCard";
import {
  favoriteRepository,
  getGitHubRepository,
  importRepositoriesFromArticle,
  refreshFavorite,
} from "../services/tauri";
import { useDiscoverySession } from "../context/DiscoverySessionContext";
import { GitHubRepository } from "../types/github";

export interface AddRepositoryPageProps {
  onNavigateToFavorite?: (favoriteId: number) => void;
}

function classifyUrl(raw: string): {
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
      errorMessage: "URL 不能为空",
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
      errorMessage: "URL 格式无效，请输入合法的 HTTP 或 HTTPS 地址",
    };
  }

  const scheme = parsed.protocol.toLowerCase();
  if (scheme !== "http:" && scheme !== "https:") {
    return {
      valid: false,
      isDirectGitHub: false,
      errorType: "UnsupportedScheme",
      errorMessage: `不支持的 URL 协议 '${parsed.protocol}'，仅支持 http:// 或 https://`,
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
  const {
    add,
    setAdd,
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
  } = add;

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

    const classification = classifyUrl(trimmed);
    if (!classification.valid) {
      setAdd((prev) => ({
        ...prev,
        loading: false,
        errorInfo: {
          type: classification.errorType || "InvalidUrl",
          message: classification.errorMessage || "URL 格式无效",
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
          userFacingMessage = `URL 格式无效: ${message}`;
        } else if (code === "GitHubNotFound") {
          userFacingMessage = "未找到该项目，请核对仓库地址是否拼写正确。";
        } else if (code === "PrivateRepositoryUnsupported") {
          userFacingMessage = "RepoShelf V1 仅支持公开开源项目，不支持私有仓库。";
        } else if (code === "GitHubAuth") {
          userFacingMessage = "GitHub 身份验证失败，请检查所配置的 Token 是否有效。";
        } else if (code === "GitHubRateLimit") {
          userFacingMessage = `GitHub API 请求频率已达上限: ${message}`;
        } else if (code === "GitHubTimeout") {
          userFacingMessage = "请求超时，请检查网络连接或稍后重试。";
        } else if (code === "GitHubNetwork") {
          userFacingMessage = `网络连接错误: ${message}`;
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
          userFacingMessage = `URL 格式无效: ${message}`;
        } else if (code === "UnsupportedScheme") {
          userFacingMessage = `URL 协议不支持: ${message}`;
        } else if (code === "BlockedAddress") {
          userFacingMessage = "目标地址已被安全策略拒绝访问（禁止访问本地或内网地址）。";
        } else if (code === "TooManyRedirects") {
          userFacingMessage = "文章页面重定向次数过多，已终止请求。";
        } else if (code === "Timeout") {
          userFacingMessage = "抓取文章页面超时，请检查网络连接或稍后重试。";
        } else if (code === "ResponseTooLarge") {
          userFacingMessage = "文章页面大小超过 2 MiB 预算限制。";
        } else if (code === "UnsupportedContentType") {
          userFacingMessage = "文章内容类型不受支持，仅支持标准 HTML 网页。";
        } else if (code === "NoGitHubRepositories") {
          userFacingMessage = "未在文章中发现有效的 GitHub 仓库链接。";
        } else if (code === "GitHubRateLimit") {
          userFacingMessage = `GitHub API 请求频率已达上限: ${message}`;
        } else if (code === "HttpError") {
          userFacingMessage = `获取文章失败: ${message}`;
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

  const handleFavoriteItem = async (repo: GitHubRepository) => {
    setAdd((prev) => ({ ...prev, actionBanner: null }));
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
      setAdd((prev) => ({
        ...prev,
        actionBanner: {
          type: "success",
          message: `成功将项目 ${repo.name} 收藏至本地库！`,
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
            message: `收藏失败: ${errObj?.message || String(err)}`,
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
            message: "已刷新该项目的最新元数据！",
          },
        }));
      } else {
        setAdd((prev) => ({
          ...prev,
          actionBanner: {
            type: "error",
            message: `刷新失败: ${res.message || res.status}`,
          },
        }));
      }
    } catch (err) {
      setAdd((prev) => ({
        ...prev,
        actionBanner: {
          type: "error",
          message: `刷新请求出错: ${String(err)}`,
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
          <span>➕</span> 添加项目
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            Add GitHub Repository
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          URL地址：输入GitHub地址或者包含GitHub链接的文章
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
          URL地址：输入GitHub地址或者包含GitHub链接的文章
        </label>
        <div className="flex gap-3">
          <input
            id="add-repo-url-input"
            data-testid="add-repo-url-input"
            type="text"
            placeholder="例如: https://github.com/tauri-apps/tauri 或 https://example.com/blog/article"
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
            {loading ? "获取中..." : "获取项目信息"}
          </button>
        </div>
        <p className="text-xs text-slate-500">
          支持直接输入 GitHub 仓库地址，或输入普通文章 URL 自动提取并验证其中的 GitHub 仓库链接。
        </p>
      </form>

      {/* Loading state */}
      {loading && (
        <div
          data-testid="add-repo-loading"
          className="rounded-lg border border-slate-700 bg-slate-800/50 p-10 text-center"
        >
          <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
          <p className="text-sm text-slate-300">正在获取项目元数据...</p>
        </div>
      )}

      {/* Error state */}
      {errorInfo && !loading && (
        <div
          data-testid="add-repo-error"
          className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-5 text-rose-300 space-y-1 text-sm"
        >
          <div className="font-semibold text-rose-200 flex items-center gap-1.5">
            <span>⚠️</span> 获取项目信息失败 ({errorInfo.type})
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
              <span className="font-medium text-slate-300">Repository Preview</span>
              <span className="text-slate-500">
                {directIsFavorited ? "已存入本地库" : "元数据预览（尚未存入本地库）"}
              </span>
            </div>
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
              {directIsFavorited ? "★ 已在本地库" : "⭐ 收藏至本地库"}
            </button>
          </div>

          <div data-testid="repo-preview-container">
            <RepositoryCard
              repo={previewRepo}
              isFavorited={directIsFavorited}
              onFavorite={() => handleFavoriteItem(previewRepo)}
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
                <span className="font-medium text-slate-200">文章提取结果</span>
              </div>
              <span className="text-xs text-slate-400 font-mono truncate max-w-md">
                {articleSummary.sourceUrl}
              </span>
            </div>

            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-300" data-testid="article-summary-counts">
              <span>
                找到 <strong className="text-indigo-400 font-mono">{articleSummary.extractedUniqueCount}</strong> 个 GitHub 仓库
              </span>
              <span>
                成功读取 <strong className="text-emerald-400 font-mono">{articleSummary.validCount}</strong> 个
              </span>
              {articleSummary.invalidCount > 0 && (
                <span>
                  ，<strong className="text-rose-400 font-mono">{articleSummary.invalidCount}</strong> 个无效或不可访问
                </span>
              )}
              {articleSummary.skippedCount > 0 && (
                <span>
                  ，<strong className="text-amber-400 font-mono">{articleSummary.skippedCount}</strong> 个已跳过
                </span>
              )}
            </div>

            {articleSummary.truncated && (
              <div className="text-xs text-amber-400 bg-amber-950/30 border border-amber-800/40 rounded px-2.5 py-1.5 mt-1" data-testid="article-truncated-warning">
                ⚠️ 页面中包含较多链接，已仅处理前 50 个唯一 GitHub 仓库链接。
              </div>
            )}

            {articleSummary.warning && !articleSummary.truncated && (
              <div className="text-xs text-amber-400 bg-amber-950/30 border border-amber-800/40 rounded px-2.5 py-1.5 mt-1" data-testid="article-general-warning">
                ⚠️ {articleSummary.warning}
              </div>
            )}
          </div>

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
                    />
                  </div>
                );
              })}
            </div>
          ) : (
            <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-8 text-center text-slate-400 text-sm">
              未在文章中提取到可正常访问的公开 GitHub 仓库。
            </div>
          )}
        </div>
      )}

      {/* Idle state */}
      {!previewRepo && (!articleResults || articleResults.length === 0) && !articleSummary && !errorInfo && !loading && (
        <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-10 text-center text-slate-400 space-y-2">
          <div className="text-3xl">📥</div>
          <h3 className="text-sm font-medium text-slate-300">等待输入项目地址</h3>
          <p className="text-xs text-slate-500 max-w-sm mx-auto">
            在上方输入框粘贴任何公开的 GitHub Repository 链接或包含 GitHub 仓库的文章网页链接，点击“获取项目信息”即可预览。
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
