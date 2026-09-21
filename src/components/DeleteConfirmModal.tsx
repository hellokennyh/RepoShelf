import React, { useEffect, useRef } from "react";
import { useI18n } from "../i18n/I18nContext";

export interface DeleteConfirmModalProps {
  isOpen: boolean;
  targetCount: number;
  targetNames: string[];
  isDeleting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export const DeleteConfirmModal: React.FC<DeleteConfirmModalProps> = ({
  isOpen,
  targetCount,
  targetNames,
  isDeleting,
  onConfirm,
  onCancel,
}) => {
  const { t } = useI18n();
  const cancelBtnRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!isOpen) return;

    // Focus cancel button by default without scrolling underlying container
    cancelBtnRef.current?.focus({ preventScroll: true });

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !isDeleting) {
        e.preventDefault();
        onCancel();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, isDeleting, onCancel]);

  if (!isOpen) return null;

  const previewList = targetNames.slice(0, 5);
  const remainingCount = targetCount > previewList.length ? targetCount - previewList.length : 0;

  return (
    <div
      data-testid="delete-confirm-modal"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget && !isDeleting) {
          onCancel();
        }
      }}
    >
      <div
        className="w-full max-w-lg rounded-lg border border-slate-700 bg-slate-900 p-6 shadow-2xl space-y-4"
        role="dialog"
        aria-modal="true"
        aria-labelledby="delete-confirm-title"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-slate-800 pb-3">
          <h3
            id="delete-confirm-title"
            data-testid="delete-confirm-title"
            className="text-base font-semibold text-rose-300 flex items-center gap-2"
          >
            <span>🗑️</span>
            <span>{t("library.deleteModal.title")}</span>
          </h3>
          <button
            type="button"
            data-testid="delete-confirm-close-x"
            disabled={isDeleting}
            onClick={onCancel}
            className="text-slate-400 hover:text-slate-200 text-sm disabled:opacity-50"
          >
            ✕
          </button>
        </div>

        {/* Description & count */}
        <p
          data-testid="delete-confirm-desc"
          className="text-sm text-slate-200"
        >
          {t("library.deleteModal.description", { count: targetCount })}
        </p>

        {/* Preview List */}
        {previewList.length > 0 && (
          <div className="rounded-md border border-slate-800 bg-slate-950/60 p-3 space-y-1.5 text-xs">
            <span className="font-medium text-slate-400 block mb-1">
              {t("library.deleteModal.previewHeader")}
            </span>
            <ul
              data-testid="delete-confirm-preview-list"
              className="space-y-1 text-slate-300 max-h-36 overflow-y-auto"
            >
              {previewList.map((name, idx) => (
                <li key={idx} className="flex items-center gap-1.5 truncate">
                  <span className="text-slate-500">•</span>
                  <span className="font-mono text-slate-200 truncate">{name}</span>
                </li>
              ))}
            </ul>
            {remainingCount > 0 && (
              <p className="text-slate-500 text-xs italic pt-1">
                {t("library.deleteModal.moreItems", { count: remainingCount })}
              </p>
            )}
          </div>
        )}

        {/* Warning Notice */}
        <div
          data-testid="delete-confirm-warning"
          className="rounded-md border border-rose-900/60 bg-rose-950/40 p-3 text-xs text-rose-300/90 leading-relaxed"
        >
          ⚠️ {t("library.deleteModal.warningNotice")}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-end gap-3 pt-2">
          <button
            ref={cancelBtnRef}
            type="button"
            data-testid="delete-confirm-cancel-btn"
            disabled={isDeleting}
            onClick={onCancel}
            className="px-4 py-1.5 rounded-md border border-slate-700 bg-slate-800 text-slate-200 text-xs font-medium hover:bg-slate-700 disabled:opacity-50 transition-colors"
          >
            {t("library.deleteModal.cancelBtn")}
          </button>
          <button
            type="button"
            data-testid="delete-confirm-submit-btn"
            disabled={isDeleting}
            onClick={onConfirm}
            className="px-4 py-1.5 rounded-md bg-rose-700 text-white text-xs font-medium hover:bg-rose-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-1.5"
          >
            {isDeleting && (
              <div className="animate-spin w-3 h-3 border-2 border-white border-t-transparent rounded-full" />
            )}
            <span>
              {isDeleting
                ? t("library.deleteModal.deletingBtn")
                : t("library.deleteModal.confirmBtn")}
            </span>
          </button>
        </div>
      </div>
    </div>
  );
};
