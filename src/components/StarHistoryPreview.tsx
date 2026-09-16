import React, { useEffect, useRef } from "react";
import { useStarHistory, useStarHistoryEntry } from "../context/StarHistoryContext";
import { useI18n } from "../i18n/I18nContext";

export interface StarHistoryPreviewProps {
  owner: string;
  repo: string;
}

export const StarHistoryPreview: React.FC<StarHistoryPreviewProps> = ({
  owner,
  repo,
}) => {
  const { t } = useI18n();
  const { requestPreview } = useStarHistory();
  const entry = useStarHistoryEntry(owner, repo);
  const containerRef = useRef<HTMLDivElement>(null);

  const repoFullName = `${owner}/${repo}`;

  useEffect(() => {
    // If already loaded or in progress, do nothing
    if (entry && entry.status !== "unknown") {
      return;
    }

    const currentEl = containerRef.current;
    if (!currentEl) {
      return;
    }

    // Lazy load when entering or approaching viewport
    if (typeof IntersectionObserver !== "undefined") {
      const observer = new IntersectionObserver(
        (entries) => {
          for (const obsEntry of entries) {
            if (obsEntry.isIntersecting) {
              requestPreview(owner, repo);
              observer.disconnect();
              break;
            }
          }
        },
        { rootMargin: "100px" }
      );

      observer.observe(currentEl);
      return () => {
        observer.disconnect();
      };
    } else {
      // Fallback for test environments without IntersectionObserver
      requestPreview(owner, repo);
    }
  }, [owner, repo, entry, requestPreview]);

  // If no preview is available, or load errored, do not render any blank space
  if (!entry || entry.status === "none" || entry.status === "error") {
    return <div ref={containerRef} className="h-0 w-0 overflow-hidden" aria-hidden="true" />;
  }

  // When loading, keep a 0-height anchor so IntersectionObserver can observe
  if (entry.status === "loading") {
    return <div ref={containerRef} className="h-0 w-0 overflow-hidden" aria-hidden="true" />;
  }

  // When ready with an objectUrl, render the contained preview image
  if (entry.status === "ready" && entry.objectUrl) {
    return (
      <div
        ref={containerRef}
        data-testid="star-history-preview"
        className="pt-2 border-t border-slate-700/40 w-full"
      >
        <img
          src={entry.objectUrl}
          alt={t("starHistory.altText", { repository: repoFullName })}
          className="w-full max-h-[120px] object-contain rounded"
          data-testid="star-history-image"
        />
      </div>
    );
  }

  return <div ref={containerRef} className="h-0 w-0 overflow-hidden" aria-hidden="true" />;
};
