import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { backend } from "./api/backend";
import type { GalleryPage } from "./api/contracts";
import { galleryId } from "./core/types";
import { browserFixtureThumbnailAdapter, ThumbnailClient, ThumbnailProvider } from "./thumbnail";

const testThumbnailClient = new ThumbnailClient(browserFixtureThumbnailAdapter);
const TestApp = () => <ThumbnailProvider client={testThumbnailClient}><App /></ThumbnailProvider>;

const settle = (delay = 20) => new Promise((resolve) => window.setTimeout(resolve, delay));

const explorePage = (page: number, totalPages = 20): GalleryPage => ({
  page,
  totalPages,
  items: [{
    id: galleryId(9_000_000 + page),
    title: `Explore page ${page}`,
    artist: "paging fixture",
    pages: 1,
    language: "korean",
    tags: [],
    series: [],
    characters: [],
    publishedRank: 20260820,
    popularity: 0,
    thumbnailWidth: 512,
    thumbnailHeight: 768,
  }],
});

const clickButtonContaining = (container: HTMLElement, label: string): HTMLButtonElement => {
  const button = [...container.querySelectorAll<HTMLButtonElement>("button")]
    .find((item) => item.textContent?.includes(label));
  if (!button) throw new Error(`Button containing ${label} was not found`);
  button.click();
  return button;
};

describe("App Phase 3A backend flow", () => {
  beforeEach(() => {
    class TestResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", TestResizeObserver);
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(Date.now()), 0));
    vi.stubGlobal("cancelAnimationFrame", (id: number) => window.clearTimeout(id));
  });

  afterEach(() => {
    testThumbnailClient.dispose();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

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
      root.render(<TestApp />);
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

  it("projects cached Explore pages, warms adjacent pages once, and restores each page scroll position", async () => {
    const searchSubmit = vi.spyOn(backend, "searchSubmit").mockResolvedValue({
      ok: true,
      data: { queryId: "paging-query", firstPage: explorePage(1) },
    });
    const searchPageGet = vi.spyOn(backend, "searchPageGet").mockImplementation(async (_queryId, page) => ({
      ok: true,
      data: explorePage(page),
    }));
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<TestApp />);
      await settle();
    });
    expect(searchSubmit).toHaveBeenCalledOnce();
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 2)).toHaveLength(1);

    const viewport = container.querySelector<HTMLElement>(".gallery-viewport");
    if (!viewport) throw new Error("Explore viewport was not rendered");
    await act(async () => {
      clickButtonContaining(container, "다음");
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "다음");
      await settle();
    });
    expect(container.textContent).toContain("Explore page 3");
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 3)).toHaveLength(1);
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 4)).toHaveLength(1);
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 2)).toHaveLength(1);

    viewport.scrollTop = 417;
    const fourthCallsBeforeForeground = searchPageGet.mock.calls.filter(([, page]) => page === 4).length;
    await act(async () => {
      clickButtonContaining(container, "다음");
      await settle();
    });
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 4)).toHaveLength(fourthCallsBeforeForeground);
    viewport.scrollTop = 88;
    const thirdCallsBeforeReturn = searchPageGet.mock.calls.filter(([, page]) => page === 3).length;
    await act(async () => {
      clickButtonContaining(container, "이전");
      await settle();
    });
    expect(searchPageGet.mock.calls.filter(([, page]) => page === 3)).toHaveLength(thirdCallsBeforeReturn);
    expect(viewport.scrollTop).toBe(417);

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
      root.render(<TestApp />);
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

  it("keeps series and character favorites in detail and related metadata while cards stay compact", async () => {
    const favoriteSet = vi.spyOn(backend, "favoriteSet");
    const container = document.createElement("div");
    document.body.append(container);
    let root = createRoot(container);

    await act(async () => {
      root.render(<TestApp />);
      await settle();
    });
    const archiveCard = container.querySelector<HTMLElement>('[data-gallery-id="4051038"]');
    if (!archiveCard) throw new Error("Archive fixture card was not rendered");
    expect(archiveCard.querySelector('[title^="시리즈 · rain archives"]')).toBeNull();
    expect(archiveCard.querySelector('[title^="캐릭터 · mira lane"]')).toBeNull();

    await act(async () => {
      archiveCard.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
      await settle();
    });
    const detailSeries = container.querySelector<HTMLButtonElement>('.detail-workspace [title^="rain archives"]');
    const detailCharacter = container.querySelector<HTMLButtonElement>('.detail-workspace [title^="mira lane"]');
    if (!detailSeries || !detailCharacter) throw new Error("Detail series/character chips were not rendered");
    await act(async () => {
      detailSeries.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      detailCharacter.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true }));
      await settle();
    });
    expect(favoriteSet).toHaveBeenCalledWith({ namespace: "series", value: "rain archives" }, true);
    expect(favoriteSet).toHaveBeenCalledWith({ namespace: "character", value: "mira lane" }, true);
    expect([...container.querySelectorAll<HTMLButtonElement>('.detail-workspace [title^="rain archives"]')]
      .every((chip) => chip.classList.contains("favorite"))).toBe(true);
    expect([...container.querySelectorAll<HTMLButtonElement>('.detail-workspace [title^="mira lane"]')]
      .every((chip) => chip.classList.contains("favorite"))).toBe(true);

    const matchingDetailChips = [...container.querySelectorAll<HTMLButtonElement>(
      '.detail-workspace [title^="rain archives"], .detail-workspace [title^="시리즈 · rain archives"]',
    )];
    expect(matchingDetailChips.length).toBeGreaterThanOrEqual(2);
    expect(matchingDetailChips.every((chip) => chip.classList.contains("favorite"))).toBe(true);

    await act(async () => root.unmount());
    container.replaceChildren();
    root = createRoot(container);
    await act(async () => {
      root.render(<TestApp />);
      await settle();
    });
    expect(container.querySelector('[data-gallery-id="4051038"] [title^="시리즈 · rain archives"]')).toBeNull();
    expect(container.querySelector('[data-gallery-id="4051038"] [title^="캐릭터 · mira lane"]')).toBeNull();

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
      root.render(<TestApp />);
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
      root.render(<TestApp />);
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

  it("recovers a failed snapshot, scans and cancels explicitly, then reviews real evidence with CAS reload", async () => {
    await backend.downloadQueueAdd(
      [galleryId(4051038), galleryId(4050754)],
      "app-duplicate-review-downloads",
    );
    const snapshot = vi.spyOn(backend, "duplicateSnapshot").mockResolvedValueOnce({
      ok: false,
      error: {
        code: "BACKEND_UNAVAILABLE",
        message: "initial duplicate snapshot unavailable",
        retryable: true,
        action: "retry",
      },
    });
    const scanStart = vi.spyOn(backend, "duplicateScanStart");
    const scanCancel = vi.spyOn(backend, "duplicateScanCancel");
    const decision = vi.spyOn(backend, "duplicateDecisionApply");
    const quarantine = vi.spyOn(backend, "downloadQuarantine");
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(<TestApp />);
      await settle();
    });
    await act(async () => {
      clickButtonContaining(container, "Downloads");
      await settle();
    });
    expect(container.textContent).toContain("initial duplicate snapshot unavailable");

    await act(async () => {
      clickButtonContaining(container, "전체 작품 간 중복 검사");
      await settle(15);
    });
    expect(scanStart).toHaveBeenCalledTimes(1);
    expect(snapshot).toHaveBeenCalledTimes(2);
    expect(container.textContent).toContain("중복 검사 중");
    await act(async () => {
      clickButtonContaining(container, "중복 검사 취소");
      await settle();
    });
    expect(scanCancel).toHaveBeenCalledTimes(1);
    expect(container.textContent).toContain("중복 검사 취소됨");

    await act(async () => {
      clickButtonContaining(container, "전체 작품 간 중복 검사");
      await settle(130);
    });
    expect(container.textContent).toContain("중복 검사 완료");
    const warning = container.querySelector<HTMLButtonElement>(
      '[data-gallery-id="4051038"] .status-pill.has-duplicate-count',
    );
    expect(warning).toHaveTextContent("1");
    expect(warning).toHaveAccessibleName(expect.stringContaining("중복 후보 1개"));

    await act(async () => {
      warning?.focus();
      warning?.click();
      await settle();
    });
    expect(container.querySelector(".review-dialog")).toHaveAttribute("open");
    expect(container.querySelector(".review-summary")).toHaveTextContent("신뢰도 94%");
    expect(container.textContent).toContain("브라우저 검토 fixture");
    expect(container.textContent).toContain("원본 페이지 번호를 보존한 순서 정렬");
    expect(container.textContent).not.toContain("82%");
    expect(container.textContent).not.toContain("first gid");

    await act(async () => {
      container.querySelector<HTMLButtonElement>('.review-dialog button[aria-label="닫기"]')?.click();
      await settle();
    });
    expect(document.activeElement).toBe(warning);
    await act(async () => {
      warning?.click();
      await settle();
    });

    decision.mockResolvedValueOnce({
      ok: false,
      error: {
        code: "REVISION_CONFLICT",
        message: "stale",
        retryable: false,
        action: "review",
        details: { resource: "duplicateCandidate", expectedRevision: 0, actualRevision: 1 },
      },
    });
    await act(async () => {
      clickButtonContaining(container, "부모 숨기기");
      await settle();
    });
    expect(container.textContent).toContain("다른 창에서 판정이 변경되어 최신 근거와 이력을 다시 불러왔습니다.");

    await act(async () => {
      clickButtonContaining(container, "부모 숨기기");
      await settle();
    });
    expect(container.querySelector(".decision-history")).toHaveTextContent("부모 숨김");
    expect(container.textContent).toContain("자동으로 파일을 삭제하지 않으며");
    expect(quarantine).not.toHaveBeenCalled();

    await act(async () => root.unmount());
    container.remove();
  });
});
