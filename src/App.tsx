import { useCallback, useEffect, useLayoutEffect, useMemo, useReducer, useRef, useState } from "react";
import { backend } from "./api/backend";
import type { DownloadChangedEvent, SearchRequest, SettingsPatch } from "./api/contracts";
import { ActivityDrawer } from "./components/ActivityDrawer";
import { DetailWorkspace } from "./components/DetailWorkspace";
import { DuplicateReviewDialog } from "./components/DuplicateReviewDialog";
import { ExitConfirmDialog } from "./components/ExitConfirmDialog";
import { FluentIcon } from "./components/FluentIcon";
import { GalleryCard } from "./components/GalleryCard";
import { SelectionToolbar } from "./components/SelectionToolbar";
import { SettingsDialog } from "./components/SettingsDialog";
import { SideRail } from "./components/SideRail";
import { ViewHeader } from "./components/ViewHeader";
import { retryableDownloadStates, type DownloadState, type Gallery, type GalleryId, type SearchSort, type ViewId } from "./core/types";
import { useSettings } from "./hooks/useSettings";
import { useWindowPlacement } from "./hooks/useWindowPlacement";
import { resolveGalleryColumns } from "./layout/galleryColumns";
import { applyDownloadChanged } from "./state/downloadProjection";
import { mergeDownloadEntries, mergeGalleryDetail, mergeGalleryPage } from "./state/galleryProjection";
import { galleryQueryReducer, initialGalleryQueryState } from "./state/galleryQuery";
import { visibleGalleries } from "./state/selectors";
import { initialUiState, uiReducer } from "./state/uiState";

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

const activeDownloadStates: ReadonlySet<DownloadState> = new Set([
  "queued",
  "resolving_metadata",
  "downloading",
  "hashing",
  "verifying",
  "retry_wait",
]);

type Toast = { id: number; message: string } | null;

export default function App() {
  const [ui, dispatch] = useReducer(uiReducer, initialUiState);
  const [query, dispatchQuery] = useReducer(galleryQueryReducer, initialGalleryQueryState);
  const [galleries, setGalleries] = useState<ReadonlyMap<GalleryId, Gallery>>(() => new Map());
  const [exploreIds, setExploreIds] = useState<GalleryId[]>([]);
  const [downloadIds, setDownloadIds] = useState<GalleryId[]>([]);
  const [downloadsLoading, setDownloadsLoading] = useState(true);
  const [downloadsError, setDownloadsError] = useState<string | null>(null);
  const [searchRefresh, setSearchRefresh] = useState(0);
  const [downloadsRefresh, setDownloadsRefresh] = useState(0);
  const [favoriteMetadata, setFavoriteMetadata] = useState<ReadonlySet<string>>(
    () => new Set(["female:glasses", "female:kimono"]),
  );
  const [toast, setToast] = useState<Toast>(null);
  const [settingsPreview, setSettingsPreview] = useState<{ maxColumns: number; previewWidth: number } | null>(null);
  const [exitActiveDownloads, setExitActiveDownloads] = useState<number | null>(null);
  const [exitStatusError, setExitStatusError] = useState(false);
  const [exitActionPending, setExitActionPending] = useState(false);
  const [pendingDownloadEntries, setPendingDownloadEntries] = useState<ReadonlySet<string>>(() => new Set());
  const exitConfirmOpenRef = useRef(false);
  const exitActionPendingRef = useRef(false);
  const toastTimer = useRef<number | undefined>(undefined);
  const searchToken = useRef(0);
  const downloadHydrationToken = useRef(0);
  const queueRequestSequence = useRef(0);
  const pendingDownloadEntriesRef = useRef(new Set<string>());
  const hydratedDetails = useRef(new Set<GalleryId>());
  const galleriesRef = useRef(galleries);
  const visibleIdsRef = useRef<GalleryId[]>([]);
  const activityOpener = useRef<HTMLElement | null>(null);
  const galleryViewport = useRef<HTMLElement>(null);
  const { settings, loading: settingsLoading, error: settingsError, save: saveSettings } = useSettings();
  const maximumColumns = settingsPreview?.maxColumns ?? settings.maxColumns;
  const previewWidth = settingsPreview?.previewWidth ?? settings.previewWidth;
  const [galleryColumns, setGalleryColumns] = useState(1);

  useWindowPlacement();

  const showToast = useCallback((message: string) => {
    window.clearTimeout(toastTimer.current);
    setToast({ id: Date.now(), message });
    toastTimer.current = window.setTimeout(() => setToast(null), 2400);
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

  useLayoutEffect(() => {
    document.documentElement.style.setProperty("--preview-width", `${previewWidth}px`);
  }, [previewWidth]);

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
    let cancelled = false;
    const token = ++searchToken.current;
    const request: SearchRequest = {
      text: ui.search.explore.committed,
      includeTags: [],
      excludeTags: [],
      languages: ui.search.explore.languages,
      sort: ui.exploreSort,
      pageSize: 50,
    };
    dispatchQuery({ type: "submit.started", token });
    void backend.searchSubmit(request).then((result) => {
      if (cancelled || token !== searchToken.current) return;
      if (!result.ok) {
        dispatchQuery({ type: "submit.failed", token, error: result.error });
        return;
      }
      dispatchQuery({ type: "submit.succeeded", token, submission: result.data });
      setExploreIds(result.data.firstPage.items.map((item) => item.id));
      setGalleries((current) => mergeGalleryPage(current, result.data.firstPage).galleries);
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
  }, [searchRefresh, ui.exploreSort, ui.search.explore.committed, ui.search.explore.languages]);

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

  const scopedGalleries = useMemo(() => {
    const ids = ui.view === "explore" ? exploreIds : ui.view === "downloads" ? downloadIds : [...galleries.keys()];
    return ids.flatMap((id) => {
      const gallery = galleries.get(id);
      return gallery ? [gallery] : [];
    });
  }, [downloadIds, exploreIds, galleries, ui.view]);
  const visible = useMemo(() => visibleGalleries(ui, scopedGalleries), [ui, scopedGalleries]);
  const visibleIds = useMemo(() => visible.map((gallery) => gallery.id), [visible]);
  galleriesRef.current = galleries;
  visibleIdsRef.current = visibleIds;
  const allGalleries = useMemo(() => [...galleries.values()], [galleries]);
  const autoFindCount = useMemo(() => allGalleries.filter((gallery) => gallery.favorite).length, [allGalleries]);
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
  const openReview = useCallback((id: GalleryId) => dispatch({ type: "overlay.review", galleryId: id }), []);
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
    (id: GalleryId) => {
      const gallery = galleriesRef.current.get(id);
      if (!gallery) return;
      if (gallery.download?.state === "completed") {
        showToast(`${gallery.title} 파일 실행은 artifact adapter 연결 단계에서 수행합니다.`);
      } else {
        showToast(`${gallery.title}은 아직 실행할 수 있는 완료 파일이 없습니다.`);
      }
    },
    [showToast],
  );

  const searchMetadata = useCallback((value: string) => {
    dispatch({ type: "navigate", view: "explore" });
    dispatch({ type: "search.commit", view: "explore", value });
  }, []);

  const toggleMetadataFavorite = useCallback((value: string) => {
    setFavoriteMetadata((current) => {
      const next = new Set(current);
      if (next.has(value)) next.delete(value);
      else next.add(value);
      return next;
    });
    if (value.startsWith("artist:")) {
      const artist = value.slice("artist:".length);
      setGalleries((current) => {
        const next = new Map(current);
        current.forEach((gallery, id) => {
          if (gallery.artist === artist) next.set(id, { ...gallery, favorite: !gallery.favorite });
        });
        return next;
      });
    }
    showToast(`${value} 즐겨찾기를 변경했습니다.`);
  }, [showToast]);

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

  const loadExplorePage = useCallback(async (page: number) => {
    if (!query.queryId || page < 1 || query.phase === "loading-page") return;
    const queryId = query.queryId;
    dispatchQuery({ type: "page.started", queryId, page });
    try {
      const result = await backend.searchPageGet(queryId, page);
      if (!result.ok) {
        dispatchQuery({ type: "page.failed", queryId, page, error: result.error });
        return;
      }
      dispatchQuery({ type: "page.succeeded", queryId, page: result.data });
      setExploreIds(result.data.items.map((item) => item.id));
      setGalleries((current) => mergeGalleryPage(current, result.data).galleries);
      galleryViewport.current?.scrollTo({ top: 0, behavior: "smooth" });
    } catch {
      dispatchQuery({
        type: "page.failed",
        queryId,
        page,
        error: { code: "BACKEND_UNAVAILABLE", message: "검색 페이지를 불러오지 못했습니다.", retryable: true, action: "retry" },
      });
    }
  }, [query.phase, query.queryId]);

  const selectedIds = useMemo(() => [...ui.selection.ids], [ui.selection.ids]);

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
        showToast(
          ui.view === "downloads"
            ? `${selectedIds.length}개 항목의 격리 계획을 엽니다. 실제 파일은 변경하지 않았습니다.`
            : `${selectedIds.length}개 항목의 제외 확인을 엽니다.`,
        );
      }
    };
    window.addEventListener("keydown", keyDown);
    return () => window.removeEventListener("keydown", keyDown);
  }, [closeActivity, galleries, openArtifact, openExitConfirm, queueGalleries, selectedIds, showToast, ui.detail.activeId, ui.overlays, ui.search, ui.view]);

  const reviewParent = ui.overlays.reviewGalleryId === null ? undefined : galleries.get(ui.overlays.reviewGalleryId);
  const reviewCandidate = reviewParent
    ? allGalleries.find((gallery) => gallery.id !== reviewParent.id && gallery.artist === reviewParent.artist) ?? allGalleries.find((gallery) => gallery.id !== reviewParent.id)
    : undefined;

  const saveSettingsPatch = useCallback(
    async (patch: SettingsPatch) => {
      const result = await saveSettings(patch);
      showToast(result.ok ? "설정을 저장했습니다." : result.error.message);
      return result.ok;
    },
    [saveSettings, showToast],
  );

  const config = viewConfig[ui.view];
  const resultSourceLabel = backend.runtime === "tauri" ? "Hitomi 실데이터" : "브라우저 fixture";
  const autoFindRefreshLabel = backend.runtime === "tauri"
    ? "자동 갱신 · 미연결"
    : "마지막 갱신 · 브라우저 fixture";

  return (
    <>
      <div className={`app-shell${ui.railCollapsed ? " sidebar-collapsed" : ""}`}>
        <SideRail
          view={ui.view}
          collapsed={ui.railCollapsed}
          autoFindCount={autoFindCount}
          attentionCount={attentionCount}
          onNavigate={(view) => dispatch({ type: "navigate", view })}
          onToggle={() => dispatch({ type: "rail.toggle" })}
        />
        <main className="workspace">
          <ViewHeader
            view={ui.view}
            search={ui.search[ui.view]}
            activityCount={activeDownloadCount}
            activityOpen={ui.overlays.activityOpen}
            onDraft={(value) => dispatch({ type: "search.draft", view: ui.view, value })}
            onSuggestions={(open, active) => dispatch({ type: "search.suggestions", view: ui.view, open, active })}
            onCommit={(value) => {
              dispatch({ type: "search.commit", view: ui.view, value });
              if (ui.view !== "explore") showToast("현재 결과를 필터했습니다.");
            }}
            onLanguages={(languages) => dispatch({ type: "search.languages", view: ui.view, languages })}
            onRefresh={() => {
              if (ui.view === "explore") setSearchRefresh((value) => value + 1);
              else if (ui.view === "downloads") setDownloadsRefresh((value) => value + 1);
              else showToast("즐겨찾기 작가 갱신은 Phase 4 계약으로 연결합니다.");
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
                  <button type="button" className="text-button" onClick={() => showToast("즐겨찾기 작가 갱신은 Phase 4 계약으로 연결합니다.")}><FluentIcon glyph="\uE72C" /> 즐겨찾기 작가 갱신</button>
                  <button type="button" className="text-button dark" onClick={() => void queueGalleries(visibleIds)}><FluentIcon glyph="\uE896" /> 후보 다운로드</button>
                </>
              ) : ui.view === "downloads" ? (
                <>
                  <GroupingControl value={ui.grouping.downloads} onChange={(grouping) => dispatch({ type: "grouping.set", view: "downloads", grouping })} />
                  <button type="button" className="text-button" onClick={() => showToast("내부 중복 검사는 Phase 6 계약 뒤에 연결합니다.")}><FluentIcon glyph="\uE9D9" /> 내부 중복 검사</button>
                  <button type="button" className="text-button primary" onClick={() => void queueGalleries(visibleIds)}><FluentIcon glyph="\uE896" /> 전체 다운로드</button>
                </>
              ) : null}
            </div>
          </section>
          <section className="context-row">
            <div className="context-left">
              {ui.view === "explore" ? (
                <div className="select-control"><label htmlFor="sort-select">정렬</label><select id="sort-select" value={ui.exploreSort} onChange={(event) => dispatch({ type: "sort.set", sort: event.target.value as SearchSort })}>{sortOptions.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select></div>
              ) : ui.view === "auto-find" ? (
                <span className="context-summary">{autoFindRefreshLabel}</span>
              ) : (
                <div className="segmented status-filter" role="group" aria-label="다운로드 상태 필터">
                  {(["all", "active", "review", "failed", "complete"] as const).map((filter) => (
                    <button key={filter} type="button" aria-pressed={ui.downloadsFilter === filter} className={ui.downloadsFilter === filter ? "is-active" : ""} onClick={() => dispatch({ type: "downloads.filter", filter })}>
                      {{ all: "전체", active: "작업 중", review: "검토", failed: "실패", complete: "완료" }[filter]}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div className="context-summary">{visible.length}개 결과 · {resultSourceLabel}</div>
          </section>
          <SelectionToolbar
            count={ui.selection.ids.size}
            downloadsView={ui.view === "downloads"}
            onAll={() => dispatch({ type: "selection.all", ids: visibleIds })}
            onClear={() => dispatch({ type: "selection.clear" })}
            onPrimary={() => void queueGalleries(selectedIds)}
            onDelete={() => showToast(ui.view === "downloads" ? "격리 계획만 준비하며 실제 파일은 변경하지 않습니다." : "제외 확인 화면을 준비합니다.")}
          />
          <section ref={galleryViewport} className="gallery-viewport">
            {settingsLoading || (ui.view === "explore" && query.phase === "submitting") || (ui.view === "downloads" && downloadsLoading) ? (
              <div className="loading-state" role="status"><span className="spinner" /> {settingsLoading ? "저장된 화면 설정을 불러오는 중" : ui.view === "explore" ? "갤러리를 검색하는 중" : "다운로드 목록을 불러오는 중"}</div>
            ) : ui.view === "explore" && query.error && !query.page ? (
              <div className="empty-state" role="alert"><FluentIcon glyph="\uE7BA" /><h2>검색 결과를 불러오지 못했습니다</h2><p>{query.error.message}</p><button type="button" className="text-button" onClick={() => setSearchRefresh((value) => value + 1)}>다시 시도</button></div>
            ) : ui.view === "downloads" && downloadsError ? (
              <div className="empty-state" role="alert"><FluentIcon glyph="\uE7BA" /><h2>다운로드 목록을 불러오지 못했습니다</h2><p>{downloadsError}</p><button type="button" className="text-button" onClick={() => setDownloadsRefresh((value) => value + 1)}>다시 시도</button></div>
            ) : visible.length ? (
              <div className={`gallery-grid${ui.selection.ids.size ? " is-selection-context" : ""}`} style={{ gridTemplateColumns: `repeat(${galleryColumns}, minmax(0, 1fr))` }} role="list" aria-label={config.title}>
                {visible.map((gallery, index) => (
                  <GalleryCard
                    key={gallery.id}
                    gallery={gallery}
                    thumbnailPriority={index < galleryColumns ? "visible" : "prefetch"}
                    view={ui.view}
                    selected={ui.selection.ids.has(gallery.id)}
                    selectionContext={ui.selection.ids.size > 0}
                    favoriteMetadata={favoriteMetadata}
                    onSelect={selectGallery}
                    onOpenDetail={openDetail}
                    onOpenArtifact={openArtifact}
                    onOpenReview={openReview}
                    onStatusDetail={openStatusDetail}
                    onMetadataSearch={searchMetadata}
                    onMetadataFavorite={toggleMetadataFavorite}
                  />
                ))}
              </div>
            ) : (
              <div className="empty-state"><FluentIcon glyph="\uE11A" /><h2>표시할 갤러리가 없습니다</h2><p>검색어나 언어·상태 필터를 바꿔 보세요.</p></div>
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
        galleries={galleries}
        favoriteMetadata={favoriteMetadata}
        onActivate={(id) => dispatch({ type: "detail.activate", id })}
        onClose={(id) => dispatch({ type: "detail.close", id })}
        onCloseAll={() => dispatch({ type: "detail.closeAll" })}
        onMinimize={() => dispatch({ type: "detail.minimize", minimized: true })}
        onRestore={() => dispatch({ type: "detail.minimize", minimized: false })}
        onOpenRelated={openRelatedDetail}
        onQueue={(id) => void queueGalleries([id])}
        onMetadataSearch={searchMetadata}
        onMetadataFavorite={toggleMetadataFavorite}
        onPreview={(page) => showToast(`${page}페이지 확대는 실제 thumbnail adapter와 연결합니다.`)}
      />

      <SettingsDialog
        open={ui.overlays.settingsOpen}
        settings={settings}
        loading={settingsLoading}
        error={settingsError}
        onClose={() => dispatch({ type: "overlay.settings", open: false })}
        onSave={saveSettingsPatch}
        onNotice={showToast}
        onPreviewLayout={setSettingsPreview}
      />

      <DuplicateReviewDialog
        open={ui.overlays.reviewGalleryId !== null}
        parent={reviewParent}
        candidate={reviewCandidate}
        onClose={() => dispatch({ type: "overlay.review", galleryId: null })}
        onScan={() => showToast("전수 검사는 Phase 5 evidence 계약 뒤에 연결합니다.")}
        onDecision={(label) => {
          showToast(`${label} 판정 계획을 mock으로 확인했습니다.`);
          dispatch({ type: "overlay.review", galleryId: null });
        }}
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
