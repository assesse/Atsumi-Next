import type {
  InternalDuplicateGroup,
  InternalPageEvidence,
  InternalRemovalPlan,
  InternalRemovalSelection,
} from "../api/contracts";

export type InternalEditionTrack = {
  id: string;
  ordinal: number;
  label: string;
  pages: number[];
  coveredRows: number;
  missingRows: number;
  firstPage?: InternalPageEvidence;
};

export type InternalReviewBlock = {
  blockId: string;
  rows: InternalDuplicateGroup[];
  tracks: InternalEditionTrack[];
  edition: boolean;
};

const trackLabel = (ordinal: number): string => ordinal < 26
  ? `세트 ${String.fromCharCode(65 + ordinal)}`
  : `세트 ${ordinal + 1}`;

const validTrack = (page: InternalPageEvidence): page is InternalPageEvidence & {
  editionTrackId: string;
  editionTrackOrdinal: number;
} => typeof page.editionTrackId === "string"
  && page.editionTrackId.trim().length > 0
  && typeof page.editionTrackOrdinal === "number"
  && Number.isInteger(page.editionTrackOrdinal)
  && page.editionTrackOrdinal >= 0;

export function buildInternalReviewBlocks(groups: InternalDuplicateGroup[]): InternalReviewBlock[] {
  const rowsByBlock = new Map<string, InternalDuplicateGroup[]>();
  for (const group of groups) {
    const rows = rowsByBlock.get(group.blockId) ?? [];
    rows.push(group);
    rowsByBlock.set(group.blockId, rows);
  }
  return [...rowsByBlock.entries()].map(([blockId, rows]) => {
    const sortedRows = [...rows].sort((left, right) => left.sequenceIndex - right.sequenceIndex);
    const pages = sortedRows.flatMap((row) => row.pages);
    if (sortedRows.length < 2 || pages.some((page) => !validTrack(page))) {
      return { blockId, rows: sortedRows, tracks: [], edition: false };
    }

    const trackPages = new Map<string, InternalPageEvidence[]>();
    const trackOrdinals = new Map<string, number>();
    let invalid = false;
    for (const row of sortedRows) {
      const rowTracks = new Set<string>();
      for (const page of row.pages) {
        if (!validTrack(page) || rowTracks.has(page.editionTrackId)) {
          invalid = true;
          break;
        }
        rowTracks.add(page.editionTrackId);
        const ordinal = trackOrdinals.get(page.editionTrackId);
        if (ordinal !== undefined && ordinal !== page.editionTrackOrdinal) {
          invalid = true;
          break;
        }
        trackOrdinals.set(page.editionTrackId, page.editionTrackOrdinal);
        const values = trackPages.get(page.editionTrackId) ?? [];
        values.push(page);
        trackPages.set(page.editionTrackId, values);
      }
      if (invalid) break;
    }
    const tracks = [...trackPages.entries()]
      .map(([id, values]) => ({
        id,
        ordinal: trackOrdinals.get(id)!,
        label: trackLabel(trackOrdinals.get(id)!),
        pages: values.map((page) => page.sourcePage).sort((left, right) => left - right),
        coveredRows: values.length,
        missingRows: sortedRows.length - values.length,
        firstPage: [...values].sort((left, right) => left.sourcePage - right.sourcePage)[0],
      }))
      .sort((left, right) => left.ordinal - right.ordinal || left.id.localeCompare(right.id));
    const edition = !invalid
      && tracks.length >= 2
      && tracks.every((track) => track.coveredRows >= 2)
      && new Set(tracks.map((track) => track.ordinal)).size === tracks.length;
    return { blockId, rows: sortedRows, tracks: edition ? tracks : [], edition };
  });
}

export function buildInternalRemovalSelections(
  blocks: InternalReviewBlock[],
  selectedTrackByBlock: Record<string, string>,
  keepPages: Record<string, number>,
): InternalRemovalSelection[] {
  return blocks.flatMap((block) => {
    if (!block.edition) {
      return block.rows.flatMap((group) => {
        const keepSourcePage = keepPages[group.groupId] ?? group.recommendedKeepSourcePage;
        const removeSourcePages = group.pages
          .map((page) => page.sourcePage)
          .filter((page) => page !== keepSourcePage)
          .sort((left, right) => left - right);
        return removeSourcePages.length ? [{
          groupId: group.groupId,
          expectedRevision: group.revision,
          keepSourcePage,
          removeSourcePages,
        }] : [];
      });
    }
    const selectedTrack = selectedTrackByBlock[block.blockId] ?? block.tracks[0]?.id;
    if (!selectedTrack) return [];
    return block.rows.flatMap((group) => {
      const keep = group.pages.find((page) => page.editionTrackId === selectedTrack);
      if (!keep) return [];
      const removeSourcePages = group.pages
        .map((page) => page.sourcePage)
        .filter((page) => page !== keep.sourcePage)
        .sort((left, right) => left - right);
      return removeSourcePages.length ? [{
        groupId: group.groupId,
        expectedRevision: group.revision,
        keepSourcePage: keep.sourcePage,
        removeSourcePages,
      }] : [];
    });
  });
}

const normalizedSelections = (selections: InternalRemovalSelection[]) => selections
  .map((selection) => ({ ...selection, removeSourcePages: [...selection.removeSourcePages].sort((left, right) => left - right) }))
  .sort((left, right) => left.groupId.localeCompare(right.groupId));

export function selectionsMatchPlan(
  selections: InternalRemovalSelection[],
  plan?: InternalRemovalPlan | null,
): boolean {
  if (!plan) return false;
  return JSON.stringify(normalizedSelections(selections)) === JSON.stringify(normalizedSelections(plan.selections));
}
