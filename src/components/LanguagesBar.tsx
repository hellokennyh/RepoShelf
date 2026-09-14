import React from "react";

export interface LanguagesBarProps {
  languages?: Record<string, number> | null;
  mode?: "compact" | "full";
  className?: string;
}

const COLOR_PALETTE = [
  "bg-blue-500",
  "bg-amber-400",
  "bg-emerald-500",
  "bg-purple-500",
  "bg-rose-500",
  "bg-cyan-400",
  "bg-orange-400",
  "bg-indigo-400",
  "bg-teal-400",
  "bg-pink-400",
  "bg-yellow-400",
  "bg-violet-400",
];

const DOT_COLOR_PALETTE = [
  "bg-blue-500",
  "bg-amber-400",
  "bg-emerald-500",
  "bg-purple-500",
  "bg-rose-500",
  "bg-cyan-400",
  "bg-orange-400",
  "bg-indigo-400",
  "bg-teal-400",
  "bg-pink-400",
  "bg-yellow-400",
  "bg-violet-400",
];

export const LanguagesBar: React.FC<LanguagesBarProps> = ({
  languages,
  mode = "compact",
  className = "",
}) => {
  // 1. Unknown / not fetched state
  if (languages === null || languages === undefined) {
    return (
      <div
        data-testid="languages-unknown"
        className={`text-xs text-slate-500 ${className}`}
      >
        <span>Languages —</span>
      </div>
    );
  }

  const entries = Object.entries(languages)
    .map(([name, pct]) => ({ name, pct: Number(pct) }))
    .filter((e) => !isNaN(e.pct) && e.pct >= 0);

  // 2. Known empty state ({})
  if (entries.length === 0) {
    return (
      <div
        data-testid="languages-empty"
        className={`text-xs text-slate-500 ${className}`}
      >
        <span>无代码语言统计</span>
      </div>
    );
  }

  // 3. Sort descending by percentage, then by name ascending for stability
  entries.sort((a, b) => {
    if (b.pct !== a.pct) {
      return b.pct - a.pct;
    }
    return a.name.localeCompare(b.name);
  });

  const displayEntries =
    mode === "compact" && entries.length > 3 ? entries.slice(0, 3) : entries;
  const remainingCount =
    mode === "compact" && entries.length > 3 ? entries.length - 3 : 0;

  return (
    <div
      data-testid="languages-bar-container"
      className={`space-y-1.5 ${className}`}
    >
      {/* Segmented percentage bar */}
      <div
        data-testid="languages-segmented-bar"
        className="h-1.5 w-full rounded-full overflow-hidden flex bg-slate-700/60"
      >
        {entries.map((entry, idx) => {
          const colorClass = COLOR_PALETTE[idx % COLOR_PALETTE.length];
          const widthPct = Math.min(100, Math.max(0.5, entry.pct));
          return (
            <div
              key={entry.name}
              data-testid={`lang-segment-${entry.name}`}
              className={`${colorClass} h-full transition-all`}
              style={{ width: `${widthPct}%` }}
              title={`${entry.name}: ${entry.pct.toFixed(1)}%`}
            />
          );
        })}
      </div>

      {/* Language legend */}
      <div
        data-testid="languages-legend"
        className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
      >
        {displayEntries.map((entry, idx) => {
          const dotColor = DOT_COLOR_PALETTE[idx % DOT_COLOR_PALETTE.length];
          const displayPct =
            entry.pct > 0 && entry.pct < 0.1
              ? "<0.1%"
              : `${entry.pct.toFixed(1)}%`;

          return (
            <div
              key={entry.name}
              data-testid={`lang-item-${entry.name}`}
              className="flex items-center gap-1.5 text-slate-300"
            >
              <span
                className={`h-2 w-2 rounded-full shrink-0 ${dotColor}`}
                aria-hidden="true"
              />
              <span className="font-medium text-slate-200">{entry.name}</span>
              <span className="text-slate-400 font-mono text-[11px]">
                {displayPct}
              </span>
            </div>
          );
        })}

        {remainingCount > 0 && (
          <span
            data-testid="languages-more-count"
            className="text-[11px] text-slate-500 font-medium"
          >
            +{remainingCount}
          </span>
        )}
      </div>
    </div>
  );
};
