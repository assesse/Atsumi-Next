import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { DuplicateReview } from "../api/contracts";
import { galleryId } from "../core/types";
import { ThumbnailClient, type ThumbnailRequest } from "../thumbnail";
import { DuplicateReviewDialog } from "./DuplicateReviewDialog";

const reviewFixture = (patch: Partial<DuplicateReview> = {}): DuplicateReview => ({
  candidate: {
    candidateId: "candidate-real-evidence",
    revision: 7,
    parent: {
      galleryId: galleryId(101),
      entryId: "verified-parent-entry",
      title: "Verified Parent",
      artist: "artist a",
      pageCount: 20,
    },
    candidate: {
      galleryId: galleryId(202),
      entryId: "verified-candidate-entry",
      title: "Verified Candidate",
      artist: "artist a",
      pageCount: 16,
    },
    relation: "partial",
    confidence: 0.73,
    matchedPages: 2,
    parentCoverage: 0.1,
    candidateCoverage: 0.125,
    createdAt: "2026-08-15T00:00:00.000Z",
    updatedAt: "2026-08-15T00:00:00.000Z",
  },
  evidence: [{
    evidenceId: "evidence-sequence",
    kind: "sequence_alignment",
    confidence: 0.73,
    matchedPages: 2,
    description: "Persisted one-to-one sequence evidence",
  }],
  pagePairs: [
    {
      parentSourcePage: 2,
      candidateSourcePage: 9,
      exactSha256: true,
      dHashDistance: 0,
      pHashDistance: 0,
      detailHashDistance: 0,
      edgeSimilarity: 1,
      visualSimilarity: 1,
      lowInformation: false,
    },
    {
      parentSourcePage: 11,
      candidateSourcePage: 14,
      exactSha256: false,
      dHashDistance: 4,
      pHashDistance: 5,
      detailHashDistance: 37,
      edgeSimilarity: 0.91,
      visualSimilarity: 0.92,
      lowInformation: false,
    },
  ],
  decisions: [],
  seriesGroups: [],
  ...patch,
});

const setInput = (input: HTMLInputElement, value: string) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
};

describe("DuplicateReviewDialog backend evidence", () => {
  it("renders persisted confidence and exact artifact source-page pairs without placeholder values", async () => {
    const resolve = vi.fn((_request: ThumbnailRequest) => ({
      kind: "missing" as const,
      reason: "test fixture",
    }));
    const client = new ThumbnailClient({ resolve });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => root.render(
      <DuplicateReviewDialog
        open={false}
        review={reviewFixture()}
        thumbnailClient={client}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onRescan={vi.fn()}
        onDecision={vi.fn()}
      />,
    ));

    expect(container.querySelector(".review-summary")).toHaveTextContent("신뢰도 73%");
    expect(container.querySelector(".review-summary")).toHaveTextContent("2개 페이지 일치");
    expect(container.querySelector(".match-pairs summary")).toHaveTextContent("2쌍");
    expect(container.textContent).toContain("Persisted one-to-one sequence evidence");
    expect(container.textContent).toContain("detail 37 · edge 91%");
    expect(container.textContent).not.toContain("82%");
    expect(container.textContent).not.toContain("first gid");
    expect(container.textContent).not.toContain("parent gid");

    const artifactPages = resolve.mock.calls
      .map(([request]) => request.key)
      .filter((key) => key.kind === "artifact-page")
      .map((key) => [key.entryId, key.page]);
    expect(artifactPages).toEqual([
      ["verified-parent-entry", 2],
      ["verified-candidate-entry", 9],
      ["verified-parent-entry", 11],
      ["verified-candidate-entry", 14],
    ]);

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("submits every decision with the current candidate revision and required series targets", async () => {
    const onDecision = vi.fn();
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const base = reviewFixture();
    const render = (review: DuplicateReview) => root.render(
      <DuplicateReviewDialog
        open={false}
        review={review}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onRescan={vi.fn()}
        onDecision={onDecision}
      />,
    );
    await act(async () => render(base));

    const click = async (label: string) => {
      const button = [...container.querySelectorAll<HTMLButtonElement>("button")]
        .find((item) => item.textContent?.includes(label));
      if (!button) throw new Error(`${label} button missing`);
      await act(async () => button.click());
    };
    await click("부모 숨기기");
    await click("후보 숨기기");
    await click("이 작품 쌍 제외");
    const name = container.querySelector<HTMLInputElement>('input[aria-label="새 연작 이름"]');
    if (!name) throw new Error("Series name input missing");
    await act(async () => setInput(name, "Rain sequence"));
    await click("연작으로 묶기");

    expect(onDecision).toHaveBeenCalledWith({
      candidateId: "candidate-real-evidence",
      expectedRevision: 7,
      action: "hide_parent",
    });
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({ action: "hide_candidate", expectedRevision: 7 }));
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({ action: "exclude_pair", expectedRevision: 7 }));
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({
      action: "series_link",
      seriesName: "Rain sequence",
      expectedRevision: 7,
    }));

    const withGroup: DuplicateReview = {
      ...base,
      seriesGroups: [{
        seriesGroupId: "series-rain",
        name: "Rain sequence",
        revision: 1,
        members: [base.candidate.parent, base.candidate.candidate],
        createdAt: "2026-08-15T00:00:00.000Z",
        updatedAt: "2026-08-15T00:00:00.000Z",
      }],
    };
    await act(async () => render(withGroup));
    await click("부모 묶음 풀기");
    await click("후보 묶음 풀기");
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({
      action: "series_unlink",
      targetGalleryId: galleryId(101),
      seriesGroupId: "series-rain",
    }));
    expect(onDecision).toHaveBeenCalledWith(expect.objectContaining({
      action: "series_unlink",
      targetGalleryId: galleryId(202),
      seriesGroupId: "series-rain",
    }));

    await act(async () => root.unmount());
    container.remove();
  });
});
