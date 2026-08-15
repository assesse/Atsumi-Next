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
  it("requests at most 24 valid source pages and none for a zero-page gallery", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
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
        onPreview={vi.fn()}
      />
    );

    try {
      await act(async () => render(gallery));

      expect(container.querySelectorAll(".preview-thumb")).toHaveLength(24);
      expect(container.querySelectorAll('[data-thumbnail-kind="source-page"]')).toHaveLength(24);

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
    }
  });

  it("keeps series and character favorites consistent in detail and related metadata", async () => {
    vi.stubGlobal("requestAnimationFrame", vi.fn(() => 0));
    const parent: Gallery = { ...mockGalleries[0]!, relatedIds: [mockGalleries[6]!.id] };
    const related = mockGalleries[6]!;
    const onMetadataSearch = vi.fn();
    const onMetadataFavorite = vi.fn();
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
          onOpenRelated={vi.fn()}
          onQueue={vi.fn()}
          onMetadataSearch={onMetadataSearch}
          onMetadataFavorite={onMetadataFavorite}
          onPreview={vi.fn()}
        />,
      ));

      const mainSeries = container.querySelector<HTMLButtonElement>('[title^="rain archives"]');
      const relatedSeries = container.querySelector<HTMLButtonElement>('[title^="시리즈 · rain archives"]');
      const mainCharacter = container.querySelector<HTMLButtonElement>('[title^="mira lane ·"]');
      const relatedCharacter = container.querySelector<HTMLButtonElement>('[title^="캐릭터 · mira lane"]');
      expect(mainSeries).toHaveClass("favorite");
      expect(relatedSeries).toHaveClass("favorite");
      expect(mainCharacter).toHaveClass("favorite");
      expect(relatedCharacter).toHaveClass("favorite");

      await act(async () => {
        mainSeries?.click();
        relatedCharacter?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      });
      expect(onMetadataSearch).toHaveBeenCalledWith("series:rain_archives");
      expect(onMetadataFavorite).toHaveBeenCalledWith("character:mira lane");
    } finally {
      await act(async () => root.unmount());
      client.dispose();
      container.remove();
    }
  });
});
