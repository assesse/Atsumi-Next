import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { InternalDuplicateReview, InternalRemovalPlan } from "../api/contracts";
import { galleryId } from "../core/types";
import { ThumbnailClient, type ThumbnailRequest } from "../thumbnail";
import { InternalDuplicateDialog } from "./InternalDuplicateDialog";

const review: InternalDuplicateReview = {
  entryId: "verified-entry-1",
  galleryId: galleryId(101),
  title: "Original Source Pages",
  groups: [{
    groupId: "group-1",
    blockId: "block-1",
    sequenceIndex: 0,
    revision: 4,
    entryId: "verified-entry-1",
    galleryId: galleryId(101),
    relation: "exact",
    confidence: 1,
    recommendedKeepSourcePage: 2,
    pages: [2, 8].map((sourcePage) => ({
      sourcePage,
      exactSha256: true,
      visualSimilarity: 1,
      detailHashDistance: 0,
      lowInformation: false,
    })),
    resolved: false,
    createdAt: "2026-08-16T00:00:00.000Z",
    updatedAt: "2026-08-16T00:00:00.000Z",
  }],
  quarantineRecords: [],
};

const plan: InternalRemovalPlan = {
  planId: "plan-1",
  entryId: review.entryId,
  selections: [{
    groupId: "group-1",
    expectedRevision: 4,
    keepSourcePage: 2,
    removeSourcePages: [8],
  }],
  filesToQuarantine: 1,
  bytesToQuarantine: 512_000,
  expiresAt: String(Date.now() + 60_000),
};

describe("InternalDuplicateDialog", () => {
  it("uses verified artifact pages, preserves source numbers, previews a plan, and never offers deletion", async () => {
    const resolve = vi.fn((_request: ThumbnailRequest) => ({ kind: "missing" as const, reason: "test" }));
    const client = new ThumbnailClient({ resolve });
    const onPlan = vi.fn();
    const onApply = vi.fn();
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const render = (currentPlan?: InternalRemovalPlan) => root.render(
      <InternalDuplicateDialog
        open={false}
        review={review}
        plan={currentPlan}
        thumbnailClient={client}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onRescan={vi.fn()}
        onPlan={onPlan}
        onApply={onApply}
        onUndo={vi.fn()}
      />,
    );
    await act(async () => render());

    expect(container.textContent).toContain("원본 페이지 번호는 바뀌지 않습니다");
    expect(container.textContent).toContain("원본 2p");
    expect(container.textContent).toContain("원본 8p");
    expect(container.textContent).not.toContain("영구 삭제 적용");
    expect(resolve.mock.calls.map(([request]) => request.key)).toEqual([
      expect.objectContaining({ kind: "artifact-page", entryId: "verified-entry-1", page: 2 }),
      expect.objectContaining({ kind: "artifact-page", entryId: "verified-entry-1", page: 8 }),
    ]);

    const preview = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("격리 계획 미리보기"));
    await act(async () => preview?.click());
    expect(onPlan).toHaveBeenCalledWith({
      entryId: "verified-entry-1",
      selections: [{
        groupId: "group-1",
        expectedRevision: 4,
        keepSourcePage: 2,
        removeSourcePages: [8],
      }],
    });

    await act(async () => render(plan));
    expect(container.querySelector(".internal-plan-summary")).toHaveTextContent("1개 파일");
    const apply = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("계획대로 격리 적용"));
    await act(async () => apply?.click());
    expect(onApply).toHaveBeenCalledWith(plan);

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("offers undo only for persisted quarantined records", async () => {
    const onUndo = vi.fn();
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const quarantined: InternalDuplicateReview = {
      ...review,
      groups: [],
      quarantineRecords: [{
        recordId: "record-8",
        planId: "plan-1",
        entryId: review.entryId,
        galleryId: review.galleryId,
        sourcePage: 8,
        originalRelativePath: "album/0008.webp",
        quarantineRelativePath: "album/.atsumi-page-quarantine/plan-1/0008.webp",
        reason: "review",
        state: "quarantined",
        createdAt: "2026-08-16T00:00:00.000Z",
        updatedAt: "2026-08-16T00:00:01.000Z",
      }],
    };
    await act(async () => root.render(
      <InternalDuplicateDialog
        open={false}
        review={quarantined}
        onClose={vi.fn()}
        onRetry={vi.fn()}
        onRescan={vi.fn()}
        onPlan={vi.fn()}
        onApply={vi.fn()}
        onUndo={onUndo}
      />,
    ));
    const undo = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("모두 되돌리기"));
    await act(async () => undo?.click());
    expect(onUndo).toHaveBeenCalledWith(["record-8"]);
    await act(async () => root.unmount());
    container.remove();
  });

  it("keeps exactly one radio choice and plans every other page in an N-way row", async () => {
    const fourWay: InternalDuplicateReview = {
      ...review,
      groups: [{
        ...review.groups[0]!,
        groupId: "group-four-way",
        recommendedKeepSourcePage: 1,
        pages: [1, 6, 11, 16].map((sourcePage) => ({
          sourcePage, exactSha256: false, visualSimilarity: 0.91, detailHashDistance: 32, lowInformation: false,
        })),
      }],
    };
    const onPlan = vi.fn();
    const client = new ThumbnailClient({ resolve: () => ({ kind: "missing" as const, reason: "test" }) });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    await act(async () => root.render(
      <InternalDuplicateDialog open={false} review={fourWay} thumbnailClient={client} onClose={vi.fn()} onRetry={vi.fn()} onRescan={vi.fn()} onPlan={onPlan} onApply={vi.fn()} onUndo={vi.fn()} />,
    ));
    expect(container.querySelectorAll('input[type="radio"]')).toHaveLength(4);
    expect(container.querySelector('input[type="radio"]:checked')?.parentElement).toHaveTextContent("원본 1p");
    const preview = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.includes("격리 계획 미리보기"));
    await act(async () => preview?.click());
    expect(onPlan).toHaveBeenCalledWith(expect.objectContaining({
      selections: [expect.objectContaining({ keepSourcePage: 1, removeSourcePages: [6, 11, 16] })],
    }));
    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("selects an entire edition set, preserves a missing scene, and hides a stale plan", async () => {
    const editionReview: InternalDuplicateReview = {
      ...review,
      groups: Array.from({ length: 5 }, (_, sequenceIndex) => ({
        ...review.groups[0]!,
        groupId: `edition-row-${sequenceIndex + 1}`,
        blockId: "edition-block",
        sequenceIndex,
        recommendedKeepSourcePage: sequenceIndex + 1,
        pages: [0, 1, 2, 3]
          .filter((track) => !(track === 2 && sequenceIndex === 2))
          .map((track) => ({
            sourcePage: track * 5 + sequenceIndex + 1,
            exactSha256: track === 0,
            visualSimilarity: track === 0 ? 1 : .92,
            detailHashDistance: track === 0 ? 0 : 11,
            lowInformation: false,
            editionTrackId: `edition-block-t${track}`,
            editionTrackOrdinal: track,
          })),
      })),
    };
    const onPlan = vi.fn();
    const client = new ThumbnailClient({ resolve: () => ({ kind: "missing" as const, reason: "test" }) });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    await act(async () => root.render(
      <InternalDuplicateDialog open={false} review={editionReview} thumbnailClient={client} onClose={vi.fn()} onRetry={vi.fn()} onRescan={vi.fn()} onPlan={onPlan} onApply={vi.fn()} onUndo={vi.fn()} />,
    ));
    expect(container.textContent).toContain("남길 판본 세트 선택");
    expect(container.querySelectorAll('input[type="radio"]')).toHaveLength(4);
    expect(container.querySelectorAll(".internal-scene-matrix-row")).toHaveLength(6);
    expect(container.querySelectorAll(".internal-page-option")).toHaveLength(0);
    const setB = container.querySelectorAll<HTMLInputElement>('input[name="track-edition-block"]')[1];
    await act(async () => setB?.click());
    const preview = [...container.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.includes("격리 계획 미리보기"));
    await act(async () => preview?.click());
    expect(onPlan).toHaveBeenCalledWith(expect.objectContaining({
      selections: expect.arrayContaining([
        expect.objectContaining({ groupId: "edition-row-1", keepSourcePage: 6, removeSourcePages: [1, 11, 16] }),
        expect.objectContaining({ groupId: "edition-row-3", keepSourcePage: 8, removeSourcePages: [3, 18] }),
      ]),
    }));
    expect(onPlan.mock.calls[0]?.[0].selections).toHaveLength(5);
    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });
});
