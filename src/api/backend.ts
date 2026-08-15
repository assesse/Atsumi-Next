import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { GalleryId } from "../core/types";
import type {
  ApiResult,
  DownloadChangedEvent,
  DownloadEntry,
  DownloadListRequest,
  DownloadPage,
  GalleryDetail,
  GalleryPage,
  JobEvent,
  JobRef,
  ReconcileReport,
  SearchRequest,
  SearchSubmission,
  SettingsPatch,
  SettingsSnapshot,
  ThumbnailCompletionEvent,
  ThumbnailInvalidation,
  ThumbnailRequestDto,
  ThumbnailRequestToken,
  ThumbnailWorkerStats,
  WindowPlacement,
  WindowPlacementSnapshot,
} from "./contracts";
import {
  galleryDetailFixture,
  runSearchFixture,
  searchFixturePage,
  searchFixtureQueryId,
  searchRequestValidationError,
  type SearchFixtureResult,
} from "./searchFixture";

export type BackendEventMap = {
  "job:changed": JobEvent;
  "download:changed": DownloadChangedEvent;
  "thumbnail:ready": ThumbnailCompletionEvent;
  "settings:changed": SettingsSnapshot;
  "app:exit-requested": null;
};

export type Unsubscribe = () => void;

export interface BackendClient {
  readonly runtime: "tauri" | "browser-mock";
  settingsGet(): Promise<ApiResult<SettingsSnapshot>>;
  settingsUpdate(patch: SettingsPatch, expectedRevision: number): Promise<ApiResult<SettingsSnapshot>>;
  windowPlacementGet(): Promise<ApiResult<WindowPlacementSnapshot>>;
  windowPlacementUpdate(
    placement: WindowPlacement,
    expectedRevision: number,
  ): Promise<ApiResult<WindowPlacementSnapshot>>;
  searchSubmit(request: SearchRequest): Promise<ApiResult<SearchSubmission>>;
  searchPageGet(queryId: string, page: number): Promise<ApiResult<GalleryPage>>;
  galleryDetailGet(galleryId: GalleryDetail["id"]): Promise<ApiResult<GalleryDetail>>;
  downloadQueueAdd(galleries: GalleryId[], requestId: string): Promise<ApiResult<DownloadEntry[]>>;
  downloadEntriesList(request: DownloadListRequest): Promise<ApiResult<DownloadPage>>;
  downloadRetry(entryIds: string[]): Promise<ApiResult<JobRef[]>>;
  downloadCancel(entryIds: string[]): Promise<ApiResult<DownloadEntry[]>>;
  downloadQuarantine(entryIds: string[], reason: string): Promise<ApiResult<DownloadEntry[]>>;
  downloadQuarantineUndo(entryIds: string[]): Promise<ApiResult<DownloadEntry[]>>;
  downloadActiveCount(): Promise<ApiResult<number>>;
  artifactOpenFirst(entryId: string): Promise<ApiResult<null>>;
  appReconcile(): Promise<ApiResult<ReconcileReport>>;
  thumbnailRequest(request: ThumbnailRequestDto): Promise<ApiResult<ThumbnailRequestToken>>;
  thumbnailCancel(requestId: string): Promise<ApiResult<boolean>>;
  thumbnailReprioritize(requestId: string, priority: ThumbnailRequestDto["priority"]): Promise<ApiResult<boolean>>;
  thumbnailInvalidate(key: ThumbnailRequestDto["key"]): Promise<ApiResult<ThumbnailInvalidation>>;
  thumbnailStats(): Promise<ApiResult<ThumbnailWorkerStats>>;
  appMinimizeToTray(): Promise<ApiResult<null>>;
  appQuit(): Promise<ApiResult<null>>;
  on<K extends keyof BackendEventMap>(event: K, handler: (payload: BackendEventMap[K]) => void): Promise<Unsubscribe>;
}

const defaultSettings: SettingsSnapshot = {
  revision: 0,
  downloadRoot: "",
  maxColumns: 3,
  previewWidth: 220,
  cacheLimitGb: 10,
  concurrentImageRequests: 5,
  requestStartIntervalMs: 25,
};

const defaultPlacement: WindowPlacementSnapshot = {
  revision: 0,
  x: null,
  y: null,
  width: 1280,
  height: 820,
  maximized: false,
};

const ok = <T,>(data: T): ApiResult<T> => ({ ok: true, data });

const conflict = (kind: string): ApiResult<never> => ({
  ok: false,
  error: {
    code: "REVISION_CONFLICT",
    message: `${kind}이(가) 다른 창에서 변경되었습니다.`,
    retryable: false,
    action: "review",
  },
});

const validationError = (field: string, reason: string): ApiResult<never> => ({
  ok: false,
  error: {
    code: "VALIDATION_ERROR",
    message: `${field}: ${reason}`,
    retryable: false,
    action: "none",
    details: { field, reason },
  },
});

const notFoundError = (
  code: string,
  message: string,
  details?: Record<string, unknown>,
): ApiResult<never> => ({
  ok: false,
  error: { code, message, retryable: false, action: "none", ...(details ? { details } : {}) },
});

const validateIntegerRange = (
  value: number,
  field: string,
  minimum: number,
  maximum: number,
): ApiResult<never> | null => {
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    return validationError(field, `${minimum} 이상 ${maximum} 이하의 정수여야 합니다.`);
  }
  return null;
};

const activeDownloadStates: ReadonlySet<DownloadEntry["state"]> = new Set([
  "queued",
  "resolving_metadata",
  "downloading",
  "hashing",
  "verifying",
  "retry_wait",
]);

const cancellableDownloadStates: ReadonlySet<DownloadEntry["state"]> = new Set([
  ...activeDownloadStates,
  "review_required",
  "interrupted",
  "failed",
  "cancelled",
]);

const cloneDownloadEntry = (entry: DownloadEntry): DownloadEntry => ({ ...entry });

const normalizedGallerySet = (galleries: GalleryId[]): GalleryId[] =>
  [...new Set(galleries)].sort((left, right) => left - right);

const gallerySetKey = (galleries: GalleryId[]): string => galleries.join(",");

type Handler<K extends keyof BackendEventMap> = (payload: BackendEventMap[K]) => void;

class BrowserMockBackend implements BackendClient {
  readonly runtime = "browser-mock" as const;
  private settings = { ...defaultSettings };
  private placement = { ...defaultPlacement };
  private listeners: { [K in keyof BackendEventMap]: Set<Handler<K>> } = {
    "job:changed": new Set(),
    "download:changed": new Set(),
    "thumbnail:ready": new Set(),
    "settings:changed": new Set(),
    "app:exit-requested": new Set(),
  };
  private searchQueries = new Map<string, SearchFixtureResult>();
  private downloadEntries = new Map<string, DownloadEntry>();
  private activeDownloadEntryByGallery = new Map<number, string>();
  private downloadQueueRequests = new Map<string, { gallerySetKey: string; entries: DownloadEntry[] }>();
  private nextDownloadEntryId = 1;
  private nextThumbnailRequestId = 1;
  private pendingThumbnailRequests = new Map<string, ThumbnailRequestDto>();
  private thumbnailRequestsTotal = 0;

  async settingsGet(): Promise<ApiResult<SettingsSnapshot>> {
    return ok({ ...this.settings });
  }

  async settingsUpdate(patch: SettingsPatch, expectedRevision: number): Promise<ApiResult<SettingsSnapshot>> {
    if (expectedRevision !== this.settings.revision) return conflict("설정");
    const next = { ...this.settings, ...patch };
    const invalid =
      validateIntegerRange(next.maxColumns, "maxColumns", 1, 4) ??
      validateIntegerRange(next.previewWidth, "previewWidth", 160, 360) ??
      validateIntegerRange(next.cacheLimitGb, "cacheLimitGb", 1, 30) ??
      validateIntegerRange(next.concurrentImageRequests, "concurrentImageRequests", 1, 30) ??
      validateIntegerRange(next.requestStartIntervalMs, "requestStartIntervalMs", 0, 5_000);
    if (invalid) return invalid;
    this.settings = { ...next, revision: this.settings.revision + 1 };
    this.emit("settings:changed", { ...this.settings });
    return ok({ ...this.settings });
  }

  async windowPlacementGet(): Promise<ApiResult<WindowPlacementSnapshot>> {
    return ok({ ...this.placement });
  }

  async windowPlacementUpdate(
    placement: WindowPlacement,
    expectedRevision: number,
  ): Promise<ApiResult<WindowPlacementSnapshot>> {
    if (expectedRevision !== this.placement.revision) return conflict("창 위치");
    this.placement = { ...placement, revision: this.placement.revision + 1 };
    return ok({ ...this.placement });
  }

  async searchSubmit(request: SearchRequest): Promise<ApiResult<SearchSubmission>> {
    const invalid = searchRequestValidationError(request);
    if (invalid) return validationError(invalid.field, invalid.reason);

    const queryId = searchFixtureQueryId(request);
    const fixture = runSearchFixture(request);
    this.searchQueries.set(queryId, fixture);
    const firstPage = searchFixturePage(fixture, 1);
    return ok({ queryId, firstPage });
  }

  async searchPageGet(queryId: string, page: number): Promise<ApiResult<GalleryPage>> {
    const normalizedQueryId = queryId.trim();
    if (!normalizedQueryId) return validationError("queryId", "must not be empty");
    if (new TextEncoder().encode(normalizedQueryId).length > 200) {
      return validationError("queryId", "must be at most 200 bytes");
    }
    if (!Number.isInteger(page) || page < 1) return validationError("page", "must be one-based");
    const fixture = this.searchQueries.get(normalizedQueryId);
    if (!fixture) {
      return notFoundError(
        "QUERY_NOT_FOUND",
        "The search query is no longer available; submit it again",
        { queryId: normalizedQueryId },
      );
    }
    const pageResult = searchFixturePage(fixture, page);
    if ((pageResult.totalPages === 0 && page !== 1) || (pageResult.totalPages > 0 && page > pageResult.totalPages)) {
      return validationError("page", "must not exceed the available search pages");
    }
    return ok(pageResult);
  }

  async galleryDetailGet(galleryId: GalleryDetail["id"]): Promise<ApiResult<GalleryDetail>> {
    if (!Number.isInteger(galleryId) || galleryId <= 0) {
      return validationError("galleryId", "must be a positive integer");
    }
    const detail = galleryDetailFixture(galleryId);
    return detail
      ? ok(detail)
      : notFoundError(
        "SOURCE_NOT_FOUND",
        "The gallery could not be found in the current source",
        { galleryId },
      );
  }

  async downloadQueueAdd(
    galleries: GalleryId[],
    requestId: string,
  ): Promise<ApiResult<DownloadEntry[]>> {
    const normalizedRequestId = requestId.trim();
    if (!normalizedRequestId) return validationError("requestId", "must not be empty");
    if (new TextEncoder().encode(normalizedRequestId).length > 200) {
      return validationError("requestId", "must be at most 200 bytes");
    }
    if (!galleries.length) return validationError("galleries", "must not be empty");
    if (galleries.length > 200) {
      return validationError("galleries", "must contain at most 200 IDs");
    }
    const invalidGallery = galleries.find((galleryId) => !Number.isInteger(galleryId) || galleryId <= 0);
    if (invalidGallery !== undefined) {
      return validationError("galleries", "gallery IDs must be positive integers");
    }

    const normalizedGalleries = normalizedGallerySet(galleries);
    const normalizedSetKey = gallerySetKey(normalizedGalleries);
    const replay = this.downloadQueueRequests.get(normalizedRequestId);
    if (replay) {
      if (replay.gallerySetKey !== normalizedSetKey) {
        return {
          ok: false,
          error: {
            code: "IDEMPOTENCY_CONFLICT",
            message: "The request ID was already used for a different gallery set",
            retryable: false,
            action: "review",
            details: { requestId: normalizedRequestId },
          },
        };
      }
      return ok(replay.entries.map(cloneDownloadEntry));
    }

    const entries = normalizedGalleries.map((galleryId) => {
      const activeEntryId = this.activeDownloadEntryByGallery.get(galleryId);
      const activeEntry = activeEntryId === undefined ? undefined : this.downloadEntries.get(activeEntryId);
      if (activeEntry && activeDownloadStates.has(activeEntry.state)) return cloneDownloadEntry(activeEntry);
      if (activeEntryId !== undefined) this.activeDownloadEntryByGallery.delete(galleryId);

      const entry: DownloadEntry = {
        entryId: `browser-entry-${galleryId}-${this.nextDownloadEntryId++}`,
        galleryId,
        revision: 0,
        state: "queued",
        progress: 0,
        attempt: 1,
      };
      this.downloadEntries.set(entry.entryId, entry);
      this.activeDownloadEntryByGallery.set(galleryId, entry.entryId);
      if (this.listeners["download:changed"].size > 0) this.runFixtureDownload(entry.entryId, 1);
      return cloneDownloadEntry(entry);
    });
    this.downloadQueueRequests.set(normalizedRequestId, {
      gallerySetKey: normalizedSetKey,
      entries: entries.map(cloneDownloadEntry),
    });
    return ok(entries.map(cloneDownloadEntry));
  }

  async downloadEntriesList(request: DownloadListRequest): Promise<ApiResult<DownloadPage>> {
    if (!Number.isInteger(request.page) || request.page < 1) {
      return validationError("page", "must be one-based");
    }
    if (!Number.isInteger(request.pageSize) || request.pageSize < 1 || request.pageSize > 200) {
      return validationError("pageSize", "must be between 1 and 200");
    }
    const query = request.query?.trim().toLowerCase() ?? "";
    if (new TextEncoder().encode(query).length > 500) {
      return validationError("query", "must be at most 500 bytes");
    }
    const entries = [...this.downloadEntries.values()]
      .filter((entry) => request.state === undefined || entry.state === request.state)
      .filter((entry) => {
        if (!query) return true;
        return `${entry.entryId} ${entry.galleryId}`.toLowerCase().includes(query);
      })
      .sort((left, right) => left.galleryId - right.galleryId || left.entryId.localeCompare(right.entryId));
    const offset = (request.page - 1) * request.pageSize;
    return ok({
      page: request.page,
      totalItems: entries.length,
      entries: entries.slice(offset, offset + request.pageSize).map(cloneDownloadEntry),
    });
  }

  async downloadRetry(entryIds: string[]): Promise<ApiResult<JobRef[]>> {
    const normalized = [...new Set(entryIds.map((entryId) => entryId.trim()))];
    if (!normalized.length || normalized.some((entryId) => !entryId)) {
      return validationError("entryIds", "must contain at least one non-empty entry ID");
    }
    if (normalized.length > 200) return validationError("entryIds", "must contain at most 200 IDs");
    const entries = normalized.map((entryId) => this.downloadEntries.get(entryId));
    const missingIndex = entries.findIndex((entry) => entry === undefined);
    if (missingIndex >= 0) {
      return notFoundError("DOWNLOAD_ENTRY_NOT_FOUND", "The download entry does not exist", {
        entryId: normalized[missingIndex],
      });
    }
    const invalid = entries.find((entry) => entry && !activeDownloadStates.has(entry.state)
      && !["failed", "interrupted", "cancelled"].includes(entry.state));
    if (invalid) {
      return {
        ok: false,
        error: {
          code: "INVALID_DOWNLOAD_STATE",
          message: `Download entry ${invalid.entryId} cannot be retried from ${invalid.state}`,
          retryable: false,
          action: "review",
          details: { entryId: invalid.entryId, state: invalid.state, operation: "retry" },
        },
      };
    }

    return ok(entries.map((entry) => {
      const current = entry!;
      const reused = activeDownloadStates.has(current.state);
      if (!reused) {
        const attempt = (current.attempt ?? 1) + 1;
        const next: DownloadEntry = {
          ...current,
          revision: current.revision + 1,
          state: "queued",
          progress: 0,
          attempt,
          errorCode: undefined,
          errorMessage: undefined,
        };
        this.downloadEntries.set(next.entryId, next);
        this.activeDownloadEntryByGallery.set(next.galleryId, next.entryId);
        this.emit("download:changed", cloneDownloadEntry(next));
        if (this.listeners["download:changed"].size > 0) this.runFixtureDownload(next.entryId, attempt);
      }
      return { jobId: `browser-fixture-${current.entryId}`, reused };
    }));
  }

  async downloadCancel(entryIds: string[]): Promise<ApiResult<DownloadEntry[]>> {
    const normalized = [...new Set(entryIds.map((entryId) => entryId.trim()))];
    if (!normalized.length || normalized.some((entryId) => !entryId)) {
      return validationError("entryIds", "must contain at least one non-empty entry ID");
    }
    if (normalized.length > 200) return validationError("entryIds", "must contain at most 200 IDs");
    const entries = normalized.map((entryId) => this.downloadEntries.get(entryId));
    const missingIndex = entries.findIndex((entry) => entry === undefined);
    if (missingIndex >= 0) {
      return notFoundError("DOWNLOAD_ENTRY_NOT_FOUND", "The download entry does not exist", {
        entryId: normalized[missingIndex],
      });
    }
    const invalid = entries.find((entry) => entry && !cancellableDownloadStates.has(entry.state));
    if (invalid) {
      return {
        ok: false,
        error: {
          code: "INVALID_DOWNLOAD_STATE",
          message: `Download entry ${invalid.entryId} cannot be cancelled from ${invalid.state}`,
          retryable: false,
          action: "review",
          details: { entryId: invalid.entryId, state: invalid.state, operation: "cancel" },
        },
      };
    }

    const cancelled = entries.map((entry) => {
      const current = entry!;
      if (current.state === "cancelled") return cloneDownloadEntry(current);
      const preserveFailure = current.state === "failed" || current.state === "interrupted";
      const next: DownloadEntry = {
        ...current,
        revision: current.revision + 1,
        state: "cancelled",
        ...(preserveFailure ? {} : { errorCode: undefined, errorMessage: undefined }),
      };
      this.downloadEntries.set(next.entryId, next);
      if (this.activeDownloadEntryByGallery.get(next.galleryId) === next.entryId) {
        this.activeDownloadEntryByGallery.delete(next.galleryId);
      }
      this.emit("download:changed", cloneDownloadEntry(next));
      return cloneDownloadEntry(next);
    });
    return ok(cancelled);
  }

  async downloadQuarantine(): Promise<ApiResult<DownloadEntry[]>> {
    return {
      ok: false,
      error: {
        code: "ARTIFACT_UNAVAILABLE_IN_BROWSER",
        message: "브라우저 fixture에는 격리할 실제 다운로드 파일이 없습니다.",
        retryable: false,
        action: "none",
      },
    };
  }

  async downloadQuarantineUndo(): Promise<ApiResult<DownloadEntry[]>> {
    return {
      ok: false,
      error: {
        code: "ARTIFACT_UNAVAILABLE_IN_BROWSER",
        message: "브라우저 fixture에는 복원할 실제 다운로드 파일이 없습니다.",
        retryable: false,
        action: "none",
      },
    };
  }

  async thumbnailRequest(request: ThumbnailRequestDto): Promise<ApiResult<ThumbnailRequestToken>> {
    if (!Number.isInteger(request.key.galleryId) || request.key.galleryId <= 0) {
      return validationError("key.galleryId", "must be a positive integer");
    }
    if (request.key.kind === "galleryPage" && (!Number.isInteger(request.key.sourcePage) || request.key.sourcePage < 1)) {
      return validationError("key.sourcePage", "must be one-based");
    }
    const requestId = `browser-thumbnail-${this.nextThumbnailRequestId++}`;
    this.thumbnailRequestsTotal += 1;
    this.pendingThumbnailRequests.set(requestId, request);
    const token: ThumbnailRequestToken = { requestId, key: request.key };
    queueMicrotask(() => {
      if (!this.pendingThumbnailRequests.delete(requestId)) return;
      const label = request.key.kind === "galleryCover"
        ? `G${request.key.galleryId} · COVER`
        : `G${request.key.galleryId} · PAGE ${request.key.sourcePage}`;
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512"><rect width="512" height="512" fill="#49656b"/><text x="28" y="470" fill="white" font-family="Segoe UI" font-size="24">${label}</text></svg>`;
      this.emit("thumbnail:ready", {
        ...token,
        outcome: {
          status: "ready",
          delivery: {
            key: request.key,
            cacheStatus: "resolved",
            thumbnail: {
              contentType: "image/svg+xml",
              bytes: [...new TextEncoder().encode(svg)],
              width: 512,
              height: 512,
              sourceRevision: "browser-fixture-v1",
            },
          },
        },
      });
    });
    return ok(token);
  }

  async thumbnailCancel(requestId: string): Promise<ApiResult<boolean>> {
    return ok(this.pendingThumbnailRequests.delete(requestId.trim()));
  }

  async thumbnailReprioritize(
    requestId: string,
    priority: ThumbnailRequestDto["priority"],
  ): Promise<ApiResult<boolean>> {
    const current = this.pendingThumbnailRequests.get(requestId.trim());
    if (!current) return ok(false);
    this.pendingThumbnailRequests.set(requestId.trim(), { ...current, priority });
    return ok(true);
  }

  async thumbnailInvalidate(key: ThumbnailRequestDto["key"]): Promise<ApiResult<ThumbnailInvalidation>> {
    return ok({ key, successCacheRemoved: false, negativeCacheRemoved: false });
  }

  async thumbnailStats(): Promise<ApiResult<ThumbnailWorkerStats>> {
    return ok({
      workerCount: this.settings.concurrentImageRequests,
      concurrencyLimit: this.settings.concurrentImageRequests,
      requestStartIntervalMs: this.settings.requestStartIntervalMs,
      activeWorkers: 0,
      queuedKeys: this.pendingThumbnailRequests.size,
      inFlightKeys: this.pendingThumbnailRequests.size,
      subscriberCount: this.pendingThumbnailRequests.size,
      successCacheEntries: 0,
      successCacheBytes: 0,
      negativeCacheEntries: 0,
      requestsTotal: this.thumbnailRequestsTotal,
      successCacheHits: 0,
      negativeCacheHits: 0,
      joinedInFlight: 0,
      resolvedSuccess: this.thumbnailRequestsTotal - this.pendingThumbnailRequests.size,
      resolvedFailure: 0,
      cancelledSubscribers: 0,
      cancelledWork: 0,
    });
  }

  async appMinimizeToTray(): Promise<ApiResult<null>> {
    return ok(null);
  }

  async downloadActiveCount(): Promise<ApiResult<number>> {
    return ok([...this.downloadEntries.values()].filter((entry) => activeDownloadStates.has(entry.state)).length);
  }

  async artifactOpenFirst(): Promise<ApiResult<null>> {
    return {
      ok: false,
      error: {
        code: "ARTIFACT_UNAVAILABLE_IN_BROWSER",
        message: "브라우저 fixture에는 실제 다운로드 파일이 없습니다.",
        retryable: false,
        action: "none",
      },
    };
  }

  async appReconcile(): Promise<ApiResult<ReconcileReport>> {
    return ok({
      inspectedArtifacts: 0,
      verifiedArtifacts: 0,
      resumedJobs: 0,
      issues: [],
    });
  }

  async appQuit(): Promise<ApiResult<null>> {
    return ok(null);
  }

  async on<K extends keyof BackendEventMap>(
    event: K,
    handler: (payload: BackendEventMap[K]) => void,
  ): Promise<Unsubscribe> {
    const handlers = this.listeners[event] as Set<(payload: BackendEventMap[K]) => void>;
    handlers.add(handler);
    return () => handlers.delete(handler);
  }

  private emit<K extends keyof BackendEventMap>(event: K, payload: BackendEventMap[K]): void {
    const handlers = this.listeners[event] as Set<(payload: BackendEventMap[K]) => void>;
    handlers.forEach((handler) => handler(payload));
  }

  private runFixtureDownload(entryId: string, workerAttempt: number): void {
    const steps: Array<{
      delay: number;
      expectedState: DownloadEntry["state"];
      state: DownloadEntry["state"];
      message: string;
    }> = [
      {
        delay: 75,
        expectedState: "queued",
        state: "resolving_metadata",
        message: "저장 fixture의 대기열 요청을 확인하고 있습니다.",
      },
      {
        delay: 225,
        expectedState: "resolving_metadata",
        state: "interrupted",
        message: "실제 원격 artifact 다운로드 기반은 아직 구현되지 않았습니다.",
      },
    ];

    for (const step of steps) {
      window.setTimeout(() => {
        const current = this.downloadEntries.get(entryId);
        if (
          !current
          || current.state !== step.expectedState
          || current.attempt !== workerAttempt
        ) return;
        const failed = step.state === "interrupted";
        const next: DownloadEntry = {
          ...current,
          revision: current.revision + 1,
          state: step.state,
          progress: 0,
          attempt: workerAttempt,
          errorCode: failed ? "DOWNLOAD_FOUNDATION_UNAVAILABLE" : undefined,
          errorMessage: failed ? step.message : undefined,
        };
        this.downloadEntries.set(entryId, next);
        if (step.state === "interrupted" && this.activeDownloadEntryByGallery.get(current.galleryId) === entryId) {
          this.activeDownloadEntryByGallery.delete(current.galleryId);
        }
        this.emit("job:changed", {
          jobId: `browser-fixture-${entryId}`,
          galleryId: current.galleryId,
          revision: next.revision,
          state: next.state,
          completedUnits: 0,
          totalUnits: 1,
          message: step.message,
        });
        this.emit("download:changed", cloneDownloadEntry(next));
      }, step.delay);
    }
  }

}

class TauriBackend implements BackendClient {
  readonly runtime = "tauri" as const;

  settingsGet(): Promise<ApiResult<SettingsSnapshot>> {
    return invoke("settings_get");
  }

  settingsUpdate(patch: SettingsPatch, expectedRevision: number): Promise<ApiResult<SettingsSnapshot>> {
    return invoke("settings_update", { patch, expectedRevision });
  }

  windowPlacementGet(): Promise<ApiResult<WindowPlacementSnapshot>> {
    return invoke("window_placement_get");
  }

  windowPlacementUpdate(
    placement: WindowPlacement,
    expectedRevision: number,
  ): Promise<ApiResult<WindowPlacementSnapshot>> {
    return invoke("window_placement_update", { placement, expectedRevision });
  }

  searchSubmit(request: SearchRequest): Promise<ApiResult<SearchSubmission>> {
    return invoke("search_submit", { request });
  }

  searchPageGet(queryId: string, page: number): Promise<ApiResult<GalleryPage>> {
    return invoke("search_page_get", { queryId, page });
  }

  galleryDetailGet(galleryId: GalleryDetail["id"]): Promise<ApiResult<GalleryDetail>> {
    return invoke("gallery_detail_get", { galleryId });
  }

  downloadQueueAdd(
    galleries: GalleryId[],
    requestId: string,
  ): Promise<ApiResult<DownloadEntry[]>> {
    return invoke("download_queue_add", { galleries, requestId });
  }

  downloadEntriesList(request: DownloadListRequest): Promise<ApiResult<DownloadPage>> {
    return invoke("download_entries_list", { request });
  }

  downloadRetry(entryIds: string[]): Promise<ApiResult<JobRef[]>> {
    return invoke("download_retry", { entryIds });
  }

  downloadCancel(entryIds: string[]): Promise<ApiResult<DownloadEntry[]>> {
    return invoke("download_cancel", { entryIds });
  }

  downloadQuarantine(entryIds: string[], reason: string): Promise<ApiResult<DownloadEntry[]>> {
    return invoke("download_quarantine", { entryIds, reason });
  }

  downloadQuarantineUndo(entryIds: string[]): Promise<ApiResult<DownloadEntry[]>> {
    return invoke("download_quarantine_undo", { entryIds });
  }

  downloadActiveCount(): Promise<ApiResult<number>> {
    return invoke("download_active_count");
  }

  artifactOpenFirst(entryId: string): Promise<ApiResult<null>> {
    return invoke("artifact_open_first", { entryId });
  }

  appReconcile(): Promise<ApiResult<ReconcileReport>> {
    return invoke("app_reconcile");
  }

  thumbnailRequest(request: ThumbnailRequestDto): Promise<ApiResult<ThumbnailRequestToken>> {
    return invoke("thumbnail_request", { request });
  }

  thumbnailCancel(requestId: string): Promise<ApiResult<boolean>> {
    return invoke("thumbnail_cancel", { requestId });
  }

  thumbnailReprioritize(
    requestId: string,
    priority: ThumbnailRequestDto["priority"],
  ): Promise<ApiResult<boolean>> {
    return invoke("thumbnail_reprioritize", { requestId, priority });
  }

  thumbnailInvalidate(key: ThumbnailRequestDto["key"]): Promise<ApiResult<ThumbnailInvalidation>> {
    return invoke("thumbnail_invalidate", { key });
  }

  thumbnailStats(): Promise<ApiResult<ThumbnailWorkerStats>> {
    return invoke("thumbnail_stats");
  }

  appMinimizeToTray(): Promise<ApiResult<null>> {
    return invoke("app_minimize_to_tray");
  }

  appQuit(): Promise<ApiResult<null>> {
    return invoke("app_quit");
  }

  async on<K extends keyof BackendEventMap>(
    event: K,
    handler: (payload: BackendEventMap[K]) => void,
  ): Promise<Unsubscribe> {
    const unlisten: UnlistenFn = await listen<BackendEventMap[K]>(event, ({ payload }) => handler(payload));
    return unlisten;
  }
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export const backend: BackendClient = window.__TAURI_INTERNALS__
  ? new TauriBackend()
  : new BrowserMockBackend();
