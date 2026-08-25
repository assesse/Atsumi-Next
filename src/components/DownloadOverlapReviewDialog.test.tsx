import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { DownloadOverlapReview } from "../api/contracts";
import { galleryId } from "../core/types";
import { ThumbnailClient, type ThumbnailRequest } from "../thumbnail";
import { DownloadOverlapReviewDialog } from "./DownloadOverlapReviewDialog";

const fixture = (): DownloadOverlapReview => ({
  reviewId: "review-overlap",
  entryId: "incoming-entry",
  incoming: {
    entryId: "incoming-entry",
    galleryId: galleryId(200),
    title: "New edition",
    artists: ["artist a"],
    pageCount: 12,
  },
  revision: 4,
  state: "pending",
  profileVersion: 1,
  policyVersion: 1,
  incomingFingerprint: "incoming-fingerprint",
  candidates: ["near_equivalent", "incoming_contains_existing", "existing_contains_incoming", "partial_overlap"].map((relation, index) => ({
    candidateId: `candidate-${index + 1}`,
    existing: {
      entryId: `existing-entry-${index + 1}`,
      galleryId: galleryId(100 + index),
      title: `Owned edition ${index + 1}`,
      artists: ["artist a"],
      pageCount: 10,
    },
    existingFingerprint: `fingerprint-${index + 1}`,
    relation: relation as DownloadOverlapReview["candidates"][number]["relation"],
    confidence: 0.94,
    matchedPages: 8,
    exactPages: 3,
    visualPages: 5,
    existingCoverage: 0.8,
    incomingCoverage: 2 / 3,
    existingUniquePages: 2,
    incomingUniquePages: 4,
    longestAlignedRun: 6,
    rank: index + 1,
    pagePairs: [{
      incomingSourcePage: index + 2,
      existingSourcePage: index + 1,
      exactSha256: false,
      dHashDistance: 2,
      pHashDistance: 3,
      detailHashDistance: 19,
      edgeSimilarity: 0.91,
      visualSimilarity: 0.93,
      lowInformation: false,
    }],
  })),
  createdAt: "2026-08-25T00:00:00.000Z",
  updatedAt: "2026-08-25T00:00:00.000Z",
});

describe("DownloadOverlapReviewDialog", () => {
  it("renders all candidate relations and verified artifact page pairs", async () => {
    const resolve = vi.fn((_request: ThumbnailRequest) => ({ kind: "missing" as const, reason: "fixture" }));
    const client = new ThumbnailClient({ resolve });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => root.render(
      <DownloadOverlapReviewDialog
        open={false}
        review={fixture()}
        thumbnailClient={client}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onDecision={vi.fn()}
      />,
    ));

    expect(container.querySelectorAll('[role="tab"]')).toHaveLength(4);
    expect(container.textContent).toContain("거의 같은 판본");
    expect(container.textContent).toContain("기존 범위");
    expect(container.textContent).toContain("80%");
    expect(container.textContent).toContain("자동 삭제하거나 대체하지 않습니다");
    const artifactPages = resolve.mock.calls
      .map(([request]) => request.key)
      .filter((key) => key.kind === "artifact-page")
      .map((key) => [key.entryId, key.page]);
    expect(artifactPages).toEqual(expect.arrayContaining([
      ["existing-entry-1", 1],
      ["incoming-entry", 2],
    ]));

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("submits revision-checked keep, false-positive, and cancel decisions", async () => {
    const onDecision = vi.fn();
    const client = new ThumbnailClient({ resolve: () => ({ kind: "missing", reason: "fixture" }) });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    await act(async () => root.render(
      <DownloadOverlapReviewDialog
        open={false}
        review={fixture()}
        thumbnailClient={client}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onDecision={onDecision}
      />,
    ));
    const click = async (label: string) => {
      const button = [...container.querySelectorAll<HTMLButtonElement>("button")]
        .find((item) => item.textContent?.includes(label));
      if (!button) throw new Error(`${label} button missing`);
      await act(async () => button.click());
    };
    await click("이 후보는 오탐");
    await click("둘 다 보관하고 완료");
    await click("새 다운로드 취소");
    expect(onDecision).toHaveBeenCalledWith({
      reviewId: "review-overlap",
      expectedRevision: 4,
      action: "false_positive_continue",
      candidateId: "candidate-1",
    });
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({ action: "continue_keep_both" }));
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({ action: "cancel_incoming" }));

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });
});
