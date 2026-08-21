import type { DetailPreviewLayout } from "./detailPreviewLayout";

export type DetailPreviewWindowMetrics = Readonly<{
  pageCount: number;
  columns: number;
  gridWidth: number;
  relatedHeight: number;
  viewportHeight: number;
  rowGap: number;
  layout: Pick<DetailPreviewLayout, "orientation"> | Readonly<{ orientation: "pending" }>;
}>;

const finitePositive = (value: number, fallback: number): number =>
  Number.isFinite(value) && value > 0 ? value : fallback;

/**
 * Chooses one page window whose grid height is close to the Related list.
 * Until both columns have been measured, a deliberately small viewport-based
 * window keeps initial thumbnail subscriptions bounded.
 */
export function detailPreviewWindowSize(metrics: DetailPreviewWindowMetrics): number {
  const pageCount = Math.max(0, Math.floor(metrics.pageCount));
  if (!pageCount) return 0;
  const columns = Math.max(1, Math.floor(metrics.columns));
  const gap = Math.max(0, metrics.rowGap);
  const gridWidth = finitePositive(metrics.gridWidth, columns * 112 + gap * (columns - 1));
  const cellWidth = Math.max(72, (gridWidth - gap * (columns - 1)) / columns);
  // The locked layout is a stable input. Never sample a rendered preview row:
  // its content height depends on this window size and would feed back into it.
  const expectedAspectRatio = metrics.layout.orientation === "landscape"
    ? 16 / 9
    : metrics.layout.orientation === "portrait"
      ? 2 / 3
      : 1;
  const rowHeight = cellWidth / expectedAspectRatio;
  const targetHeight = finitePositive(metrics.relatedHeight, finitePositive(metrics.viewportHeight, 480) * 0.48);
  const rows = Math.max(1, Math.round((targetHeight + gap) / (rowHeight + gap)));
  return Math.min(pageCount, Math.max(columns, rows * columns));
}

export function detailPreviewWindowStart(page: number, pageCount: number, windowSize: number): number {
  const total = Math.max(0, Math.floor(pageCount));
  const size = Math.max(1, Math.floor(windowSize));
  if (!total) return 1;
  const clamped = Math.min(total, Math.max(1, Math.floor(page)));
  return Math.floor((clamped - 1) / size) * size + 1;
}

/** Clamps a preserved tab position without snapping it to a new window boundary. */
export function detailPreviewWindowClampStart(start: number, pageCount: number, windowSize: number): number {
  const total = Math.max(0, Math.floor(pageCount));
  const size = Math.max(1, Math.floor(windowSize));
  if (!total) return 1;
  const lastStart = Math.max(1, total - size + 1);
  return Math.min(lastStart, Math.max(1, Math.floor(start)));
}

export function detailPreviewWindowRange(start: number, pageCount: number, windowSize: number): readonly number[] {
  const total = Math.max(0, Math.floor(pageCount));
  const size = Math.max(0, Math.floor(windowSize));
  if (!total || !size) return [];
  const first = Math.min(total, Math.max(1, Math.floor(start)));
  return Array.from({ length: Math.min(size, total - first + 1) }, (_, index) => first + index);
}
