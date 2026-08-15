import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { backend } from "./api/backend";
import { galleryId } from "./core/types";

const settle = (delay = 20) => new Promise((resolve) => window.setTimeout(resolve, delay));

const clickButtonContaining = (container: HTMLElement, label: string): HTMLButtonElement => {
  const button = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((item) => item.textContent?.includes(label));
  if (!button) throw new Error(`Button containing ${label} was not found`);
  button.click();
  return button;
};

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
    expect(container.textContent).toContain("브라우저 fixture");
    expect(container.textContent).not.toContain("backend fixture");

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

  it("does not search while typing and faithfully replays a structured history request", async () => {
    const replayRequest = {
      text: "archive",
      includeTags: ["full_color"],
      excludeTags: ["male:suit"],
      languages: ["english", "korean"] as const,
      sort: "popular_week" as const,
      pageSize: 17,
    };
    await backend.searchSubmit({ ...replayRequest, languages: [...replayRequest.languages] });
    const search = vi.spyOn(backend, "searchSubmit");
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<App />);
      await settle();
    });

    const input = container.querySelector<HTMLInputElement>('input[aria-label="검색"]');
    if (!input) throw new Error("Search input was not found");
    await act(async () => {
      input.focus();
      await settle();
    });
    const historySuggestion = [...container.querySelectorAll<HTMLButtonElement>(".suggestion")]
      .find((item) => item.textContent?.includes("archive"));
    if (!historySuggestion) throw new Error("Structured history suggestion was not found");
    await act(async () => {
      historySuggestion.click();
      await settle();
    });
    expect(search).toHaveBeenLastCalledWith({ ...replayRequest, languages: ["korean", "english"] });

    const callsAfterReplay = search.mock.calls.length;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    await act(async () => {
      valueSetter?.call(input, "typing must stay local");
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await settle();
    });
    expect(search).toHaveBeenCalledTimes(callsAfterReplay);

    await act(async () => root.unmount());
    container.remove();
  });

  it("persists series and character favorites across cards, detail, related items, and remount", async () => {
    const favoriteSet = vi.spyOn(backend, "favoriteSet");
    const container = document.createElement("div");
    document.body.append(container);
    let root = createRoot(container);

    await act(async () => {
      root.render(<App />);
      await settle();
    });
    const archiveCard = container.querySelector<HTMLElement>('[data-gallery-id="4051038"]');
    const series = archiveCard?.querySelector<HTMLButtonElement>('[title^="시리즈 · rain archives"]');
    const character = archiveCard?.querySelector<HTMLButtonElement>('[title^="캐릭터 · mira lane"]');
    if (!archiveCard || !series || !character) throw new Error("Series/character fixture chips were not rendered");

    await act(async () => {
      series.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      character.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      await settle();
    });
    expect(favoriteSet).toHaveBeenCalledWith({ namespace: "series", value: "rain archives" }, true);
    expect(favoriteSet).toHaveBeenCalledWith({ namespace: "character", value: "mira lane" }, true);
    expect([...container.querySelectorAll<HTMLButtonElement>('[title^="시리즈 · rain archives"]')]
      .every((chip) => chip.classList.contains("favorite"))).toBe(true);
    expect([...container.querySelectorAll<HTMLButtonElement>('[title^="캐릭터 · mira lane"]')]
      .every((chip) => chip.classList.contains("favorite"))).toBe(true);

    await act(async () => {
      archiveCard.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
      await settle();
    });
    const matchingDetailChips = [...container.querySelectorAll<HTMLButtonElement>(
      '.detail-workspace [title^="rain archives"], .detail-workspace [title^="시리즈 · rain archives"]',
    )];
    expect(matchingDetailChips.length).toBeGreaterThanOrEqual(2);
    expect(matchingDetailChips.every((chip) => chip.classList.contains("favorite"))).toBe(true);

    await act(async () => root.unmount());
    container.replaceChildren();
    root = createRoot(container);
    await act(async () => {
      root.render(<App />);
      await settle();
    });
    expect(container.querySelector<HTMLButtonElement>('[title^="시리즈 · rain archives"]')).toHaveClass("favorite");
    expect(container.querySelector<HTMLButtonElement>('[title^="캐릭터 · mira lane"]')).toHaveClass("favorite");

    await act(async () => root.unmount());
    container.remove();
    await backend.favoriteSet({ namespace: "series", value: "rain archives" }, false);
    await backend.favoriteSet({ namespace: "character", value: "mira lane" }, false);
  });

  it("cancels, restores, groups, batches, and excludes Auto Find candidates", async () => {
    await backend.favoriteSet({ namespace: "artist", value: "serein" }, true);
    await backend.favoriteSet({ namespace: "artist", value: "mizuno" }, true);
    const refresh = vi.spyOn(backend, "autoFindRefresh");
    const cancel = vi.spyOn(backend, "autoFindCancel");
    const exclude = vi.spyOn(backend, "autoFindExclude");
    const queue = vi.spyOn(backend, "downloadQueueAdd");
    const container = document.createElement("div");
    document.body.append(container);
    let root = createRoot(container);

    await act(async () => {
      root.render(<App />);
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "Auto Find");
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "즐겨찾기 작가 갱신");
      await settle(10);
    });
    expect(refresh).toHaveBeenCalledTimes(1);
    await act(async () => {
      clickButtonContaining(container, "탐색 취소");
      await settle();
    });
    expect(cancel).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("탐색 취소됨");

    await act(async () => {
      clickButtonContaining(container, "즐겨찾기 작가 갱신");
      await settle(150);
      container.querySelector<HTMLButtonElement>('button[aria-label="언어 필터"]')?.click();
      await settle();
      const english = [...container.querySelectorAll<HTMLLabelElement>(".language-popover label")]
        .find((label) => label.textContent?.includes("영어"))
        ?.querySelector<HTMLInputElement>('input[type="checkbox"]');
      english?.click();
      await settle();
    });
    expect(container.textContent).toContain("탐색 완료");
    expect(container.textContent).toContain("The Last Tram");
    expect(container.textContent).toContain("Blue Lane");

    await act(async () => {
      clickButtonContaining(container, "후보 다운로드");
      await settle();
    });
    expect(queue).toHaveBeenCalledWith(
      expect.arrayContaining([galleryId(4050754), galleryId(4050642)]),
      expect.stringMatching(/^frontend-queue-\d+-\d+$/),
    );

    await act(async () => root.unmount());
    container.replaceChildren();
    root = createRoot(container);
    await act(async () => {
      root.render(<App />);
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "Auto Find");
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "작가별");
      await settle();
    });
    expect(container.querySelectorAll(".gallery-group").length).toBeGreaterThanOrEqual(1);
    expect(container.textContent).toContain("The Last Tram");

    const cardsBeforeExclude = container.querySelectorAll(".gallery-card").length;
    const firstCard = container.querySelector<HTMLDivElement>(".gallery-card");
    await act(async () => {
      firstCard?.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
      await settle();
      clickButtonContaining(container, "제외");
      await settle();
    });
    expect(exclude).toHaveBeenCalledWith(
      [expect.any(Number)],
      "사용자가 Auto Find 후보 목록에서 제외함",
    );
    expect(container.querySelectorAll(".gallery-card")).toHaveLength(cardsBeforeExclude - 1);

    await act(async () => root.unmount());
    container.remove();
    await backend.favoriteSet({ namespace: "artist", value: "serein" }, false);
    await backend.favoriteSet({ namespace: "artist", value: "mizuno" }, false);
  });
});
