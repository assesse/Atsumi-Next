import type { Gallery, GalleryId } from "../core/types";

export type ThumbnailConsumer = "explore" | "downloads" | "detail" | "review";

export type ThumbnailPriority = "critical" | "visible" | "prefetch";

type FixtureCellFallback = {
  readonly kind: "fixture-sheet-cell";
  readonly index: number;
};

export type GalleryCoverThumbnailKey = {
  readonly kind: "gallery-cover";
  readonly galleryId: GalleryId;
  /** Opaque source identifier. It is never interpreted as a display URL. */
  readonly sourceKey?: string;
  readonly fallback?: FixtureCellFallback;
};

export type SourcePageThumbnailKey = {
  readonly kind: "source-page";
  readonly galleryId: GalleryId;
  /** One-based source page number. */
  readonly page: number;
  /** Opaque gallery/source identifier owned by the backend coordinator. */
  readonly sourceKey?: string;
  readonly fallback?: FixtureCellFallback;
};

export type ThumbnailKey = GalleryCoverThumbnailKey | SourcePageThumbnailKey;

export type ThumbnailRequest = {
  readonly key: ThumbnailKey;
  readonly consumer: ThumbnailConsumer;
  readonly priority: ThumbnailPriority;
};

const normalizedFixtureCell = (value: number): number => {
  if (!Number.isInteger(value)) return 0;
  return ((value % 6) + 6) % 6;
};

export function galleryCoverThumbnailKey(
  gallery: Pick<Gallery, "id" | "thumbnailKey" | "coverIndex">,
): GalleryCoverThumbnailKey {
  return {
    kind: "gallery-cover",
    galleryId: gallery.id,
    ...(gallery.thumbnailKey?.trim() ? { sourceKey: gallery.thumbnailKey.trim() } : {}),
    fallback: { kind: "fixture-sheet-cell", index: normalizedFixtureCell(gallery.coverIndex) },
  };
}

export function sourcePageThumbnailKey(
  gallery: Pick<Gallery, "id" | "thumbnailKey" | "coverIndex">,
  page: number,
): SourcePageThumbnailKey {
  if (!Number.isInteger(page) || page < 1) {
    throw new RangeError("Thumbnail source pages are one-based positive integers");
  }
  return {
    kind: "source-page",
    galleryId: gallery.id,
    page,
    ...(gallery.thumbnailKey?.trim() ? { sourceKey: gallery.thumbnailKey.trim() } : {}),
    fallback: {
      kind: "fixture-sheet-cell",
      index: normalizedFixtureCell(gallery.coverIndex + page - 1),
    },
  };
}

/**
 * Stable identity used only to merge frontend subscriptions. The backend remains
 * the canonical cache/network owner and receives the structured key as-is.
 */
export function thumbnailKeyIdentity(key: ThumbnailKey): string {
  return key.kind === "gallery-cover"
    ? `gallery-cover:${key.galleryId}`
    : `source-page:${key.galleryId}:${key.page}`;
}

export function thumbnailConsumerForView(view: "explore" | "auto-find" | "downloads"): ThumbnailConsumer {
  if (view === "downloads") return "downloads";
  if (view === "auto-find") return "review";
  return "explore";
}
