export type NavTab = "trending" | "search" | "add" | "library" | "settings";

export const APP_NAME = "RepoShelf";
export const APP_VERSION = "1.0.0";
export const APP_VERSION_DISPLAY = `v${APP_VERSION}`;

export interface AppInfo {
  name: string;
  phase: string;
}
