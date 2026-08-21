import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type Gallery } from "../core/types";
import { mockGalleries } from "../data/mockGalleries";
import { ThumbnailClient } from "../thumbnail";
import { DetailWorkspace } from "./DetailWorkspace";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("DetailWorkspace page previews", () => {
  it("renders only the current page window and keeps a zero-page gallery safe", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const previousShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    const previousClose = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "close");
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value: vi.fn(function (this: HTMLDialogElement) {
        this.setAttribute("open", "");
      }),
    });
    Object.defineProperty(HTMLDialogElement.prototype, "close", {
      configurable: true,
      value: vi.fn(function (this: HTMLDialogElement) {
        this.removeAttribute("open");
      }),
    });
    const previousScrollTo = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollTo");
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      configurable: true,
      value: vi.fn(),
    });
    const source = mockGalleries[0]!;
    const gallery: Gallery = { ...source, pages: 99 };
    const client = new ThumbnailClient({
      resolve: () => ({ kind: "missing", reason: "test fixture" }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const render = (item: Gallery) => root.render(
      <DetailWorkspace
        tabs={[item.id]}
        activeId={item.id}
        minimized={false}
        galleries={new Map([[item.id, item]])}
        favoriteMetadata={new Set()}
        thumbnailClient={client}
        onActivate={vi.fn()}
        onClose={vi.fn()}
        onCloseAll={vi.fn()}
        onMinimize={vi.fn()}
        onRestore={vi.fn()}
        onOpenRelated={vi.fn()}
        onQueue={vi.fn()}
        onMetadataSearch={vi.fn()}
        onMetadataFavorite={vi.fn()}
      />
    );

    try {
      await act(async () => render(gallery));

      expect(container.querySelectorAll(".preview-thumb").length).toBeGreaterThan(0);
      expect(container.querySelectorAll(".preview-thumb").length).toBeLessThan(gallery.pages);
      expect(container.querySelectorAll('[data-thumbnail-kind="source-page"]')).toHaveLength(container.querySelectorAll(".preview-thumb").length);
      expect(container.querySelector(".preview-grid")).toHaveAttribute("data-preview-columns", "3");
      expect(container.querySelector(".preview-grid")).toHaveAttribute("data-preview-orientation", "mixed");
      expect(container.querySelector(".detail-cover")).toHaveAttribute("data-thumbnail-kind", "gallery-cover");

      await act(async () => {
        container.querySelector<HTMLButtonElement>(".preview-thumb")?.click();
      });
      expect(container.querySelector(".page-preview-dialog")).toHaveAttribute("open");
      expect(container.querySelector("#page-preview-title")).toHaveTextContent("1페이지");
      expect(container.querySelectorAll('[data-thumbnail-kind="source-page"]')).toHaveLength(container.querySelectorAll(".preview-thumb").length + 1);

      await act(async () => render({ ...gallery, pages: 0 }));

      expect(container.querySelectorAll(".preview-thumb")).toHaveLength(0);
      expect(container.querySelectorAll('[data-thumbnail-kind="source-page"]')).toHaveLength(0);
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
      if (previousScrollTo) {
        Object.defineProperty(HTMLElement.prototype, "scrollTo", previousScrollTo);
      } else {
        Reflect.deleteProperty(HTMLElement.prototype, "scrollTo");
      }
      if (previousShowModal) {
        Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      } else {
        Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      }
      if (previousClose) {
        Object.defineProperty(HTMLDialogElement.prototype, "close", previousClose);
      } else {
        Reflect.deleteProperty(HTMLDialogElement.prototype, "close");
      }
    }
  });

  it("moves only the bounded page window with previous and next controls", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const resizeCallbacks: Array<() => void> = [];
    class TestResizeObserver {
      constructor(callback: () => void) { resizeCallbacks.push(callback); }
      observe() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", TestResizeObserver);
    const gallery: Gallery = { ...mockGalleries[0]!, pages: 5000 };
    const client = new ThumbnailClient({ resolve: () => ({ kind: "missing", reason: "test fixture" }) });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    try {
      await act(async () => root.render(
        <DetailWorkspace tabs={[gallery.id]} activeId={gallery.id} minimized={false} galleries={new Map([[gallery.id, gallery]])} favoriteMetadata={new Set()} thumbnailClient={client} onActivate={vi.fn()} onClose={vi.fn()} onCloseAll={vi.fn()} onMinimize={vi.fn()} onRestore={vi.fn()} onOpenRelated={vi.fn()} onQueue={vi.fn()} onMetadataSearch={vi.fn()} onMetadataFavorite={vi.fn()} />,
      ));
      expect(container.querySelectorAll(".preview-thumb").length).toBeLessThan(20);
      const nav = container.querySelector(".preview-window-nav");
      expect(nav).not.toHaveTextContent("처음");
      expect(nav).not.toHaveTextContent("마지막");
      expect(container.querySelector('input[aria-label="페이지 번호로 이동"]')).toBeNull();
      const next = [...container.querySelectorAll<HTMLButtonElement>(".preview-window-nav button")]
        .find((button) => button.textContent === "다음 묶음");
      const previous = [...container.querySelectorAll<HTMLButtonElement>(".preview-window-nav button")]
        .find((button) => button.textContent === "이전 묶음");
      const initialStart = container.querySelector<HTMLButtonElement>(".preview-thumb")?.textContent;
      await act(async () => {
        next?.click();
      });
      expect(container.querySelector<HTMLButtonElement>(".preview-thumb")?.textContent).not.toBe(initialStart);
      const movedStart = container.querySelector<HTMLButtonElement>(".preview-thumb")?.textContent;
      await act(async () => {
        resizeCallbacks.forEach((callback) => callback());
      });
      expect(container.querySelector<HTMLButtonElement>(".preview-thumb")?.textContent).toBe(movedStart);
      await act(async () => {
        previous?.click();
      });
      expect(container.querySelector<HTMLButtonElement>(".preview-thumb")?.textContent).toBe(initialStart);
      expect(container.querySelectorAll(".preview-thumb").length).toBeLessThan(20);
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
    }
  });

  it("locks a landscape preview grid for the active detail tab", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const gallery: Gallery = { ...mockGalleries[0]!, pages: 8 };
    const client = new ThumbnailClient({
      resolve: () => ({ kind: "image" as const, url: "https://images.example.test/landscape.jpg", width: 1600, height: 900 }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    try {
      await act(async () => root.render(
        <DetailWorkspace
          tabs={[gallery.id]}
          activeId={gallery.id}
          minimized={false}
          galleries={new Map([[gallery.id, gallery]])}
          favoriteMetadata={new Set()}
          thumbnailClient={client}
          onActivate={vi.fn()}
          onClose={vi.fn()}
          onCloseAll={vi.fn()}
          onMinimize={vi.fn()}
          onRestore={vi.fn()}
          onOpenRelated={vi.fn()}
          onQueue={vi.fn()}
          onMetadataSearch={vi.fn()}
          onMetadataFavorite={vi.fn()}
        />,
      ));
      expect(container.querySelector(".preview-grid")).toHaveAttribute("data-preview-columns", "2");
      expect(container.querySelector(".preview-grid")).toHaveAttribute("data-preview-orientation", "landscape");
      expect(container.querySelector(".preview-thumb .gallery-thumbnail")).toHaveStyle({ aspectRatio: "1600 / 900" });
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
    }
  });

  it("changes only the dialog page and its source-page thumbnail key", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const previousShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value: vi.fn(function (this: HTMLDialogElement) { this.setAttribute("open", ""); }),
    });
    const gallery: Gallery = { ...mockGalleries[0]!, pages: 25 };
    const client = new ThumbnailClient({
      resolve: () => ({ kind: "image" as const, url: "https://images.example.test/page.jpg", width: 800, height: 1000 }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    try {
      await act(async () => root.render(
        <DetailWorkspace tabs={[gallery.id]} activeId={gallery.id} minimized={false} galleries={new Map([[gallery.id, gallery]])} favoriteMetadata={new Set()} thumbnailClient={client} onActivate={vi.fn()} onClose={vi.fn()} onCloseAll={vi.fn()} onMinimize={vi.fn()} onRestore={vi.fn()} onOpenRelated={vi.fn()} onQueue={vi.fn()} onMetadataSearch={vi.fn()} onMetadataFavorite={vi.fn()} />,
      ));
      await act(async () => container.querySelector<HTMLButtonElement>(".preview-thumb")?.click());
      expect(container.querySelector("#page-preview-title")).toHaveTextContent("1페이지");
      expect(container.querySelector<HTMLImageElement>(".page-preview-media img")?.alt).toContain("1페이지");
      await act(async () => {
        [...container.querySelectorAll<HTMLButtonElement>(".page-preview-controls button")]
          .find((button) => button.textContent === "다음")?.click();
      });
      expect(container.querySelector("#page-preview-title")).toHaveTextContent("2페이지");
      expect(container.querySelector<HTMLImageElement>(".page-preview-media img")?.alt).toContain("2페이지");
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
      if (previousShowModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      else Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
    }
  });

  it("uses the card tag order in detail and keeps series and characters out of related galleries", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const parent: Gallery = { ...mockGalleries[0]!, relatedIds: [mockGalleries[6]!.id] };
    const related = mockGalleries[6]!;
    const onMetadataSearch = vi.fn();
    const onMetadataFavorite = vi.fn();
    const onOpenRelated = vi.fn();
    const client = new ThumbnailClient({
      resolve: () => ({ kind: "missing", reason: "test fixture" }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    try {
      await act(async () => root.render(
        <DetailWorkspace
          tabs={[parent.id]}
          activeId={parent.id}
          minimized={false}
          galleries={new Map([[parent.id, parent], [related.id, related]])}
          favoriteMetadata={new Set(["series:rain archives", "character:mira lane"])}
          thumbnailClient={client}
          onActivate={vi.fn()}
          onClose={vi.fn()}
          onCloseAll={vi.fn()}
          onMinimize={vi.fn()}
          onRestore={vi.fn()}
          onOpenRelated={onOpenRelated}
          onQueue={vi.fn()}
          onMetadataSearch={onMetadataSearch}
          onMetadataFavorite={onMetadataFavorite}
        />,
      ));

      const mainSeries = container.querySelector<HTMLButtonElement>('[title^="rain archives"]');
      const mainCharacter = container.querySelector<HTMLButtonElement>('[title^="mira lane ·"]');
      expect(mainSeries).toHaveClass("favorite");
      expect(mainCharacter).toHaveClass("favorite");
      expect(container.querySelector(".related-card")?.textContent).not.toContain("rain archives");
      expect(container.querySelector(".related-card")?.textContent).not.toContain("mira lane");

      const relatedTags = [...container.querySelectorAll<HTMLButtonElement>(".related-card .tag")]
        .map((chip) => chip.textContent?.replace(/[★FM]/g, "").trim());
      expect(relatedTags).toEqual(["coat", "suit", "rain", "drama"]);
      expect(container.querySelector(".related-open-command")).toBeNull();
      expect(container.querySelector(".related-card .meta-bottom")).toHaveTextContent(`${related.pages}p`);
      expect(container.querySelector(".related-card .meta-bottom")).toHaveTextContent(`#${related.id}`);
      expect(container.querySelector(".related-card")).toHaveAttribute("tabindex", "0");

      await act(async () => {
        mainSeries?.click();
        mainCharacter?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
        container.querySelector<HTMLButtonElement>(".related-card .tag")?.click();
        container.querySelector<HTMLButtonElement>(".related-card .byline")?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      });
      expect(onMetadataSearch).toHaveBeenCalledWith("series:rain_archives");
      expect(onMetadataFavorite).toHaveBeenCalledWith("character:mira lane");
      expect(onOpenRelated).not.toHaveBeenCalled();
      const relatedCard = container.querySelector<HTMLElement>(".related-card");
      await act(async () => {
        relatedCard?.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
        relatedCard?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      });
      expect(onOpenRelated).toHaveBeenCalledTimes(2);
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
    }
  });
});
