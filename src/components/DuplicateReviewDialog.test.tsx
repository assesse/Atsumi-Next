import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { type Gallery } from "../core/types";
import { mockGalleries } from "../data/mockGalleries";
import { ThumbnailClient, type ThumbnailRequest } from "../thumbnail";
import { DuplicateReviewDialog } from "./DuplicateReviewDialog";

describe("DuplicateReviewDialog page pairs", () => {
  it("only requests pairs whose source pages exist in both galleries", async () => {
    const parent: Gallery = { ...mockGalleries[0]!, pages: 2 };
    const candidate: Gallery = { ...mockGalleries[1]!, pages: 4 };
    const resolve = vi.fn((_request: ThumbnailRequest) => ({
      kind: "missing" as const,
      reason: "test fixture",
    }));
    const client = new ThumbnailClient({ resolve });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const render = (nextCandidate: Gallery) => root.render(
      <DuplicateReviewDialog
        open={false}
        parent={parent}
        candidate={nextCandidate}
        thumbnailClient={client}
        onClose={vi.fn()}
        onDecision={vi.fn()}
        onScan={vi.fn()}
      />
    );

    await act(async () => render(candidate));

    expect(container.querySelectorAll(".pair")).toHaveLength(2);
    expect(container.querySelector(".match-pairs summary")).toHaveTextContent("2쌍");
    const requestedPages = resolve.mock.calls
      .map(([thumbnailRequest]) => thumbnailRequest.key)
      .filter((key) => key.kind === "source-page")
      .map((key) => [Number(key.galleryId), key.page]);
    expect(requestedPages).toEqual([
      [Number(parent.id), 1],
      [Number(candidate.id), 3],
      [Number(parent.id), 2],
      [Number(candidate.id), 4],
    ]);

    await act(async () => render({ ...candidate, pages: 2 }));

    expect(container.querySelectorAll(".pair")).toHaveLength(0);
    expect(container.querySelector(".match-pairs summary")).toHaveTextContent("0쌍");

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });
});
