import React, { useEffect, useState } from "react";
import { ALL_CATEGORIES, FavoriteItem } from "../types/library";
import {
  deleteFavorite,
  openGitHubUrl,
  refreshFavorite,
  updateFavoriteUserMetadata,
} from "../services/tauri";

export interface FavoriteDetailModalProps {
  isOpen: boolean;
  favorite: FavoriteItem | null;
  onClose: () => void;
  onUpdated: (updated: FavoriteItem) => void;
  onDeleted: (deletedId: number) => void;
}

export const FavoriteDetailModal: React.FC<FavoriteDetailModalProps> = ({
  isOpen,
  favorite,
  onClose,
  onUpdated,
  onDeleted,
}) => {
  const [currentFav, setCurrentFav] = useState<FavoriteItem | null>(favorite);
  const [category, setCategory] = useState<string>("Other");
  const [rating, setRating] = useState<number | null>(null);
  const [notes, setNotes] = useState<string>("");
  const [userTags, setUserTags] = useState<string[]>([]);
  const [newTagInput, setNewTagInput] = useState<string>("");

  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [feedbackMessage, setFeedbackMessage] = useState<{
    type: "success" | "error" | "info";
    text: string;
  } | null>(null);

  useEffect(() => {
    if (favorite) {
      const isNewFav = !currentFav || currentFav.id !== favorite.id;
      setCurrentFav(favorite);
      setCategory(favorite.category);
      setRating(favorite.rating ?? null);
      setNotes(favorite.notes ?? "");
      const uTags = favorite.tags
        .filter((t) => t.source === "user")
        .map((t) => t.tag);
      setUserTags(uTags);
      setNewTagInput("");
      setShowDeleteConfirm(false);
      if (isNewFav) {
        setFeedbackMessage(null);
      }
    }
  }, [favorite]);

  if (!isOpen || !currentFav) return null;

  const handleOpenGitHub = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      await openGitHubUrl(currentFav.github_url);
    } catch (err) {
      console.error("Failed to open GitHub URL:", err);
    }
  };

  const handleAddUserTag = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = newTagInput.trim();
    if (!trimmed) return;
    if (!userTags.some((t) => t.toLowerCase() === trimmed.toLowerCase())) {
      setUserTags([...userTags, trimmed]);
    }
    setNewTagInput("");
  };

  const handleRemoveUserTag = (tagToRemove: string) => {
    setUserTags(userTags.filter((t) => t !== tagToRemove));
  };

  const handleSave = async () => {
    setSaving(true);
    setFeedbackMessage(null);
    try {
      const updated = await updateFavoriteUserMetadata({
        favorite_id: currentFav.id,
        category,
        rating,
        notes,
        user_tags: userTags,
      });
      setCurrentFav(updated);
      onUpdated(updated);
      setFeedbackMessage({
        type: "success",
        text: "收藏信息已保存成功！",
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setFeedbackMessage({
        type: "error",
        text: `保存失败: ${msg}`,
      });
    } finally {
      setSaving(false);
    }
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    setFeedbackMessage(null);
    try {
      const res = await refreshFavorite(currentFav.id);
      if (res.status === "succeeded" && res.favorite) {
        setCurrentFav(res.favorite);
        onUpdated(res.favorite);
        setFeedbackMessage({
          type: "success",
          text: "GitHub 元数据刷新成功，用户标签、评级与备注已保留。",
        });
      } else if (res.status === "not_found") {
        setFeedbackMessage({
          type: "error",
          text: "GitHub 提示该仓库不存在或已被删除（404 Not Found），本地收藏已完整保留。",
        });
      } else if (res.status === "rate_limited") {
        setFeedbackMessage({
          type: "error",
          text: "GitHub API 请求频率已达上限，刷新已跳过。",
        });
      } else if (res.status === "conflict") {
        setFeedbackMessage({
          type: "error",
          text: `刷新冲突: ${res.message || "仓库标识与已有其他收藏冲突"}`,
        });
      } else {
        setFeedbackMessage({
          type: "error",
          text: `刷新失败: ${res.message || "未知错误"}`,
        });
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setFeedbackMessage({
        type: "error",
        text: `刷新请求失败: ${msg}`,
      });
    } finally {
      setRefreshing(false);
    }
  };

  const handleDelete = async () => {
    setDeleting(true);
    try {
      await deleteFavorite(currentFav.id);
      onDeleted(currentFav.id);
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setFeedbackMessage({
        type: "error",
        text: `删除失败: ${msg}`,
      });
      setDeleting(false);
      setShowDeleteConfirm(false);
    }
  };

  const formattedFavoritedAt = currentFav.favorited_at
    ? new Date(currentFav.favorited_at).toLocaleString()
    : "Unknown";

  const formattedRefreshedAt = currentFav.metadata_refreshed_at
    ? new Date(currentFav.metadata_refreshed_at).toLocaleString()
    : "Unknown";

  const formattedPushedAt = currentFav.pushed_at
    ? new Date(currentFav.pushed_at).toLocaleDateString()
    : "Unknown";

  const githubTags = currentFav.tags.filter((t) => t.source === "github");
  const aiTags = currentFav.tags.filter((t) => t.source === "ai");

  return (
    <div
      data-testid="favorite-detail-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm overflow-y-auto"
    >
      <div className="w-full max-w-2xl rounded-xl border border-slate-700 bg-slate-900 p-6 shadow-2xl space-y-5 my-8 max-h-[90vh] overflow-y-auto">
        {/* Header */}
        <div className="flex items-start justify-between gap-4 border-b border-slate-800 pb-4">
          <div>
            <div className="flex items-center gap-2.5 flex-wrap">
              <h2
                data-testid="detail-repo-name"
                className="text-xl font-bold text-slate-100"
              >
                {currentFav.name}
              </h2>
              <span className="px-2.5 py-0.5 rounded-full text-xs font-medium bg-indigo-950/80 text-indigo-300 border border-indigo-700/60">
                {currentFav.category}
              </span>
              <span className="flex items-center gap-1 text-xs text-amber-300 font-medium px-2 py-0.5 bg-slate-800 rounded border border-slate-700">
                ★ {currentFav.stars.toLocaleString()}
              </span>
            </div>
            <a
              href={currentFav.github_url}
              onClick={handleOpenGitHub}
              data-testid="detail-github-url-link"
              className="text-xs text-indigo-400 hover:text-indigo-300 hover:underline flex items-center gap-1 mt-1 font-mono"
            >
              <span>{currentFav.github_url}</span>
              <span>↗</span>
            </a>
          </div>

          <button
            type="button"
            data-testid="detail-close-btn"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 text-lg p-1 rounded-md hover:bg-slate-800 transition-colors"
          >
            ✕
          </button>
        </div>

        {/* Feedback Banner */}
        {feedbackMessage && (
          <div
            data-testid="detail-feedback-banner"
            className={`p-3 rounded-md text-xs border ${
              feedbackMessage.type === "success"
                ? "bg-emerald-950/40 border-emerald-800 text-emerald-300"
                : feedbackMessage.type === "error"
                ? "bg-rose-950/40 border-rose-800 text-rose-300"
                : "bg-slate-800 border-slate-700 text-slate-300"
            }`}
          >
            {feedbackMessage.text}
          </div>
        )}

        {/* Description & Readonly Metadata */}
        <div className="space-y-2">
          <p className="text-sm text-slate-300 bg-slate-800/40 p-3 rounded-md border border-slate-800">
            {currentFav.description || "暂无项目简介"}
          </p>

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs text-slate-400 pt-1">
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">License</span>
              <span className="font-medium text-slate-200">
                {currentFav.license || "Unknown"}
              </span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">最后活跃</span>
              <span className="font-medium text-slate-200">{formattedPushedAt}</span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">收藏时间</span>
              <span className="font-medium text-slate-200 truncate block" title={formattedFavoritedAt}>
                {formattedFavoritedAt}
              </span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">元数据刷新</span>
              <span className="font-medium text-slate-200 truncate block" title={formattedRefreshedAt}>
                {formattedRefreshedAt}
              </span>
            </div>
          </div>
        </div>

        {/* GitHub & AI Tags (Read-only tags) */}
        {(githubTags.length > 0 || aiTags.length > 0) && (
          <div className="space-y-1.5 pt-1">
            <div className="text-xs font-medium text-slate-400">
              GitHub Topics & 元数据标签 (只读)
            </div>
            <div className="flex flex-wrap gap-1.5">
              {githubTags.map((t) => (
                <span
                  key={t.id}
                  className="px-2 py-0.5 rounded text-xs bg-slate-800 text-slate-300 border border-slate-700"
                >
                  #{t.tag}
                  <span className="ml-1 text-[10px] text-slate-500 font-mono">
                    (github)
                  </span>
                </span>
              ))}
              {aiTags.map((t) => (
                <span
                  key={t.id}
                  className="px-2 py-0.5 rounded text-xs bg-purple-950/60 text-purple-300 border border-purple-800/60"
                >
                  #{t.tag}
                  <span className="ml-1 text-[10px] text-purple-400 font-mono">
                    (ai)
                  </span>
                </span>
              ))}
            </div>
          </div>
        )}

        <hr className="border-slate-800" />

        {/* Editable Section: Category, Rating, Notes, User Tags */}
        <div className="space-y-4">
          <h4 className="text-xs font-bold uppercase tracking-wider text-indigo-400">
            用户本地属性管理 (可编辑)
          </h4>

          {/* Category Edit */}
          <div>
            <label
              htmlFor="detail-category-select"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              项目分类 (Category)
            </label>
            <select
              id="detail-category-select"
              data-testid="detail-category-select"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-800 px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              {ALL_CATEGORIES.map((cat) => (
                <option key={cat} value={cat}>
                  {cat}
                </option>
              ))}
            </select>
          </div>

          {/* Rating Edit */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-xs font-medium text-slate-300">
                评级 (Rating)
              </label>
              {rating != null && (
                <button
                  type="button"
                  data-testid="detail-clear-rating-btn"
                  onClick={() => setRating(null)}
                  className="text-[11px] text-rose-400 hover:text-rose-300"
                >
                  清除评级
                </button>
              )}
            </div>
            <div
              data-testid="detail-rating-stars"
              className="flex items-center gap-2"
            >
              {[1, 2, 3, 4, 5].map((star) => (
                <button
                  key={star}
                  type="button"
                  data-testid={`detail-star-${star}`}
                  onClick={() => setRating(star)}
                  className={`text-xl transition-transform hover:scale-110 ${
                    rating != null && star <= rating
                      ? "text-amber-400"
                      : "text-slate-600 hover:text-amber-500/50"
                  }`}
                >
                  ★
                </button>
              ))}
              <span className="text-xs text-slate-400 ml-2">
                {rating != null ? `${rating} 星` : "未评级"}
              </span>
            </div>
          </div>

          {/* User Tags Edit */}
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1">
              用户标签 (User Tags)
            </label>
            <div
              data-testid="detail-user-tags-container"
              className="flex flex-wrap gap-1.5 mb-2"
            >
              {userTags.map((tag) => (
                <span
                  key={tag}
                  data-testid={`user-tag-${tag}`}
                  className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded text-xs bg-indigo-950/80 text-indigo-300 border border-indigo-700/60"
                >
                  #{tag}
                  <button
                    type="button"
                    data-testid={`remove-tag-${tag}`}
                    onClick={() => handleRemoveUserTag(tag)}
                    className="text-indigo-400 hover:text-indigo-200 font-bold ml-0.5"
                  >
                    ✕
                  </button>
                </span>
              ))}
              {userTags.length === 0 && (
                <span className="text-xs text-slate-500 italic">
                  暂无用户自定义标签
                </span>
              )}
            </div>

            <form onSubmit={handleAddUserTag} className="flex gap-2">
              <input
                type="text"
                data-testid="detail-add-tag-input"
                placeholder="输入标签名称..."
                value={newTagInput}
                onChange={(e) => setNewTagInput(e.target.value)}
                className="flex-1 rounded-md border border-slate-700 bg-slate-800 px-3 py-1 text-xs text-slate-100 focus:outline-none focus:border-indigo-500"
              />
              <button
                type="submit"
                data-testid="detail-add-tag-btn"
                disabled={!newTagInput.trim()}
                className="px-3 py-1 bg-slate-700 hover:bg-slate-600 disabled:opacity-50 text-slate-200 text-xs rounded-md transition-colors"
              >
                + 添加标签
              </button>
            </form>
          </div>

          {/* Notes Edit */}
          <div>
            <label
              htmlFor="detail-notes-textarea"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              个人备注 (Notes)
            </label>
            <textarea
              id="detail-notes-textarea"
              data-testid="detail-notes-textarea"
              rows={4}
              placeholder="记录你的研究心得、使用评价或参考要点（自由文本）..."
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-800 p-2.5 text-xs text-slate-100 focus:outline-none focus:border-indigo-500"
            />
          </div>
        </div>

        {/* Delete Confirmation Warning (if active) */}
        {showDeleteConfirm && (
          <div
            data-testid="delete-confirmation-box"
            className="rounded-lg border border-rose-800/80 bg-rose-950/40 p-4 space-y-3"
          >
            <div className="flex items-center gap-2 text-rose-300 font-semibold text-sm">
              <span>⚠️</span> 确定要删除该收藏记录吗？
            </div>
            <p className="text-xs text-rose-300/90 leading-relaxed">
              只会从 RepoShelf 本地收藏库删除，不会修改 GitHub 仓库。
            </p>
            <div className="flex gap-2 justify-end pt-1">
              <button
                type="button"
                data-testid="confirm-delete-btn"
                disabled={deleting}
                onClick={handleDelete}
                className="px-4 py-1.5 bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium rounded-md shadow-sm transition-colors"
              >
                {deleting ? "删除中..." : "确认删除"}
              </button>
              <button
                type="button"
                data-testid="cancel-delete-btn"
                onClick={() => setShowDeleteConfirm(false)}
                className="px-4 py-1.5 border border-slate-700 bg-slate-800 text-slate-300 text-xs rounded-md hover:bg-slate-700 transition-colors"
              >
                取消
              </button>
            </div>
          </div>
        )}

        {/* Footer Actions */}
        <div className="flex items-center justify-between border-t border-slate-800 pt-4">
          <div>
            {!showDeleteConfirm && (
              <button
                type="button"
                data-testid="detail-delete-btn"
                onClick={() => setShowDeleteConfirm(true)}
                className="text-xs text-rose-400 hover:text-rose-300 transition-colors"
              >
                删除收藏
              </button>
            )}
          </div>

          <div className="flex items-center gap-3">
            <button
              type="button"
              data-testid="detail-refresh-btn"
              disabled={refreshing || saving}
              onClick={handleRefresh}
              className="px-3.5 py-2 border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5"
            >
              <span>{refreshing ? "刷新中..." : "刷新元数据"}</span>
              <span>🔄</span>
            </button>

            <button
              type="button"
              data-testid="detail-save-btn"
              disabled={saving || refreshing}
              onClick={handleSave}
              className="px-5 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium rounded-md shadow-sm transition-colors"
            >
              {saving ? "保存中..." : "保存修改"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
