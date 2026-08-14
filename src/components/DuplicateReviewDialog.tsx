import { useEffect, useRef } from "react";
import type { Gallery } from "../core/types";
import {
  galleryCoverThumbnailKey,
  sourcePageThumbnailKey,
  type ThumbnailClient,
} from "../thumbnail";
import { FluentIcon } from "./FluentIcon";
import { GalleryThumbnail } from "./GalleryThumbnail";

type DuplicateReviewDialogProps = {
  open: boolean;
  parent?: Gallery;
  candidate?: Gallery;
  thumbnailClient?: ThumbnailClient;
  onClose: () => void;
  onDecision: (label: string) => void;
  onScan: () => void;
};

const sourcePageCount = (gallery: Gallery): number =>
  Number.isFinite(gallery.pages) ? Math.max(0, Math.floor(gallery.pages)) : 0;

function ReviewCard({ gallery, label, thumbnailClient }: {
  gallery: Gallery;
  label: string;
  thumbnailClient?: ThumbnailClient;
}) {
  return (
    <section className="review-card">
      <h3>{label}</h3>
      <GalleryThumbnail
        className="review-hero"
        thumbnailKey={galleryCoverThumbnailKey(gallery)}
        consumer="review"
        priority="critical"
        client={thumbnailClient}
        alt={`${gallery.title} 표지`}
      />
      <dl className="review-fields">
        <dt>제목</dt><dd>{gallery.title}<br />{gallery.subtitle}</dd>
        <dt>작가</dt><dd>{gallery.artist}</dd>
        <dt>언어</dt><dd>{gallery.language}</dd>
        <dt>페이지</dt><dd>{gallery.pages}p</dd>
        <dt>EH ID</dt><dd>#{gallery.id}</dd>
        <dt>first gid</dt><dd>#{gallery.id - 137} · 일치</dd>
        <dt>parent gid</dt><dd>- · 불일치</dd>
      </dl>
    </section>
  );
}

export function DuplicateReviewDialog({
  open,
  parent,
  candidate,
  thumbnailClient,
  onClose,
  onDecision,
  onScan,
}: DuplicateReviewDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    if (open && parent && candidate) {
      if (!dialog.current?.open) dialog.current?.showModal();
    } else if (dialog.current?.open) dialog.current.close();
  }, [open, parent, candidate]);

  if (!parent || !candidate) return null;
  const parentPageCount = sourcePageCount(parent);
  const candidatePageCount = sourcePageCount(candidate);
  const candidatePageOffset = 2;
  const pairCount = Math.min(
    14,
    parentPageCount,
    Math.max(0, candidatePageCount - candidatePageOffset),
  );

  return (
    <dialog
      className="review-dialog"
      ref={dialog}
      aria-labelledby="review-dialog-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClose={onClose}
    >
      <div className="review-form">
        <header className="dialog-header">
          <div><span className="eyebrow">DUPLICATE REVIEW</span><h2 id="review-dialog-title">중복 작품 판독</h2></div>
          <button type="button" className="icon-button small" title="닫기" aria-label="닫기" onClick={onClose}><FluentIcon glyph="\uE711" /></button>
        </header>
        <div className="review-summary">
          <span className="review-signal">해시 판독</span>
          <strong>후보가 부모를 포함 · 82% · {pairCount}쌍 비교 가능</strong>
          <span>자동 삭제하지 않습니다. 일치 페이지와 관계 정보를 확인하세요.</span>
        </div>
        <div className="review-columns">
          <ReviewCard gallery={parent} label="부모 상세" thumbnailClient={thumbnailClient} />
          <ReviewCard gallery={candidate} label="후보 상세" thumbnailClient={thumbnailClient} />
        </div>
        <details className="match-pairs" open>
          <summary>일치 페이지 전체보기 · {pairCount}쌍</summary>
          <div className="pair-strip">
            {Array.from({ length: pairCount }, (_, index) => (
              <div key={index} className="pair">
                <GalleryThumbnail
                  className="pair-image"
                  thumbnailKey={sourcePageThumbnailKey(parent, index + 1)}
                  consumer="review"
                  priority={index < 4 ? "visible" : "prefetch"}
                  client={thumbnailClient}
                  alt={`${parent.title} ${index + 1}페이지 미리보기`}
                >
                  <span>{index + 1}</span>
                </GalleryThumbnail>
                <GalleryThumbnail
                  className="pair-image"
                  thumbnailKey={sourcePageThumbnailKey(candidate, index + 1 + candidatePageOffset)}
                  consumer="review"
                  priority={index < 4 ? "visible" : "prefetch"}
                  client={thumbnailClient}
                  alt={`${candidate.title} ${index + 1 + candidatePageOffset}페이지 미리보기`}
                >
                  <span>{index + 1 + candidatePageOffset}</span>
                </GalleryThumbnail>
              </div>
            ))}
          </div>
        </details>
        <div className="review-actions">
          <button type="button" className="text-button scan-button" onClick={onScan}><FluentIcon glyph="\uE9D9" /> 전수 검사</button>
          <span />
          <button type="button" className="text-button danger-button" onClick={() => onDecision("부모 숨기기")}>부모 숨기기</button>
          <button type="button" className="text-button danger-button" onClick={() => onDecision("후보 숨기기")}>후보 숨기기</button>
          <button type="button" className="text-button series-button" onClick={() => onDecision("연작으로 묶기")}>연작으로 묶기</button>
          <button type="button" className="text-button" onClick={() => onDecision("묶음 풀기")}>묶음 풀기</button>
        </div>
      </div>
    </dialog>
  );
}
