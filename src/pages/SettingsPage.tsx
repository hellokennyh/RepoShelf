import React, { useEffect, useState } from "react";
import { fetchAppInfo } from "../services/tauri";
import { AppInfo } from "../types/app";

export const SettingsPage: React.FC = () => {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchAppInfo()
      .then((info) => {
        setAppInfo(info);
      })
      .finally(() => {
        setLoading(false);
      });
  }, []);

  return (
    <div className="space-y-6">
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>⚙️</span> 设置
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          管理 GitHub Token、可选 AI Provider 与应用偏好。
        </p>
      </div>

      <div className="rounded-lg border border-dashed border-slate-700 bg-slate-800/50 p-8 text-center">
        <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-700/60 text-indigo-400 mb-3 text-xl">
          ⚙️
        </div>
        <h2 className="text-lg font-medium text-slate-200">Settings & Preferences</h2>
        <p className="text-slate-400 mt-2 text-sm max-w-md mx-auto">
          Phase 0 — 功能将在后续阶段实现。
        </p>
      </div>

      {/* IPC Baseline Validation Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm">
        <h3 className="text-sm font-semibold uppercase tracking-wider text-indigo-400 mb-3 flex items-center gap-2">
          <span>🔌</span> Tauri IPC 通信验证 (get_app_info)
        </h3>
        {loading ? (
          <p className="text-xs text-slate-400">正在与 Rust 后端通信...</p>
        ) : appInfo ? (
          <div className="grid grid-cols-2 gap-3 text-sm bg-slate-900/60 rounded p-4 border border-slate-700/50">
            <div>
              <span className="text-slate-400 text-xs block">应用名称</span>
              <span className="font-medium text-slate-200" data-testid="ipc-app-name">
                {appInfo.name}
              </span>
            </div>
            <div>
              <span className="text-slate-400 text-xs block">当前阶段</span>
              <span className="font-medium text-emerald-400" data-testid="ipc-app-phase">
                {appInfo.phase}
              </span>
            </div>
          </div>
        ) : (
          <p className="text-xs text-rose-400">IPC 通信异常或未能获取信息。</p>
        )}
      </div>
    </div>
  );
};
