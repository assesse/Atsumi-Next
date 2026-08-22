import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { BackendClient, BackendEventMap } from "../api/backend";
import { mockGalleries } from "../data/mockGalleries";
import { ThumbnailClient } from "../thumbnail";
import { ProgressiveDetailHero } from "./ProgressiveDetailHero";

describe("ProgressiveDetailHero", () => {
  it("starts the independent original even while the shared cover is still pending", async () => {
    const gallery = { ...mockGalleries[0]!, pageDimensions: [{ sourcePage: 1, width: 720, height: 1080 }] };
    const request = vi.fn(async () => ({
      ok: true as const,
      data: { requestId: "original-pending-cover", galleryId: gallery.id, sourcePage: 1 },
    }));
    const backend = {
      on: vi.fn(async () => () => undefined),
      detailOriginalRequest: request,
      detailOriginalCancel: vi.fn(async () => ({ ok: true as const, data: true })),
      detailOriginalRelease: vi.fn(async () => ({ ok: true as const, data: true })),
    } as unknown as BackendClient;
    const client = new ThumbnailClient({
      resolve: () => new Promise(() => undefined),
      cancel: vi.fn(),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<ProgressiveDetailHero gallery={gallery} client={client} backend={backend} />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(request).toHaveBeenCalledWith({ galleryId: gallery.id, sourcePage: 1 });
    expect(container.querySelector(".detail-cover")).toBeTruthy();

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("accepts a ready event that arrives before the request token response", async () => {
    let ready: ((event: BackendEventMap["detail-original:ready"]) => void) | undefined;
    const gallery = { ...mockGalleries[0]!, pageDimensions: [{ sourcePage: 1, width: 720, height: 1080 }] };
    const token = { requestId: "original-early", galleryId: gallery.id, sourcePage: 1 };
    const backend = {
      on: vi.fn(async (_event: "detail-original:ready", handler: typeof ready) => {
        ready = handler;
        return () => { ready = undefined; };
      }),
      detailOriginalRequest: vi.fn(async () => {
        // This mirrors the real worker: it can emit a cache-backed result before
        // the asynchronous invoke response has installed requestId.current.
        queueMicrotask(() => ready?.({
          ...token,
          mediaUrl: "detail-original://localhost/original-early",
          contentType: "image/webp",
          width: 720,
          height: 1080,
        }));
        return { ok: true as const, data: token };
      }),
      detailOriginalCancel: vi.fn(async () => ({ ok: true as const, data: true })),
      detailOriginalRelease: vi.fn(async () => ({ ok: true as const, data: true })),
    } as unknown as BackendClient;
    const client = new ThumbnailClient({
      resolve: () => ({ kind: "image" as const, url: "blob:cover", width: 512, height: 512 }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<ProgressiveDetailHero gallery={gallery} client={client} backend={backend} />);
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(container.querySelector<HTMLImageElement>(".detail-hero-original"))
      .toHaveAttribute("src", "detail-original://localhost/original-early");

    await act(async () => root.unmount());
    client.dispose();
    container.remove();
  });

  it("keeps the cover until the opaque original image has loaded", async () => {
    let ready: ((event: BackendEventMap["detail-original:ready"]) => void) | undefined;
    const request = vi.fn(async () => ({ ok: true as const, data: { requestId: "original-1", galleryId: mockGalleries[0]!.id, sourcePage: 1 } }));
    const release = vi.fn(async () => ({ ok: true as const, data: true }));
    const backend = {
      on: vi.fn(async (_event: "detail-original:ready", handler: typeof ready) => {
        ready = handler;
        return () => { ready = undefined; };
      }),
      detailOriginalRequest: request,
      detailOriginalCancel: vi.fn(async () => ({ ok: true as const, data: true })),
      detailOriginalRelease: release,
    } as unknown as BackendClient;
    const client = new ThumbnailClient({ resolve: () => ({ kind: "image" as const, url: "blob:cover", width: 512, height: 512 }) });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const gallery = { ...mockGalleries[0]!, pageDimensions: [{ sourcePage: 1, width: 720, height: 1080 }] };
    await act(async () => root.render(<ProgressiveDetailHero gallery={gallery} client={client} backend={backend} />));
    await act(async () => { await Promise.resolve(); });
    expect(request).toHaveBeenCalledWith({ galleryId: gallery.id, sourcePage: 1 });
    expect(container.querySelector(".detail-cover")).toBeTruthy();
    await act(async () => ready?.({ requestId: "original-1", galleryId: gallery.id, sourcePage: 1, mediaUrl: "detail-original://localhost/original-1", contentType: "image/webp", width: 720, height: 1080 }));
    const original = container.querySelector<HTMLImageElement>(".detail-hero-original");
    expect(original).toHaveClass("detail-hero-original");
    expect(original).not.toHaveClass("is-ready");
    Object.defineProperty(original!, "decode", { configurable: true, value: vi.fn(async () => undefined) });
    await act(async () => original?.dispatchEvent(new Event("load")));
    expect(original).toHaveClass("is-ready");
    await act(async () => root.unmount());
    expect(release).toHaveBeenCalledWith("original-1");
    client.dispose();
    container.remove();
  });
});
