import React from "react";
import { BatchFavoriteResult, BatchRefreshResult } from "../types/library";

export interface BatchResultModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  favoriteResult?: BatchFavoriteResult | null;
  refreshResult?: BatchRefreshResult | null;
}

export const BatchResultModal: React.FC<BatchResultModalProps> = ({
  isOpen,
  onClose,
  title,
  favoriteResult,
  refreshResult,
}) => {
  if (!isOpen) return null;

  return (
    <div
      data-testid="batch-result-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
    >
      <div className="w-full max-w-md rounded-lg border border-slate-700 bg-slate-900 p-6 shadow-xl space-y-4">
        <div className="flex items-center justify-between border-b border-slate-800 pb-3">
          <h3
            data-testid="batch-result-title"
            className="text-base font-semibold text-slate-100"
          >
            {title}
          </h3>
          <button
            type="button"
            data-testid="batch-result-close-x"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 text-sm"
          >
            ✕
          </button>
        </div>

        {/* Favorite Result Breakdown */}
        {favoriteResult && (
          <div data-testid="batch-favorite-summary" className="space-y-3 text-sm">
            <div className="grid grid-cols-3 gap-2 text-center">
              <div className="p-3 rounded-lg bg-emerald-950/40 border border-emerald-800/60">
                <span className="block text-xs text-emerald-400 font-medium">
                  成功收藏
                </span>
                <span
                  data-testid="batch-fav-succeeded-count"
                  className="text-xl font-bold text-emerald-300"
                >
                  {favoriteResult.succeeded}
                </span>
              </div>
              <div className="p-3 rounded-lg bg-amber-950/40 border border-amber-800/60">
                <span className="block text-xs text-amber-400 font-medium">
                  已存在
                </span>
                <span
                  data-testid="batch-fav-already-exists-count"
                  className="text-xl font-bold text-amber-300"
                >
                  {favoriteResult.already_exists}
                </span>
              </div>
              <div className="p-3 rounded-lg bg-rose-950/40 border border-rose-800/60">
                <span className="block text-xs text-rose-400 font-medium">
                  失败
                </span>
                <span
                  data-testid="batch-fav-failed-count"
                  className="text-xl font-bold text-rose-300"
                >
                  {favoriteResult.failed}
                </span>
              </div>
            </div>
          </div>
        )}

        {/* Refresh Result Breakdown */}
        {refreshResult && (
          <div data-testid="batch-refresh-summary" className="space-y-3 text-sm">
            <div className="grid grid-cols-2 gap-2 text-center">
              <div className="p-2.5 rounded-lg bg-emerald-950/40 border border-emerald-800/60">
                <span className="block text-xs text-emerald-400">
                  成功刷新 (Succeeded)
                </span>
                <span
                  data-testid="batch-ref-succeeded-count"
                  className="text-lg font-bold text-emerald-300"
                >
                  {refreshResult.succeeded}
                </span>
              </div>
              <div className="p-2.5 rounded-lg bg-rose-950/40 border border-rose-800/60">
                <span className="block text-xs text-rose-400">
                  失败 (Failed)
                </span>
                <span
                  data-testid="batch-ref-failed-count"
                  className="text-lg font-bold text-rose-300"
                >
                  {refreshResult.failed}
                </span>
              </div>
              <div className="p-2.5 rounded-lg bg-amber-950/40 border border-amber-800/60">
                <span className="block text-xs text-amber-400">
                  频率限额 (Rate Limited)
                </span>
                <span
                  data-testid="batch-ref-ratelimit-count"
                  className="text-lg font-bold text-amber-300"
                >
                  {refreshResult.rate_limited}
                </span>
              </div>
              <div className="p-2.5 rounded-lg bg-slate-800 border border-slate-700">
                <span className="block text-xs text-slate-400">
                  未找到 (Not Found)
                </span>
                <span
                  data-testid="batch-ref-notfound-count"
                  className="text-lg font-bold text-slate-300"
                >
                  {refreshResult.not_found}
                </span>
              </div>
            </div>
            <div className="text-xs text-slate-400 text-center">
              总计处理: {refreshResult.total} 项
            </div>
          </div>
        )}

        <div className="pt-2 flex justify-end">
          <button
            type="button"
            data-testid="batch-result-ok-btn"
            onClick={onClose}
            className="rounded-md bg-indigo-600 px-4 py-2 text-xs font-medium text-white hover:bg-indigo-500 transition-colors"
          >
            完成
          </button>
        </div>
      </div>
    </div>
  );
};
