import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DownloadChangedEvent } from "../api/contracts";
import type { Gallery, GalleryId } from "../core/types";
import { mockGalleries } from "../data/mockGalleries";
import { applyDownloadChanged } from "../state/downloadProjection";
import { browserFixtureThumbnailAdapter, ThumbnailClient } from "../thumbnail";
import { GalleryCard } from "./GalleryCard";
import { fitTagChips, sortGalleryTags, splitGalleryTitle } from "./galleryCardLayout";

const defaultThumbnailClient = new ThumbnailClient(browserFixtureThumbnailAdapter);

const callbacks = {
  thumbnailClient: defaultThumbnailClient,
  onSelect: vi.fn(),
  onOpenDetail: vi.fn(),
  onOpenArtifact: vi.fn(),
  onOpenReview: vi.fn(),
  onStatusDetail: vi.fn(),
  onMetadataSearch: vi.fn(),
  onMetadataFavorite: vi.fn(),
};

describe("GalleryCard event projection", () => {
  beforeEach(() => vi.clearAllMocks());

  it("does not render an untouched memoized card for a target-only download event", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    let untouchedTitleReads = 0;
    const target = mockGalleries[0]!;
    const untouched = new Proxy(mockGalleries[1]!, {
      get(gallery, property, receiver) {
        if (property === "title") untouchedTitleReads += 1;
        return Reflect.get(gallery, property, receiver);
      },
    });
    let galleries: ReadonlyMap<GalleryId, Gallery> = new Map([
      [target.id, target],
      [untouched.id, untouched],
    ]);
    const favorites = new Set<string>();
    const render = () => (
      <div>
        {[...galleries.values()].map((gallery) => (
          <GalleryCard
            key={gallery.id}
            gallery={gallery}
            view="downloads"
            selected={false}
            selectionContext={false}
            favoriteMetadata={favorites}
            {...callbacks}
          />
        ))}
      </div>
    );

    await act(async () => root.render(render()));
    untouchedTitleReads = 0;
    const event: DownloadChangedEvent = {
      entryId: "entry-target",
      galleryId: target.id,
      revision: 1,
      state: "downloading",
      progress: 37,
    };
    galleries = applyDownloadChanged(galleries, event).galleries;
    await act(async () => root.render(render()));

    expect(untouchedTitleReads).toBe(0);
    await act(async () => root.unmount());
    container.remove();
  });

  it.each(["explore", "auto-find"] as const)("opens detail from a double click in %s", async (view) => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view={view}
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    const article = container.querySelector("article");
    await act(async () => {
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
      article?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
    });

    expect(callbacks.onOpenDetail).toHaveBeenCalledWith(gallery.id);
    expect(callbacks.onOpenArtifact).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("uses the artifact action for every Downloads double click", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[1]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="downloads"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    const article = container.querySelector("article");
    await act(async () => {
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
      article?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
    });

    expect(callbacks.onOpenArtifact).toHaveBeenCalledWith(gallery.id);
    expect(callbacks.onOpenDetail).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("marks duplicate counts and opens Review only from the warning or Downloads context action", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      download: { entryId: "entry-completed-duplicate", state: "completed", progress: 100 },
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="downloads"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        duplicateCandidateCount={2}
        {...callbacks}
      />,
    ));
    const article = container.querySelector<HTMLElement>("article");
    const warning = container.querySelector<HTMLButtonElement>(".status-pill.has-duplicate-count");
    expect(warning).toHaveTextContent("2");
    expect(warning).toHaveAccessibleName(expect.stringContaining("중복 후보 2개"));

    await act(async () => warning?.click());
    expect(callbacks.onOpenReview).toHaveBeenCalledWith(gallery.id);

    callbacks.onOpenReview.mockClear();
    await act(async () => article?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true })));
    expect(callbacks.onOpenReview).toHaveBeenCalledWith(gallery.id);

    callbacks.onOpenReview.mockClear();
    callbacks.onOpenArtifact.mockClear();
    await act(async () => {
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
      article?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
    });
    expect(callbacks.onOpenArtifact).toHaveBeenCalledWith(gallery.id);
    expect(callbacks.onOpenReview).not.toHaveBeenCalled();

    await act(async () => root.unmount());
    container.remove();
  });

  it("routes an internal metadata click to selection while a selection exists", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    await act(async () => {
      container.querySelector<HTMLButtonElement>(".card-byline .byline")?.click();
    });

    expect(callbacks.onSelect).toHaveBeenCalledWith(gallery.id, expect.anything());
    expect(callbacks.onMetadataSearch).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("keeps the normal metadata action when there is no selection context", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    await act(async () => {
      container.querySelector<HTMLButtonElement>(".card-byline .byline")?.click();
    });

    expect(callbacks.onMetadataSearch).toHaveBeenCalledWith(`artist:${gallery.artist}`);
    expect(callbacks.onSelect).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("does not render the removed cover selection and detail buttons", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => root.render(
      <GalleryCard
        gallery={mockGalleries[0]!}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    expect(container.querySelector(".card-selection-toggle")).toBeNull();
    expect(container.querySelector(".card-detail-open")).toBeNull();
    await act(async () => root.unmount());
    container.remove();
  });

  it("renders the horizontal density hierarchy without date or score metadata", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = { ...mockGalleries[0]!, subtitle: "" };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const article = container.querySelector("article");
    const cover = article?.querySelector(":scope > .cover");
    const content = article?.querySelector(":scope > .card-content");
    const footer = content?.querySelector(".meta-bottom");

    expect(cover).not.toBeNull();
    expect(content).not.toBeNull();
    expect(cover?.nextElementSibling).toBe(content);
    expect(article?.querySelector(".meta-column")).toBeNull();
    expect(article?.querySelector(".score")).toBeNull();
    expect(article?.querySelector(".title-sub")).toBeNull();
    expect(footer).toHaveTextContent(`${gallery.pages}p`);
    expect(footer).toHaveTextContent(`#${gallery.id}`);
    expect(article).not.toHaveTextContent(gallery.publishedAt.slice(2));
    expect(article).not.toHaveTextContent(String(gallery.score));
    expect(article).toHaveAccessibleName(expect.not.stringContaining(", ,"));
    await act(async () => root.unmount());
    container.remove();
  });

  it("reserves measured +N space without returning to a fixed tag count", () => {
    const overflow = [
      { width: 25, height: 24 },
      { width: 32, height: 24 },
      { width: 39, height: 24 },
    ];
    expect(fitTagChips(
      Array.from({ length: 3 }, () => ({ width: 40, height: 24 })),
      overflow,
      140,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 3, hiddenCount: 0, showOverflow: false });
    expect(fitTagChips(
      Array.from({ length: 4 }, () => ({ width: 45, height: 24 })),
      overflow,
      190,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 3, hiddenCount: 1, showOverflow: true });
    expect(fitTagChips(
      Array.from({ length: 4 }, () => ({ width: 50, height: 24 })),
      overflow,
      180,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 2, hiddenCount: 2, showOverflow: true });
    expect(fitTagChips(
      Array.from({ length: 10 }, () => ({ width: 30, height: 24 })),
      overflow,
      70,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 1, hiddenCount: 9, showOverflow: true });
    expect(fitTagChips(
      Array.from({ length: 11 }, () => ({ width: 30, height: 24 })),
      overflow,
      70,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 1, hiddenCount: 10, showOverflow: true });
    expect(fitTagChips(
      Array.from({ length: 101 }, () => ({ width: 30, height: 24 })),
      overflow,
      70,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 0, hiddenCount: 101, showOverflow: true });
    expect(fitTagChips([], overflow, 100, 24, 6, 4))
      .toEqual({ visibleCount: 0, hiddenCount: 0, showOverflow: false });
    expect(fitTagChips(
      [{ width: 80, height: 30 }],
      [{ width: 25, height: 24 }],
      60,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 0, hiddenCount: 1, showOverflow: true });
    expect(fitTagChips(
      [{ width: 80, height: 30 }],
      [{ width: 25, height: 30 }],
      60,
      24,
      6,
      4,
    )).toEqual({ visibleCount: 0, hiddenCount: 1, showOverflow: false });
  });

  it("sorts display tags by favorite then Female, Male and neutral without mutating input", () => {
    const input = ["neutral-1", "male:a", "female:a", "female:b", "male:b", "neutral-2"];
    const original = [...input];
    const sorted = sortGalleryTags(input, new Set(["female:b", "male:a", "neutral-2"]));
    expect(sorted.map((tag) => tag.value)).toEqual([
      "female:b",
      "male:a",
      "neutral-2",
      "female:a",
      "male:b",
      "neutral-1",
    ]);
    expect(input).toEqual(original);
  });

  it("splits a pipe title for display while preserving the canonical title", () => {
    expect(splitGalleryTitle("Archive of Rain | 비 내리는 도시의 기록")).toEqual({
      primary: "Archive of Rain",
      secondary: "비 내리는 도시의 기록",
    });
    expect(splitGalleryTitle("Archive of Rain | Pipe subtitle", "Explicit subtitle")).toEqual({
      primary: "Archive of Rain",
      secondary: "Pipe subtitle · Explicit subtitle",
    });
    expect(splitGalleryTitle("Archive of Rain", "Archive of Rain")).toEqual({
      primary: "Archive of Rain",
      secondary: "",
    });
    expect(splitGalleryTitle("Archive of Rain | 한국어 | English", "English")).toEqual({
      primary: "Archive of Rain",
      secondary: "한국어 · English",
    });
  });

  it("renders a non-color selection indicator while preserving selection in the card name", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected
        selectionContext
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const article = container.querySelector("article");
    const indicator = article?.querySelector(".selection-indicator");
    expect(indicator).toHaveAttribute("aria-hidden", "true");
    expect(indicator?.querySelector("svg path")).not.toBeNull();
    expect(article).toHaveAccessibleName(expect.stringContaining("선택됨"));
    await act(async () => root.unmount());
    container.remove();
  });

  it("renders a resolved thumbnail URL with intrinsic dimensions and accessible loading hints", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      thumbnailWidth: 720,
      thumbnailHeight: 1080,
    };
    const thumbnailClient = new ThumbnailClient({
      resolve: () => ({
        kind: "image",
        url: "https://images.example.test/gallery-4051038.jpg",
        width: 720,
        height: 1080,
      }),
    });

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        thumbnailPriority="visible"
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
        thumbnailClient={thumbnailClient}
      />,
    ));

    const cover = container.querySelector<HTMLElement>(".cover");
    const image = cover?.querySelector<HTMLImageElement>(".cover-image");

    expect(cover).toHaveClass("has-thumbnail-image");
    expect(cover).toHaveStyle({ aspectRatio: "720 / 1080" });
    expect(cover).toHaveAttribute("data-thumbnail-kind", "gallery-cover");
    expect(cover).toHaveAttribute("data-thumbnail-consumer", "explore");
    expect(cover).toHaveAttribute("data-thumbnail-priority", "visible");
    expect(image).toHaveAttribute("src", "https://images.example.test/gallery-4051038.jpg");
    expect(image).toHaveAttribute("width", "720");
    expect(image).toHaveAttribute("height", "1080");
    expect(image).toHaveAttribute("loading", "eager");
    expect(image).toHaveAttribute("decoding", "async");
    expect(image).toHaveAttribute("alt", `${gallery.title} 표지`);
    await act(async () => root.unmount());
    container.remove();
  });

  it("keeps the sprite fallback square without treating an opaque thumbnail key as a URL", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      thumbnailKey: "opaque-thumbnail-key",
      thumbnailWidth: 720,
      thumbnailHeight: 1080,
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const cover = container.querySelector<HTMLElement>(".cover");
    const image = cover?.querySelector<HTMLImageElement>(".cover-image--sprite");
    expect(cover).toHaveClass("has-sprite-image");
    expect(cover).not.toHaveClass("has-thumbnail-image");
    expect(cover).toHaveStyle({ aspectRatio: "720 / 1080" });
    expect(image?.getAttribute("src")).not.toBe("opaque-thumbnail-key");
    expect(image).toHaveAttribute("width", "1536");
    expect(image).toHaveAttribute("height", "1024");
    expect(image).toHaveAttribute("alt", `${gallery.title} 표지`);
    await act(async () => root.unmount());
    container.remove();
  });

  it.each([
    [undefined, undefined],
    [720, undefined],
    [0, 1080],
    [Number.NaN, 1080],
  ])("uses a stable square thumbnail ratio for incomplete dimensions (%s × %s)", async (width, height) => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      ...(width !== undefined ? { thumbnailWidth: width } : {}),
      ...(height !== undefined ? { thumbnailHeight: height } : {}),
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    expect(container.querySelector<HTMLElement>(".cover")).toHaveStyle({ aspectRatio: "1 / 1" });
    await act(async () => root.unmount());
    container.remove();
  });

  it("projects bottom-up download fill and progress onto the media pane", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[1]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="downloads"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const article = container.querySelector("article");
    const cover = article?.querySelector(":scope > .cover");
    const progress = cover?.querySelector<HTMLElement>(".progress-track");

    expect(article).toHaveClass("is-downloading");
    expect(article).toHaveStyle({ "--download-progress": "41%" });
    expect(cover?.querySelector(".status-wash")).not.toBeNull();
    expect(cover?.querySelector('.status-pill [data-status-icon="downloading"]')).not.toBeNull();
    expect(progress).toHaveAttribute("aria-valuenow", "41");
    expect(progress?.querySelector("span")).toHaveStyle({ width: "41%" });
    await act(async () => root.unmount());
    container.remove();
  });

  it.each([
    [140, 100],
    [-5, 0],
  ])("clamps an out-of-range progress value of %s to %s", async (reportedProgress, expectedProgress) => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[1]!,
      download: { entryId: `entry-${reportedProgress}`, state: "downloading", progress: reportedProgress },
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="downloads"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const article = container.querySelector("article");
    const progress = article?.querySelector<HTMLElement>(".progress-track");
    expect(article).toHaveStyle({ "--download-progress": `${expectedProgress}%` });
    expect(progress).toHaveAttribute("aria-valuenow", String(expectedProgress));
    expect(progress?.querySelector("span")).toHaveStyle({ width: `${expectedProgress}%` });
    await act(async () => root.unmount());
    container.remove();
  });

  it.each([
    ["downloading", "다운로드 중", "is-downloading"],
    ["review_required", "중복 의심", "is-review_required"],
  ] as const)("renders %s as an accessible icon-only status", async (state, label, className) => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      download: { entryId: `entry-${state}`, state, progress: state === "downloading" ? 41 : 0 },
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="downloads"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const status = container.querySelector<HTMLButtonElement>(`.status-pill.${className}`);
    expect(status).not.toBeNull();
    expect(status).toHaveAccessibleName(expect.stringContaining(label));
    expect(status?.textContent).not.toContain(label);
    expect(status?.querySelector(".fluent")).toBeNull();
    expect(status?.querySelector(`[data-status-icon="${state === "downloading" ? "downloading" : "warning"}"]`)).not.toBeNull();
    await act(async () => root.unmount());
    container.remove();
  });

  it("uses the quiet completion icon instead of a text check", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      download: { entryId: "entry-complete", state: "completed", progress: 100 },
    };

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));

    const completion = container.querySelector(".download-check");
    expect(completion?.querySelector('[data-status-icon="complete"]')).not.toBeNull();
    expect(completion?.textContent).not.toContain("✓");
    expect(container.querySelector("article")).toHaveAccessibleName(expect.stringContaining("다운로드 완료"));
    await act(async () => root.unmount());
    container.remove();
  });

  it("suppresses the double-click action when the gesture started in selection context", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected
        selectionContext
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    const article = container.querySelector("article");
    await act(async () => {
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
      article?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
    });

    expect(callbacks.onSelect).toHaveBeenCalledWith(gallery.id, expect.anything());
    expect(callbacks.onOpenDetail).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("keeps a modifier double click in selection instead of opening detail", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    const article = container.querySelector("article");
    await act(async () => {
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, detail: 1 }));
      article?.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, detail: 2 }));
      article?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, ctrlKey: true, detail: 2 }));
    });

    expect(callbacks.onSelect).toHaveBeenCalledTimes(1);
    expect(callbacks.onOpenDetail).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("selects only once when an internal action is modifier-double-clicked", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
      />,
    ));
    const byline = container.querySelector<HTMLButtonElement>(".card-byline .byline");
    await act(async () => {
      byline?.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, detail: 1 }));
      byline?.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true, detail: 2 }));
      byline?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, ctrlKey: true, detail: 2 }));
    });

    expect(callbacks.onSelect).toHaveBeenCalledTimes(1);
    expect(callbacks.onMetadataSearch).not.toHaveBeenCalled();
    expect(callbacks.onOpenDetail).not.toHaveBeenCalled();
    await act(async () => root.unmount());
    container.remove();
  });

  it("keeps Female and Male markers when favorite stars are shown and reorders immediately", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      tags: ["neutral-1", "male:a", "female:a", "female:b", "male:b", "neutral-2"],
    };
    const render = (favorites: ReadonlySet<string>) => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={favorites}
        {...callbacks}
      />,
    );

    await act(async () => render(new Set(["female:b", "male:a", "neutral-2"])));
    const tagValues = () => [...container.querySelectorAll<HTMLButtonElement>(".tag")]
      .map((tag) => tag.querySelector(".tag-label")?.textContent);
    expect(tagValues()).toEqual(["b", "a", "neutral-2", "a", "b", "neutral-1"]);

    const favoriteFemale = container.querySelector<HTMLButtonElement>('.tag[aria-label^="b, Female 태그, 즐겨찾기"]');
    const favoriteMale = container.querySelector<HTMLButtonElement>('.tag[aria-label^="a, Male 태그, 즐겨찾기"]');
    const favoriteNeutral = container.querySelector<HTMLButtonElement>('.tag[aria-label^="neutral-2, 중립 태그, 즐겨찾기"]');
    const normalFemale = container.querySelector<HTMLButtonElement>('.tag[aria-label^="a, Female 태그, 좌클릭"]');
    expect(favoriteFemale?.querySelector(".tag-namespace")).toHaveTextContent("F");
    expect(favoriteFemale?.querySelector(".tag-favorite")).toHaveTextContent("★");
    expect(favoriteMale?.querySelector(".tag-namespace")).toHaveTextContent("M");
    expect(favoriteMale?.querySelector(".tag-favorite")).toHaveTextContent("★");
    expect(favoriteNeutral?.querySelector(".tag-namespace")).toBeNull();
    expect(favoriteNeutral?.querySelector(".tag-favorite")).toHaveTextContent("★");
    expect(normalFemale?.querySelector(".tag-namespace")).toHaveTextContent("F");
    expect(normalFemale?.querySelector(".tag-favorite")).toBeNull();

    await act(async () => {
      favoriteFemale?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
    });
    expect(callbacks.onMetadataFavorite).toHaveBeenCalledWith("female:b");

    await act(async () => render(new Set(["female:a"])));
    expect(tagValues().slice(0, 3)).toEqual(["a", "b", "a"]);

    await act(async () => root.unmount());
    container.remove();
  });

  it("renders the measured maximum tags plus a non-interactive +N and recalculates on resize", async () => {
    let availableWidth = 175;
    let resolveFonts: (() => void) | undefined;
    const fontReady = new Promise<void>((resolve) => { resolveFonts = resolve; });
    const originalFonts = Object.getOwnPropertyDescriptor(document, "fonts");
    Object.defineProperty(document, "fonts", {
      configurable: true,
      value: { ready: fontReady },
    });
    const observed: Array<{ target: Element; callback: ResizeObserverCallback }> = [];
    const originalResizeObserver = globalThis.ResizeObserver;
    class ControlledResizeObserver implements ResizeObserver {
      constructor(private readonly callback: ResizeObserverCallback) {}
      observe(target: Element) { observed.push({ target, callback: this.callback }); }
      unobserve() {}
      disconnect() {}
    }
    globalThis.ResizeObserver = ControlledResizeObserver;
    const width = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("tag-list") ? availableWidth : 300;
    });
    const height = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("tag-list") ? 24 : 220;
    });
    const rect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const chipWidth = this.classList.contains("tag")
        ? 45
        : this.classList.contains("tag-overflow") ? 25 : 220;
      const chipHeight = this.classList.contains("tag") || this.classList.contains("tag-overflow") ? 24 : 220;
      return {
        x: 0,
        y: 0,
        width: chipWidth,
        height: chipHeight,
        top: 0,
        right: chipWidth,
        bottom: chipHeight,
        left: 0,
        toJSON: () => ({}),
      };
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      tags: ["tag-1", "tag-2", "tag-3", "tag-4"],
    };

    try {
      await act(async () => root.render(
        <GalleryCard
          gallery={gallery}
          view="explore"
          selected={false}
          selectionContext={false}
          favoriteMetadata={new Set()}
          {...callbacks}
        />,
      ));
      expect(container.querySelectorAll(".tag")).toHaveLength(3);
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)")).toHaveTextContent("+1");
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)"))
        .toHaveAccessibleName("추가 태그 1개");
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)")).not.toHaveAttribute("tabindex");
      expect([...container.querySelectorAll(".tag-label")].map((label) => label.textContent))
        .not.toContain("tag-4");

      availableWidth = 100;
      const contentObserver = observed.find(({ target }) => target.classList.contains("card-content"));
      await act(async () => contentObserver?.callback([], {} as ResizeObserver));
      expect(container.querySelectorAll(".tag")).toHaveLength(1);
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)")).toHaveTextContent("+3");

      availableWidth = 175;
      await act(async () => { resolveFonts?.(); await fontReady; });
      expect(container.querySelectorAll(".tag")).toHaveLength(3);
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)")).toHaveTextContent("+1");

      await act(async () => contentObserver?.callback([], {} as ResizeObserver));
      expect(container.querySelector(".tag-overflow:not(.tag-overflow-measure)")).toHaveTextContent("+1");
    } finally {
      await act(async () => root.unmount());
      container.remove();
      rect.mockRestore();
      height.mockRestore();
      width.mockRestore();
      globalThis.ResizeObserver = originalResizeObserver;
      if (originalFonts) Object.defineProperty(document, "fonts", originalFonts);
      else Reflect.deleteProperty(document, "fonts");
    }
  });

  it("keeps series and character metadata out of compact cards", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = mockGalleries[0]!;

    await act(async () => root.render(
      <GalleryCard
        gallery={gallery}
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set(["series:rain archives"])}
        {...callbacks}
      />,
    ));
    expect(container.querySelector('[title^="시리즈 · rain archives"]')).toBeNull();
    expect(container.querySelector('[title^="캐릭터 · mira lane"]')).toBeNull();

    await act(async () => root.unmount());
    container.remove();
  });
});
