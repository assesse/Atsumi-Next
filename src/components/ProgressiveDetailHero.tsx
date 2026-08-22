import { useCallback, useEffect, useRef, useState } from "react";
import type { BackendClient } from "../api/backend";
import type { Gallery } from "../core/types";
import { galleryCoverThumbnailKey, type ThumbnailClient } from "../thumbnail";
import { GalleryThumbnail } from "./GalleryThumbnail";

type ProgressiveDetailHeroProps = {
  gallery: Gallery;
  pageDimension?: { readonly sourcePage: number; readonly width?: number; readonly height?: number };
  client?: ThumbnailClient;
  backend: BackendClient;
};

type Original = { requestId: string; url: string; width: number; height: number };

/**
 * Keeps the small, retained cover visible until a separately fetched page-one
 * original has actually loaded and decoded. The original is an opaque backend
 * local-media URL, never an IPC byte array or a WebView remote fetch.
 */
export function ProgressiveDetailHero({ gallery, pageDimension, client, backend }: ProgressiveDetailHeroProps) {
  const [listenerReady, setListenerReady] = useState(false);
  const [original, setOriginal] = useState<Original | null>(null);
  const [displayOriginal, setDisplayOriginal] = useState(false);
  const requestId = useRef<string | null>(null);
  const generation = useRef(0);
  // The worker is allowed to finish before the invoke response delivers its
  // token. Keep that completion briefly instead of losing the only readiness
  // event and leaving the cover in place forever.
  const earlyReady = useRef(new Map<string, Original>());

  const acceptReady = useCallback((event: {
    requestId: string;
    galleryId: number;
    sourcePage: number;
    mediaUrl: string;
    width: number;
    height: number;
  }) => {
    if (event.galleryId !== gallery.id || event.sourcePage !== 1) return;
    const ready = {
      requestId: event.requestId,
      url: event.mediaUrl,
      width: event.width,
      height: event.height,
    };
    if (event.requestId === requestId.current) {
      setOriginal(ready);
      return;
    }
    earlyReady.current.set(event.requestId, ready);
    // Only a request started by this mounted hero can be accepted later. The
    // small bound also protects against arbitrary late backend events.
    if (earlyReady.current.size > 4) {
      const oldest = earlyReady.current.keys().next().value;
      if (oldest) earlyReady.current.delete(oldest);
    }
  }, [gallery.id]);

  useEffect(() => {
    const currentGeneration = ++generation.current;
    requestId.current = null;
    earlyReady.current.clear();
    setOriginal(null);
    setDisplayOriginal(false);
    return () => {
      const id = requestId.current;
      requestId.current = null;
      if (id) {
        void backend.detailOriginalCancel(id);
        void backend.detailOriginalRelease(id);
      }
      // Late events are ignored by their generation/request ID checks below.
      generation.current = Math.max(generation.current, currentGeneration + 1);
    };
  }, [backend, gallery.id]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let active = true;
    setListenerReady(false);
    void backend.on("detail-original:ready", (event) => {
      if (!active) return;
      acceptReady(event);
    }).then((dispose) => {
      if (active) {
        unlisten = dispose;
        setListenerReady(true);
      }
      else dispose();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [acceptReady, backend]);

  useEffect(() => {
    // The retained cover is only the progressive display layer. Starting the
    // page-one original must not depend on that shared thumbnail request
    // reaching a terminal state; a stalled cover would otherwise stall the
    // independent original forever.
    if (!listenerReady || requestId.current || original || !gallery.id) return;
    let active = true;
    const requestGeneration = generation.current;
    void backend.detailOriginalRequest({ galleryId: gallery.id, sourcePage: 1 }).then((result) => {
      if (!active || !result.ok || generation.current !== requestGeneration) {
        if (result.ok) {
          void backend.detailOriginalCancel(result.data.requestId);
          void backend.detailOriginalRelease(result.data.requestId);
        }
        return;
      }
      requestId.current = result.data.requestId;
      const ready = earlyReady.current.get(result.data.requestId);
      if (ready) {
        earlyReady.current.delete(result.data.requestId);
        setOriginal(ready);
      }
    });
    return () => { active = false; };
  }, [backend, gallery.id, listenerReady, original]);

  const ratio = pageDimension ?? gallery.pageDimensions?.find((page) => page.sourcePage === 1);
  const expectedAspectRatio = ratio?.width !== undefined && ratio?.height !== undefined
    ? { width: ratio.width, height: ratio.height }
    : gallery.thumbnailWidth !== undefined && gallery.thumbnailHeight !== undefined
      ? { width: gallery.thumbnailWidth, height: gallery.thumbnailHeight }
      : { width: 1, height: 1 };

  const abandonOriginal = () => {
    const id = requestId.current;
    requestId.current = null;
    setOriginal(null);
    setDisplayOriginal(false);
    if (id) void backend.detailOriginalRelease(id);
  };

  return (
    <div className="detail-hero" style={{ aspectRatio: `${expectedAspectRatio.width} / ${expectedAspectRatio.height}` }}>
      <GalleryThumbnail
        className="detail-cover"
        thumbnailKey={galleryCoverThumbnailKey(gallery)}
        consumer="detail"
        priority="critical"
        client={client}
        sizing="container"
        expectedAspectRatio={expectedAspectRatio}
        alt={`${gallery.title} 표지`}
      />
      {original ? (
        <img
          className={`detail-hero-original${displayOriginal ? " is-ready" : ""}`}
          src={original.url}
          width={original.width}
          height={original.height}
          alt=""
          onError={abandonOriginal}
          onLoad={(event) => {
            const image = event.currentTarget;
            void (typeof image.decode === "function" ? image.decode() : Promise.resolve()).then(
              () => setDisplayOriginal(true),
              abandonOriginal,
            );
          }}
        />
      ) : null}
    </div>
  );
}
