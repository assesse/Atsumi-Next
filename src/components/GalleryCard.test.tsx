import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DownloadChangedEvent } from "../api/contracts";
import type { Gallery, GalleryId } from "../core/types";
import { mockGalleries } from "../data/mockGalleries";
import { applyDownloadChanged } from "../state/downloadProjection";
import { ThumbnailClient } from "../thumbnail";
import { GalleryCard } from "./GalleryCard";

const callbacks = {
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

  it("shows four representative tags followed by an accessible overflow count", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery: Gallery = {
      ...mockGalleries[0]!,
      tags: ["one", "two", "three", "four", "five", "six", "seven"],
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

    const tagList = container.querySelector(".tag-list");
    const overflow = tagList?.querySelector<HTMLElement>(".tag-overflow");
    expect(tagList?.querySelectorAll(".tag")).toHaveLength(4);
    expect(tagList).toHaveAccessibleName(`태그: ${gallery.tags.join(", ")}`);
    expect(overflow).toHaveTextContent("+3");
    expect(overflow).toHaveAccessibleName("추가 태그 3개");
    expect(overflow).toHaveAttribute("title", "추가 태그: five, six, seven");
    await act(async () => root.unmount());
    container.remove();
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
        thumbnailClient={thumbnailClient}
        thumbnailPriority="visible"
        view="explore"
        selected={false}
        selectionContext={false}
        favoriteMetadata={new Set()}
        {...callbacks}
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
    expect(cover).toHaveStyle({ aspectRatio: "1 / 1" });
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
});
