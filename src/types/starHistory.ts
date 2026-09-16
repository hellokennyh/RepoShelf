export type StarHistoryStatus = "unknown" | "loading" | "none" | "ready" | "error";

export interface StarHistoryPreviewReady {
  status: "ready";
  svg_data: string;
  embed_url: string;
}

export interface StarHistoryPreviewNone {
  status: "none";
}

export interface StarHistoryPreviewError {
  status: "error";
  message: string;
}

export type StarHistoryPreviewResult =
  | StarHistoryPreviewReady
  | StarHistoryPreviewNone
  | StarHistoryPreviewError;

export interface StarHistoryCacheEntry {
  status: StarHistoryStatus;
  svgData?: string;
  embedUrl?: string;
  objectUrl?: string;
  errorMessage?: string;
  lastAccessed: number;
}
