import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import type { Gallery, GalleryId } from "../core/types";
import {
  galleryCoverThumbnailKey,
  sourcePageThumbnailKey,
  type ThumbnailClient,
} from "../thumbnail";
import { FluentIcon } from "./FluentIcon";
import { GalleryThumbnail } from "./GalleryThumbnail";
import { MetadataChip } from "./MetadataChip";
import {
  DETAIL_ORIENTATION_SAMPLE_SIZE,
  detailPreviewLayout,
  type DetailPreviewLayout,
  type DetailPreviewSample,
} from "./detailPreviewLayout";
import { sortGalleryTags } from "./galleryCardLayout";

type DetailWorkspaceProps = {
  tabs: GalleryId[];
  activeId: GalleryId | null;
  minimized: boolean;
  galleries: ReadonlyMap<GalleryId, Gallery>;
  favoriteMetadata: ReadonlySet<string>;
  relatedPreviewWidth?: number;
  thumbnailClient?: ThumbnailClient;
  onActivate: (id: GalleryId) => void;
  onClose: (id: GalleryId) => void;
  onCloseAll: () => void;
  onMinimize: () => void;
  onRestore: () => void;
  onOpenRelated: (id: GalleryId, parentId: GalleryId) => void;
  onQueue: (id: GalleryId) => void;
  onMetadataSearch: (value: string) => void;
  onMetadataFavorite: (value: string) => void;
};

type MetadataBoxProps = {
  label: string;
  values: string[];
  type: string;
  favorite?: boolean;
  favoriteMetadata?: ReadonlySet<string>;
  onSearch: (value: string) => void;
  onFavorite: (value: string) => void;
};

const boundedPageCount = (pages: number, maximum: number): number =>
  Number.isFinite(pages) ? Math.min(maximum, Math.max(0, Math.floor(pages))) : 0;

const metadataSearchToken = (namespace: string, value: string): string =>
  `${namespace}:${value.trim().replace(/\s+/g, "_")}`;

function MetadataBox({ label, values, type, favorite, favoriteMetadata, onSearch, onFavorite }: MetadataBoxProps) {
  return (
    <div className="metadata-box">
      <span>{label}</span>
      <div className="metadata-value">
        {values.map((value) => (
          <MetadataChip
            key={`${type}:${value}`}
            value={`${type}:${value}`}
            searchValue={["series", "character"].includes(type) ? metadataSearchToken(type, value) : undefined}
            label={["series", "character"].includes(type) ? value.replaceAll("_", " ") : value}
            favorite={favorite ?? favoriteMetadata?.has(`${type}:${value}`)}
            onSearch={onSearch}
            onToggleFavorite={onFavorite}
          />
        ))}
      </div>
    </div>
  );
}

export function DetailWorkspace(props: DetailWorkspaceProps) {
  const {
    tabs,
    activeId,
    minimized,
    galleries,
    favoriteMetadata,
    relatedPreviewWidth = 240,
    thumbnailClient,
    onActivate,
    onClose,
    onCloseAll,
    onMinimize,
    onRestore,
    onOpenRelated,
    onQueue,
    onMetadataSearch,
    onMetadataFavorite,
  } = props;
  const detailBody = useRef<HTMLDivElement>(null);
  const workspace = useRef<HTMLElement>(null);
  const restoreButton = useRef<HTMLButtonElement>(null);
  const previousVisible = useRef(false);
  const previousTabCount = useRef(0);
  const opener = useRef<HTMLElement | null>(null);
  const previewDialog = useRef<HTMLDialogElement>(null);
  const previewCloseButton = useRef<HTMLButtonElement>(null);
  const previewOpener = useRef<HTMLButtonElement | null>(null);
  const previewClosingInternally = useRef(false);
  const [previewPage, setPreviewPage] = useState<number | null>(null);
  const previewTerminals = useRef(new Map<GalleryId, Map<number, DetailPreviewSample>>());
  const previewLayouts = useRef(new Map<GalleryId, DetailPreviewLayout>());
  const [, setPreviewLayoutRevision] = useState(0);

  useEffect(() => {
    const visible = tabs.length > 0 && !minimized;
    if (previousTabCount.current === 0 && tabs.length > 0) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
    if (visible && !previousVisible.current) {
      window.requestAnimationFrame(() => {
        workspace.current?.querySelector<HTMLElement>("[role='tab'][aria-selected='true']")?.focus();
      });
    } else if (!visible && minimized) {
      window.requestAnimationFrame(() => restoreButton.current?.focus());
    } else if (previousTabCount.current > 0 && tabs.length === 0) {
      const target = opener.current;
      opener.current = null;
      window.requestAnimationFrame(() => {
        if (target?.isConnected) target.focus();
        else document.querySelector<HTMLElement>(".view-header input")?.focus();
      });
    }
    previousVisible.current = visible;
    previousTabCount.current = tabs.length;
  }, [minimized, tabs.length]);

  useEffect(() => {
    const activeTabs = new Set(tabs);
    for (const id of previewTerminals.current.keys()) {
      if (!activeTabs.has(id)) previewTerminals.current.delete(id);
    }
    for (const id of previewLayouts.current.keys()) {
      if (!activeTabs.has(id)) previewLayouts.current.delete(id);
    }
  }, [tabs]);

  useEffect(() => {
    detailBody.current?.scrollTo?.({ top: 0, left: 0 });
    if (!minimized && activeId !== null) {
      window.requestAnimationFrame(() => {
        workspace.current?.querySelector<HTMLElement>("[role='tab'][aria-selected='true']")?.focus();
      });
    }
  }, [activeId, minimized]);

  const navigateTabs = (event: KeyboardEvent<HTMLElement>, index: number) => {
    if (!tabs.length) return;
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (index + 1) % tabs.length;
    else if (event.key === "ArrowLeft") nextIndex = (index - 1 + tabs.length) % tabs.length;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = tabs.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    const nextId = tabs[nextIndex];
    if (nextId === undefined) return;
    onActivate(nextId);
    const tabsHost = event.currentTarget.closest(".detail-tabs");
    window.requestAnimationFrame(() => {
      if (nextIndex !== null) tabsHost?.querySelectorAll<HTMLElement>("[role='tab']")[nextIndex]?.focus();
    });
  };

  const gallery = activeId === null ? undefined : galleries.get(activeId);
  const previewPageCount = gallery ? boundedPageCount(gallery.pages, 24) : 0;
  const previewSampleCount = Math.min(previewPageCount, DETAIL_ORIENTATION_SAMPLE_SIZE);
  const lockedPreviewLayout = gallery ? previewLayouts.current.get(gallery.id) : undefined;
  const previewLayout = lockedPreviewLayout ?? { columns: 3 as const, orientation: "pending" as const };

  const recordPreviewTerminal = (page: number, terminal: DetailPreviewSample) => {
    if (!gallery || page > previewSampleCount || previewLayouts.current.has(gallery.id)) return;
    const entries = previewTerminals.current.get(gallery.id) ?? new Map<number, DetailPreviewSample>();
    const previous = entries.get(page);
    if (previous?.status === terminal.status && previous?.width === terminal.width && previous?.height === terminal.height) return;
    entries.set(page, terminal);
    previewTerminals.current.set(gallery.id, entries);
    if (entries.size !== previewSampleCount) return;
    const samples = Array.from({ length: previewSampleCount }, (_, index) => entries.get(index + 1)!);
    previewLayouts.current.set(gallery.id, detailPreviewLayout(samples));
    setPreviewLayoutRevision((revision) => revision + 1);
  };

  useEffect(() => {
    const node = previewDialog.current;
    if (!node) return;
    if (previewPage !== null && (previewPageCount === 0 || previewPage > previewPageCount)) {
      setPreviewPage(null);
      return;
    }
    if (previewPage !== null && gallery && !node.open) {
      node.showModal();
      window.requestAnimationFrame(() => previewCloseButton.current?.focus());
    } else if ((previewPage === null || !gallery) && node.open) {
      previewClosingInternally.current = true;
      node.close();
      const target = previewOpener.current;
      previewOpener.current = null;
      window.requestAnimationFrame(() => {
        if (target?.isConnected) target.focus();
        else workspace.current?.querySelector<HTMLElement>("[role='tab'][aria-selected='true']")?.focus();
      });
    }
  }, [gallery, previewPage, previewPageCount]);

  if (!tabs.length) return null;

  return (
    <>
      {minimized ? (
        <button ref={restoreButton} type="button" className="detail-restore" onClick={onRestore}>
          <FluentIcon glyph="\uE8A7" />
          <span>상세 탭 {tabs.length}</span>
        </button>
      ) : null}
      {!minimized && gallery ? (
        <section ref={workspace} className="detail-workspace" aria-label={`${gallery.title} 상세`}>
          <div className="detail-tabbar">
            <div className="detail-tabs" role="tablist">
              {tabs.map((id, index) => {
                const tab = galleries.get(id);
                if (!tab) return null;
                return (
                  <div key={id} role="presentation" className={`detail-tab${id === activeId ? " is-active" : ""}`}>
                    <button
                      type="button"
                      role="tab"
                      id={`detail-tab-${id}`}
                      aria-controls={`detail-panel-${id}`}
                      tabIndex={id === activeId ? 0 : -1}
                      aria-selected={id === activeId}
                      className="tab-activate"
                      onClick={() => onActivate(id)}
                      onKeyDown={(event) => navigateTabs(event, index)}
                    >
                      {tab.title}
                    </button>
                    <button
                      type="button"
                      className="tab-close"
                      aria-label={`${tab.title} 탭 닫기`}
                      onClick={(event) => {
                        event.stopPropagation();
                        onClose(id);
                        window.requestAnimationFrame(() => {
                          workspace.current?.querySelector<HTMLElement>("[role='tab'][aria-selected='true']")?.focus();
                        });
                      }}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            </div>
            <button type="button" className="icon-button small" title="상세 최소화" aria-label="상세 최소화" onClick={onMinimize}>
              <FluentIcon glyph="\uE921" />
            </button>
            <button type="button" className="icon-button small" title="상세 전체 닫기" aria-label="상세 전체 닫기" onClick={onCloseAll}>
              <FluentIcon glyph="\uE711" />
            </button>
          </div>
          <div
            className="detail-body"
            ref={detailBody}
            id={`detail-panel-${gallery.id}`}
            role="tabpanel"
            aria-labelledby={`detail-tab-${gallery.id}`}
          >
            <div className="detail-layout">
              <section className="detail-media">
                <GalleryThumbnail
                  className="detail-cover"
                  thumbnailKey={galleryCoverThumbnailKey(gallery)}
                  consumer="detail"
                  priority="critical"
                  client={thumbnailClient}
                  sizing="intrinsic"
                  expectedAspectRatio={gallery.thumbnailWidth !== undefined && gallery.thumbnailHeight !== undefined
                    ? { width: gallery.thumbnailWidth, height: gallery.thumbnailHeight }
                    : undefined}
                  alt={`${gallery.title} 표지`}
                />
                <div
                  className="preview-grid"
                  data-preview-columns={previewLayout.columns}
                  data-preview-orientation={previewLayout.orientation}
                >
                  {Array.from({ length: previewPageCount }, (_, index) => (
                    <button
                      key={index}
                      type="button"
                      className="preview-thumb"
                      title={`${index + 1}페이지 확대`}
                      onClick={(event) => {
                        previewOpener.current = event.currentTarget;
                        setPreviewPage(index + 1);
                      }}
                    >
                      <GalleryThumbnail
                        as="span"
                        thumbnailKey={sourcePageThumbnailKey(gallery, index + 1)}
                        consumer="detail"
                        priority={index < DETAIL_ORIENTATION_SAMPLE_SIZE ? "visible" : "prefetch"}
                        client={thumbnailClient}
                        sizing="intrinsic"
                        onTerminalSnapshot={(terminal) => recordPreviewTerminal(index + 1, terminal)}
                        alt={`${gallery.title} ${index + 1}페이지 미리보기`}
                      />
                      <span>{index + 1}</span>
                    </button>
                  ))}
                </div>
              </section>
              <section className="detail-info">
                <div className="detail-title-row">
                  <div>
                    <span className="eyebrow">FLOATING DETAIL</span>
                    <h2>
                      {gallery.title}
                      <br />
                      {gallery.subtitle}
                    </h2>
                    <p>#{gallery.id} · {gallery.pages} pages</p>
                  </div>
                  <button type="button" className="icon-button" title="다운로드" aria-label="다운로드" onClick={() => onQueue(gallery.id)}>
                    <FluentIcon glyph="\uE896" />
                  </button>
                </div>
                <div className="metadata-grid">
                  <MetadataBox label="작가" values={[gallery.artist]} type="artist" favorite={gallery.favorite} onSearch={onMetadataSearch} onFavorite={onMetadataFavorite} />
                  <MetadataBox label="그룹" values={gallery.group ? [gallery.group] : []} type="group" favoriteMetadata={favoriteMetadata} onSearch={onMetadataSearch} onFavorite={onMetadataFavorite} />
                  <MetadataBox label="언어" values={[gallery.language]} type="language" onSearch={onMetadataSearch} onFavorite={onMetadataFavorite} />
                  <MetadataBox label="시리즈" values={gallery.series ?? []} type="series" favoriteMetadata={favoriteMetadata} onSearch={onMetadataSearch} onFavorite={onMetadataFavorite} />
                  <MetadataBox label="캐릭터" values={gallery.characters ?? []} type="character" favoriteMetadata={favoriteMetadata} onSearch={onMetadataSearch} onFavorite={onMetadataFavorite} />
                  <div className="metadata-box tags-box">
                    <span>태그</span>
                    <div className="metadata-value">
                      {sortGalleryTags(gallery.tags, favoriteMetadata).map((tag) => (
                        <MetadataChip key={tag.value} value={tag.value} kind="tag" favorite={tag.favorite} onSearch={onMetadataSearch} onToggleFavorite={onMetadataFavorite} />
                      ))}
                    </div>
                  </div>
                </div>
                <section className="related-section">
                  <div className="section-heading">
                    <h3>Related galleries</h3>
                    <span>{Math.min(5, gallery.relatedIds?.length ?? 0)}</span>
                  </div>
                  <div className="related-list">
                    {(gallery.relatedIds ?? [])
                      .flatMap((id) => {
                        const item = galleries.get(id);
                        return item ? [item] : [];
                      })
                      .slice(0, 5)
                      .map((item) => (
                        <article
                          key={item.id}
                          className="related-card"
                          style={{ "--related-preview-width": `${relatedPreviewWidth}px` } as CSSProperties}
                          title="더블클릭 또는 우클릭으로 상세 열기"
                          onDoubleClick={() => onOpenRelated(item.id, gallery.id)}
                          onContextMenu={(event) => {
                            event.preventDefault();
                            onOpenRelated(item.id, gallery.id);
                          }}
                        >
                          <GalleryThumbnail
                            className="related-cover"
                            thumbnailKey={galleryCoverThumbnailKey(item)}
                            consumer="detail"
                            priority="visible"
                            client={thumbnailClient}
                            sizing="intrinsic"
                            expectedAspectRatio={item.thumbnailWidth !== undefined && item.thumbnailHeight !== undefined
                              ? { width: item.thumbnailWidth, height: item.thumbnailHeight }
                              : undefined}
                            alt={`${item.title} 표지`}
                          />
                          <div className="related-copy">
                            <strong>{item.title} | {item.subtitle}</strong>
                            <div className="related-byline">
                              <MetadataChip value={`artist:${item.artist}`} label={item.artist} kind="byline" favorite={item.favorite} onSearch={onMetadataSearch} onToggleFavorite={onMetadataFavorite} />
                              {item.group ? <MetadataChip value={`group:${item.group}`} label={item.group} kind="byline" favorite={favoriteMetadata.has(`group:${item.group}`)} onSearch={onMetadataSearch} onToggleFavorite={onMetadataFavorite} /> : null}
                            </div>
                            <div className="tag-list">
                              {sortGalleryTags(item.tags, favoriteMetadata).slice(0, 4).map((tag) => (
                                <MetadataChip key={tag.value} value={tag.value} kind="tag" favorite={tag.favorite} onSearch={onMetadataSearch} onToggleFavorite={onMetadataFavorite} />
                              ))}
                            </div>
                          </div>
                          <div className="related-meta">
                            <button
                              type="button"
                              className="related-open-command"
                              aria-label={`${item.title} 상세 열기`}
                              onClick={(event) => {
                                event.stopPropagation();
                                onOpenRelated(item.id, gallery.id);
                              }}
                            >
                              열기
                            </button>
                            <span>{item.pages}p</span><span>#{item.id}</span>
                          </div>
                        </article>
                      ))}
                  </div>
                </section>
              </section>
            </div>
          </div>
        </section>
      ) : null}
      <dialog
        ref={previewDialog}
        className="page-preview-dialog"
        aria-labelledby="page-preview-title"
        onCancel={(event) => {
          event.preventDefault();
          setPreviewPage(null);
        }}
        onClose={() => {
          if (previewClosingInternally.current) {
            previewClosingInternally.current = false;
            return;
          }
          setPreviewPage(null);
          const target = previewOpener.current;
          previewOpener.current = null;
          window.requestAnimationFrame(() => target?.isConnected && target.focus());
        }}
      >
        {gallery && previewPage !== null ? (
          <div className="page-preview-dialog-body">
            <header className="dialog-header">
              <div>
                <span className="eyebrow">PAGE PREVIEW</span>
                <h2 id="page-preview-title">{gallery.title} · {previewPage}페이지</h2>
              </div>
              <button ref={previewCloseButton} type="button" className="icon-button small" title="페이지 미리보기 닫기" aria-label="페이지 미리보기 닫기" onClick={() => setPreviewPage(null)}>
                <FluentIcon glyph="\uE711" />
              </button>
            </header>
            <GalleryThumbnail
              className="page-preview-media"
              thumbnailKey={sourcePageThumbnailKey(gallery, previewPage)}
              consumer="detail"
              priority="critical"
              client={thumbnailClient}
              alt={`${gallery.title} ${previewPage}페이지 확대 미리보기`}
            />
          </div>
        ) : null}
      </dialog>
    </>
  );
}
