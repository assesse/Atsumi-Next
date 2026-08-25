import { useEffect, useMemo, useRef, useState } from "react";
import type {
  DownloadOverlapCandidate,
  DownloadOverlapDecisionRequest,
  DownloadOverlapGalleryRef,
  DownloadOverlapReview,
} from "../api/contracts";
import { artifactPageThumbnailKey, type ThumbnailClient } from "../thumbnail";
import { FluentIcon } from "./FluentIcon";
import { GalleryThumbnail } from "./GalleryThumbnail";

type Props = {
  open: boolean;
  review?: DownloadOverlapReview;
  loading?: boolean;
  error?: string | null;
  decisionPending?: boolean;
  browserFixture?: boolean;
  thumbnailClient?: ThumbnailClient;
  onClose: () => void;
  onRetry: () => void;
  onDecision: (request: DownloadOverlapDecisionRequest) => void;
};

const relationLabel: Record<DownloadOverlapCandidate["relation"], string> = {
  near_equivalent: "거의 같은 판본",
  incoming_contains_existing: "새 다운로드가 기존 작품을 포함",
  existing_contains_incoming: "기존 작품이 새 다운로드를 포함",
  partial_overlap: "강한 부분 겹침",
  translation_edition: "번역·가공 판본으로 보이는 일치",
};

const percent = (value: number) => `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;

function ArtifactSummary({ gallery, label, page, thumbnailClient }: {
  gallery: DownloadOverlapGalleryRef;
  label: string;
  page: number;
  thumbnailClient?: ThumbnailClient;
}) {
  return (
    <article className="download-overlap-artifact">
      <GalleryThumbnail
        className="download-overlap-cover"
        thumbnailKey={artifactPageThumbnailKey(gallery.entryId, page, Number(gallery.galleryId) % 6)}
        consumer="review"
        priority="critical"
        client={thumbnailClient}
        alt={`${gallery.title} ${page}페이지`}
      />
      <div>
        <span className="eyebrow">{label}</span>
        <strong>{gallery.title}</strong>
        <span>{gallery.artists.join(", ") || "작가 정보 없음"}</span>
        <span>#{gallery.galleryId} · {gallery.pageCount}p</span>
      </div>
    </article>
  );
}

export function DownloadOverlapReviewDialog({
  open,
  review,
  loading = false,
  error = null,
  decisionPending = false,
  browserFixture = false,
  thumbnailClient,
  onClose,
  onRetry,
  onDecision,
}: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const [candidateId, setCandidateId] = useState<string | null>(null);

  const pendingCandidates = useMemo(
    () => review?.candidates.filter((candidate) => candidate.decision === undefined) ?? [],
    [review],
  );
  const candidate = review?.candidates.find((item) => item.candidateId === candidateId)
    ?? pendingCandidates[0]
    ?? review?.candidates[0];

  useEffect(() => {
    setCandidateId(review?.candidates.find((item) => item.decision === undefined)?.candidateId ?? review?.candidates[0]?.candidateId ?? null);
  }, [review?.reviewId, review?.revision]);

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
      if (opener.current?.isConnected) opener.current.focus();
      opener.current = null;
    };
  }, [open]);

  const decide = (action: DownloadOverlapDecisionRequest["action"], currentCandidateId?: string) => {
    if (!review || decisionPending) return;
    onDecision({
      reviewId: review.reviewId,
      expectedRevision: review.revision,
      action,
      ...(currentCandidateId ? { candidateId: currentCandidateId } : {}),
    });
  };

  if (!open && !review) return null;

  return (
    <dialog
      className="review-dialog download-overlap-dialog"
      ref={dialog}
      aria-labelledby="download-overlap-title"
      aria-describedby={review ? "download-overlap-safety" : undefined}
      aria-busy={loading || decisionPending}
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      onClose={onClose}
    >
      <div className="review-form">
        <header className="dialog-header">
          <div>
            <span className="eyebrow">DOWNLOAD OVERLAP REVIEW</span>
            <h2 id="download-overlap-title">다운로드 판본 중복 검토</h2>
          </div>
          <button ref={closeButton} type="button" className="icon-button small" title="닫기" aria-label="닫기" onClick={onClose}>
            <FluentIcon glyph="\uE711" />
          </button>
        </header>

        {loading && !review ? (
          <div className="review-loading" role="status"><span className="spinner" /> 판본 겹침 근거를 불러오는 중</div>
        ) : error && !review ? (
          <div className="review-loading" role="alert">
            <FluentIcon glyph="\uE7BA" />
            <strong>다운로드 검토를 불러오지 못했습니다.</strong>
            <span>{error}</span>
            <button type="button" className="text-button" onClick={onRetry}>다시 불러오기</button>
          </div>
        ) : review && candidate ? (
          <div className="review-scroll">
            {error ? <div className="inline-error review-inline-error" role="alert">{error}</div> : null}
            <div className="review-summary">
              <span className="review-signal">완료 전 일시 정지</span>
              <strong>{relationLabel[candidate.relation]} · 신뢰도 {percent(candidate.confidence)}</strong>
              <span id="download-overlap-safety">
                새 파일은 모두 검증됐지만 아직 완료 manifest를 만들지 않았습니다. 어떤 선택도 기존 파일을 자동 삭제하거나 대체하지 않습니다.
                {browserFixture ? " · 브라우저 검토 fixture" : ""}
              </span>
            </div>

            {review.candidates.length > 1 ? (
              <div className="download-overlap-candidate-tabs" role="tablist" aria-label="겹침 후보">
                {review.candidates.map((item) => (
                  <button
                    type="button"
                    role="tab"
                    aria-selected={item.candidateId === candidate.candidateId}
                    className={item.candidateId === candidate.candidateId ? "is-active" : ""}
                    key={item.candidateId}
                    onClick={() => setCandidateId(item.candidateId)}
                  >
                    후보 {item.rank} · #{item.existing.galleryId}
                    {item.decision ? <small>처리됨</small> : null}
                  </button>
                ))}
              </div>
            ) : null}

            <div className="download-overlap-columns">
              <ArtifactSummary gallery={candidate.existing} label="기존 보유 앨범" page={candidate.pagePairs[0]?.existingSourcePage ?? 1} thumbnailClient={thumbnailClient} />
              <ArtifactSummary gallery={review.incoming} label="새 다운로드" page={candidate.pagePairs[0]?.incomingSourcePage ?? 1} thumbnailClient={thumbnailClient} />
            </div>

            <dl className="download-overlap-metrics">
              <div><dt>일치 페이지</dt><dd>{candidate.matchedPages}장</dd></div>
              <div><dt>SHA-256 / 시각</dt><dd>{candidate.exactPages} / {candidate.visualPages}</dd></div>
              <div><dt>기존 범위</dt><dd>{percent(candidate.existingCoverage)}</dd></div>
              <div><dt>새 다운로드 범위</dt><dd>{percent(candidate.incomingCoverage)}</dd></div>
              <div><dt>연속 일치</dt><dd>{candidate.longestAlignedRun}장</dd></div>
              <div><dt>고유 페이지</dt><dd>기존 {candidate.existingUniquePages} · 신규 {candidate.incomingUniquePages}</dd></div>
            </dl>

            <details className="match-pairs" open>
              <summary>일치 페이지 비교 · {candidate.pagePairs.length}쌍</summary>
              <div className="pair-strip">
                {candidate.pagePairs.map((pair, index) => (
                  <article className="pair" key={`${pair.existingSourcePage}:${pair.incomingSourcePage}`}>
                    <GalleryThumbnail
                      className="pair-image"
                      thumbnailKey={artifactPageThumbnailKey(candidate.existing.entryId, pair.existingSourcePage, index)}
                      consumer="review"
                      priority={index < 4 ? "visible" : "prefetch"}
                      client={thumbnailClient}
                      alt={`기존 앨범 ${pair.existingSourcePage}페이지`}
                    ><span>기존 {pair.existingSourcePage}p</span></GalleryThumbnail>
                    <GalleryThumbnail
                      className="pair-image"
                      thumbnailKey={artifactPageThumbnailKey(review.incoming.entryId, pair.incomingSourcePage, index)}
                      consumer="review"
                      priority={index < 4 ? "visible" : "prefetch"}
                      client={thumbnailClient}
                      alt={`새 다운로드 ${pair.incomingSourcePage}페이지`}
                    ><span>신규 {pair.incomingSourcePage}p</span></GalleryThumbnail>
                    <div className="pair-metrics">
                      <strong>{pair.exactSha256 ? "SHA-256 일치" : `시각 ${percent(pair.visualSimilarity)}`}</strong>
                      <span>detail {pair.detailHashDistance} · edge {percent(pair.edgeSimilarity)}</span>
                    </div>
                  </article>
                ))}
              </div>
            </details>
          </div>
        ) : null}

        <div className="review-actions download-overlap-actions">
          <button type="button" className="text-button" onClick={onClose}>나중에 검토</button>
          <span />
          <button type="button" className="text-button danger-button" disabled={!review || decisionPending} onClick={() => decide("cancel_incoming")}>새 다운로드 취소</button>
          <button type="button" className="text-button" disabled={!review || !candidate || decisionPending || Boolean(candidate.decision)} onClick={() => candidate && decide("false_positive_continue", candidate.candidateId)}>이 후보는 오탐</button>
          <button type="button" className="primary-button" disabled={!review || decisionPending} onClick={() => decide("continue_keep_both")}>둘 다 보관하고 완료</button>
        </div>
      </div>
    </dialog>
  );
}
