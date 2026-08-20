import {
  memo,
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import type { Gallery, GalleryId, ViewId } from "../core/types";
import { languagePresentation } from "../data/languages";
import {
  galleryCoverThumbnailKey,
  thumbnailConsumerForView,
  type ThumbnailClient,
  type ThumbnailPriority,
} from "../thumbnail";
import { GalleryThumbnail } from "./GalleryThumbnail";
import { GalleryStatusIcon } from "./GalleryStatusIcon";
import { MetadataChip } from "./MetadataChip";
import { fitTagChipCount, splitGalleryTitle } from "./galleryCardLayout";

type GalleryCardProps = {
  gallery: Gallery;
  thumbnailPriority?: ThumbnailPriority;
  thumbnailClient?: ThumbnailClient;
  view: ViewId;
  selected: boolean;
  selectionContext: boolean;
  favoriteMetadata: ReadonlySet<string>;
  duplicateCandidateCount?: number;
  onSelect: (id: GalleryId, modifiers: { ctrlKey: boolean; shiftKey: boolean }) => void;
  onOpenDetail: (id: GalleryId) => void;
  onOpenArtifact: (id: GalleryId) => void;
  onOpenReview: (id: GalleryId) => void;
  onStatusDetail: (id: GalleryId) => void;
  onMetadataSearch: (value: string) => void;
  onMetadataFavorite: (value: string) => void;
};

const workLabel: Partial<Record<NonNullable<Gallery["download"]>["state"], string>> = {
  queued: "대기",
  resolving_metadata: "정보 확인 중",
  downloading: "다운로드 중",
  hashing: "해시 중",
  verifying: "검사 중",
  retry_wait: "재시도 대기",
  review_required: "검토 필요",
  interrupted: "중단됨",
  failed: "실패",
  completed: "완료",
  quarantined: "격리됨",
  cancelled: "취소됨",
};

function GalleryCardComponent({
  gallery,
  thumbnailPriority = "prefetch",
  thumbnailClient,
  view,
  selected,
  selectionContext,
  favoriteMetadata,
  duplicateCandidateCount = 0,
  onSelect,
  onOpenDetail,
  onOpenArtifact,
  onOpenReview,
  onStatusDetail,
  onMetadataSearch,
  onMetadataFavorite,
}: GalleryCardProps) {
  const download = gallery.download;
  const gestureSelectionContext = useRef(selectionContext);
  const progress = Math.min(
    100,
    Math.max(0, download?.state === "completed" ? 100 : download?.progress ?? 0),
  );
  const statusClass = ["failed", "interrupted"].includes(download?.state ?? "") ? " failed" : "";
  const language = languagePresentation[gallery.language];
  const { primary: displayTitle, secondary: subtitle } = splitGalleryTitle(gallery.title, gallery.subtitle);
  const thumbnailKey = galleryCoverThumbnailKey(gallery);
  const thumbnailConsumer = thumbnailConsumerForView(view);
  const tagLayoutKey = `${gallery.title}\u0000${gallery.subtitle ?? ""}\u0000${gallery.tags.join("\u0001")}`;
  const [tagLayout, setTagLayout] = useState<{ key: string; count: number } | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const tagListRef = useRef<HTMLDivElement>(null);
  const tagChipRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const visibleTagCount = tagLayout?.key === tagLayoutKey ? tagLayout.count : gallery.tags.length;
  const visibleTags = gallery.tags.slice(0, visibleTagCount);
  const hasDuplicateCandidates = duplicateCandidateCount > 0;
  const iconOnlyStatus = hasDuplicateCandidates || download?.state === "downloading" || download?.state === "review_required";
  const cardStatusClass = download?.state === "completed"
    ? " is-complete"
    : download?.state === "downloading"
      ? " is-downloading"
      : hasDuplicateCandidates || ["review_required", "interrupted", "failed", "quarantined", "cancelled"].includes(download?.state ?? "")
        ? " has-problem"
        : "";
  const statusLabel = selectionContext
    ? `${gallery.title}만 선택`
    : hasDuplicateCandidates
      ? `${gallery.title}, 중복 후보 ${duplicateCandidateCount}개, 검토 열기`
    : download?.state === "downloading"
    ? `${gallery.title}, 다운로드 중 ${progress}%, 작업 상태 열기`
    : download?.state === "review_required"
      ? `${gallery.title}, 중복 의심, 검토 열기`
      : download ? `${gallery.title}, ${workLabel[download.state]}, 작업 상태 열기` : "";

  const invalidateTagLayout = useCallback(() => {
    setTagLayout((current) => current ? null : current);
  }, []);

  useLayoutEffect(() => {
    const content = contentRef.current;
    const tagList = tagListRef.current;
    if (!content || !tagList || !gallery.tags.length || tagLayout?.key === tagLayoutKey) return;

    const style = getComputedStyle(tagList);
    const gapX = Number.parseFloat(style.columnGap) || 0;
    const gapY = Number.parseFloat(style.rowGap) || 0;
    if (tagList.clientWidth <= 0 || tagList.clientHeight <= 0) {
      setTagLayout({ key: tagLayoutKey, count: gallery.tags.length });
      return;
    }
    const measurements = tagChipRefs.current.slice(0, gallery.tags.length).map((chip) => {
      const rect = chip?.getBoundingClientRect();
      return { width: rect?.width ?? 0, height: rect?.height ?? 0 };
    });
    const count = fitTagChipCount(measurements, tagList.clientWidth, tagList.clientHeight, gapX, gapY);
    setTagLayout({ key: tagLayoutKey, count });
  }, [gallery.tags.length, tagLayout, tagLayoutKey]);

  useLayoutEffect(() => {
    const content = contentRef.current;
    if (!content || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(invalidateTagLayout);
    observer.observe(content);
    let disposed = false;
    document.fonts?.ready.then(() => {
      if (!disposed) invalidateTagLayout();
    });
    return () => {
      disposed = true;
      observer.disconnect();
    };
  }, [invalidateTagLayout]);

  const selectsInsteadOfActivating = (event: Pick<MouseEvent<HTMLElement>, "ctrlKey" | "shiftKey">) =>
    selectionContext || event.ctrlKey || event.shiftKey;

  const selectFromInteractiveTarget = (event: MouseEvent<HTMLElement>) => {
    if (!selectsInsteadOfActivating(event)) return false;
    event.preventDefault();
    event.stopPropagation();
    if (event.detail <= 1) onSelect(gallery.id, event);
    return true;
  };

  const openStatus = (event: MouseEvent<HTMLButtonElement>) => {
    if (selectFromInteractiveTarget(event)) return;
    event.stopPropagation();
    if (hasDuplicateCandidates || download?.state === "review_required") onOpenReview(gallery.id);
    else onStatusDetail(gallery.id);
  };

  const selectFromKeyboard = (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === " ") {
      event.preventDefault();
      onSelect(gallery.id, event);
    }
  };

  return (
    <article
      className={`gallery-card${selected ? " is-selected" : ""}${gallery.favorite ? " is-favorite" : ""}${cardStatusClass}`}
      data-gallery-id={gallery.id}
      style={{ "--download-progress": `${progress}%` } as CSSProperties}
      role="listitem"
      tabIndex={0}
      aria-label={[
        gallery.title,
        subtitle || null,
        download?.state === "completed" ? "다운로드 완료" : null,
        selected ? "선택됨" : "선택 안 됨",
      ].filter(Boolean).join(", ")}
      onKeyDown={selectFromKeyboard}
      onClick={(event) => {
        if ((event.target as Element).closest("button")) return;
        if (event.detail > 1) return;
        gestureSelectionContext.current = selectsInsteadOfActivating(event);
        onSelect(gallery.id, event);
      }}
      onDoubleClick={(event) => {
        if ((event.target as Element).closest("button")) return;
        if (gestureSelectionContext.current || event.ctrlKey || event.shiftKey) {
          gestureSelectionContext.current = false;
          return;
        }
        gestureSelectionContext.current = false;
        event.currentTarget.focus();
        if (view === "downloads") onOpenArtifact(gallery.id);
        else onOpenDetail(gallery.id);
      }}
      onContextMenu={(event) => {
        if ((event.target as Element).closest("button")) return;
        event.preventDefault();
        event.currentTarget.focus();
        if (view === "downloads" && hasDuplicateCandidates) onOpenReview(gallery.id);
        else onOpenDetail(gallery.id);
      }}
    >
      <span className="selection-indicator" aria-hidden="true">
        <svg viewBox="0 0 16 16" focusable="false">
          <path d="m3.5 8.1 2.8 2.8 6.2-6.2" />
        </svg>
      </span>
      <GalleryThumbnail
        className="cover"
        thumbnailKey={thumbnailKey}
        consumer={thumbnailConsumer}
        priority={thumbnailPriority}
        client={thumbnailClient}
        sizing="intrinsic"
        expectedAspectRatio={gallery.thumbnailWidth !== undefined && gallery.thumbnailHeight !== undefined
          ? { width: gallery.thumbnailWidth, height: gallery.thumbnailHeight }
          : undefined}
        alt={`${gallery.title} 표지`}
      >
        {download ? <span className="status-wash" aria-hidden="true" /> : null}
        {language.icon || language.fallback ? (
          <span className="language-flag">
            {language.icon ? <img src={language.icon} alt={language.label} /> : <span>{language.fallback}</span>}
          </span>
        ) : null}
        {view === "explore" && download?.state === "completed" ? (
          <span className="download-check" title="다운로드 완료">
            <GalleryStatusIcon kind="complete" />
          </span>
        ) : null}
        {hasDuplicateCandidates || (download && !["completed", "quarantined"].includes(download.state)) ? (
          <button
            type="button"
            className={`status-pill${statusClass}${iconOnlyStatus ? ` icon-only is-${hasDuplicateCandidates ? "review_required" : download?.state}` : ""}${hasDuplicateCandidates ? " has-duplicate-count" : ""}`}
            title={selectionContext ? `${gallery.title}만 선택` : hasDuplicateCandidates ? `중복 후보 ${duplicateCandidateCount}개 · 클릭하여 검토` : download?.state === "downloading" ? `다운로드 중 · ${progress}%` : download?.state === "review_required" ? "중복 의심 · 클릭하여 검토" : download ? workLabel[download.state] : "작업 상태"}
            aria-label={statusLabel}
            onClick={openStatus}
          >
            {hasDuplicateCandidates ? (
              <><GalleryStatusIcon kind="warning" /><span className="duplicate-count">{duplicateCandidateCount}</span></>
            ) : download?.state === "downloading" ? (
              <GalleryStatusIcon kind="downloading" />
            ) : download?.state === "review_required" ? (
              <GalleryStatusIcon kind="warning" />
            ) : download ? workLabel[download.state] : null}
          </button>
        ) : null}
        {view === "downloads" ? (
          <div
            className="progress-track"
            role="progressbar"
            aria-label={`${download ? workLabel[download.state] : "다운로드"} 진행률`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progress}
          >
            <span style={{ width: `${progress}%` }} />
          </div>
        ) : null}
      </GalleryThumbnail>
      <div ref={contentRef} className="card-content">
        <div className="card-title" title={gallery.title}>
          <strong title={gallery.title}>{displayTitle}</strong>
          {subtitle ? <span className="title-sub">{subtitle}</span> : null}
        </div>
        <div className="card-byline" aria-label="작가 및 그룹">
          <MetadataChip
            value={`artist:${gallery.artist}`}
            label={gallery.artist}
            kind="byline"
            favorite={gallery.favorite}
            onClickCapture={selectFromInteractiveTarget}
            onSearch={onMetadataSearch}
            onToggleFavorite={onMetadataFavorite}
          />
          {gallery.group ? (
            <>
              <span className="byline-separator" aria-hidden="true">·</span>
              <MetadataChip
                value={`group:${gallery.group}`}
                label={gallery.group}
                kind="byline"
                favorite={favoriteMetadata.has(`group:${gallery.group}`)}
                onClickCapture={selectFromInteractiveTarget}
                onSearch={onMetadataSearch}
                onToggleFavorite={onMetadataFavorite}
              />
            </>
          ) : null}
        </div>
        <div ref={tagListRef} className="tag-list" aria-label={`태그: ${gallery.tags.join(", ")}`}>
          {visibleTags.map((tag, index) => (
            <MetadataChip
              key={tag}
              ref={(chip) => { tagChipRefs.current[index] = chip; }}
              value={tag}
              favorite={favoriteMetadata.has(tag)}
              kind="tag"
              onClickCapture={selectFromInteractiveTarget}
              onSearch={onMetadataSearch}
              onToggleFavorite={onMetadataFavorite}
            />
          ))}
        </div>
        <div className="meta-bottom">
          <span>{gallery.pages}p</span>
          <span>#{gallery.id}</span>
        </div>
      </div>
    </article>
  );
}

export const GalleryCard = memo(GalleryCardComponent);
