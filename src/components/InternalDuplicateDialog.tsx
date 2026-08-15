import { useEffect, useMemo, useRef, useState } from "react";
import type {
  InternalDuplicateReview,
  InternalRemovalPlan,
  InternalRemovalPlanRequest,
} from "../api/contracts";
import { artifactPageThumbnailKey, type ThumbnailClient } from "../thumbnail";
import { FluentIcon } from "./FluentIcon";
import { GalleryThumbnail } from "./GalleryThumbnail";

type InternalDuplicateDialogProps = {
  open: boolean;
  review?: InternalDuplicateReview;
  plan?: InternalRemovalPlan;
  loading?: boolean;
  busy?: boolean;
  error?: string | null;
  thumbnailClient?: ThumbnailClient;
  onClose: () => void;
  onRetry: () => void;
  onRescan: () => void;
  onPlan: (request: InternalRemovalPlanRequest) => void;
  onApply: (plan: InternalRemovalPlan) => void;
  onUndo: (recordIds: string[]) => void;
};

const percent = (value: number): string => `${Math.round(Math.min(1, Math.max(0, value)) * 100)}%`;
const bytes = (value: number): string => new Intl.NumberFormat("ko-KR", {
  style: "unit",
  unit: value >= 1024 * 1024 ? "megabyte" : "kilobyte",
  maximumFractionDigits: 1,
}).format(value / (value >= 1024 * 1024 ? 1024 * 1024 : 1024));

export function InternalDuplicateDialog({
  open,
  review,
  plan,
  loading = false,
  busy = false,
  error = null,
  thumbnailClient,
  onClose,
  onRetry,
  onRescan,
  onPlan,
  onApply,
  onUndo,
}: InternalDuplicateDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [keepPages, setKeepPages] = useState<Record<string, number>>({});

  useEffect(() => {
    if (!open) return;
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (!dialog.current?.open) {
      if (typeof dialog.current?.showModal === "function") dialog.current.showModal();
      else dialog.current?.setAttribute("open", "");
    }
    window.requestAnimationFrame(() => closeButton.current?.focus());
    return () => {
      if (dialog.current?.open) {
        if (typeof dialog.current.close === "function") dialog.current.close();
        else dialog.current.removeAttribute("open");
      }
      const target = opener.current;
      opener.current = null;
      if (target?.isConnected) target.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!review) return;
    setKeepPages(Object.fromEntries(review.groups.map((group) => [group.groupId, group.recommendedKeepSourcePage])));
  }, [review]);

  const activeRecords = useMemo(
    () => review?.quarantineRecords.filter((record) => record.state !== "restored") ?? [],
    [review?.quarantineRecords],
  );
  const activePlan = plan?.selections.every(
    (selection) => (keepPages[selection.groupId] ?? selection.keepSourcePage) === selection.keepSourcePage,
  ) ? plan : undefined;
  const blocks = useMemo(() => {
    const grouped = new Map<string, NonNullable<typeof review>["groups"]>();
    for (const group of review?.groups ?? []) {
      const rows = grouped.get(group.blockId) ?? [];
      rows.push(group);
      grouped.set(group.blockId, rows);
    }
    return [...grouped.entries()];
  }, [review?.groups]);

  const preview = () => {
    if (!review) return;
    onPlan({
      entryId: review.entryId,
      selections: review.groups.map((group) => {
        const keepSourcePage = keepPages[group.groupId] ?? group.recommendedKeepSourcePage;
        return {
          groupId: group.groupId,
          expectedRevision: group.revision,
          keepSourcePage,
          removeSourcePages: group.pages
            .map((page) => page.sourcePage)
            .filter((page) => page !== keepSourcePage),
        };
      }),
    });
  };

  if (!open && !review) return null;
  return (
    <dialog
      ref={dialog}
      className="review-dialog internal-review-dialog"
      aria-labelledby="internal-review-title"
      aria-busy={loading || busy}
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      onClose={onClose}
    >
      <div className="review-form">
        <header className="dialog-header">
          <div>
            <span className="eyebrow">INTERNAL PAGE REVIEW</span>
            <h2 id="internal-review-title">앨범 내부 중복 페이지 검토</h2>
          </div>
          <button ref={closeButton} type="button" className="icon-button small" aria-label="닫기" onClick={onClose}>
            <FluentIcon glyph="\uE711" />
          </button>
        </header>

        {loading && !review ? (
          <div className="review-loading" role="status"><span className="spinner" /> 저장된 내부 중복 근거를 불러오는 중</div>
        ) : error && !review ? (
          <div className="review-loading" role="alert">
            <strong>내부 중복 검토를 불러오지 못했습니다.</strong>
            <span>{error}</span>
            <button type="button" className="text-button" onClick={onRetry}>다시 불러오기</button>
          </div>
        ) : review ? (
          <div className="review-scroll internal-review-scroll">
            {error ? <div className="inline-error" role="alert"><span>{error}</span><button type="button" className="text-button" onClick={onRetry}>최신 내용 다시 불러오기</button></div> : null}
            <div className="review-summary">
              <span className="review-signal">{review.groups.length}개 동기화 행</span>
              <strong>{review.title}</strong>
              <span>원본 페이지 번호는 바뀌지 않습니다. 선택한 파일은 앨범 폴더 안의 격리 영역으로 이동하며 자동 영구 삭제되지 않습니다.</span>
            </div>

            {blocks.length ? blocks.map(([blockId, groups], blockIndex) => (
              <section className="internal-scene-block" key={blockId} aria-labelledby={`internal-block-${blockIndex}`}>
                <header>
                  <div><h3 id={`internal-block-${blockIndex}`}>장면 묶음 {blockIndex + 1}</h3><span>{groups[0]?.relation === "exact" ? "SHA-256 정확 일치" : "연속 장면 시각 일치"}</span></div>
                  <span>{groups.length}행</span>
                </header>
                <div className="internal-scene-rows">
                  {groups.map((group) => (
                    <fieldset key={group.groupId} className="internal-scene-row">
                      <legend>행 {group.sequenceIndex + 1} · 신뢰도 {percent(group.confidence)}</legend>
                      <div className="internal-page-options">
                        {group.pages.map((page, pageIndex) => {
                          const selected = (keepPages[group.groupId] ?? group.recommendedKeepSourcePage) === page.sourcePage;
                          return (
                            <label key={page.sourcePage} className={`internal-page-option${selected ? " is-kept" : ""}`}>
                              <input
                                type="radio"
                                name={`keep-${group.groupId}`}
                                checked={selected}
                                onChange={() => setKeepPages((current) => ({ ...current, [group.groupId]: page.sourcePage }))}
                              />
                              <GalleryThumbnail
                                className="internal-page-image"
                                thumbnailKey={artifactPageThumbnailKey(review.entryId, page.sourcePage, page.sourcePage - 1)}
                                consumer="review"
                                priority={blockIndex === 0 && pageIndex < 4 ? "visible" : "prefetch"}
                                client={thumbnailClient}
                                alt={`${review.title} 원본 ${page.sourcePage}페이지`}
                              ><span>{page.sourcePage}p</span></GalleryThumbnail>
                              <strong>{selected ? "유지" : "격리 예정"} · 원본 {page.sourcePage}p</strong>
                              <small>{page.exactSha256 ? "파일 일치" : `시각 ${percent(page.visualSimilarity)} · detail ${page.detailHashDistance}`}</small>
                            </label>
                          );
                        })}
                      </div>
                    </fieldset>
                  ))}
                </div>
              </section>
            )) : <div className="review-empty">현재 검토할 내부 중복 페이지가 없습니다.</div>}

            {activePlan ? (
              <section className="internal-plan-summary" aria-live="polite">
                <div><strong>격리 계획 확인</strong><span>{activePlan.filesToQuarantine}개 파일 · {bytes(activePlan.bytesToQuarantine)}</span></div>
                <p>이 작업은 파일을 영구 삭제하지 않습니다. 적용 후 아래 격리 이력에서 되돌릴 수 있습니다.</p>
                <button type="button" className="text-button danger-button" disabled={busy} onClick={() => onApply(activePlan)}>계획대로 격리 적용</button>
              </section>
            ) : null}

            {activeRecords.length ? (
              <section className="internal-quarantine-history">
                <div><h3>격리 이력</h3><span>{activeRecords.length}개 파일</span></div>
                <ul>{activeRecords.map((record) => <li key={record.recordId}><strong>원본 {record.sourcePage}p</strong><span>{record.state === "quarantined" ? "격리됨" : "처리 중"}</span></li>)}</ul>
                <button type="button" className="text-button" disabled={busy || activeRecords.some((record) => record.state !== "quarantined")} onClick={() => onUndo(activeRecords.map((record) => record.recordId))}>선택 앨범의 격리 페이지 모두 되돌리기</button>
              </section>
            ) : null}
          </div>
        ) : null}

        <div className="review-actions">
          <button type="button" className="text-button" disabled={busy} onClick={onRescan}><FluentIcon glyph="\uE9D9" /> 다시 검사</button>
          <span />
          <button type="button" className="text-button" disabled={!review?.groups.length || busy} onClick={preview}>격리 계획 미리보기</button>
          <button type="button" className="text-button" onClick={onClose}>닫기</button>
        </div>
      </div>
    </dialog>
  );
}
