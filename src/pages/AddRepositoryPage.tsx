import React from "react";

export const AddRepositoryPage: React.FC = () => {
  return (
    <div className="space-y-4">
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>➕</span> 添加项目
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          手动输入 GitHub Repository URL 录入本地库。
        </p>
      </div>

      <div className="rounded-lg border border-dashed border-slate-700 bg-slate-800/50 p-8 text-center">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-700/60 text-indigo-400 mb-3 text-xl">
          ➕
        </div>
        <h2 className="text-lg font-medium text-slate-200">Add GitHub Repository</h2>
        <p className="text-slate-400 mt-2 text-sm max-w-md mx-auto">
          Phase 0 — 功能将在后续阶段实现。
        </p>
      </div>
    </div>
  );
};
