import { describe, expect, it } from "vitest";
import {
  detailPreviewWindowClampStart,
  detailPreviewWindowRange,
  detailPreviewWindowSize,
  detailPreviewWindowStart,
} from "./detailPreviewWindow";

describe("detailPreviewWindow", () => {
  it("balances rows against the Related height and responds to thumbnail/grid changes", () => {
    const layout = { orientation: "mixed" as const };
    const compact = detailPreviewWindowSize({ pageCount: 1000, columns: 3, gridWidth: 300, relatedHeight: 620, viewportHeight: 600, rowGap: 8, layout });
    const wide = detailPreviewWindowSize({ pageCount: 1000, columns: 3, gridWidth: 540, relatedHeight: 620, viewportHeight: 600, rowGap: 8, layout });
    const tallerRelated = detailPreviewWindowSize({ pageCount: 1000, columns: 3, gridWidth: 300, relatedHeight: 930, viewportHeight: 600, rowGap: 8, layout });
    expect(wide).toBeLessThan(compact);
    expect(tallerRelated).toBeGreaterThan(compact);
  });

  it("keeps final partial windows reachable without allocating every page", () => {
    const size = 12;
    const start = detailPreviewWindowStart(5000, 5000, size);
    expect(start).toBe(4993);
    expect(detailPreviewWindowRange(start, 5000, size)).toEqual([4993, 4994, 4995, 4996, 4997, 4998, 4999, 5000]);
  });

  it("keeps a preserved start when stable metrics change and reaches page 5000 by bounded next windows", () => {
    const size = 12;
    expect(detailPreviewWindowClampStart(97, 5000, 18)).toBe(97);
    let start = 1;
    const lastStart = 5000 - size + 1;
    while (start < lastStart) start = Math.min(lastStart, start + size);
    expect(detailPreviewWindowRange(start, 5000, size).at(-1)).toBe(5000);
  });
});
