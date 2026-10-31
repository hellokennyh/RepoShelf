import React, { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";

export interface CellViewerModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  repoName: string;
  type: "description" | "notes" | "userTags";
  content?: string;
  tags?: string[];
  isTranslation?: boolean;
}

export const CellViewerModal: React.FC<CellViewerModalProps> = ({
  isOpen,
  onClose,
  title,
  repoName,
  type,
  content,
  tags,
  isTranslation,
}) => {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!isOpen) {
      setCopied(false);
      return;
    }

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const handleCopy = async () => {
    let textToCopy = "";
    if (type === "userTags" && tags) {
      textToCopy = tags.map((t) => `#${t}`).join(" ");
    } else if (content) {
      textToCopy = content;
    }

    if (!textToCopy) return;

    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(textToCopy);
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.warn("Failed to copy text to clipboard:", err);
    }
  };

  return (
    <div
      data-testid="cell-viewer-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="cell-viewer-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4 animate-in fade-in duration-150"
      onClick={onClose}
    >
      <div
        className="w-full max-w-lg rounded-xl border border-slate-700 bg-slate-800 shadow-2xl p-6 text-slate-100 space-y-4 max-h-[85vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start justify-between border-b border-slate-700/80 pb-3">
          <div className="space-y-1 pr-4">
            <div className="flex items-center gap-2">
              <h3 id="cell-viewer-title" className="text-base font-semibold text-slate-100">
                {title}
              </h3>
              {isTranslation && (
                <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-950 text-emerald-300 border border-emerald-700/60">
                  {t("detail.translationValid")}
                </span>
              )}
            </div>
            <p className="text-xs text-slate-400 font-mono">{repoName}</p>
          </div>
          <button
            type="button"
            data-testid="cell-viewer-close-btn"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 text-sm p-1 rounded-md hover:bg-slate-700 transition-colors"
            title={t("common.close")}
          >
            ✕
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto pr-1">
          {type === "userTags" ? (
            <div className="flex flex-wrap gap-2 pt-1">
              {tags && tags.length > 0 ? (
                tags.map((tag, idx) => (
                  <span
                    key={idx}
                    className="px-2.5 py-1 rounded-md text-xs font-medium bg-indigo-950 text-indigo-300 border border-indigo-700/60"
                  >
                    #{tag}
                  </span>
                ))
              ) : (
                <span className="text-xs text-slate-500 italic">—</span>
              )}
            </div>
          ) : (
            <div className="text-xs text-slate-200 leading-relaxed whitespace-pre-wrap break-words select-text bg-slate-900/60 p-3 rounded-lg border border-slate-700/50">
              {content || "—"}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between border-t border-slate-700/80 pt-3 text-xs">
          <button
            type="button"
            data-testid="cell-viewer-copy-btn"
            onClick={handleCopy}
            className="px-3 py-1.5 rounded-md border border-slate-700 bg-slate-700/60 text-slate-200 hover:bg-slate-700 hover:text-white transition-colors flex items-center gap-1.5"
          >
            <span>{copied ? "✓" : "📋"}</span>
            <span>{copied ? t("library.table.copied") : t("library.table.copy")}</span>
          </button>
          <button
            type="button"
            onClick={onClose}
            className="px-3.5 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-medium transition-colors"
          >
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
};
