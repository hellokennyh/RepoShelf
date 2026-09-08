import { invoke } from "@tauri-apps/api/core";
import { AppInfo } from "../types/app";
import {
  AiRepositorySearchRequest,
  AiRepositorySearchResponse,
  AiSearchReadiness,
  GitHubConnectionStatus,
  GitHubRepository,
  RepositorySearchRequest,
  RepositorySearchResponse,
} from "../types/github";
import { AiConnectionResult, SecretStatus, Settings } from "../types/settings";
import { TrendingPeriod, TrendingResponse } from "../types/trending";

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

export async function testAiConnection(): Promise<AiConnectionResult> {
  return await invoke<AiConnectionResult>("test_ai_connection");
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

export async function getAiSearchReadiness(): Promise<AiSearchReadiness> {
  return await invoke<AiSearchReadiness>("get_ai_search_readiness");
}

export async function searchAiGitHubRepositories(
  request: AiRepositorySearchRequest
): Promise<AiRepositorySearchResponse> {
  return await invoke<AiRepositorySearchResponse>("github_ai_search_repositories", {
    request,
  });
}

export async function openGitHubUrl(url: string): Promise<void> {
  await invoke<void>("open_github_url", { url });
}

export async function fetchTrending(
  period: TrendingPeriod,
  limit: number
): Promise<TrendingResponse> {
  return await invoke<TrendingResponse>("get_trending", { period, limit });
}

export async function checkFavoriteStatus(
  url: string,
  repoId?: number | null
): Promise<import("../types/library").FavoriteStatus> {
  return await invoke<import("../types/library").FavoriteStatus>(
    "check_favorite_status",
    {
      repoId: repoId ?? null,
      url,
    }
  );
}

export async function favoriteRepository(
  input: import("../types/library").FavoriteInput
): Promise<import("../types/library").FavoriteItem> {
  return await invoke<import("../types/library").FavoriteItem>(
    "favorite_repository",
    { input }
  );
}

export async function favoriteRepositoriesBatch(
  inputs: import("../types/library").FavoriteInput[]
): Promise<import("../types/library").BatchFavoriteResult> {
  return await invoke<import("../types/library").BatchFavoriteResult>(
    "favorite_repositories_batch",
    {
      inputs,
    }
  );
}

export async function getFavorite(
  id: number
): Promise<import("../types/library").FavoriteItem> {
  return await invoke<import("../types/library").FavoriteItem>("get_favorite", {
    id,
  });
}

export async function listFavorites(
  query: import("../types/library").LibraryQuery = {}
): Promise<import("../types/library").FavoriteItem[]> {
  return await invoke<import("../types/library").FavoriteItem[]>(
    "list_favorites",
    {
      query,
    }
  );
}

export async function listDistinctTags(): Promise<string[]> {
  return await invoke<string[]>("list_distinct_tags");
}

export async function updateFavoriteUserMetadata(
  input: import("../types/library").UpdateFavoriteUserMetadataInput
): Promise<import("../types/library").FavoriteItem> {
  return await invoke<import("../types/library").FavoriteItem>(
    "update_favorite_user_metadata",
    { input }
  );
}

export async function deleteFavorite(id: number): Promise<boolean> {
  return await invoke<boolean>("delete_favorite", { id });
}

export async function refreshFavorite(
  id: number
): Promise<import("../types/library").RefreshResult> {
  return await invoke<import("../types/library").RefreshResult>(
    "refresh_favorite",
    {
      id,
    }
  );
}

export async function refreshFavoritesBatch(
  ids: number[]
): Promise<import("../types/library").BatchRefreshResult> {
  return await invoke<import("../types/library").BatchRefreshResult>(
    "refresh_favorites_batch",
    {
      ids,
    }
  );
}



