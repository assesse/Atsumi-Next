import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react";
import { backend } from "./api/backend";
import type {
  AutoFindRun,
  AutoFindSnapshot,
  DownloadChangedEvent,
  DuplicateDecisionRequest,
  DuplicateReview,
  DuplicateScanRun,
  DuplicateSnapshot,
  FavoriteKey,
  FavoriteNamespace,
  FavoriteRecord,
  InternalDuplicateReview,
  InternalDuplicateSnapshot,
  InternalRemovalPlan,
  InternalRemovalPlanRequest,
  InternalScanRun,
  SearchHistoryEntry,
  SearchRequest,
  SettingsPatch,
  MaintenanceAction,
  MaintenanceResult,
  ApiResult,
} from "./api/contracts";
import { ActivityDrawer } from "./components/ActivityDrawer";
import { DetailWorkspace } from "./components/DetailWorkspace";
import { DuplicateReviewDialog } from "./components/DuplicateReviewDialog";
import { InternalDuplicateDialog } from "./components/InternalDuplicateDialog";
import { ExitConfirmDialog } from "./components/ExitConfirmDialog";
import { FluentIcon } from "./components/FluentIcon";
import { GalleryCard } from "./components/GalleryCard";
import { GalleryGrid } from "./components/GalleryGrid";
import { GalleryGridSkeleton } from "./components/GalleryGridSkeleton";
import { SelectionToolbar } from "./components/SelectionToolbar";
import { SettingsDialog } from "./components/SettingsDialog";
import { SideRail } from "./components/SideRail";
import { ViewHeader, type SearchSuggestion } from "./components/ViewHeader";
import { retryableDownloadStates, type DownloadState, type Gallery, type GalleryId, type SearchSort, type ViewId } from "./core/types";
import { useSettings } from "./hooks/useSettings";
import { useWindowPlacement } from "./hooks/useWindowPlacement";
import { resolveGalleryColumns } from "./layout/galleryColumns";
import { buildSearchSuggestionCatalog } from "./search/searchSuggestions";
import { metadataSearchToken, searchTokenKind } from "./search/searchTokens";
import { applyDownloadChanged } from "./state/downloadProjection";
import {
  duplicateEventNeedsSnapshot,
  duplicateRunIsNewer,
  mergeHydratedDuplicateSnapshot,
  validDuplicateRun,
} from "./state/duplicateProjection";
import { mergeDownloadEntries, mergeGalleryDetail, mergeGalleryPage } from "./state/galleryProjection";
import { galleryQueryReducer, initialGalleryQueryState } from "./state/galleryQuery";
import { ExplorePageSession } from "./state/explorePageSession";
import { visibleGalleries } from "./state/selectors";
import { initialUiState, uiReducer } from "./state/uiState";
import { useThumbnailClient } from "./thumbnail";

const viewConfig: Record<ViewId, { eyebrow: string; title: string }> = {
  explore: { eyebrow: "EXPLORE", title: "갤러리 탐색" },
  "auto-find": { eyebrow: "AUTO FIND", title: "즐겨찾기 작가 자동 탐색" },
  downloads: { eyebrow: "DOWNLOADS", title: "다운로드 목록" },
};

const sortOptions: Array<{ value: SearchSort; label: string }> = [
  { value: "recent", label: "최신순" },
  { value: "popular_today", label: "인기순 · 오늘" },
  { value: "popular_week", label: "인기순 · 이번 주" },
  { value: "popular_month", label: "인기순 · 이번 달" },
  { value: "popular_year", label: "인기순 · 올해" },
  { value: "random", label: "무작위" },
];

const previewFolderNameTemplate = (template: string) => backend.folderNameTemplatePreview(template);

const activeDownloadStates: ReadonlySet<DownloadState> = new Set([
  "queued",
  "resolving_metadata",
  "downloading",
  "hashing",
  "verifying",
  "retry_wait",
]);

type Toast = { id: number; message: string } | null;

const normalizeMetadataToken = (value: string): string => value.trim().toLocaleLowerCase();

const favoriteToken = (favorite: Pick<FavoriteRecord, "namespace" | "value">): string =>
  favorite.namespace === "tag"
    ? normalizeMetadataToken(favorite.value)
    : `${favorite.namespace}:${normalizeMetadataToken(favorite.value)}`;

const favoriteKeyFromToken = (token: string): FavoriteKey => {
  const normalized = normalizeMetadataToken(token);
  const separator = normalized.indexOf(":");
  const possibleNamespace = separator > 0 ? normalized.slice(0, separator) : "";
  const namespaces: ReadonlySet<string> = new Set(["artist", "group", "series", "character"]);
  if (separator > 0 && namespaces.has(possibleNamespace)) {
    return {
      namespace: possibleNamespace as Exclude<FavoriteNamespace, "tag">,
      value: normalized.slice(separator + 1),
    };
  }
  return { namespace: "tag", value: normalized };
};

const autoFindStatusLabel = (loading: boolean, error: string | null, run?: AutoFindRun): string => {
  if (loading) return "저장된 자동 탐색 결과를 불러오는 중";
  if (error) return `자동 탐색 오류 · ${error}`;
  if (!run) return "아직 실행한 자동 탐색이 없습니다.";
  if (run.state === "running") {
    return `탐색 중 · 작가 ${run.completedFavorites}/${run.totalFavorites} · 후보 ${run.candidatesFound}개`;
  }
  if (run.state === "failed") return `탐색 실패 · ${run.errorMessage ?? run.errorCode ?? "원인을 확인해 주세요."}`;
  if (run.state === "cancelled") return `탐색 취소됨 · 후보 ${run.candidatesFound}개 보존`;
  return `탐색 완료 · 작가 ${run.completedFavorites}/${run.totalFavorites} · 후보 ${run.candidatesFound}개`;
};

const duplicateStatusLabel = (loading: boolean, error: string | null, run?: DuplicateScanRun): string => {
  if (loading) return "저장된 작품 중복 검사 결과를 불러오는 중";
  if (error) return `작품 중복 검사 오류 · ${error}`;
  if (!run) return "아직 실행한 작품 중복 검사가 없습니다.";
  if (run.state === "running") {
    return `중복 검사 중 · 아티팩트 ${run.hashedArtifacts}/${run.totalArtifacts} · 비교 ${run.comparedPairs}/${run.totalPairs} · 후보 ${run.candidatesFound}개`;
  }
  if (run.state === "failed") return `중복 검사 실패 · ${run.errorMessage ?? run.errorCode ?? "원인을 확인해 주세요."}`;
  if (run.state === "cancelled") return `중복 검사 취소됨 · 비교 ${run.comparedPairs}/${run.totalPairs} · 기존 후보 보존`;
  return `중복 검사 완료 · 비교 ${run.comparedPairs}/${run.totalPairs} · 후보 ${run.candidatesFound}개`;
};

const internalStatusLabel = (loading: boolean, error: string | null, run?: InternalScanRun): string => {
  if (loading) return "저장된 내부 중복 결과를 불러오는 중";
  if (error) return `내부 중복 오류 · ${error}`;
  if (!run) return "내부 중복 검사를 아직 실행하지 않았습니다.";
  if (run.state === "running") return `내부 중복 검사 중 · 앨범 ${run.scannedArtifacts}/${run.totalArtifacts} · 페이지 ${run.totalPages} · 행 ${run.groupsFound}`;
  if (run.state === "failed") return `내부 중복 검사 실패 · ${run.errorMessage ?? run.errorCode ?? "원인을 확인해 주세요."}`;
  if (run.state === "cancelled") return `내부 중복 검사 취소됨 · 기존 검토 결과 보존`;
  return `내부 중복 검사 완료 · 앨범 ${run.scannedArtifacts}개 · 검토 행 ${run.groupsFound}개`;
};

export default function App() {
  const thumbnailClient = useThumbnailClient();
  const [ui, dispatch] = useReducer(uiReducer, initialUiState);
  const [query, dispatchQuery] = useReducer(galleryQueryReducer, initialGalleryQueryState);
  const [galleries, setGalleries] = useState<ReadonlyMap<GalleryId, Gallery>>(() => new Map());
  const [exploreIds, setExploreIds] = useState<GalleryId[]>([]);
  const [downloadIds, setDownloadIds] = useState<GalleryId[]>([]);
  const [downloadsLoading, setDownloadsLoading] = useState(true);
  const [downloadsError, setDownloadsError] = useState<string | null>(null);
  const [searchRefresh, setSearchRefresh] = useState(0);
  const [exploreSearchOverride, setExploreSearchOverride] = useState<SearchRequest | null>(null);
  const [downloadsRefresh, setDownloadsRefresh] = useState(0);
  const [favoriteMetadata, setFavoriteMetadata] = useState<ReadonlySet<string>>(() => new Set());
  const [favoriteRecords, setFavoriteRecords] = useState<FavoriteRecord[]>([]);
  const [searchHistory, setSearchHistory] = useState<SearchHistoryEntry[]>([]);
  const [autoFindSnapshot, setAutoFindSnapshot] = useState<AutoFindSnapshot>({ candidates: [], cutoffEvidence: [], truncations: [] });
  const [autoFindIds, setAutoFindIds] = useState<GalleryId[]>([]);
  const [autoFindLoading, setAutoFindLoading] = useState(true);
  const [autoFindError, setAutoFindError] = useState<string | null>(null);
  const [autoFindPending, setAutoFindPending] = useState(false);
  const [duplicateSnapshot, setDuplicateSnapshot] = useState<DuplicateSnapshot | null>(null);
  const [duplicateRun, setDuplicateRun] = useState<DuplicateScanRun | undefined>(undefined);
  const [duplicateLoading, setDuplicateLoading] = useState(true);
  const [duplicateError, setDuplicateError] = useState<string | null>(null);
  const [duplicatePending, setDuplicatePending] = useState(false);
  const [duplicateReviewCandidateId, setDuplicateReviewCandidateId] = useState<string | null>(null);
  const [duplicateReview, setDuplicateReview] = useState<DuplicateReview | null>(null);
  const [duplicateReviewLoading, setDuplicateReviewLoading] = useState(false);
  const [duplicateReviewError, setDuplicateReviewError] = useState<string | null>(null);
  const [duplicateDecisionPending, setDuplicateDecisionPending] = useState(false);
  const [internalSnapshot, setInternalSnapshot] = useState<InternalDuplicateSnapshot>({ groups: [], quarantineRecords: [] });
  const [internalRun, setInternalRun] = useState<InternalScanRun | undefined>(undefined);
  const [internalLoading, setInternalLoading] = useState(true);
  const [internalError, setInternalError] = useState<string | null>(null);
  const [internalPending, setInternalPending] = useState(false);
  const [internalReviewEntryId, setInternalReviewEntryId] = useState<string | null>(null);
  const [internalReview, setInternalReview] = useState<InternalDuplicateReview | null>(null);
  const [internalReviewLoading, setInternalReviewLoading] = useState(false);
  const [internalReviewError, setInternalReviewError] = useState<string | null>(null);
  const [internalPlan, setInternalPlan] = useState<InternalRemovalPlan | null>(null);
  const [toast, setToast] = useState<Toast>(null);
  const [reconcilingArtifacts, setReconcilingArtifacts] = useState(false);
  const [settingsPreview, setSettingsPreview] = useState<{ maxColumns: number; previewWidth: number } | null>(null);
  const [exitActiveDownloads, setExitActiveDownloads] = useState<number | null>(null);
  const [exitStatusError, setExitStatusError] = useState(false);
  const [exitActionPending, setExitActionPending] = useState(false);
  const [pendingDownloadEntries, setPendingDownloadEntries] = useState<ReadonlySet<string>>(() => new Set());
  const exitConfirmOpenRef = useRef(false);
  const exitActionPendingRef = useRef(false);
  const toastTimer = useRef<number | undefined>(undefined);
  const searchToken = useRef(0);
  const autoFindHydrationToken = useRef(0);
  const duplicateHydrationToken = useRef(0);
  const duplicateReviewToken = useRef(0);
  const duplicateRunRef = useRef<DuplicateScanRun | undefined>(undefined);
  const duplicateSnapshotRef = useRef<DuplicateSnapshot | null>(null);
  const duplicatePendingRef = useRef(false);
  const duplicateDecisionPendingRef = useRef(false);
  const internalHydrationToken = useRef(0);
  const internalReviewToken = useRef(0);
  const internalRunRef = useRef<InternalScanRun | undefined>(undefined);
  const internalPendingRef = useRef(false);
  const downloadHydrationToken = useRef(0);
  const queueRequestSequence = useRef(0);
  const pendingDownloadEntriesRef = useRef(new Set<string>());
  const pendingFavoriteTokens = useRef(new Set<string>());
  const hydratedDetails = useRef(new Set<GalleryId>());
  const galleriesRef = useRef(galleries);
  const visibleIdsRef = useRef<GalleryId[]>([]);
  const activityOpener = useRef<HTMLElement | null>(null);
  const galleryViewport = useRef<HTMLElement>(null);
  const explorePageSession = useRef<ExplorePageSession | null>(null);
  const exploreNavigationToken = useRef(0);
  const exploreRestoreFrame = useRef<number | null>(null);
  if (!explorePageSession.current) {
    explorePageSession.current = new ExplorePageSession({
      fetchPage: (queryId, page, requestId) => backend.searchPageGet(queryId, page, requestId),
      cancelPage: (requestId) => backend.searchPageCancel(requestId),
      warmPage: (page) => {
        const releases = page.items.map((item, index) => thumbnailClient.subscribe({
          key: {
            kind: "gallery-cover" as const,
            galleryId: item.id,
            ...(item.thumbnailKey?.trim() ? { sourceKey: item.thumbnailKey.trim() } : {}),
            fallback: { kind: "fixture-sheet-cell" as const, index: index % 6 },
          },
          consumer: "explore",
          priority: "prefetch",
        }, () => undefined));
        return () => releases.forEach((release) => release());
      },
    });
  }
  const { settings, loading: settingsLoading, error: settingsError, save: saveSettings } = useSettings();
  const maximumColumns = settingsPreview?.maxColumns ?? settings.maxColumns;
  const previewWidth = settingsPreview?.previewWidth ?? settings.previewWidth;
  const [galleryColumns, setGalleryColumns] = useState(1);

  useWindowPlacement();

  useEffect(() => () => {
    explorePageSession.current?.clear();
    if (exploreRestoreFrame.current !== null) window.cancelAnimationFrame(exploreRestoreFrame.current);
  }, []);

  const showToast = useCallback((message: string) => {
    window.clearTimeout(toastTimer.current);
    setToast({ id: Date.now(), message });
    toastTimer.current = window.setTimeout(() => setToast(null), 2400);
  }, []);

  const runMaintenance = useCallback(async (action: MaintenanceAction): Promise<ApiResult<MaintenanceResult>> => {
    try {
      const preview = await backend.maintenancePreview(action);
      if (!preview.ok) return preview;
      const result = await backend.maintenanceExecute(preview.data.previewId, action);
      if (!result.ok) return result;
      if (action.kind === "quickRepair" || (action.kind === "rebuildLibrary" && action.rebuildThumbnailData)) {
        thumbnailClient.clearRetainedCache();
        explorePageSession.current?.clear();
      }
      return result;
    } catch {
      return {
        ok: false,
        error: {
          code: "MAINTENANCE_FAILED",
          message: "유지보수 작업을 완료하지 못했습니다.",
          retryable: true,
          action: "retry",
        },
      };
    }
  }, [thumbnailClient]);

  const applyAutoFindSnapshot = useCallback((snapshot: AutoFindSnapshot) => {
    setAutoFindSnapshot(snapshot);
    setAutoFindIds(snapshot.candidates.map((candidate) => candidate.id));
    setGalleries((current) => mergeGalleryPage(current, {
      page: 1,
      totalPages: snapshot.candidates.length ? 1 : 0,
      items: snapshot.candidates,
    }).galleries);
  }, []);

  const hydrateFavorites = useCallback(async () => {
    try {
      const result = await backend.favoritesList();
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      setFavoriteRecords(result.data);
      setFavoriteMetadata(new Set(result.data.map(favoriteToken)));
    } catch {
      showToast("즐겨찾기 목록을 불러오지 못했습니다.");
    }
  }, [showToast]);

  const hydrateSearchHistory = useCallback(async () => {
    try {
      const result = await backend.searchHistoryList(20);
      if (result.ok) setSearchHistory(result.data);
    } catch {
      // Search history is an enhancement; a transient failure must not block searching.
    }
  }, []);

  const hydrateAutoFind = useCallback(async (showLoading = false) => {
    const token = ++autoFindHydrationToken.current;
    if (showLoading) setAutoFindLoading(true);
    try {
      const result = await backend.autoFindSnapshot();
      if (token !== autoFindHydrationToken.current) return;
      if (!result.ok) {
        setAutoFindError(result.error.message);
        return;
      }
      setAutoFindError(null);
      applyAutoFindSnapshot(result.data);
    } catch {
      if (token === autoFindHydrationToken.current) {
        setAutoFindError("자동 탐색 backend에 연결하지 못했습니다.");
      }
    } finally {
      if (token === autoFindHydrationToken.current) setAutoFindLoading(false);
    }
  }, [applyAutoFindSnapshot]);

  const hydrateDuplicateSnapshot = useCallback(async (showLoading = false) => {
    const token = ++duplicateHydrationToken.current;
    if (showLoading) setDuplicateLoading(true);
    try {
      const result = await backend.duplicateSnapshot();
      if (token !== duplicateHydrationToken.current) return;
      if (!result.ok) {
        setDuplicateError(result.error.message);
        return;
      }
      setDuplicateError(null);
      const merged = mergeHydratedDuplicateSnapshot(
        duplicateSnapshotRef.current,
        result.data,
        duplicateRunRef.current,
      );
      duplicateSnapshotRef.current = merged;
      duplicateRunRef.current = merged.run;
      setDuplicateRun(merged.run);
      setDuplicateSnapshot(merged);
    } catch {
      if (token === duplicateHydrationToken.current) {
        setDuplicateError("작품 중복 검사 backend에 연결하지 못했습니다.");
      }
    } finally {
      if (token === duplicateHydrationToken.current) setDuplicateLoading(false);
    }
  }, []);

  const hydrateInternalSnapshot = useCallback(async (showLoading = false) => {
    const token = ++internalHydrationToken.current;
    if (showLoading) setInternalLoading(true);
    try {
      const result = await backend.internalDuplicateSnapshot();
      if (token !== internalHydrationToken.current) return;
      if (!result.ok) {
        setInternalError(result.error.message);
        return;
      }
      const incoming = result.data.run;
      const current = internalRunRef.current;
      const stale = Boolean(
        incoming && current && (
          (incoming.runId === current.runId && incoming.revision < current.revision)
          || (incoming.runId !== current.runId && incoming.startedAt < current.startedAt)
        ),
      );
      if (stale) return;
      internalRunRef.current = incoming;
      setInternalRun(incoming);
      setInternalSnapshot(result.data);
      setInternalError(null);
    } catch {
      if (token === internalHydrationToken.current) {
        setInternalError("내부 중복 검사 backend에 연결하지 못했습니다.");
      }
    } finally {
      if (token === internalHydrationToken.current) setInternalLoading(false);
    }
  }, []);

  const beginDownloadMutation = useCallback((entryId: string): boolean => {
    if (pendingDownloadEntriesRef.current.has(entryId)) return false;
    pendingDownloadEntriesRef.current.add(entryId);
    setPendingDownloadEntries(new Set(pendingDownloadEntriesRef.current));
    return true;
  }, []);

  const finishDownloadMutation = useCallback((entryId: string) => {
    pendingDownloadEntriesRef.current.delete(entryId);
    setPendingDownloadEntries(new Set(pendingDownloadEntriesRef.current));
  }, []);

  useEffect(() => () => window.clearTimeout(toastTimer.current), []);

  useEffect(() => {
    void hydrateFavorites();
    void hydrateSearchHistory();
    void hydrateAutoFind(true);
    void hydrateDuplicateSnapshot(true);
    void hydrateInternalSnapshot(true);
  }, [hydrateAutoFind, hydrateDuplicateSnapshot, hydrateFavorites, hydrateInternalSnapshot, hydrateSearchHistory]);

  useLayoutEffect(() => {
    const viewport = galleryViewport.current;
    if (!viewport) return;
    const update = () => {
      const next = resolveGalleryColumns(viewport.clientWidth, maximumColumns, previewWidth);
      setGalleryColumns((current) => current === next ? current : next);
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [maximumColumns, previewWidth, settingsLoading]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void backend.on("download:changed", (event: DownloadChangedEvent) => {
      setGalleries((current) => {
        const projection = applyDownloadChanged(current, event);
        return projection.galleries;
      });
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unsubscribe = cleanup;
    }).catch(() => {
      if (!disposed) showToast("작업 상태 event stream에 연결하지 못했습니다.");
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [showToast]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void backend.on("auto-find:changed", (run) => {
      setAutoFindSnapshot((current) => {
        if (current.run?.runId === run.runId && current.run.revision > run.revision) return current;
        return { ...current, run };
      });
      void hydrateAutoFind();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unsubscribe = cleanup;
    }).catch(() => {
      if (!disposed) setAutoFindError("자동 탐색 상태 event stream에 연결하지 못했습니다.");
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [hydrateAutoFind]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void backend.on("duplicate:changed", (run) => {
      if (!validDuplicateRun(run)) return;
      const previous = duplicateRunRef.current;
      if (!duplicateRunIsNewer(previous, run)) return;
      if (previous?.runId !== run.runId) duplicateHydrationToken.current += 1;
      duplicateRunRef.current = run;
      setDuplicateRun(run);
      if (duplicateSnapshotRef.current) {
        const next = { ...duplicateSnapshotRef.current, run };
        duplicateSnapshotRef.current = next;
        setDuplicateSnapshot(next);
      }
      if (duplicateEventNeedsSnapshot(previous, run)) void hydrateDuplicateSnapshot();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unsubscribe = cleanup;
    }).catch(() => {
      if (!disposed) setDuplicateError("작품 중복 검사 event stream에 연결하지 못했습니다.");
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [hydrateDuplicateSnapshot]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void backend.on("internal-duplicate:changed", (run) => {
      const current = internalRunRef.current;
      if (current?.runId === run.runId && current.revision >= run.revision) return;
      if (current?.runId !== run.runId && current && run.startedAt < current.startedAt) return;
      internalRunRef.current = run;
      setInternalRun(run);
      setInternalSnapshot((snapshot) => ({ ...snapshot, run }));
      if (run.state !== "running") void hydrateInternalSnapshot();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unsubscribe = cleanup;
    }).catch(() => {
      if (!disposed) setInternalError("내부 중복 상태 event stream에 연결하지 못했습니다.");
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [hydrateInternalSnapshot]);

  useEffect(() => {
    let cancelled = false;
    const token = ++searchToken.current;
    exploreNavigationToken.current += 1;
    if (exploreRestoreFrame.current !== null) {
      window.cancelAnimationFrame(exploreRestoreFrame.current);
      exploreRestoreFrame.current = null;
    }
    const request: SearchRequest = exploreSearchOverride ?? {
      text: ui.search.explore.committed,
      includeTags: [],
      excludeTags: [],
      languages: ui.search.explore.languages,
      sort: ui.exploreSort,
      pageSize: 50,
    };
    explorePageSession.current?.clear();
    dispatchQuery({ type: "submit.started", token });
    void backend.searchSubmit(request).then((result) => {
      if (cancelled || token !== searchToken.current) return;
      if (!result.ok) {
        dispatchQuery({ type: "submit.failed", token, error: result.error });
        return;
      }
      dispatchQuery({ type: "submit.succeeded", token, submission: result.data });
      explorePageSession.current?.start(result.data.queryId, result.data.firstPage);
      setExploreIds(result.data.firstPage.items.map((item) => item.id));
      setGalleries((current) => mergeGalleryPage(current, result.data.firstPage).galleries);
      if (galleryViewport.current) galleryViewport.current.scrollTop = 0;
      explorePageSession.current?.prefetchAdjacent();
      if (request.text.trim() || request.includeTags.length || request.excludeTags.length) {
        void hydrateSearchHistory();
      }
    }).catch(() => {
      if (!cancelled && token === searchToken.current) {
        dispatchQuery({
          type: "submit.failed",
          token,
          error: { code: "BACKEND_UNAVAILABLE", message: "검색 backend에 연결하지 못했습니다.", retryable: true, action: "retry" },
        });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [exploreSearchOverride, hydrateSearchHistory, searchRefresh, ui.exploreSort, ui.search.explore.committed, ui.search.explore.languages]);

  useEffect(() => {
    let cancelled = false;
    const token = ++downloadHydrationToken.current;
    setDownloadsLoading(true);
    setDownloadsError(null);
    void backend.downloadEntriesList({ page: 1, pageSize: 200 }).then(async (result) => {
      if (cancelled || token !== downloadHydrationToken.current) return;
      if (!result.ok) {
        setDownloadsError(result.error.message);
        return;
      }
      const entries = result.data.entries;
      setGalleries((current) => mergeDownloadEntries(current, entries));
      setDownloadIds([...new Set(entries.map((entry) => entry.galleryId))]);
      setDownloadsLoading(false);
      const detailResults = await Promise.allSettled(
        [...new Set(entries.map((entry) => entry.galleryId))].map((id) => backend.galleryDetailGet(id)),
      );
      if (cancelled || token !== downloadHydrationToken.current) return;
      setGalleries((current) => {
        let next: ReadonlyMap<GalleryId, Gallery> = current;
        for (const detailResult of detailResults) {
          if (detailResult.status === "fulfilled" && detailResult.value.ok) {
            hydratedDetails.current.add(detailResult.value.data.id);
            next = mergeGalleryDetail(next, detailResult.value.data);
          }
        }
        return next;
      });
    }).catch(() => {
      if (!cancelled && token === downloadHydrationToken.current) {
        setDownloadsError("다운로드 목록 backend에 연결하지 못했습니다.");
      }
    }).finally(() => {
      if (!cancelled && token === downloadHydrationToken.current) setDownloadsLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [downloadsRefresh]);

  const displayGalleries = useMemo<ReadonlyMap<GalleryId, Gallery>>(() => {
    const next = new Map<GalleryId, Gallery>();
    galleries.forEach((gallery, id) => {
      const favorite = favoriteMetadata.has(`artist:${normalizeMetadataToken(gallery.artist)}`);
      next.set(id, gallery.favorite === favorite ? gallery : { ...gallery, favorite });
    });
    return next;
  }, [favoriteMetadata, galleries]);
  const favoriteMetadataForDisplay = useMemo<ReadonlySet<string>>(() => {
    const next = new Set(favoriteMetadata);
    galleries.forEach((gallery) => {
      if (gallery.group) {
        const token = `group:${gallery.group}`;
        if (favoriteMetadata.has(normalizeMetadataToken(token))) next.add(token);
      }
      gallery.tags.forEach((tag) => {
        if (favoriteMetadata.has(normalizeMetadataToken(tag))) next.add(tag);
      });
      (gallery.series ?? []).forEach((series) => {
        const token = `series:${series}`;
        if (favoriteMetadata.has(normalizeMetadataToken(token))) next.add(token);
      });
      (gallery.characters ?? []).forEach((character) => {
        const token = `character:${character}`;
        if (favoriteMetadata.has(normalizeMetadataToken(token))) next.add(token);
      });
    });
    return next;
  }, [favoriteMetadata, galleries]);

  const scopedGalleries = useMemo(() => {
    const ids = ui.view === "explore" ? exploreIds : ui.view === "downloads" ? downloadIds : autoFindIds;
    return ids.flatMap((id) => {
      const gallery = displayGalleries.get(id);
      return gallery ? [gallery] : [];
    });
  }, [autoFindIds, displayGalleries, downloadIds, exploreIds, ui.view]);
  const visible = useMemo(() => visibleGalleries(ui, scopedGalleries), [ui, scopedGalleries]);
  const visibleIds = useMemo(() => visible.map((gallery) => gallery.id), [visible]);
  galleriesRef.current = displayGalleries;
  visibleIdsRef.current = visibleIds;
  const allGalleries = useMemo(() => [...displayGalleries.values()], [displayGalleries]);
  const duplicateCandidateCounts = useMemo(() => {
    const counts = new Map<GalleryId, number>();
    for (const candidate of duplicateSnapshot?.candidates ?? []) {
      counts.set(candidate.parent.galleryId, (counts.get(candidate.parent.galleryId) ?? 0) + 1);
      counts.set(candidate.candidate.galleryId, (counts.get(candidate.candidate.galleryId) ?? 0) + 1);
    }
    return counts;
  }, [duplicateSnapshot?.candidates]);
  const autoFindCount = autoFindIds.length;
  const attentionCount = useMemo(
    () => allGalleries.filter((gallery) => ["failed", "interrupted", "review_required"].includes(gallery.download?.state ?? "")).length,
    [allGalleries],
  );
  const activeDownloadCount = useMemo(
    () => allGalleries.filter((gallery) => gallery.download && activeDownloadStates.has(gallery.download.state)).length,
    [allGalleries],
  );
  const refreshExitDownloadCount = useCallback(async () => {
    if (backend.runtime === "browser-mock") {
      setExitActiveDownloads(activeDownloadCount);
      setExitStatusError(false);
      return;
    }
    try {
      const result = await backend.downloadActiveCount();
      if (result.ok) {
        setExitActiveDownloads(result.data);
        setExitStatusError(false);
      } else {
        setExitActiveDownloads(null);
        setExitStatusError(true);
      }
    } catch {
      setExitActiveDownloads(null);
      setExitStatusError(true);
    }
  }, [activeDownloadCount]);
  const openExitConfirm = useCallback(() => {
    if (exitConfirmOpenRef.current || exitActionPendingRef.current) return;
    exitConfirmOpenRef.current = true;
    setExitActiveDownloads(backend.runtime === "browser-mock" ? activeDownloadCount : null);
    setExitStatusError(false);
    exitActionPendingRef.current = false;
    setExitActionPending(false);
    dispatch({ type: "overlay.exit", open: true });
  }, [activeDownloadCount]);
  const closeExitConfirm = useCallback(() => {
    if (exitActionPendingRef.current) return;
    exitConfirmOpenRef.current = false;
    dispatch({ type: "overlay.exit", open: false });
  }, []);

  useEffect(() => {
    if (ui.overlays.exitConfirmOpen) void refreshExitDownloadCount();
  }, [activeDownloadCount, refreshExitDownloadCount, ui.overlays.exitConfirmOpen]);

  useEffect(() => {
    if (backend.runtime !== "tauri") return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void backend.on("app:exit-requested", openExitConfirm).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    }).catch(() => {
      showToast("창 닫기 동작을 연결하지 못했습니다.");
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [openExitConfirm, showToast]);

  const selectGallery = useCallback(
    (id: GalleryId, modifiers: { ctrlKey: boolean; shiftKey: boolean }) => {
      dispatch({ type: "selection.click", id, visibleIds: visibleIdsRef.current, ctrl: modifiers.ctrlKey, shift: modifiers.shiftKey });
    },
    [],
  );

  useLayoutEffect(() => {
    dispatch({ type: "selection.retain", ids: visibleIds });
  }, [visibleIds]);

  const hydrateDetail = useCallback(async (id: GalleryId) => {
    if (hydratedDetails.current.has(id)) return;
    try {
      const result = await backend.galleryDetailGet(id);
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      hydratedDetails.current.add(id);
      setGalleries((current) => mergeGalleryDetail(current, result.data));
    } catch {
      showToast("상세 정보를 불러오지 못했습니다.");
    }
  }, [showToast]);
  const openDetail = useCallback((id: GalleryId) => {
    dispatch({ type: "detail.open", id });
    void hydrateDetail(id);
  }, [hydrateDetail]);
  const openRelatedDetail = useCallback((id: GalleryId, parentId: GalleryId) => {
    dispatch({ type: "detail.open", id, parentId });
    void hydrateDetail(id);
  }, [hydrateDetail]);
  const hydrateDuplicateReview = useCallback(async (candidateId: string) => {
    const token = ++duplicateReviewToken.current;
    setDuplicateReviewLoading(true);
    setDuplicateReviewError(null);
    try {
      const result = await backend.duplicateReviewGet(candidateId);
      if (token !== duplicateReviewToken.current) return;
      if (!result.ok) {
        setDuplicateReviewError(result.error.message);
        return;
      }
      setDuplicateReview(result.data);
    } catch {
      if (token === duplicateReviewToken.current) {
        setDuplicateReviewError("중복 검토 backend에 연결하지 못했습니다.");
      }
    } finally {
      if (token === duplicateReviewToken.current) setDuplicateReviewLoading(false);
    }
  }, []);
  const openReview = useCallback((id: GalleryId) => {
    const candidate = duplicateSnapshot?.candidates.find((item) =>
      item.parent.galleryId === id || item.candidate.galleryId === id,
    );
    if (!candidate) {
      showToast("저장된 작품 중복 후보를 찾을 수 없습니다. 중복 검사 결과를 새로 불러옵니다.");
      void hydrateDuplicateSnapshot();
      return;
    }
    setDuplicateReviewCandidateId(candidate.candidateId);
    setDuplicateReview(null);
    setDuplicateReviewError(null);
    dispatch({ type: "overlay.review", galleryId: id });
    void hydrateDuplicateReview(candidate.candidateId);
  }, [duplicateSnapshot?.candidates, hydrateDuplicateReview, hydrateDuplicateSnapshot, showToast]);
  const closeDuplicateReview = useCallback(() => {
    duplicateReviewToken.current += 1;
    setDuplicateReviewCandidateId(null);
    setDuplicateReview(null);
    setDuplicateReviewError(null);
    setDuplicateReviewLoading(false);
    dispatch({ type: "overlay.review", galleryId: null });
  }, []);
  const applyDuplicateDecision = useCallback(async (request: DuplicateDecisionRequest) => {
    if (duplicateDecisionPendingRef.current) return;
    duplicateDecisionPendingRef.current = true;
    setDuplicateDecisionPending(true);
    setDuplicateReviewError(null);
    try {
      const result = await backend.duplicateDecisionApply(request);
      if (!result.ok) {
        if (result.error.code === "REVISION_CONFLICT") {
          await Promise.all([
            hydrateDuplicateReview(request.candidateId),
            hydrateDuplicateSnapshot(),
          ]);
          setDuplicateReviewError("다른 창에서 판정이 변경되어 최신 근거와 이력을 다시 불러왔습니다.");
          return;
        }
        setDuplicateReviewError(result.error.message);
        return;
      }
      setDuplicateReview(result.data);
      await hydrateDuplicateSnapshot();
      setDownloadsRefresh((value) => value + 1);
      showToast("중복 판정을 저장했습니다. 파일은 자동으로 영구 삭제되지 않습니다.");
    } catch {
      setDuplicateReviewError("중복 판정 요청을 backend에 전달하지 못했습니다.");
    } finally {
      duplicateDecisionPendingRef.current = false;
      setDuplicateDecisionPending(false);
    }
  }, [hydrateDuplicateReview, hydrateDuplicateSnapshot, showToast]);

  const hydrateInternalReview = useCallback(async (entryId: string) => {
    const token = ++internalReviewToken.current;
    setInternalReviewLoading(true);
    setInternalReviewError(null);
    try {
      const result = await backend.internalDuplicateReviewGet(entryId);
      if (token !== internalReviewToken.current) return;
      if (!result.ok) {
        setInternalReviewError(result.error.message);
        return;
      }
      setInternalReview(result.data);
    } catch {
      if (token === internalReviewToken.current) {
        setInternalReviewError("내부 중복 검토 backend에 연결하지 못했습니다.");
      }
    } finally {
      if (token === internalReviewToken.current) setInternalReviewLoading(false);
    }
  }, []);

  const openInternalReview = useCallback((entryId: string) => {
    setInternalReviewEntryId(entryId);
    setInternalReview(null);
    setInternalPlan(null);
    setInternalReviewError(null);
    void hydrateInternalReview(entryId);
  }, [hydrateInternalReview]);

  const closeInternalReview = useCallback(() => {
    internalReviewToken.current += 1;
    setInternalReviewEntryId(null);
    setInternalReview(null);
    setInternalPlan(null);
    setInternalReviewError(null);
    setInternalReviewLoading(false);
  }, []);

  const startInternalScan = useCallback(async () => {
    if (internalPendingRef.current || internalRun?.state === "running") return;
    internalPendingRef.current = true;
    setInternalPending(true);
    setInternalError(null);
    try {
      const result = await backend.internalDuplicateScanStart();
      if (!result.ok) {
        setInternalError(result.error.message);
        showToast(result.error.message);
        return;
      }
      internalRunRef.current = result.data;
      setInternalRun(result.data);
      setInternalSnapshot((snapshot) => ({ ...snapshot, run: result.data }));
      await hydrateInternalSnapshot();
      showToast("검증된 앨범 파일을 기준으로 내부 중복 페이지 검사를 시작했습니다.");
    } catch {
      const message = "내부 중복 검사를 시작하지 못했습니다.";
      setInternalError(message);
      showToast(message);
    } finally {
      internalPendingRef.current = false;
      setInternalPending(false);
    }
  }, [hydrateInternalSnapshot, internalRun?.state, showToast]);

  const cancelInternalScan = useCallback(async () => {
    if (internalPendingRef.current || internalRun?.state !== "running") return;
    internalPendingRef.current = true;
    setInternalPending(true);
    try {
      const result = await backend.internalDuplicateScanCancel();
      if (!result.ok) {
        setInternalError(result.error.message);
        showToast(result.error.message);
        return;
      }
      internalRunRef.current = result.data;
      setInternalRun(result.data);
      setInternalSnapshot((snapshot) => ({ ...snapshot, run: result.data }));
      showToast("내부 중복 검사를 취소했습니다. 기존 검토 결과는 유지됩니다.");
    } catch {
      showToast("내부 중복 검사 취소 요청을 전달하지 못했습니다.");
    } finally {
      internalPendingRef.current = false;
      setInternalPending(false);
    }
  }, [internalRun?.state, showToast]);

  const previewInternalRemoval = useCallback(async (request: InternalRemovalPlanRequest) => {
    if (internalPendingRef.current) return;
    internalPendingRef.current = true;
    setInternalPending(true);
    setInternalReviewError(null);
    try {
      const result = await backend.internalRemovalPlan(request);
      if (!result.ok) {
        setInternalReviewError(result.error.message);
        if (result.error.code === "REVISION_CONFLICT") await hydrateInternalReview(request.entryId);
        return;
      }
      setInternalPlan(result.data);
    } catch {
      setInternalReviewError("격리 계획을 계산하지 못했습니다.");
    } finally {
      internalPendingRef.current = false;
      setInternalPending(false);
    }
  }, [hydrateInternalReview]);

  const applyInternalRemoval = useCallback(async (plan: InternalRemovalPlan) => {
    if (internalPendingRef.current) return;
    internalPendingRef.current = true;
    setInternalPending(true);
    setInternalReviewError(null);
    try {
      const result = await backend.internalRemovalApply({
        plan,
        reason: "사용자가 내부 중복 검토에서 명시적으로 격리함",
      });
      if (!result.ok) {
        setInternalReviewError(result.error.message);
        if (result.error.code === "REVISION_CONFLICT" && internalReviewEntryId) {
          await hydrateInternalReview(internalReviewEntryId);
        }
        return;
      }
      setInternalReview(result.data.review);
      setInternalPlan(null);
      await hydrateInternalSnapshot();
      setDownloadsRefresh((value) => value + 1);
      showToast(`${result.data.records.length}개 페이지를 안전 격리했습니다. 영구 삭제되지 않았습니다.`);
    } catch {
      setInternalReviewError("페이지 격리 요청을 완료하지 못했습니다. 앱 재시작 시 안전하게 조정됩니다.");
    } finally {
      internalPendingRef.current = false;
      setInternalPending(false);
    }
  }, [hydrateInternalReview, hydrateInternalSnapshot, internalReviewEntryId, showToast]);

  const undoInternalRemoval = useCallback(async (recordIds: string[]) => {
    if (internalPendingRef.current || !recordIds.length) return;
    internalPendingRef.current = true;
    setInternalPending(true);
    setInternalReviewError(null);
    try {
      const result = await backend.internalRemovalUndo({ recordIds });
      if (!result.ok) {
        setInternalReviewError(result.error.message);
        return;
      }
      setInternalReview(result.data.review);
      await hydrateInternalSnapshot();
      setDownloadsRefresh((value) => value + 1);
      showToast(`${result.data.records.length}개 페이지를 원래 위치로 복원했습니다.`);
    } catch {
      setInternalReviewError("격리 페이지 복원 요청을 완료하지 못했습니다. 앱 재시작 시 안전하게 조정됩니다.");
    } finally {
      internalPendingRef.current = false;
      setInternalPending(false);
    }
  }, [hydrateInternalSnapshot, showToast]);
  const openActivity = useCallback(() => {
    activityOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dispatch({ type: "overlay.activity", open: true });
  }, []);
  const closeActivity = useCallback(() => {
    dispatch({ type: "overlay.activity", open: false });
    const target = activityOpener.current;
    activityOpener.current = null;
    window.requestAnimationFrame(() => {
      if (target?.isConnected) target.focus();
      else document.querySelector<HTMLElement>("[aria-controls='activity-panel']")?.focus();
    });
  }, []);
  const openStatusDetail = useCallback((_: GalleryId) => openActivity(), [openActivity]);

  const openArtifact = useCallback(
    async (id: GalleryId) => {
      const gallery = galleriesRef.current.get(id);
      if (!gallery) return;
      if (gallery.download?.state !== "completed") {
        showToast(`${gallery.title}은 아직 실행할 수 있는 완료 파일이 없습니다.`);
        return;
      }
      try {
        const result = await backend.artifactOpenFirst(gallery.download.entryId);
        if (!result.ok) {
          showToast(result.error.message);
          setDownloadsRefresh((value) => value + 1);
        }
      } catch {
        showToast("완료 파일을 Windows 기본 뷰어로 열지 못했습니다.");
      }
    },
    [showToast],
  );

  const searchMetadata = useCallback((value: string) => {
    const target = metadataSearchToken(value);
    const kind = searchTokenKind(target.displayToken);
    const request: SearchRequest = target.includeTag
      ? {
        text: "",
        includeTags: [target.includeTag],
        excludeTags: [],
        languages: ui.search.explore.languages,
        sort: ui.exploreSort,
        pageSize: 50,
      }
      : {
        text: target.displayToken,
        includeTags: [],
        excludeTags: [],
        languages: ui.search.explore.languages,
        sort: ui.exploreSort,
        pageSize: 50,
      };
    if (!kind && !target.displayToken) return;
    setExploreSearchOverride(request);
    dispatch({ type: "navigate", view: "explore" });
    dispatch({ type: "search.commit", view: "explore", value: target.displayToken });
    if (galleryViewport.current) galleryViewport.current.scrollTop = 0;
  }, [ui.exploreSort, ui.search.explore.languages]);

  const toggleMetadataFavorite = useCallback(async (value: string) => {
    const token = normalizeMetadataToken(value);
    if (!token || pendingFavoriteTokens.current.has(token)) return;
    const key = favoriteKeyFromToken(token);
    const enabled = !favoriteMetadata.has(token);
    pendingFavoriteTokens.current.add(token);
    try {
      const result = await backend.favoriteSet(key, enabled);
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      const normalizedToken = result.data.favorite ? favoriteToken(result.data.favorite) : token;
      setFavoriteMetadata((current) => {
        const next = new Set(current);
        if (result.data.enabled) next.add(normalizedToken);
        else next.delete(normalizedToken);
        return next;
      });
      setFavoriteRecords((current) => {
        const withoutKey = current.filter((favorite) => favoriteToken(favorite) !== normalizedToken);
        return result.data.favorite ? [...withoutKey, result.data.favorite] : withoutKey;
      });
      showToast(`${value} 즐겨찾기를 ${result.data.enabled ? "추가" : "해제"}했습니다.`);
    } catch {
      showToast("즐겨찾기 변경을 저장하지 못했습니다.");
    } finally {
      pendingFavoriteTokens.current.delete(token);
    }
  }, [favoriteMetadata, showToast]);

  const queueGalleries = useCallback(
    async (ids: GalleryId[]) => {
      const uniqueIds = [...new Set(ids)];
      const newGalleryIds = uniqueIds.filter((id) => !galleries.get(id)?.download);
      const retryEntryIds = uniqueIds.flatMap((id) => {
        const download = galleries.get(id)?.download;
        return download && retryableDownloadStates.has(download.state) ? [download.entryId] : [];
      });
      if (!newGalleryIds.length && !retryEntryIds.length) {
        showToast("현재 상태에서 시작할 수 있는 항목이 없습니다.");
        dispatch({ type: "selection.clear" });
        return;
      }
      let started = 0;
      try {
        if (retryEntryIds.length) {
          const retryResult = await backend.downloadRetry(retryEntryIds);
          if (!retryResult.ok) {
            showToast(retryResult.error.message);
            return;
          }
          started += retryResult.data.length;
          setDownloadsRefresh((value) => value + 1);
        }
        if (newGalleryIds.length) {
          const requestId = `frontend-queue-${Date.now()}-${++queueRequestSequence.current}`;
          const queueResult = await backend.downloadQueueAdd(newGalleryIds, requestId);
          if (!queueResult.ok) {
            showToast(queueResult.error.message);
            return;
          }
          setGalleries((current) => mergeDownloadEntries(current, queueResult.data));
          setDownloadIds((current) => [...new Set([...current, ...queueResult.data.map((entry) => entry.galleryId)])]);
          started += queueResult.data.length;
        }
        showToast(`${started}개 항목의 다운로드를 시작했습니다.`);
      } catch {
        showToast("다운로드 대기열에 연결하지 못했습니다.");
      }
      dispatch({ type: "selection.clear" });
    },
    [galleries, showToast],
  );

  const retryGallery = useCallback(
    async (id: GalleryId) => {
      const download = galleriesRef.current.get(id)?.download;
      if (!download || !retryableDownloadStates.has(download.state)) {
        showToast("현재 상태에서는 이 항목을 재시도할 수 없습니다.");
        return;
      }
      if (!beginDownloadMutation(download.entryId)) return;
      try {
        const result = await backend.downloadRetry([download.entryId]);
        if (!result.ok) {
          showToast(result.error.message);
          return;
        }
        setDownloadsRefresh((value) => value + 1);
        showToast("다운로드를 다시 시작했습니다.");
      } catch {
        showToast("재시도 요청을 backend에 전달하지 못했습니다.");
      } finally {
        finishDownloadMutation(download.entryId);
      }
    },
    [beginDownloadMutation, finishDownloadMutation, showToast],
  );

  const cancelGallery = useCallback(async (id: GalleryId) => {
    const download = galleriesRef.current.get(id)?.download;
    if (!download) return;
    if (!beginDownloadMutation(download.entryId)) return;
    try {
      const result = await backend.downloadCancel([download.entryId]);
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      setGalleries((current) => mergeDownloadEntries(current, result.data));
      showToast("다운로드를 취소했습니다.");
    } catch {
      showToast("취소 요청을 backend에 전달하지 못했습니다.");
    } finally {
      finishDownloadMutation(download.entryId);
    }
  }, [beginDownloadMutation, finishDownloadMutation, showToast]);

  const quarantineGalleries = useCallback(async (ids: GalleryId[]) => {
    const downloads = ids
      .map((id) => galleriesRef.current.get(id)?.download)
      .filter((download): download is NonNullable<Gallery["download"]> => download !== undefined);
    const restoring = downloads.length > 0 && downloads.every((download) => download.state === "quarantined");
    const eligible = downloads.filter((download) =>
      restoring ? download.state === "quarantined" : download.state === "completed",
    );
    if (!eligible.length || eligible.length !== downloads.length) {
      showToast(restoring
        ? "선택한 모든 항목이 격리 상태일 때만 함께 복원할 수 있습니다."
        : "검증이 완료된 다운로드만 격리할 수 있습니다.");
      return;
    }
    const confirmed = window.confirm(restoring
      ? `${eligible.length}개 항목을 원래 위치로 복원할까요?`
      : `${eligible.length}개 항목을 복구 가능한 격리 폴더로 옮길까요? 자동으로 영구 삭제되지 않습니다.`);
    if (!confirmed) return;
    try {
      const result = restoring
        ? await backend.downloadQuarantineUndo(eligible.map((download) => download.entryId))
        : await backend.downloadQuarantine(
            eligible.map((download) => download.entryId),
            "사용자가 Downloads 화면에서 격리를 확인함",
          );
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      setGalleries((current) => mergeDownloadEntries(current, result.data));
      dispatch({ type: "selection.clear" });
      showToast(restoring ? "격리한 파일을 원래 위치로 복원했습니다." : "파일을 복구 가능한 격리 폴더로 옮겼습니다.");
    } catch {
      showToast(restoring ? "격리 파일 복원 요청에 실패했습니다." : "파일 격리 요청에 실패했습니다.");
    }
  }, [showToast]);

  const reconcileArtifacts = useCallback(async () => {
    if (reconcilingArtifacts) return;
    setReconcilingArtifacts(true);
    try {
      const result = await backend.appReconcile();
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      setDownloadsRefresh((value) => value + 1);
      const summary = result.data.issues.length
        ? `${result.data.inspectedArtifacts}개 검사 · ${result.data.issues.length}개 문제를 안전 상태로 표시했습니다.`
        : `${result.data.verifiedArtifacts}개 artifact의 DB·manifest·파일 무결성을 확인했습니다.`;
      showToast(result.data.resumedJobs
        ? `${summary} ${result.data.resumedJobs}개 작업을 재개했습니다.`
        : summary);
    } catch {
      showToast("artifact 무결성 검사를 실행하지 못했습니다.");
    } finally {
      setReconcilingArtifacts(false);
    }
  }, [reconcilingArtifacts, showToast]);

  const refreshAutoFind = useCallback(async () => {
    if (autoFindPending || autoFindSnapshot.run?.state === "running") return;
    setAutoFindPending(true);
    setAutoFindError(null);
    try {
      const result = await backend.autoFindRefresh();
      if (!result.ok) {
        setAutoFindError(result.error.message);
        showToast(result.error.message);
        return;
      }
      setAutoFindSnapshot((current) => ({ ...current, run: result.data }));
      await hydrateAutoFind();
    } catch {
      const message = "자동 탐색을 시작하지 못했습니다.";
      setAutoFindError(message);
      showToast(message);
    } finally {
      setAutoFindPending(false);
    }
  }, [autoFindPending, autoFindSnapshot.run?.state, hydrateAutoFind, showToast]);

  const cancelAutoFind = useCallback(async () => {
    if (autoFindPending || autoFindSnapshot.run?.state !== "running") return;
    setAutoFindPending(true);
    try {
      const result = await backend.autoFindCancel();
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      setAutoFindSnapshot((current) => ({ ...current, run: result.data }));
      await hydrateAutoFind();
      showToast("자동 탐색을 취소했습니다. 지금까지 찾은 후보는 보존됩니다.");
    } catch {
      showToast("자동 탐색 취소 요청을 전달하지 못했습니다.");
    } finally {
      setAutoFindPending(false);
    }
  }, [autoFindPending, autoFindSnapshot.run?.state, hydrateAutoFind, showToast]);

  const excludeAutoFindCandidates = useCallback(async (ids: GalleryId[]) => {
    const candidateIds = [...new Set(ids)].filter((id) => autoFindIds.includes(id));
    if (!candidateIds.length) return;
    try {
      const result = await backend.autoFindExclude(candidateIds, "사용자가 Auto Find 후보 목록에서 제외함");
      if (!result.ok) {
        showToast(result.error.message);
        return;
      }
      applyAutoFindSnapshot(result.data.snapshot);
      dispatch({ type: "selection.clear" });
      showToast(`${result.data.excludedGalleryIds.length}개 후보를 다음 탐색에서도 제외합니다.`);
    } catch {
      showToast("자동 탐색 후보 제외 요청을 저장하지 못했습니다.");
    }
  }, [applyAutoFindSnapshot, autoFindIds, showToast]);

  const startDuplicateScan = useCallback(async () => {
    if (duplicatePendingRef.current || duplicateRun?.state === "running") return;
    duplicatePendingRef.current = true;
    setDuplicatePending(true);
    setDuplicateError(null);
    try {
      const result = await backend.duplicateScanStart();
      if (!result.ok) {
        setDuplicateError(result.error.message);
        showToast(result.error.message);
        return;
      }
      if (duplicateRunRef.current?.runId !== result.data.runId) duplicateHydrationToken.current += 1;
      duplicateRunRef.current = result.data;
      setDuplicateRun(result.data);
      if (duplicateSnapshotRef.current) {
        const next = { ...duplicateSnapshotRef.current, run: result.data };
        duplicateSnapshotRef.current = next;
        setDuplicateSnapshot(next);
      }
      await hydrateDuplicateSnapshot();
      showToast("검증된 로컬 아티팩트를 기준으로 작품 중복 검사를 시작했습니다.");
    } catch {
      const message = "작품 중복 검사를 시작하지 못했습니다.";
      setDuplicateError(message);
      showToast(message);
    } finally {
      duplicatePendingRef.current = false;
      setDuplicatePending(false);
    }
  }, [duplicateRun?.state, hydrateDuplicateSnapshot, showToast]);

  const cancelDuplicateScan = useCallback(async () => {
    if (duplicatePendingRef.current || duplicateRun?.state !== "running") return;
    duplicatePendingRef.current = true;
    setDuplicatePending(true);
    try {
      const result = await backend.duplicateScanCancel();
      if (!result.ok) {
        setDuplicateError(result.error.message);
        showToast(result.error.message);
        return;
      }
      duplicateRunRef.current = result.data;
      setDuplicateRun(result.data);
      if (duplicateSnapshotRef.current) {
        const next = { ...duplicateSnapshotRef.current, run: result.data };
        duplicateSnapshotRef.current = next;
        setDuplicateSnapshot(next);
      }
      await hydrateDuplicateSnapshot();
      showToast("작품 중복 검사를 취소했습니다. 저장된 후보와 판정 이력은 유지됩니다.");
    } catch {
      showToast("작품 중복 검사 취소 요청을 전달하지 못했습니다.");
    } finally {
      duplicatePendingRef.current = false;
      setDuplicatePending(false);
    }
  }, [duplicateRun?.state, hydrateDuplicateSnapshot, showToast]);

  const loadExplorePage = useCallback(async (page: number) => {
    if (!query.queryId || page < 1) return;
    const queryId = query.queryId;
    const navigationToken = ++exploreNavigationToken.current;
    if (exploreRestoreFrame.current !== null) {
      window.cancelAnimationFrame(exploreRestoreFrame.current);
      exploreRestoreFrame.current = null;
    }
    if (query.page && galleryViewport.current) {
      explorePageSession.current?.recordScroll(query.page.page, galleryViewport.current.scrollTop);
    }
    dispatchQuery({ type: "page.started", queryId, page });
    const result = await explorePageSession.current?.open(page);
    if (!result || result.status === "stale" || navigationToken !== exploreNavigationToken.current) return;
    if (result.status === "failed") {
      dispatchQuery({
        type: "page.failed",
        queryId,
        page,
        error: result.error,
      });
      return;
    }
    dispatchQuery({ type: "page.succeeded", queryId, page: result.page });
    setExploreIds(result.page.items.map((item) => item.id));
    setGalleries((current) => mergeGalleryPage(current, result.page).galleries);
    exploreRestoreFrame.current = window.requestAnimationFrame(() => {
      if (navigationToken === exploreNavigationToken.current && galleryViewport.current) {
        galleryViewport.current.scrollTop = result.scrollTop;
      }
      exploreRestoreFrame.current = null;
    });
  }, [query.page, query.queryId]);

  const selectedIds = useMemo(() => [...ui.selection.ids], [ui.selection.ids]);
  const selectedCompletedEntryId = useMemo(() => {
    if (selectedIds.length !== 1) return null;
    const download = displayGalleries.get(selectedIds[0]!)?.download;
    return download?.state === "completed" ? download.entryId : null;
  }, [displayGalleries, selectedIds]);
  const selectedHasInternalResult = useMemo(() => (
    selectedCompletedEntryId !== null
    && internalSnapshot.groups.some((group) => group.entryId === selectedCompletedEntryId)
  ), [internalSnapshot.groups, selectedCompletedEntryId]);

  useEffect(() => {
    const keyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (event.key === "Escape") {
        if (event.defaultPrevented || event.repeat || event.isComposing || target.closest("dialog")) return;
        if (ui.overlays.activityOpen) {
          event.preventDefault();
          closeActivity();
          return;
        }
        if (ui.overlays.settingsOpen || ui.overlays.reviewGalleryId !== null || ui.overlays.exitConfirmOpen) return;
        if (ui.search[ui.view].suggestionsOpen) {
          dispatch({ type: "search.suggestions", view: ui.view, open: false });
          return;
        }
        if (ui.detail.activeId !== null) dispatch({ type: "detail.close", id: ui.detail.activeId });
        else if (selectedIds.length) dispatch({ type: "selection.clear" });
        else openExitConfirm();
        event.preventDefault();
        return;
      }
      if (!target.closest(".gallery-viewport, .selection-toolbar")) return;
      if (["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(target.tagName) || target.closest("dialog")) return;
      if (event.key === "Enter" && selectedIds.length) {
        event.preventDefault();
        if (ui.view === "downloads") {
          const firstCompleted = selectedIds.find((id) => galleries.get(id)?.download?.state === "completed");
          if (firstCompleted !== undefined) openArtifact(firstCompleted);
          else void queueGalleries(selectedIds);
        } else void queueGalleries(selectedIds);
      }
      if (event.key === "Delete" && selectedIds.length) {
        event.preventDefault();
        if (ui.view === "downloads") void quarantineGalleries(selectedIds);
        else if (ui.view === "auto-find") void excludeAutoFindCandidates(selectedIds);
        else showToast("후보 제외는 Auto Find 화면에서 사용할 수 있습니다.");
      }
    };
    window.addEventListener("keydown", keyDown);
    return () => window.removeEventListener("keydown", keyDown);
  }, [closeActivity, excludeAutoFindCandidates, galleries, openArtifact, openExitConfirm, quarantineGalleries, queueGalleries, selectedIds, showToast, ui.detail.activeId, ui.overlays, ui.search, ui.view]);

  const saveSettingsPatch = useCallback(
    async (patch: SettingsPatch) => {
      const result = await saveSettings(patch);
      showToast(result.ok ? "설정을 저장했습니다." : result.error.message);
      return result.ok;
    },
    [saveSettings, showToast],
  );

  const searchSuggestions = useMemo<SearchSuggestion[]>(() => {
    return buildSearchSuggestionCatalog({ history: searchHistory, favorites: favoriteRecords, galleries: displayGalleries.values() });
  }, [displayGalleries, favoriteRecords, searchHistory]);

  const autoFindGroups = useMemo(() => {
    const groups = new Map<string, Gallery[]>();
    for (const gallery of visible) {
      const current = groups.get(gallery.artist);
      if (current) current.push(gallery);
      else groups.set(gallery.artist, [gallery]);
    }
    return [...groups.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [visible]);

  const config = viewConfig[ui.view];
  const resultSourceLabel = backend.runtime === "tauri" ? "Hitomi 실데이터" : "브라우저 fixture";
  const currentAutoFindStatus = autoFindStatusLabel(autoFindLoading, autoFindError, autoFindSnapshot.run);
  const currentDuplicateStatus = duplicateStatusLabel(duplicateLoading, duplicateError, duplicateRun);
  const currentInternalStatus = internalStatusLabel(internalLoading, internalError, internalRun);
  const renderGalleryGrid = (items: Gallery[], ariaLabel: string) => (
    <GalleryGrid
      columns={galleryColumns}
      previewWidth={previewWidth}
      selectionContext={ui.selection.ids.size > 0}
      ariaLabel={ariaLabel}
    >
      {items.map((gallery, index) => (
        <GalleryCard
          key={gallery.id}
          gallery={gallery}
          thumbnailPriority={index < galleryColumns ? "visible" : "prefetch"}
          view={ui.view}
          selected={ui.selection.ids.has(gallery.id)}
          selectionContext={ui.selection.ids.size > 0}
          favoriteMetadata={favoriteMetadataForDisplay}
          duplicateCandidateCount={duplicateCandidateCounts.get(gallery.id) ?? 0}
          onSelect={selectGallery}
          onOpenDetail={openDetail}
          onOpenArtifact={openArtifact}
          onOpenReview={openReview}
          onStatusDetail={openStatusDetail}
          onMetadataSearch={searchMetadata}
          onMetadataFavorite={toggleMetadataFavorite}
        />
      ))}
    </GalleryGrid>
  );

  return (
    <>
      <div className={`app-shell${ui.railCollapsed ? " sidebar-collapsed" : ""}`}>
        <SideRail
          view={ui.view}
          collapsed={ui.railCollapsed}
          autoFindCount={autoFindCount}
          attentionCount={attentionCount}
          sourceLabel={backend.runtime === "tauri" ? "Hitomi live" : "Browser fixture"}
          onNavigate={(view) => dispatch({ type: "navigate", view })}
          onToggle={() => dispatch({ type: "rail.toggle" })}
        />
        <main className="workspace">
          <ViewHeader
            view={ui.view}
            search={ui.search[ui.view]}
            suggestions={ui.view === "explore" ? searchSuggestions : []}
            activityCount={activeDownloadCount}
            activityOpen={ui.overlays.activityOpen}
            onDraft={(value) => dispatch({ type: "search.draft", view: ui.view, value })}
            onSuggestions={(open, active) => dispatch({ type: "search.suggestions", view: ui.view, open, active })}
            onCommit={(value) => {
              if (ui.view === "explore") setExploreSearchOverride(null);
              dispatch({ type: "search.commit", view: ui.view, value });
              if (ui.view !== "explore") showToast("현재 결과를 필터했습니다.");
            }}
            onSelectSuggestion={(suggestion, value) => {
              if (ui.view === "explore" && suggestion.request) {
                setExploreSearchOverride(suggestion.request);
                dispatch({ type: "search.languages", view: "explore", languages: suggestion.request.languages });
                dispatch({ type: "sort.set", sort: suggestion.request.sort });
              } else if (ui.view === "explore") {
                setExploreSearchOverride(null);
              }
              dispatch({ type: "search.commit", view: ui.view, value });
            }}
            onCompleteSuggestion={(value) => {
              dispatch({ type: "search.draft", view: ui.view, value });
              dispatch({ type: "search.suggestions", view: ui.view, open: false });
            }}
            onLanguages={(languages) => {
              if (ui.view === "explore") setExploreSearchOverride(null);
              dispatch({ type: "search.languages", view: ui.view, languages });
            }}
            onRefresh={() => {
              if (ui.view === "explore") setSearchRefresh((value) => value + 1);
              else if (ui.view === "downloads") setDownloadsRefresh((value) => value + 1);
              else void refreshAutoFind();
            }}
            onActivity={() => ui.overlays.activityOpen ? closeActivity() : openActivity()}
            onSettings={() => dispatch({ type: "overlay.settings", open: true })}
          />
          <section className="page-heading">
            <div><span className="eyebrow">{config.eyebrow}</span><h1>{config.title}</h1></div>
            <div className="heading-actions">
              {ui.view === "auto-find" ? (
                <>
                  <GroupingControl value={ui.grouping["auto-find"]} onChange={(grouping) => dispatch({ type: "grouping.set", view: "auto-find", grouping })} />
                  <button type="button" className="text-button" disabled={autoFindPending || autoFindSnapshot.run?.state === "running"} onClick={() => void refreshAutoFind()}><FluentIcon glyph="\uE72C" /> {autoFindSnapshot.run?.state === "failed" ? "다시 탐색" : "즐겨찾기 작가 갱신"}</button>
                  {autoFindSnapshot.run?.state === "running" ? <button type="button" className="text-button danger-button" disabled={autoFindPending} onClick={() => void cancelAutoFind()}><FluentIcon glyph="\uE711" /> 탐색 취소</button> : null}
                  <button type="button" className="text-button dark" onClick={() => void queueGalleries(visibleIds)}><FluentIcon glyph="\uE896" /> 후보 다운로드</button>
                </>
              ) : ui.view === "downloads" ? (
                <>
                  <p className="sr-only" id="duplicate-scan-explanation">작품 간 검사는 서로 다른 앨범을 비교하고, 내부 페이지 검사는 각 앨범 안에서 반복되거나 유사한 페이지를 찾습니다.</p>
                  <GroupingControl value={ui.grouping.downloads} onChange={(grouping) => dispatch({ type: "grouping.set", view: "downloads", grouping })} />
                  <button type="button" className="text-button" disabled={reconcilingArtifacts} onClick={() => void reconcileArtifacts()}><FluentIcon glyph="\uE9D9" /> {reconcilingArtifacts ? "무결성 검사 중" : "무결성 검사"}</button>
                  <button type="button" className="text-button" aria-describedby="duplicate-scan-explanation" title="완료된 모든 앨범을 서로 비교해 작품 단위 중복 후보를 찾습니다." disabled={duplicateLoading || duplicatePending || duplicateRun?.state === "running"} onClick={() => void startDuplicateScan()}><FluentIcon glyph="\uE9D9" /> 전체 작품 간 중복 검사</button>
                  {duplicateRun?.state === "running" ? <button type="button" className="text-button danger-button" disabled={duplicatePending} onClick={() => void cancelDuplicateScan()}><FluentIcon glyph="\uE711" /> 중복 검사 취소</button> : null}
                  <button type="button" className="text-button" aria-describedby="duplicate-scan-explanation" title="완료된 모든 앨범 각각의 내부 페이지를 비교해 반복·유사 페이지를 찾습니다." disabled={internalLoading || internalPending || internalRun?.state === "running"} onClick={() => void startInternalScan()}><FluentIcon glyph="\uE9D9" /> 전체 앨범 내부 페이지 검사</button>
                  {internalRun?.state === "running" ? <button type="button" className="text-button danger-button" disabled={internalPending} onClick={() => void cancelInternalScan()}><FluentIcon glyph="\uE711" /> 내부 검사 취소</button> : null}
                  <button type="button" className="text-button" disabled={!selectedCompletedEntryId || internalPending} title={!selectedCompletedEntryId ? "완료된 앨범 하나를 선택하세요." : selectedHasInternalResult ? "저장된 내부 페이지 검사 결과를 엽니다." : "저장된 내부 결과가 없습니다. 먼저 전체 앨범 내부 페이지 검사를 실행하세요."} onClick={() => {
                    if (!selectedCompletedEntryId) return;
                    if (!selectedHasInternalResult) {
                      showToast("저장된 내부 결과가 없습니다. 먼저 ‘전체 앨범 내부 페이지 검사’를 실행하세요.");
                      return;
                    }
                    openInternalReview(selectedCompletedEntryId);
                  }}><FluentIcon glyph="\uE890" /> 선택 앨범 내부 결과 열기</button>
                  <button type="button" className="text-button primary" onClick={() => void queueGalleries(visibleIds)}><FluentIcon glyph="\uE896" /> 전체 다운로드</button>
                </>
              ) : null}
            </div>
          </section>
          <section className="context-row">
            <div className="context-left">
              {ui.view === "explore" ? (
                <div className="select-control"><label htmlFor="sort-select">정렬</label><select id="sort-select" value={ui.exploreSort} onChange={(event) => { setExploreSearchOverride(null); dispatch({ type: "sort.set", sort: event.target.value as SearchSort }); }}>{sortOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></div>
              ) : ui.view === "auto-find" ? (
                <div className="auto-find-evidence" role="status" aria-live="polite">
                  <span className={`context-summary auto-find-status is-${autoFindSnapshot.run?.state ?? "idle"}`}>{currentAutoFindStatus}</span>
                  {autoFindSnapshot.run?.historyMode === "newer_than_oldest_downloaded" && autoFindSnapshot.cutoffEvidence.length ? (
                    <ul aria-label="Auto Find 기록 cutoff 근거">
                      {autoFindSnapshot.cutoffEvidence.map((evidence) => (
                        <li key={evidence.artist}>
                          {evidence.artist}: {evidence.oldestOwnedGalleryId === undefined
                            ? "검증 완료·격리 소유 작품 없음"
                            : `가장 오래된 소유 gallery ID #${evidence.oldestOwnedGalleryId} 이후, ${evidence.qualifiedOwnedCount}개 확인`}
                        </li>
                      ))}
                    </ul>
                  ) : null}
                  {autoFindSnapshot.truncations.length ? (
                    <ul aria-label="Auto Find 결과 제한 경고">
                      {autoFindSnapshot.truncations.map((truncation) => (
                        <li key={`${truncation.artist}-${truncation.limit}`}>
                          {truncation.artist}: cutoff 이후 후보 {truncation.eligibleCount}개 중 {truncation.limit}개만 표시했습니다.
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              ) : (
                <>
                  <div className="segmented status-filter" role="group" aria-label="다운로드 상태 필터">
                    {(["all", "active", "review", "failed", "complete"] as const).map((filter) => (
                      <button key={filter} type="button" aria-pressed={ui.downloadsFilter === filter} className={ui.downloadsFilter === filter ? "is-active" : ""} onClick={() => dispatch({ type: "downloads.filter", filter })}>
                        {{ all: "전체", active: "작업 중", review: "검토", failed: "실패", complete: "완료" }[filter]}
                      </button>
                    ))}
                  </div>
                  <span className={`context-summary duplicate-scan-status is-${duplicateRun?.state ?? "idle"}`} role="status">{currentDuplicateStatus}</span>
                  <span className={`context-summary duplicate-scan-status is-${internalRun?.state ?? "idle"}`} role="status">{currentInternalStatus}</span>
                  {duplicateError ? <button type="button" className="text-button compact" onClick={() => void hydrateDuplicateSnapshot(true)}>결과 다시 불러오기</button> : null}
                  {internalError ? <button type="button" className="text-button compact" onClick={() => void hydrateInternalSnapshot(true)}>내부 결과 다시 불러오기</button> : null}
                </>
              )}
            </div>
            <div className="context-summary">{visible.length}개 결과 · {resultSourceLabel}</div>
          </section>
          <SelectionToolbar
            count={ui.selection.ids.size}
            downloadsView={ui.view === "downloads"}
            restoreMode={selectedIds.length > 0 && selectedIds.every((id) => displayGalleries.get(id)?.download?.state === "quarantined")}
            onAll={() => dispatch({ type: "selection.all", ids: visibleIds })}
            onClear={() => dispatch({ type: "selection.clear" })}
            onPrimary={() => void queueGalleries(selectedIds)}
            onDelete={() => ui.view === "downloads"
              ? void quarantineGalleries(selectedIds)
              : ui.view === "auto-find"
                ? void excludeAutoFindCandidates(selectedIds)
                : showToast("후보 제외는 Auto Find 화면에서 사용할 수 있습니다.")}
          />
          <section ref={galleryViewport} className="gallery-viewport">
            {settingsLoading ? (
              <div className="loading-state" role="status"><span className="spinner" /> 저장된 화면 설정을 불러오는 중</div>
            ) : ((ui.view === "explore" && query.phase === "submitting" && !visible.length)
              || (ui.view === "downloads" && downloadsLoading && !visible.length)
              || (ui.view === "auto-find" && autoFindLoading && !visible.length)) ? (
              <GalleryGridSkeleton columns={galleryColumns} previewWidth={previewWidth} />
            ) : ui.view === "explore" && query.error && !query.page ? (
              <div className="empty-state" role="alert"><FluentIcon glyph="\uE7BA" /><h2>검색 결과를 불러오지 못했습니다</h2><p>{query.error.message}</p><button type="button" className="text-button" onClick={() => setSearchRefresh((value) => value + 1)}>다시 시도</button></div>
            ) : ui.view === "downloads" && downloadsError ? (
              <div className="empty-state" role="alert"><FluentIcon glyph="\uE7BA" /><h2>다운로드 목록을 불러오지 못했습니다</h2><p>{downloadsError}</p><button type="button" className="text-button" onClick={() => setDownloadsRefresh((value) => value + 1)}>다시 시도</button></div>
            ) : ui.view === "auto-find" && autoFindError && !autoFindSnapshot.candidates.length ? (
              <div className="empty-state" role="alert"><FluentIcon glyph="\uE7BA" /><h2>자동 탐색 결과를 불러오지 못했습니다</h2><p>{autoFindError}</p><button type="button" className="text-button" onClick={() => void hydrateAutoFind(true)}>다시 시도</button></div>
            ) : visible.length ? (
              ui.view === "auto-find" && ui.grouping["auto-find"] === "artist" ? (
                <div className="gallery-groups">
                  {autoFindGroups.map(([artist, items]) => (
                    <section className="gallery-group" key={artist} aria-labelledby={`auto-find-artist-${items[0]?.id}`}>
                      <h2 id={`auto-find-artist-${items[0]?.id}`}><span>★</span> {artist}<small>{items.length}개 후보</small></h2>
                      {renderGalleryGrid(items, `${artist} 자동 탐색 후보`)}
                    </section>
                  ))}
                </div>
              ) : renderGalleryGrid(visible, config.title)
            ) : (
              <div className="empty-state"><FluentIcon glyph="\uE11A" /><h2>표시할 갤러리가 없습니다</h2><p>{ui.view === "auto-find" ? "즐겨찾기 작가를 추가한 뒤 명시적으로 갱신하거나 현재 검색·언어 필터를 바꿔 보세요." : "검색어나 언어·상태 필터를 바꿔 보세요."}</p></div>
            )}
            {ui.view === "explore" && query.page ? <div className="pager"><button type="button" className="text-button" disabled={query.phase === "loading-page" || query.page.page <= 1} onClick={() => void loadExplorePage(query.page!.page - 1)}>이전</button><span>{query.page.page} / {Math.max(1, query.page.totalPages)}{query.phase === "loading-page" ? " · 불러오는 중" : ""}</span><button type="button" className="text-button" disabled={query.phase === "loading-page" || query.page.page >= query.page.totalPages} onClick={() => void loadExplorePage(query.page!.page + 1)}>다음</button></div> : null}
          </section>
        </main>
        <ActivityDrawer
          open={ui.overlays.activityOpen}
          galleries={allGalleries}
          onClose={closeActivity}
          onReview={openReview}
          onRetry={(id) => void retryGallery(id)}
          onCancel={(id) => void cancelGallery(id)}
          pendingEntryIds={pendingDownloadEntries}
        />
      </div>

      <DetailWorkspace
        tabs={ui.detail.tabs}
        activeId={ui.detail.activeId}
        minimized={ui.detail.minimized}
        galleries={displayGalleries}
        favoriteMetadata={favoriteMetadataForDisplay}
        previewWidth={previewWidth}
        relatedPreviewWidth={settings.relatedPreviewWidth}
        onActivate={(id) => dispatch({ type: "detail.activate", id })}
        onClose={(id) => dispatch({ type: "detail.close", id })}
        onCloseAll={() => dispatch({ type: "detail.closeAll" })}
        onMinimize={() => dispatch({ type: "detail.minimize", minimized: true })}
        onRestore={() => dispatch({ type: "detail.minimize", minimized: false })}
        onOpenRelated={openRelatedDetail}
        onQueue={(id) => void queueGalleries([id])}
        onMetadataSearch={searchMetadata}
        onMetadataFavorite={toggleMetadataFavorite}
      />

      <SettingsDialog
        open={ui.overlays.settingsOpen}
        settings={settings}
        loading={settingsLoading}
        error={settingsError}
        onClose={() => dispatch({ type: "overlay.settings", open: false })}
        onSave={saveSettingsPatch}
        onPreviewLayout={setSettingsPreview}
        onPreviewFolderName={previewFolderNameTemplate}
        onMaintenance={runMaintenance}
      />

      <DuplicateReviewDialog
        open={ui.overlays.reviewGalleryId !== null && duplicateReviewCandidateId !== null}
        review={duplicateReview ?? undefined}
        galleries={displayGalleries}
        loading={duplicateReviewLoading}
        error={duplicateReviewError}
        decisionPending={duplicateDecisionPending}
        browserFixture={backend.runtime === "browser-mock"}
        onClose={closeDuplicateReview}
        onRetry={() => duplicateReviewCandidateId && void hydrateDuplicateReview(duplicateReviewCandidateId)}
        onRescan={() => void startDuplicateScan()}
        onDecision={(request) => void applyDuplicateDecision(request)}
      />

      <InternalDuplicateDialog
        open={internalReviewEntryId !== null}
        review={internalReview ?? undefined}
        plan={internalPlan ?? undefined}
        loading={internalReviewLoading}
        busy={internalPending}
        error={internalReviewError}
        onClose={closeInternalReview}
        onRetry={() => internalReviewEntryId && void hydrateInternalReview(internalReviewEntryId)}
        onRescan={() => void startInternalScan()}
        onPlan={(request) => void previewInternalRemoval(request)}
        onApply={(plan) => void applyInternalRemoval(plan)}
        onUndo={(recordIds) => void undoInternalRemoval(recordIds)}
      />

      <ExitConfirmDialog
        open={ui.overlays.exitConfirmOpen}
        activeDownloads={exitActiveDownloads}
        statusError={exitStatusError}
        actionPending={exitActionPending}
        onClose={closeExitConfirm}
        onMinimizeToTray={() => {
          if (exitActionPendingRef.current) return;
          exitActionPendingRef.current = true;
          setExitActionPending(true);
          void backend.appMinimizeToTray().then((result) => {
            if (!result.ok) {
              exitActionPendingRef.current = false;
              setExitActionPending(false);
              showToast(result.error.message);
            } else {
              exitActionPendingRef.current = false;
              setExitActionPending(false);
              exitConfirmOpenRef.current = false;
              dispatch({ type: "overlay.exit", open: false });
            }
          }).catch(() => {
            exitActionPendingRef.current = false;
            setExitActionPending(false);
            showToast("트레이로 최소화하지 못했습니다.");
          });
        }}
        onQuit={() => {
          if (exitActionPendingRef.current) return;
          exitActionPendingRef.current = true;
          setExitActionPending(true);
          void (async () => {
            if (backend.runtime === "tauri") {
              const latest = await backend.downloadActiveCount();
              if (!latest.ok) {
                if (exitStatusError) {
                  const quitResult = await backend.appQuit();
                  if (!quitResult.ok) {
                    exitActionPendingRef.current = false;
                    setExitActionPending(false);
                    showToast(quitResult.error.message);
                  }
                  return;
                }
                setExitActiveDownloads(null);
                setExitStatusError(true);
                exitActionPendingRef.current = false;
                setExitActionPending(false);
                showToast("다운로드 상태를 확인하지 못했습니다. 트레이 최소화를 권장합니다.");
                return;
              }
              if (exitActiveDownloads === null || latest.data !== exitActiveDownloads) {
                setExitActiveDownloads(latest.data);
                setExitStatusError(false);
                exitActionPendingRef.current = false;
                setExitActionPending(false);
                showToast("진행 작업 정보를 갱신했습니다. 내용을 확인하고 다시 선택해 주세요.");
                return;
              }
            }
            const result = await backend.appQuit();
            if (!result.ok) {
              exitActionPendingRef.current = false;
              setExitActionPending(false);
              showToast(result.error.message);
            }
          })().catch(() => {
            exitActionPendingRef.current = false;
            setExitActionPending(false);
            showToast("프로그램을 종료하지 못했습니다.");
          });
        }}
      />

      {toast ? <div key={toast.id} className="toast" role="status">{toast.message}</div> : null}
    </>
  );
}

function GroupingControl({ value, onChange }: { value: "all" | "artist"; onChange: (value: "all" | "artist") => void }) {
  return (
    <div className="segmented" role="group" aria-label="표시 방식">
      <button type="button" aria-pressed={value === "all"} className={value === "all" ? "is-active" : ""} onClick={() => onChange("all")}>전체</button>
      <button type="button" aria-pressed={value === "artist"} className={value === "artist" ? "is-active" : ""} onClick={() => onChange("artist")}>작가별</button>
    </div>
  );
}
