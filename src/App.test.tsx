import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { backend } from "./api/backend";
import { galleryId } from "./core/types";

const settle = () => new Promise((resolve) => window.setTimeout(resolve, 20));

describe("App Phase 3A backend flow", () => {
  afterEach(() => vi.restoreAllMocks());

  it("hydrates Recent and Downloads and queues through the formal backend client", async () => {
    const seeded = await backend.downloadQueueAdd([galleryId(4051038)], "app-test-seed-download");
    if (!seeded.ok) throw new Error(seeded.error.message);

    const search = vi.spyOn(backend, "searchSubmit");
    const downloadList = vi.spyOn(backend, "downloadEntriesList");
    const detail = vi.spyOn(backend, "galleryDetailGet");
    const queue = vi.spyOn(backend, "downloadQueueAdd");
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<App />);
      await settle();
    });

    expect(search).toHaveBeenCalledWith(expect.objectContaining({ text: "", sort: "recent" }));
    expect(downloadList).toHaveBeenCalledWith({ page: 1, pageSize: 200 });
    expect(detail).toHaveBeenCalledWith(galleryId(4051038));
    expect(container.textContent).toContain("Archive of Rain");

    const firstCard = container.querySelector<HTMLElement>('[data-gallery-id="4051027"]');
    await act(async () => {
      firstCard?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
    });
    const queueButton = container.querySelector<HTMLButtonElement>(".selection-toolbar .primary");
    await act(async () => {
      queueButton?.click();
      await settle();
    });

    expect(queue).toHaveBeenCalledWith(
      [galleryId(4051027)],
      expect.stringMatching(/^frontend-queue-\d+-\d+$/),
    );

    await act(async () => root.unmount());
    container.remove();
  });
});
