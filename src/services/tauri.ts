import { invoke } from "@tauri-apps/api/core";
import { AppInfo } from "../types/app";
import {
  GitHubConnectionStatus,
  GitHubRepository,
  RepositorySearchRequest,
  RepositorySearchResponse,
} from "../types/github";
import { SecretStatus, Settings } from "../types/settings";

export async function fetchAppInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>("get_app_info");
  } catch (error) {
    console.warn("Tauri IPC invoke failed or running outside Tauri:", error);
    return {
      name: "RepoShelf",
      phase: "Phase 1 (Web Fallback)",
    };
  }
}

export async function fetchSettings(): Promise<Settings> {
  return await invoke<Settings>("get_settings");
}

export async function saveSettings(settings: Settings): Promise<Settings> {
  return await invoke<Settings>("update_settings", { settings });
}

export async function fetchSecretStatus(): Promise<SecretStatus> {
  return await invoke<SecretStatus>("get_secret_status");
}

export async function saveGitHubToken(token: string): Promise<void> {
  await invoke<void>("set_github_token", { token });
}

export async function clearGitHubToken(): Promise<void> {
  await invoke<void>("clear_github_token");
}

export async function saveAiApiKey(apiKey: string): Promise<void> {
  await invoke<void>("set_ai_api_key", { apiKey });
}

export async function clearAiApiKey(): Promise<void> {
  await invoke<void>("clear_ai_api_key");
}

export async function testGitHubConnection(): Promise<GitHubConnectionStatus> {
  return await invoke<GitHubConnectionStatus>("github_test_connection");
}

export async function getGitHubRepository(url: string): Promise<GitHubRepository> {
  return await invoke<GitHubRepository>("github_get_repository", { url });
}

export async function searchGitHubRepositories(
  request: RepositorySearchRequest
): Promise<RepositorySearchResponse> {
  return await invoke<RepositorySearchResponse>("github_search_repositories", {
    request,
  });
}

export async function openGitHubUrl(url: string): Promise<void> {
  await invoke<void>("open_github_url", { url });
}

