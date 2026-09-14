import React, { useEffect, useState } from "react";
import { DuplicateModal } from "../components/DuplicateModal";
import { RepositoryCard } from "../components/RepositoryCard";
import {
  favoriteRepository,
  getGitHubRepository,
  refreshFavorite,
} from "../services/tauri";
import { useDiscoverySession } from "../context/DiscoverySessionContext";


export interface AddRepositoryPageProps {
  onNavigateToFavorite?: (favoriteId: number) => void;
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
    previewRepo,
    errorInfo,
    actionBanner,
  } = add;

  const setUrlInput = (u: string) => {
    setAdd((prev) => ({ ...prev, urlInput: u }));
  };

  const isFavorited = previewRepo
    ? checkIsFavorited(previewRepo.github_repo_id, previewRepo.github_url)
    : false;

  const [duplicateModal, setDuplicateModal] = useState<{
    isOpen: boolean;
    repoName: string;
    favoriteId: number | null;
  }>({ isOpen: false, repoName: "", favoriteId: null });

  // Reconcile favorite identity on preview repo change
  useEffect(() => {
    if (previewRepo) {
      reconcileIdentities([
        {
          github_repo_id: previewRepo.github_repo_id,
          github_url: previewRepo.github_url,
        },
      ]);
    }
  }, [previewRepo, reconcileIdentities]);

  const handleFetchMetadata = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = urlInput.trim();
    if (!trimmed) return;

    const currentGen = nextAddGeneration();
    setAdd((prev) => ({
      ...prev,
      loading: true,
      errorInfo: null,
      previewRepo: null,
      actionBanner: null,
      generation: currentGen,
    }));

    try {
      const repo = await getGitHubRepository(trimmed);
      setAdd((prev) => {
        if (prev.generation !== currentGen) return prev;
        return {
          ...prev,
          previewRepo: repo,
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
  };

  const handleFavoritePreview = async () => {
    if (!previewRepo) return;
    setAdd((prev) => ({ ...prev, actionBanner: null }));
    try {
      await favoriteRepository({
        github_repo_id: previewRepo.github_repo_id,
        github_url: previewRepo.github_url,
        name: previewRepo.name,
        description: previewRepo.description,
        category: previewRepo.category,
        stars: Number(previewRepo.stars),
        license: previewRepo.license,
        topics: previewRepo.topics,
        pushed_at: previewRepo.pushed_at,
        releases: previewRepo.releases,
        languages: previewRepo.languages,
      });
      markFavorited(previewRepo.github_repo_id, previewRepo.github_url);
      setAdd((prev) => ({
        ...prev,
        actionBanner: {
          type: "success",
          message: `成功将项目 ${previewRepo.name} 收藏至本地库！`,
        },
      }));
    } catch (err: unknown) {
      const errObj = err as
        | { code?: string; favorite_id?: number; message?: string }
        | undefined;
      if (errObj?.code === "FavoriteAlreadyExists") {
        markFavorited(previewRepo.github_repo_id, previewRepo.github_url);
        setDuplicateModal({
          isOpen: true,
          repoName: previewRepo.name,
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
        if (previewRepo) {
          markFavorited(previewRepo.github_repo_id, previewRepo.github_url);
        }
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
          输入公开 GitHub 项目地址，提取元数据并预览项目详情。
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
          GitHub Repository URL
        </label>
        <div className="flex gap-3">
          <input
            id="add-repo-url-input"
            data-testid="add-repo-url-input"
            type="text"
            placeholder="例如: https://github.com/tauri-apps/tauri"
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
          支持标准 GitHub URL（如 https://github.com/owner/repo），自动规范化处理。
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

      {/* Preview Card */}
      {previewRepo && !loading && (
        <div className="space-y-3">
          <div className="flex items-center justify-between text-xs text-slate-400 px-1">
            <div className="flex items-center gap-2">
              <span className="font-medium text-slate-300">Repository Preview</span>
              <span className="text-slate-500">
                {isFavorited ? "已存入本地库" : "元数据预览（尚未存入本地库）"}
              </span>
            </div>
            <button
              type="button"
              data-testid="add-repo-favorite-btn"
              onClick={handleFavoritePreview}
              className={`px-3 py-1.5 rounded text-white text-xs font-medium transition-colors ${
                isFavorited
                  ? "bg-amber-600 hover:bg-amber-500"
                  : "bg-indigo-600 hover:bg-indigo-500"
              }`}
            >
              {isFavorited ? "★ 已在本地库" : "⭐ 收藏至本地库"}
            </button>
          </div>

          <div data-testid="repo-preview-container">
            <RepositoryCard
              repo={previewRepo}
              isFavorited={isFavorited}
              onFavorite={handleFavoritePreview}
            />
          </div>
        </div>
      )}

      {/* Idle state */}
      {!previewRepo && !errorInfo && !loading && (
        <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-10 text-center text-slate-400 space-y-2">
          <div className="text-3xl">📥</div>
          <h3 className="text-sm font-medium text-slate-300">等待输入项目地址</h3>
          <p className="text-xs text-slate-500 max-w-sm mx-auto">
            在上方输入框粘贴任何公开的 GitHub Repository 链接，点击“获取项目信息”即可预览。
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

