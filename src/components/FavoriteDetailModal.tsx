import React, { useEffect, useRef, useState } from "react";
import {
  ALL_CATEGORIES,
  AiFavoriteMetadataSuggestion,
  FavoriteItem,
} from "../types/library";
import { AiSearchReadiness } from "../types/github";
import {
  deleteFavorite,
  getAiSearchReadiness,
  openGitHubUrl,
  refreshFavorite,
  suggestFavoriteAiMetadata,
  updateFavoriteUserMetadata,
} from "../services/tauri";
import { getErrorMessage } from "../utils/error";
import { filterSupplementalAiTags } from "../utils/tagUtils";
import { LanguagesBar } from "./LanguagesBar";
import { useI18n } from "../i18n/I18nContext";

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
  const { t } = useI18n();
  const [currentFav, setCurrentFav] = useState<FavoriteItem | null>(favorite);
  const [category, setCategory] = useState<string>("Other");
  const [rating, setRating] = useState<number | null>(null);
  const [ratingReason, setRatingReason] = useState<string | null>(null);
  const [ratingConfidence, setRatingConfidence] = useState<
    "low" | "medium" | "high" | null
  >(null);
  const [notes, setNotes] = useState<string>("");
  const [userTags, setUserTags] = useState<string[]>([]);
  const [aiTags, setAiTags] = useState<string[]>([]);
  const [newTagInput, setNewTagInput] = useState<string>("");

  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [feedbackMessage, setFeedbackMessage] = useState<{
    type: "success" | "error" | "info";
    text: string;
  } | null>(null);

  // AI Suggestion states
  const [aiReadiness, setAiReadiness] = useState<AiSearchReadiness | null>(null);
  const [aiLoading, setAiLoading] = useState(false);
  const [aiApplying, setAiApplying] = useState(false);
  const [suggestion, setSuggestion] = useState<AiFavoriteMetadataSuggestion | null>(null);

  useEffect(() => {
    let isMounted = true;
    if (isOpen) {
      getAiSearchReadiness()
        .then((res) => {
          if (isMounted) setAiReadiness(res);
        })
        .catch((err) => {
          console.warn("Failed to check AI readiness:", err);
          if (isMounted) {
            setAiReadiness({
              ready: false,
              reason: t("detail.aiNotReadyWarning", { reason: "Failed to check AI readiness" }),
            });
          }
        });
    } else {
      setSuggestion(null);
      setAiLoading(false);
      setAiApplying(false);
    }
    return () => {
      isMounted = false;
    };
  }, [isOpen, t]);

  const prevFavIdRef = useRef<number | null>(null);

  useEffect(() => {
    if (!isOpen || !favorite) {
      prevFavIdRef.current = null;
      return;
    }

    const isNewFav = prevFavIdRef.current !== favorite.id;
    prevFavIdRef.current = favorite.id;
    setCurrentFav({
      ...favorite,
      tags: favorite.tags.map((t) => ({ ...t })),
    });

    if (isNewFav) {
      setCategory(favorite.category);
      setRating(favorite.rating ?? null);
      setRatingReason(favorite.rating_reason ?? null);
      setRatingConfidence(favorite.rating_confidence ?? null);
      setNotes(favorite.notes ?? "");
      const uTags = favorite.tags
        .filter((t) => t.source === "user")
        .map((t) => t.tag);
      setUserTags([...uTags]);
      const aTags = favorite.tags
        .filter((t) => t.source === "ai")
        .map((t) => t.tag);
      setAiTags([...aTags]);
      setNewTagInput("");
      setShowDeleteConfirm(false);
      setDeleting(false);
      setSuggestion(null);
      setAiLoading(false);
      setAiApplying(false);
      setFeedbackMessage(null);
    }
  }, [favorite, isOpen]);

  const handleSuggestAi = async () => {
    if (!currentFav || aiLoading || aiApplying || saving || refreshing) return;
    setAiLoading(true);
    setFeedbackMessage(null);
    try {
      const res = await suggestFavoriteAiMetadata(currentFav.id);
      const ghTopics = currentFav.tags
        .filter((t) => t.source === "github")
        .map((t) => t.tag);
      const supplementalAiTags = filterSupplementalAiTags(res.tags, ghTopics);

      setSuggestion({
        ...res,
        tags: supplementalAiTags,
      });
      setCategory(res.category);
      setRating(res.rating);
      setRatingReason(res.rating_reason);
      setRatingConfidence(res.confidence);
      setAiTags(supplementalAiTags);
      setFeedbackMessage({
        type: "success",
        text: t("detail.feedbackAiApplied"),
      });
    } catch (err) {
      const msg = getErrorMessage(err);
      setFeedbackMessage({
        type: "error",
        text: t("detail.feedbackSaveFailed", { error: msg }),
      });
    } finally {
      setAiLoading(false);
    }
  };

  const handleClearRating = () => {
    setRating(null);
    setRatingReason(null);
    setRatingConfidence(null);
  };

  const handleApplyAi = () => {
    if (!currentFav || !suggestion) return;
    setCategory(suggestion.category);
    setAiTags([...suggestion.tags]);
    setSuggestion(null);
    setFeedbackMessage({
      type: "success",
      text: t("detail.feedbackAiApplied"),
    });
  };

  const handleClose = () => {
    setSuggestion(null);
    setAiLoading(false);
    setAiApplying(false);
    setFeedbackMessage(null);
    onClose();
  };

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
        rating_reason: ratingReason,
        rating_confidence: ratingConfidence,
        notes,
        user_tags: userTags,
        ai_tags: aiTags,
      });
      setCurrentFav(updated);
      onUpdated(updated);
      onClose();
    } catch (err) {
      const msg = getErrorMessage(err);
      setFeedbackMessage({
        type: "error",
        text: t("detail.feedbackSaveFailed", { error: msg }),
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
        const refreshedGhTopics = res.favorite.tags
          .filter((t) => t.source === "github")
          .map((t) => t.tag);
        setAiTags((prevAi) => filterSupplementalAiTags(prevAi, refreshedGhTopics));
        setFeedbackMessage({
          type: "success",
          text: t("detail.feedbackRefreshSuccess"),
        });
      } else if (res.status === "not_found") {
        setFeedbackMessage({
          type: "error",
          text: t("detail.feedbackNotFound"),
        });
      } else if (res.status === "rate_limited") {
        setFeedbackMessage({
          type: "error",
          text: t("detail.feedbackRateLimited"),
        });
      } else if (res.status === "conflict") {
        setFeedbackMessage({
          type: "error",
          text: t("detail.feedbackConflict", { message: res.message || "" }),
        });
      } else {
        setFeedbackMessage({
          type: "error",
          text: t("detail.feedbackRefreshFailed", { message: res.message || "" }),
        });
      }
    } catch (err) {
      const msg = getErrorMessage(err);
      setFeedbackMessage({
        type: "error",
        text: t("detail.feedbackRefreshError", { error: msg }),
      });
    } finally {
      setRefreshing(false);
    }
  };

  const handleDelete = async () => {
    if (deleting) return;

    setDeleting(true);
    try {
      await deleteFavorite(currentFav.id);
      onDeleted(currentFav.id);
      onClose();
    } catch (err) {
      const msg = getErrorMessage(err);
      setFeedbackMessage({
        type: "error",
        text: t("detail.feedbackDeleteFailed", { error: msg }),
      });
      setShowDeleteConfirm(false);
    } finally {
      setDeleting(false);
    }
  };

  const formattedFavoritedAt = currentFav.favorited_at
    ? new Date(currentFav.favorited_at).toLocaleString()
    : t("common.unknown");

  const formattedRefreshedAt = currentFav.metadata_refreshed_at
    ? new Date(currentFav.metadata_refreshed_at).toLocaleString()
    : t("common.unknown");

  const formattedPushedAt = currentFav.pushed_at
    ? new Date(currentFav.pushed_at).toLocaleDateString()
    : t("common.unknown");

  const githubTags = currentFav.tags.filter((t) => t.source === "github");

  return (
    <div
      data-testid="favorite-detail-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm overflow-y-auto"
    >
      <div className="w-full max-w-6xl mx-4 sm:mx-6 rounded-xl border border-slate-700 bg-slate-900 p-6 shadow-2xl space-y-5 my-8 max-h-[90vh] overflow-y-auto">
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
            onClick={handleClose}
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
            {currentFav.description || t("card.noDescription")}
          </p>

          <div className="grid grid-cols-2 sm:grid-cols-5 gap-2 text-xs text-slate-400 pt-1">
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">{t("detail.license")}</span>
              <span className="font-medium text-slate-200">
                {currentFav.license || t("common.unknown")}
              </span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">{t("detail.lastActive")}</span>
              <span className="font-medium text-slate-200">{formattedPushedAt}</span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">{t("detail.releases")}</span>
              <span
                data-testid="detail-releases-count"
                className="font-medium text-slate-200"
              >
                {currentFav.releases !== null && currentFav.releases !== undefined
                  ? `${t("detail.releases")} ${currentFav.releases}`
                  : t("card.releasesUnknown")}
              </span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">{t("detail.favoritedAt")}</span>
              <span className="font-medium text-slate-200 truncate block" title={formattedFavoritedAt}>
                {formattedFavoritedAt}
              </span>
            </div>
            <div className="bg-slate-800/60 p-2 rounded border border-slate-800">
              <span className="block text-[10px] text-slate-500">{t("detail.metadataRefreshedAt")}</span>
              <span className="font-medium text-slate-200 truncate block" title={formattedRefreshedAt}>
                {formattedRefreshedAt}
              </span>
            </div>
          </div>

          {/* Languages Breakdown */}
          <div className="space-y-1.5 pt-2" data-testid="detail-languages-section">
            <div className="text-xs font-medium text-slate-400">
              {t("detail.languagesSection")}
            </div>
            <LanguagesBar languages={currentFav.languages} mode="full" />
          </div>
        </div>

        {/* GitHub & AI Tags (Read-only tags) */}
        {(githubTags.length > 0 || aiTags.length > 0) && (
          <div className="space-y-1.5 pt-1">
            <div className="text-xs font-medium text-slate-400">
              {t("detail.topicsSection")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {githubTags.map((t) => (
                <span
                  key={t.id}
                  className="px-2 py-0.5 rounded text-xs bg-slate-800 text-slate-300 border border-slate-700"
                >
                  #{t.tag}
                </span>
              ))}
              {aiTags.map((tag) => (
                <span
                  key={tag}
                  className="px-2 py-0.5 rounded text-xs bg-purple-950/60 text-purple-300 border border-purple-800/60"
                >
                  #{tag}
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
            {t("detail.userPropertiesSection")}
          </h4>

          {/* Category Edit */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label
                htmlFor="detail-category-select"
                className="block text-xs font-medium text-slate-300"
              >
                {t("detail.categoryLabel")}
              </label>
            </div>
            <select
              id="detail-category-select"
              data-testid="detail-category-select"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="w-full rounded-md border border-slate-700 bg-slate-800 px-3 py-1.5 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              {ALL_CATEGORIES.map((cat) => (
                <option key={cat} value={cat}>
                  {t(`categories.${cat}`) || cat}
                </option>
              ))}
            </select>

            {/* Transient AI Suggestion Box */}
            {suggestion && (
              <div
                data-testid="ai-suggestion-box"
                className="mt-3 p-3.5 rounded-lg border border-purple-700/60 bg-purple-950/30 space-y-3"
              >
                <div className="flex items-center justify-between">
                  <span className="text-xs font-semibold text-purple-300 flex items-center gap-1.5">
                    <span>✨</span> {t("detail.aiSuggestionTitle")}
                  </span>
                  <span className="text-[10px] text-purple-400">{t("detail.aiSuggestionSubtitle")}</span>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-3 gap-2 text-xs">
                  <div className="bg-slate-900/70 p-2.5 rounded border border-purple-800/40">
                    <span className="block text-[10px] text-slate-400 mb-0.5">{t("detail.suggestedCategory")}</span>
                    <span
                      data-testid="ai-suggested-category"
                      className="font-semibold text-purple-200"
                    >
                      {suggestion.category}
                    </span>
                  </div>
                  <div className="bg-slate-900/70 p-2.5 rounded border border-purple-800/40">
                    <span className="block text-[10px] text-slate-400 mb-0.5">{t("detail.suggestedRating")}</span>
                    <span
                      data-testid="ai-suggested-rating"
                      className="font-semibold text-amber-300"
                    >
                      {"★".repeat(suggestion.rating)} ({t("detail.ratingScore", { count: suggestion.rating })})
                    </span>
                  </div>
                  <div className="bg-slate-900/70 p-2.5 rounded border border-purple-800/40">
                    <span className="block text-[10px] text-slate-400 mb-0.5">{t("detail.suggestedTags")}</span>
                    <div data-testid="ai-suggested-tags" className="flex flex-wrap gap-1">
                      {suggestion.tags.length > 0 ? (
                        suggestion.tags.map((tag) => (
                          <span
                            key={tag}
                            className="px-1.5 py-0.5 rounded text-[11px] bg-purple-900/60 text-purple-200 border border-purple-700/50"
                          >
                            #{tag}
                          </span>
                        ))
                      ) : (
                        <span className="text-[11px] text-slate-500 italic">{t("detail.noSuggestedTags")}</span>
                      )}
                    </div>
                  </div>
                </div>

                <div className="flex justify-end gap-2 pt-1">
                  <button
                    type="button"
                    data-testid="ai-cancel-btn"
                    disabled={aiApplying}
                    onClick={() => setSuggestion(null)}
                    className="px-3 py-1 text-xs text-slate-300 hover:text-slate-100 bg-slate-800 hover:bg-slate-700 rounded border border-slate-700 transition-colors"
                  >
                    {t("detail.closeSuggestionBtn")}
                  </button>
                  <button
                    type="button"
                    data-testid="ai-apply-btn"
                    disabled={aiApplying}
                    onClick={handleApplyAi}
                    className="px-3 py-1 text-xs text-white bg-purple-600 hover:bg-purple-500 rounded font-medium shadow-sm transition-colors flex items-center gap-1"
                  >
                    {aiApplying ? t("detail.applyingSuggestionBtn") : t("detail.applySuggestionBtn")}
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Rating Edit */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-xs font-medium text-slate-300">
                {t("detail.ratingLabel")}
              </label>
              {rating != null && (
                <button
                  type="button"
                  data-testid="detail-clear-rating-btn"
                  onClick={handleClearRating}
                  className="text-[11px] text-rose-400 hover:text-rose-300"
                >
                  {t("detail.clearRatingBtn")}
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
                {rating != null ? t("detail.ratingScore", { count: rating }) : t("detail.ratingNone")}
              </span>
            </div>

            {/* AI Rating Reason & Confidence Card */}
            {ratingReason && (
              <div
                data-testid="ai-rating-reason-card"
                className="mt-2.5 p-2.5 rounded-md border border-purple-800/40 bg-purple-950/20 text-xs space-y-1.5"
              >
                <div className="flex items-center justify-between">
                  <span className="text-[11px] font-medium text-purple-300 flex items-center gap-1">
                    <span>✨</span> {t("detail.aiRatingReasonTitle")}
                  </span>
                  {ratingConfidence && (
                    <span
                      data-testid="ai-rating-confidence-badge"
                      className={`px-1.5 py-0.5 rounded text-[10px] font-medium border ${
                        ratingConfidence === "high"
                          ? "bg-emerald-950/60 text-emerald-300 border-emerald-700/50"
                          : ratingConfidence === "medium"
                          ? "bg-amber-950/60 text-amber-300 border-amber-700/50"
                          : "bg-slate-800 text-slate-400 border-slate-700"
                      }`}
                    >
                      {t("detail.confidenceLabel")}{" "}
                      {ratingConfidence === "high"
                        ? t("detail.confidenceHigh")
                        : ratingConfidence === "medium"
                        ? t("detail.confidenceMedium")
                        : t("detail.confidenceLow")}
                    </span>
                  )}
                </div>
                <p
                  data-testid="ai-rating-reason-text"
                  className="text-slate-300 text-xs leading-relaxed"
                >
                  {ratingReason}
                </p>
              </div>
            )}
          </div>

          {/* User Tags Edit */}
          <div>
            <label className="block text-xs font-medium text-slate-300 mb-1">
              {t("detail.tagsLabel")}
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
                  {t("detail.noTags")}
                </span>
              )}
            </div>

            <form onSubmit={handleAddUserTag} className="flex gap-2">
              <input
                type="text"
                data-testid="detail-add-tag-input"
                placeholder={t("detail.addTagPlaceholder")}
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
                {t("detail.addTagBtn")}
              </button>
            </form>
          </div>

          {/* Notes Edit */}
          <div>
            <label
              htmlFor="detail-notes-textarea"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              {t("detail.notesLabel")}
            </label>
            <textarea
              id="detail-notes-textarea"
              data-testid="detail-notes-textarea"
              rows={4}
              placeholder={t("detail.notesPlaceholder")}
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
              <span>⚠️</span> {t("detail.deleteConfirmTitle")}
            </div>
            <p className="text-xs text-rose-300/90 leading-relaxed">
              {t("detail.deleteConfirmMessage", { name: currentFav.name })}
            </p>
            <div className="flex gap-2 justify-end pt-1">
              <button
                type="button"
                data-testid="confirm-delete-btn"
                disabled={deleting}
                onClick={handleDelete}
                className="px-4 py-1.5 bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium rounded-md shadow-sm transition-colors"
              >
                {deleting ? t("common.deleting") : t("detail.deleteConfirmBtn")}
              </button>
              <button
                type="button"
                data-testid="cancel-delete-btn"
                onClick={() => setShowDeleteConfirm(false)}
                className="px-4 py-1.5 border border-slate-700 bg-slate-800 text-slate-300 text-xs rounded-md hover:bg-slate-700 transition-colors"
              >
                {t("common.cancel")}
              </button>
            </div>
          </div>
        )}

        {/* Footer Actions */}
        <div className="flex items-center justify-between border-t border-slate-800 pt-4 flex-wrap gap-3">
          <div>
            {!showDeleteConfirm && (
              <button
                type="button"
                data-testid="detail-delete-btn"
                onClick={() => setShowDeleteConfirm(true)}
                className="text-xs text-rose-400 hover:text-rose-300 transition-colors"
              >
                {t("detail.deleteFavoriteBtn")}
              </button>
            )}
          </div>

          <div className="flex items-center gap-3 flex-wrap">
            <button
              type="button"
              data-testid="ai-suggest-btn"
              disabled={!aiReadiness?.ready || aiLoading || aiApplying || saving || refreshing}
              onClick={handleSuggestAi}
              title={
                !aiReadiness?.ready
                  ? aiReadiness?.reason || t("detail.aiNotReadyWarning", { reason: "AI not ready" })
                  : t("detail.aiSuggestBtn")
              }
              className="px-3.5 py-2 rounded-md bg-purple-950/80 hover:bg-purple-900 border border-purple-700/60 text-purple-300 disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-1.5 transition-colors text-xs font-medium"
            >
              <span>✨</span>
              <span>{aiLoading ? t("detail.aiSuggestingBtn") : t("detail.aiSuggestBtn")}</span>
            </button>

            <button
              type="button"
              data-testid="detail-refresh-btn"
              disabled={refreshing || saving || aiLoading}
              onClick={handleRefresh}
              className="px-3.5 py-2 border border-slate-700 bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium rounded-md transition-colors flex items-center gap-1.5"
            >
              <span>{refreshing ? t("detail.refreshingBtn") : t("detail.refreshMetadataBtn")}</span>
              <span>🔄</span>
            </button>

            <button
              type="button"
              data-testid="detail-save-btn"
              disabled={saving || refreshing || aiLoading}
              onClick={handleSave}
              className="px-5 py-2 bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium rounded-md shadow-sm transition-colors"
            >
              {saving ? t("detail.savingBtn") : t("detail.saveAndExitBtn")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
