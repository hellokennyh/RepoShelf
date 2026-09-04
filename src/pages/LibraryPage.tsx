import React from "react";

export const LibraryPage: React.FC = () => {
  return (
    <div className="space-y-4">
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>📚</span> 项目浏览
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          本地收藏库浏览、搜索、标签与评级管理。
        </p>
      </div>

      <div className="rounded-lg border border-dashed border-slate-700 bg-slate-800/50 p-8 text-center">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-700/60 text-indigo-400 mb-3 text-xl">
          📚
        </div>
        <h2 className="text-lg font-medium text-slate-200">Local Repository Library</h2>
        <p className="text-slate-400 mt-2 text-sm max-w-md mx-auto">
          Phase 0 — 功能将在后续阶段实现。
        </p>
      </div>
    </div>
  );
};
