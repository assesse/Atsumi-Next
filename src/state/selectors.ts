import type { Gallery, GalleryId, UiState } from "../core/types";

const matchesQuery = (gallery: Gallery, query: string): boolean => {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return true;

  const separator = needle.indexOf(":");
  if (separator > 0) {
    const namespace = needle.slice(0, separator);
    const value = needle.slice(separator + 1);
    if (namespace === "artist") return gallery.artist.toLocaleLowerCase().includes(value);
    if (namespace === "group") return gallery.group?.toLocaleLowerCase().includes(value) ?? false;
    if (namespace === "series") {
      const metadataValue = value.replaceAll("_", " ");
      return (gallery.series ?? []).some((item) => item.toLocaleLowerCase().includes(metadataValue));
    }
    if (namespace === "character") {
      const metadataValue = value.replaceAll("_", " ");
      return (gallery.characters ?? []).some((item) => item.toLocaleLowerCase().includes(metadataValue));
    }
    if (namespace === "language") return gallery.language.includes(value);
    return gallery.tags.some((tag) => tag.toLocaleLowerCase().includes(needle));
  }

  return [gallery.title, gallery.subtitle, gallery.artist, gallery.group ?? "", ...(gallery.series ?? []), ...(gallery.characters ?? []), ...gallery.tags]
    .join(" ")
    .toLocaleLowerCase()
    .includes(needle);
};

const matchesDownloadFilter = (gallery: Gallery, state: UiState): boolean => {
  if (!gallery.download) return false;
  switch (state.downloadsFilter) {
    case "all":
      return true;
    case "active":
      return ["queued", "resolving_metadata", "downloading", "hashing", "verifying", "retry_wait"].includes(
        gallery.download.state,
      );
    case "review":
      return gallery.download.state === "review_required";
    case "failed":
      return ["failed", "interrupted"].includes(gallery.download.state);
    case "complete":
      return gallery.download.state === "completed";
  }
};

export function visibleGalleries(state: UiState, galleries: Iterable<Gallery>): Gallery[] {
  const search = state.search[state.view];
  let items = [...galleries].filter((gallery) => search.languages.includes(gallery.language));

  if (state.view === "auto-find") items = items.filter((gallery) => gallery.favorite);
  if (state.view === "downloads") items = items.filter((gallery) => matchesDownloadFilter(gallery, state));
  items = items.filter((gallery) => matchesQuery(gallery, search.committed));

  if (state.view === "explore") {
    if (state.exploreSort === "recent") {
      items.sort((left, right) => right.publishedAt.localeCompare(left.publishedAt) || right.id - left.id);
    } else if (state.exploreSort.startsWith("popular")) {
      items.sort((left, right) => right.score - left.score);
    } else if (state.exploreSort === "random") {
      items.sort((left, right) => ((left.id * 2654435761) >>> 0) - ((right.id * 2654435761) >>> 0));
    }
  }
  return items;
}

export function withGalleryPatch(
  items: ReadonlyMap<GalleryId, Gallery>,
  id: GalleryId,
  patch: Partial<Gallery>,
): ReadonlyMap<GalleryId, Gallery> {
  const current = items.get(id);
  if (!current) return items;
  const next = new Map(items);
  next.set(id, { ...current, ...patch });
  return next;
}
