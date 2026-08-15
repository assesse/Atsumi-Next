import type { DownloadState, GalleryId, Language, SearchSort } from "../core/types";

export type ApiErrorAction = "retry" | "review" | "reconnect" | "reveal" | "none";

export type ApiError = {
  code: string;
  message: string;
  retryable: boolean;
  action?: ApiErrorAction;
  details?: Record<string, unknown>;
};

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

export type SettingsSnapshot = {
  revision: number;
  downloadRoot: string;
  maxColumns: number;
  previewWidth: number;
  cacheLimitGb: number;
  concurrentImageRequests: number;
  requestStartIntervalMs: number;
};

export type SettingsPatch = Partial<Omit<SettingsSnapshot, "revision">>;

export type WindowPlacementSnapshot = {
  revision: number;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  maximized: boolean;
};

export type WindowPlacement = Omit<WindowPlacementSnapshot, "revision">;

export type JobRef = {
  jobId: string;
  reused: boolean;
};

export type BackendThumbnailKey =
  | { kind: "galleryCover"; galleryId: number }
  | { kind: "galleryPage"; galleryId: number; sourcePage: number };

export type ThumbnailRequestDto = {
  key: BackendThumbnailKey;
  consumer: "explore" | "downloads" | "detail" | "review";
  priority: "critical" | "visible" | "prefetch";
};

export type ThumbnailRequestToken = {
  requestId: string;
  key: BackendThumbnailKey;
};

export type ThumbnailInvalidation = {
  key: BackendThumbnailKey;
  successCacheRemoved: boolean;
  negativeCacheRemoved: boolean;
};

export type ResolvedThumbnail = {
  contentType: string;
  bytes: number[];
  width: number;
  height: number;
  sourceRevision?: string;
};

export type ThumbnailDelivery = {
  key: BackendThumbnailKey;
  thumbnail: ResolvedThumbnail;
  cacheStatus: "resolved" | "memory";
};

export type ThumbnailFailure = {
  key: BackendThumbnailKey;
  code:
    | "cancelled"
    | "notFound"
    | "candidatesExhausted"
    | "responseInvalid"
    | "decodeFailed"
    | "temporarilyUnavailable"
    | "unauthorized"
    | "invalidData"
    | "resolver"
    | "coordinatorClosed";
  message: string;
  retryable: boolean;
  negativeCacheHit: boolean;
};

export type ThumbnailCompletionEvent = {
  requestId: string;
  key: BackendThumbnailKey;
  outcome:
    | { status: "ready"; delivery: ThumbnailDelivery }
    | { status: "failed"; failure: ThumbnailFailure };
};

export type ThumbnailWorkerStats = {
  workerCount: number;
  concurrencyLimit: number;
  requestStartIntervalMs: number;
  activeWorkers: number;
  queuedKeys: number;
  inFlightKeys: number;
  subscriberCount: number;
  successCacheEntries: number;
  successCacheBytes: number;
  negativeCacheEntries: number;
  requestsTotal: number;
  successCacheHits: number;
  negativeCacheHits: number;
  joinedInFlight: number;
  resolvedSuccess: number;
  resolvedFailure: number;
  cancelledSubscribers: number;
  cancelledWork: number;
};

export type JobEvent = {
  jobId: string;
  galleryId?: number;
  revision: number;
  state: DownloadState;
  completedUnits?: number;
  totalUnits?: number;
  message?: string;
};

export type DownloadChangedEvent = {
  entryId: string;
  galleryId: number;
  revision: number;
  state: DownloadState;
  progress?: number;
  attempt?: number;
  errorCode?: string;
  errorMessage?: string;
};

export type SearchRequest = {
  text: string;
  includeTags: string[];
  excludeTags: string[];
  languages: Language[];
  sort: SearchSort;
  pageSize: number;
};

export type GallerySummary = {
  id: GalleryId;
  title: string;
  artist: string;
  group?: string;
  pages: number;
  language: Language;
  tags: string[];
  publishedRank: number;
  popularity: number;
  thumbnailKey?: string;
  thumbnailWidth: number;
  thumbnailHeight: number;
};

export type GalleryPage = {
  page: number;
  totalPages: number;
  items: GallerySummary[];
};

export type SearchSubmission = {
  queryId: string;
  firstPage: GalleryPage;
};

export type GalleryDetail = GallerySummary & {
  related: GallerySummary[];
};

export type DownloadEntry = {
  entryId: string;
  galleryId: GalleryId;
  revision: number;
  state: DownloadState;
  progress?: number;
  attempt?: number;
  errorCode?: string;
  errorMessage?: string;
  reviewKind?: "gallery_duplicate" | "internal_pages";
  reviewId?: string;
};

export type DownloadListRequest = {
  state?: DownloadState;
  query?: string;
  page: number;
  pageSize: number;
};

export type DownloadPage = {
  page: number;
  totalItems: number;
  entries: DownloadEntry[];
};

export type ReconcileIssue = {
  entryId: string;
  code: string;
  message: string;
  recoverable: boolean;
};

export type ReconcileReport = {
  inspectedArtifacts: number;
  verifiedArtifacts: number;
  resumedJobs: number;
  issues: ReconcileIssue[];
};

export type RemovalPlan = {
  planId: string;
  entryIds: string[];
  filesToQuarantine: number;
  bytesToQuarantine: number;
  expiresAt: string;
};

export type OpenResult = { opened: boolean; path?: string };

export type ActivityItem = {
  id: string;
  label: string;
  detail: string;
  severity: "neutral" | "info" | "warning" | "danger" | "success";
  progress?: number;
};
