export type TagChipMeasurement = {
  width: number;
  height: number;
};

/**
 * Returns the number of chips that fit in the available tag-list rectangle.
 * Measurements come from the real rendered chips, so this stays independent
 * of font metrics and works for both Latin and CJK labels.
 */
export function fitTagChipCount(
  chips: readonly TagChipMeasurement[],
  availableWidth: number,
  availableHeight: number,
  gapX: number,
  gapY: number,
): number {
  if (!chips.length || availableWidth <= 0 || availableHeight <= 0) return 0;

  let count = 0;
  let rowWidth = 0;
  let rowHeight = 0;
  let usedHeight = 0;

  for (const chip of chips) {
    const width = Math.max(0, chip.width);
    const height = Math.max(0, chip.height);
    if (!width || !height) continue;

    const startsNewRow = rowWidth > 0 && rowWidth + gapX + width > availableWidth;
    if (startsNewRow) {
      usedHeight += rowHeight + gapY;
      rowWidth = 0;
      rowHeight = 0;
    }

    if (usedHeight + height > availableHeight) break;
    rowWidth = rowWidth > 0 ? rowWidth + gapX + width : width;
    rowHeight = Math.max(rowHeight, height);
    count += 1;
  }

  return count;
}

export function splitGalleryTitle(title: string, subtitle?: string): {
  primary: string;
  secondary: string;
} {
  const canonical = title.trim();
  const pipeIndex = canonical.indexOf("|");
  const primary = (pipeIndex >= 0 ? canonical.slice(0, pipeIndex) : canonical).trim();
  const pipedSecondary = pipeIndex >= 0
    ? canonical.slice(pipeIndex + 1).split("|").map((part) => part.trim()).filter(Boolean)
    : [];
  const explicitSecondary = subtitle?.trim() ?? "";
  const safePrimary = primary || canonical;
  const secondaryParts = [...pipedSecondary, explicitSecondary]
    .filter((part, index, parts) => Boolean(part) && part !== safePrimary && parts.indexOf(part) === index);
  return {
    primary: safePrimary,
    secondary: secondaryParts.join(" · "),
  };
}
