import React from "react";

export interface DuplicateModalProps {
  isOpen: boolean;
  repoName: string;
  favoriteId?: number | null;
  onClose: () => void;
  onRefresh: (favoriteId: number) => void;
  onOpenRecord: (favoriteId: number) => void;
}

export const DuplicateModal: React.FC<DuplicateModalProps> = ({
  isOpen,
  repoName,
  favoriteId,
  onClose,
  onRefresh,
  onOpenRecord,
}) => {
  if (!isOpen) return null;

  return (
    <div
      data-testid="duplicate-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
    >
      <div className="w-full max-w-md rounded-lg border border-slate-700 bg-slate-900 p-6 shadow-xl space-y-4">
        <div className="flex items-start gap-3">
          <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-amber-500/20 text-amber-400 text-lg">
            ⚠️
          </div>
          <div>
            <h3 className="text-base font-semibold text-slate-100">
              该项目已经收藏
            </h3>
            <p className="text-sm text-slate-400 mt-1">
              <span className="font-mono text-slate-300 font-medium">
                {repoName}
              </span>{" "}
              已经在本地收藏库中。你可以选择刷新最新的 GitHub 元数据，或者直接查看并编辑已有的本地收藏记录。
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-2 pt-2 sm:flex-row-reverse">
          <button
            type="button"
            data-testid="duplicate-modal-refresh-btn"
            disabled={favoriteId == null}
            onClick={() => {
              if (favoriteId != null) onRefresh(favoriteId);
            }}
            className="inline-flex justify-center items-center rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-500 disabled:opacity-50 transition-colors"
          >
            刷新项目信息
          </button>
          <button
            type="button"
            data-testid="duplicate-modal-open-btn"
            disabled={favoriteId == null}
            onClick={() => {
              if (favoriteId != null) onOpenRecord(favoriteId);
            }}
            className="inline-flex justify-center items-center rounded-md border border-slate-700 bg-slate-800 px-4 py-2 text-sm font-medium text-slate-200 hover:bg-slate-700 disabled:opacity-50 transition-colors"
          >
            打开收藏记录
          </button>
          <button
            type="button"
            data-testid="duplicate-modal-cancel-btn"
            onClick={onClose}
            className="inline-flex justify-center items-center rounded-md border border-transparent px-4 py-2 text-sm font-medium text-slate-400 hover:text-slate-200 transition-colors"
          >
            取消
          </button>
        </div>
      </div>
    </div>
  );
};
