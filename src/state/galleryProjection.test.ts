import { describe, expect, it } from "vitest";
import type { DownloadEntry, GalleryDetail, GalleryPage, GallerySummary } from "../api/contracts";
import { galleryId, type Gallery } from "../core/types";
import { mergeDownloadEntries, mergeGalleryDetail, mergeGalleryPage, projectGallerySummary } from "./galleryProjection";

const summary = (idValue: number, title = `Gallery ${idValue}`): GallerySummary => ({
  id: galleryId(idValue),
  title,
  artist: "serein",
  pages: 24,
  language: "japanese",
  tags: ["female:glasses"],
  series: ["rain archives"],
  characters: ["mira lane"],
  publishedRank: 20260814,
  popularity: 91,
  thumbnailKey: `fixture-${idValue}`,
  thumbnailWidth: 512,
  thumbnailHeight: 512,
});

describe("gallery API projection", () => {
  it("projects summary metadata while preserving an existing download projection", () => {
    const existing: Gallery = {
      ...projectGallerySummary(summary(1, "Old title")),
      download: { entryId: "entry-1", state: "downloading", progress: 42 },
    };

    const projected = projectGallerySummary(summary(1, "Fresh title"), existing);

    expect(projected).toMatchObject({
      title: "Fresh title",
      language: "japanese",
      publishedAt: "2026-08-14",
      score: 91,
      download: { entryId: "entry-1", state: "downloading", progress: 42 },
    });
  });

  it("accepts legacy summaries without newer search metadata", () => {
    const existing = projectGallerySummary(summary(1, "Existing title"));
    const legacySummary = (idValue: number, title: string) => ({
      id: galleryId(idValue),
      title,
      artist: "legacy artist",
      pages: 12,
    }) as unknown as GallerySummary;
    const page: GalleryPage = {
      page: 1,
      totalPages: 1,
      items: [legacySummary(1, "Updated legacy title"), legacySummary(2, "New legacy title")],
    };

    const projected = mergeGalleryPage(new Map([[existing.id, existing]]), page).galleries;

    expect(projected.get(galleryId(1))).toMatchObject({
      title: "Updated legacy title",
      language: "japanese",
      tags: ["female:glasses"],
      series: ["rain archives"],
      characters: ["mira lane"],
      publishedAt: "2026-08-14",
      score: 91,
      thumbnailKey: "fixture-1",
      thumbnailWidth: 512,
      thumbnailHeight: 512,
    });
    expect(projected.get(galleryId(2))).toMatchObject({
      title: "New legacy title",
      language: "korean",
      tags: [],
      series: [],
      characters: [],
      publishedAt: "0000-00-00",
      score: 0,
    });
    expect(projected.get(galleryId(2))).not.toHaveProperty("thumbnailWidth");
    expect(projected.get(galleryId(2))).not.toHaveProperty("thumbnailHeight");
  });

  it("merges only the current search page IDs without discarding prior galleries", () => {
    const initial = new Map([[galleryId(1), projectGallerySummary(summary(1))]]);
    const page: GalleryPage = { page: 2, totalPages: 2, items: [summary(2), summary(3)] };

    const result = mergeGalleryPage(initial, page);

    expect(result.ids).toEqual([galleryId(2), galleryId(3)]);
    expect([...result.galleries.keys()]).toEqual([galleryId(1), galleryId(2), galleryId(3)]);
  });

  it("hydrates related summaries and projects queue snapshots onto the same galleries", () => {
    const detail: GalleryDetail = { ...summary(1), related: [summary(2), summary(3)] };
    const hydrated = mergeGalleryDetail(new Map(), detail);
    const entries: DownloadEntry[] = [{
      entryId: "entry-1",
      galleryId: galleryId(1),
      revision: 0,
      state: "queued",
      progress: 0,
    }];
    const queued = mergeDownloadEntries(hydrated, entries);

    expect(queued.get(galleryId(1))?.relatedIds).toEqual([galleryId(2), galleryId(3)]);
    expect(queued.get(galleryId(2))?.title).toBe("Gallery 2");
    expect(queued.get(galleryId(1))?.download).toEqual({
      entryId: "entry-1",
      revision: 0,
      state: "queued",
      progress: 0,
    });
  });

  it("does not let an older list snapshot overwrite a newer event projection", () => {
    const projected = projectGallerySummary(summary(1));
    const current: Gallery = {
      ...projected,
      download: {
        entryId: "entry-1",
        revision: 5,
        state: "downloading",
        progress: 72,
        attempt: 2,
      },
    };

    const merged = mergeDownloadEntries(new Map([[current.id, current]]), [{
      entryId: "entry-1",
      galleryId: current.id,
      revision: 4,
      state: "queued",
      progress: 0,
      attempt: 2,
    }]);

    expect(merged.get(current.id)).toBe(current);
  });
});
