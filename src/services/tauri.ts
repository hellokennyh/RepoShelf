import { invoke } from "@tauri-apps/api/core";
import { AppInfo } from "../types/app";

export async function fetchAppInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>("get_app_info");
  } catch (error) {
    // In web-only test or non-Tauri preview environment, provide fallback
    console.warn("Tauri IPC invoke failed or running outside Tauri:", error);
    return {
      name: "RepoShelf",
      phase: "Phase 0 (Web Fallback)",
    };
  }
}
