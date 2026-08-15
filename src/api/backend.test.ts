import { describe, expect, it, vi } from "vitest";
import { galleryId } from "../core/types";
import { backend } from "./backend";
import type { DownloadEntry, SearchRequest } from "./contracts";

const searchRequest = (patch: Partial<SearchRequest> = {}): SearchRequest => ({
  text: "",
  includeTags: [],
  excludeTags: [],
  languages: ["korean"],
  sort: "recent",
  pageSize: 3,
  ...patch,
});

describe("browser backend settings contract", () => {
  it("rejects values outside the approved settings ranges", async () => {
    expect(backend.runtime).toBe("browser-mock");
    const current = await backend.settingsGet();
    if (!current.ok) throw new Error(current.error.message);

    const result = await backend.settingsUpdate({ maxColumns: 5 }, current.data.revision);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error.code).toBe("VALIDATION_ERROR");
    expect(result.error.details?.field).toBe("maxColumns");
  });

  it("emits the new revision when settings change", async () => {
    const revisions: number[] = [];
    const unsubscribe = await backend.on("settings:changed", (snapshot) => revisions.push(snapshot.revision));
    const current = await backend.settingsGet();
    if (!current.ok) throw new Error(current.error.message);

    const nextColumns = current.data.maxColumns === 4 ? 3 : 4;
    const result = await backend.settingsUpdate({ maxColumns: nextColumns }, current.data.revision);
    unsubscribe();

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(revisions).toEqual([result.data.revision]);
  });
});

describe("browser backend search contract", () => {
  it("reuses a canonical query key and returns deterministic Recent pages", async () => {
    const first = await backend.searchSubmit(searchRequest({
      text: "  ARCHIVE  ",
      includeTags: [" FULL_COLOR ", "MYSTERY"],
      languages: ["english", "korean", "japanese", "korean"],
    }));
    const repeated = await backend.searchSubmit(searchRequest({
      text: "archive",
      includeTags: ["mystery", "full_color"],
      languages: ["japanese", "english", "korean"],
    }));

    expect(first.ok).toBe(true);
    expect(repeated.ok).toBe(true);
    if (!first.ok || !repeated.ok) return;
    expect(repeated.data.queryId).toBe(first.data.queryId);
    expect(first.data.queryId).toBe("fixture-f68ffad46ba6b7569bf724ae0776c47d");
    expect(first.data.firstPage.items.map((item) => item.title)).toEqual(["Archive of Rain"]);
  });

  it("serves later pages from the submitted query session", async () => {
    const submitted = await backend.searchSubmit(searchRequest({ pageSize: 2 }));
    if (!submitted.ok) throw new Error(submitted.error.message);

    const second = await backend.searchPageGet(submitted.data.queryId, 2);
    expect(second.ok).toBe(true);
    if (!second.ok) return;
    expect(second.data.page).toBe(2);
    expect(second.data.items).toHaveLength(1);
    expect(second.data.items[0]?.id).not.toBe(submitted.data.firstPage.items[0]?.id);

    const outside = await backend.searchPageGet(submitted.data.queryId, 3);
    expect(outside).toMatchObject({ ok: false, error: { code: "VALIDATION_ERROR" } });
  });

  it("returns structured errors for unknown queries and galleries", async () => {
    const page = await backend.searchPageGet("missing-query", 1);
    const detail = await backend.galleryDetailGet(galleryId(999));

    expect(page).toMatchObject({ ok: false, error: { code: "QUERY_NOT_FOUND" } });
    expect(detail).toMatchObject({ ok: false, error: { code: "SOURCE_NOT_FOUND" } });
  });

  it("returns tags and related summaries from the detail fixture", async () => {
    const result = await backend.galleryDetailGet(galleryId(4051038));

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.data.tags).toContain("female:glasses");
    expect(result.data.series).toEqual(["rain archives"]);
    expect(result.data.characters).toEqual(["mira lane", "ren kujo"]);
    expect(result.data.related).toHaveLength(2);
    expect(result.data.related.every((item) => item.id !== result.data.id)).toBe(true);
  });

  it.each([
    ["series:rain_archives", [galleryId(4051038), galleryId(4050754)]],
    ["character:mira_lane", [galleryId(4051038), galleryId(4050754)]],
  ])("routes atomic %s metadata tokens in the browser fixture", async (text, expectedIds) => {
    const result = await backend.searchSubmit(searchRequest({ text, pageSize: 20 }));
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.data.firstPage.items.map((item) => item.id)).toEqual(expectedIds);
  });
});

describe("browser backend favorites and automation contract", () => {
  it("persists normalized favorites and only records submitted non-empty searches", async () => {
    const enabled = await backend.favoriteSet({ namespace: "artist", value: "  History Artist  " }, true);
    await backend.favoriteSet({ namespace: "series", value: " Rain Archives " }, true);
    await backend.favoriteSet({ namespace: "character", value: " Mira Lane " }, true);
    expect(enabled).toMatchObject({
      ok: true,
      data: { enabled: true, favorite: { namespace: "artist", value: "history artist", revision: 0 } },
    });
    const favorites = await backend.favoritesList();
    expect(favorites).toMatchObject({ ok: true, data: expect.arrayContaining([
      expect.objectContaining({ namespace: "artist", value: "history artist" }),
      expect.objectContaining({ namespace: "series", value: "rain archives" }),
      expect.objectContaining({ namespace: "character", value: "mira lane" }),
    ]) });

    await backend.searchSubmit(searchRequest());
    await backend.searchSubmit(searchRequest({
      text: "history-contract",
      includeTags: ["female:glasses"],
      excludeTags: ["male:suit"],
      languages: ["english", "korean"],
      sort: "popular_week",
      pageSize: 17,
    }));
    await backend.searchSubmit(searchRequest({
      text: " HISTORY-CONTRACT ",
      includeTags: ["FEMALE:GLASSES"],
      excludeTags: ["MALE:SUIT"],
      languages: ["korean", "english"],
      sort: "popular_week",
      pageSize: 17,
    }));
    const history = await backend.searchHistoryList(100);
    expect(history.ok).toBe(true);
    if (!history.ok) return;
    const entry = history.data.find((item) => item.text === "history-contract");
    expect(entry).toMatchObject({
      includeTags: ["female:glasses"],
      excludeTags: ["male:suit"],
      languages: ["korean", "english"],
      sort: "popular_week",
      pageSize: 17,
      useCount: 2,
    });
    expect(history.data.some((item) => !item.text && !item.includeTags.length && !item.excludeTags.length)).toBe(false);

    await backend.favoriteSet({ namespace: "artist", value: "history artist" }, false);
    await backend.favoriteSet({ namespace: "series", value: "rain archives" }, false);
    await backend.favoriteSet({ namespace: "character", value: "mira lane" }, false);
    const removed = await backend.favoritesList();
    expect(removed.ok && removed.data.some((item) => item.value === "history artist")).toBe(false);
  });

  it("preserves partial candidates on cancel and excludes them from later explicit refreshes", async () => {
    vi.useFakeTimers();
    const events: string[] = [];
    const unsubscribe = await backend.on("auto-find:changed", (run) => events.push(run.state));
    let seededDownloadEntryId: string | undefined;
    try {
      await backend.favoriteSet({ namespace: "artist", value: "serein" }, true);
      await backend.favoriteSet({ namespace: "artist", value: "mizuno" }, true);
      const seededDownload = await backend.downloadQueueAdd([galleryId(4051038)], "auto-find-downloaded-gallery");
      if (seededDownload.ok) seededDownloadEntryId = seededDownload.data[0]?.entryId;

      const started = await backend.autoFindRefresh();
      expect(started).toMatchObject({ ok: true, data: { state: "running", totalFavorites: 2 } });
      await vi.advanceTimersByTimeAsync(60);
      const partial = await backend.autoFindSnapshot();
      expect(partial).toMatchObject({
        ok: true,
        data: {
          run: { state: "running", completedFavorites: 1 },
          candidates: [expect.objectContaining({ id: galleryId(4050754), artist: "serein" })],
        },
      });

      const cancelled = await backend.autoFindCancel();
      expect(cancelled).toMatchObject({ ok: true, data: { state: "cancelled" } });
      await vi.advanceTimersByTimeAsync(500);
      const preserved = await backend.autoFindSnapshot();
      expect(preserved).toMatchObject({
        ok: true,
        data: { run: { state: "cancelled" }, candidates: [{ id: galleryId(4050754) }] },
      });

      await backend.autoFindRefresh();
      await vi.advanceTimersByTimeAsync(120);
      const completed = await backend.autoFindSnapshot();
      expect(completed).toMatchObject({ ok: true, data: { run: { state: "completed", completedFavorites: 2 } } });
      if (!completed.ok) return;
      expect(completed.data.candidates.map((candidate) => candidate.id)).not.toContain(galleryId(4051038));

      const excludedId = completed.data.candidates[0]!.id;
      const excluded = await backend.autoFindExclude([excludedId], "focused browser contract test");
      expect(excluded).toMatchObject({ ok: true, data: { excludedGalleryIds: [excludedId] } });
      await backend.autoFindRefresh();
      await vi.advanceTimersByTimeAsync(120);
      const refreshed = await backend.autoFindSnapshot();
      expect(refreshed.ok && refreshed.data.candidates.some((candidate) => candidate.id === excludedId)).toBe(false);
      expect(events).toContain("cancelled");
      expect(events).toContain("completed");
    } finally {
      unsubscribe();
      if (seededDownloadEntryId) await backend.downloadCancel([seededDownloadEntryId]);
      await backend.favoriteSet({ namespace: "artist", value: "serein" }, false);
      await backend.favoriteSet({ namespace: "artist", value: "mizuno" }, false);
      vi.useRealTimers();
    }
  });
});

describe("browser backend download contract", () => {
  it("persists idempotent cancellation and retries the same entry", async () => {
    const gallery = galleryId(7_100_000);
    const queued = await backend.downloadQueueAdd([gallery], "queue-cancel-retry-request");
    if (!queued.ok) throw new Error(queued.error.message);
    const entry = queued.data[0]!;

    const cancelled = await backend.downloadCancel([entry.entryId]);
    const cancelReplay = await backend.downloadCancel([entry.entryId]);
    expect(cancelled).toMatchObject({
      ok: true,
      data: [{ entryId: entry.entryId, state: "cancelled", revision: 1 }],
    });
    expect(cancelReplay).toEqual(cancelled);

    const retried = await backend.downloadRetry([entry.entryId]);
    expect(retried).toEqual({
      ok: true,
      data: [{ jobId: `browser-fixture-${entry.entryId}`, reused: false }],
    });
    const current = await backend.downloadEntriesList({
      query: entry.entryId,
      page: 1,
      pageSize: 20,
    });
    expect(current).toMatchObject({
      ok: true,
      data: { entries: [{ entryId: entry.entryId, state: "queued", revision: 2 }] },
    });
    await backend.downloadCancel([entry.entryId]);
  });

  it("does not let an old fixture timer advance a retried attempt", async () => {
    vi.useFakeTimers();
    const unsubscribe = await backend.on("download:changed", () => undefined);
    try {
      const gallery = galleryId(7_100_101);
      const queued = await backend.downloadQueueAdd([gallery], "queue-stale-browser-worker");
      if (!queued.ok) throw new Error(queued.error.message);
      const entry = queued.data[0]!;

      await vi.advanceTimersByTimeAsync(75);
      await backend.downloadCancel([entry.entryId]);
      const retried = await backend.downloadRetry([entry.entryId]);
      expect(retried).toMatchObject({ ok: true, data: [{ reused: false }] });

      await vi.advanceTimersByTimeAsync(150);
      const whileOldCompletionFires = await backend.downloadEntriesList({
        query: entry.entryId,
        page: 1,
        pageSize: 20,
      });
      expect(whileOldCompletionFires).toMatchObject({
        ok: true,
        data: {
          entries: [{
            entryId: entry.entryId,
            state: "resolving_metadata",
            attempt: 2,
          }],
        },
      });

      await vi.advanceTimersByTimeAsync(75);
      const currentAttemptCompletion = await backend.downloadEntriesList({
        query: entry.entryId,
        page: 1,
        pageSize: 20,
      });
      expect(currentAttemptCompletion).toMatchObject({
        ok: true,
        data: {
          entries: [{
            entryId: entry.entryId,
            state: "interrupted",
            attempt: 2,
            errorCode: "DOWNLOAD_FOUNDATION_UNAVAILABLE",
          }],
        },
      });
    } finally {
      unsubscribe();
      vi.useRealTimers();
    }
  });

  it("replays the original entries for the same request ID and normalized gallery set", async () => {
    const firstGallery = galleryId(7_100_002);
    const secondGallery = galleryId(7_100_001);
    const first = await backend.downloadQueueAdd(
      [firstGallery, secondGallery, firstGallery],
      " queue-replay-request ",
    );
    expect(await backend.downloadActiveCount()).toEqual({ ok: true, data: 2 });
    expect(first.ok).toBe(true);
    if (!first.ok) return;
    const browserState = backend as unknown as { downloadEntries: Map<string, DownloadEntry> };
    const current = first.data[0]!;
    browserState.downloadEntries.set(current.entryId, {
      ...current,
      state: "downloading",
      progress: 47,
    });
    const replay = await backend.downloadQueueAdd(
      [secondGallery, firstGallery],
      "queue-replay-request",
    );

    expect(replay.ok).toBe(true);
    if (!replay.ok) return;
    expect(first.data.map((entry) => entry.galleryId)).toEqual([secondGallery, firstGallery]);
    expect(replay.data).toEqual(first.data);
  });

  it("rejects reuse of a request ID for a different normalized gallery set", async () => {
    const firstGallery = galleryId(7_100_011);
    const secondGallery = galleryId(7_100_012);
    const queued = await backend.downloadQueueAdd([firstGallery], "queue-conflict-request");
    const conflict = await backend.downloadQueueAdd([firstGallery, secondGallery], "queue-conflict-request");

    expect(queued.ok).toBe(true);
    expect(conflict).toMatchObject({
      ok: false,
      error: { code: "IDEMPOTENCY_CONFLICT", details: { requestId: "queue-conflict-request" } },
    });
  });

  it("validates the original gallery input length before deduplication", async () => {
    const repeated = Array.from({ length: 201 }, () => galleryId(7_100_015));
    const result = await backend.downloadQueueAdd(repeated, "queue-too-many-request");

    expect(result).toMatchObject({
      ok: false,
      error: {
        code: "VALIDATION_ERROR",
        details: { field: "galleries", reason: "must contain at most 200 IDs" },
      },
    });
  });

  it("reuses an existing entry in each of the six active states for a new request ID", async () => {
    const states: DownloadEntry["state"][] = [
      "queued",
      "resolving_metadata",
      "downloading",
      "hashing",
      "verifying",
      "retry_wait",
    ];
    const browserState = backend as unknown as { downloadEntries: Map<string, DownloadEntry> };

    for (const [index, state] of states.entries()) {
      const shared = galleryId(7_100_021 + index);
      const first = await backend.downloadQueueAdd([shared], `queue-active-first-${state}`);
      if (!first.ok) throw new Error(first.error.message);
      const entry = first.data[0]!;
      browserState.downloadEntries.set(entry.entryId, { ...entry, state, progress: 23 });

      const second = await backend.downloadQueueAdd([shared], `queue-active-second-${state}`);
      expect(second).toMatchObject({
        ok: true,
        data: [{ entryId: entry.entryId, galleryId: shared, state, progress: 23 }],
      });
    }
  });

  it("filters and paginates queued entries deterministically", async () => {
    const query = "710003";
    const galleries = [
      galleryId(7_100_033),
      galleryId(7_100_031),
      galleryId(7_100_034),
      galleryId(7_100_032),
    ];
    const queued = await backend.downloadQueueAdd(galleries, "queue-list-request");
    if (!queued.ok) throw new Error(queued.error.message);

    const first = await backend.downloadEntriesList({ state: "queued", query, page: 1, pageSize: 2 });
    const second = await backend.downloadEntriesList({ state: "queued", query, page: 2, pageSize: 2 });
    const completed = await backend.downloadEntriesList({ state: "completed", query, page: 1, pageSize: 2 });

    expect(first.ok).toBe(true);
    expect(second.ok).toBe(true);
    expect(completed.ok).toBe(true);
    if (!first.ok || !second.ok || !completed.ok) return;
    expect(first.data.totalItems).toBe(4);
    expect(first.data.entries.map((entry) => entry.galleryId)).toEqual([galleryId(7_100_031), galleryId(7_100_032)]);
    expect(second.data.entries.map((entry) => entry.galleryId)).toEqual([galleryId(7_100_033), galleryId(7_100_034)]);
    expect(completed.data).toMatchObject({ totalItems: 0, entries: [] });
  });

  it("rejects list queries longer than 500 UTF-8 bytes after normalization", async () => {
    const result = await backend.downloadEntriesList({
      query: `  ${"가".repeat(167)}  `,
      page: 1,
      pageSize: 20,
    });

    expect(result).toMatchObject({
      ok: false,
      error: {
        code: "VALIDATION_ERROR",
        details: { field: "query", reason: "must be at most 500 bytes" },
      },
    });
  });

  it("ends the review fixture at interrupted without manufacturing a completed artifact", async () => {
    vi.useFakeTimers();
    const states: DownloadEntry["state"][] = [];
    const unsubscribe = await backend.on("download:changed", (entry) => states.push(entry.state));
    try {
      const gallery = galleryId(7_100_099);
      const queued = await backend.downloadQueueAdd([gallery], "queue-safe-fixture-request");
      expect(queued).toMatchObject({ ok: true, data: [{ galleryId: gallery, revision: 0, state: "queued" }] });

      await vi.advanceTimersByTimeAsync(225);

      expect(states).toEqual(["resolving_metadata", "interrupted"]);
      const interrupted = await backend.downloadEntriesList({
        state: "interrupted",
        query: String(gallery),
        page: 1,
        pageSize: 20,
      });
      expect(interrupted).toMatchObject({
        ok: true,
        data: {
          entries: [{
            galleryId: gallery,
            revision: 2,
            state: "interrupted",
            progress: 0,
            attempt: 1,
            errorCode: "DOWNLOAD_FOUNDATION_UNAVAILABLE",
          }],
        },
      });
    } finally {
      unsubscribe();
      vi.useRealTimers();
    }
  });
});

describe("browser backend duplicate review contract", () => {
  it("persists scan progress, cancellation, deterministic evidence, and revision-CAS decisions", async () => {
    vi.useFakeTimers();
    const events: string[] = [];
    const unsubscribe = await backend.on("duplicate:changed", (run) => events.push(`${run.state}:${run.revision}`));
    try {
      const initial = await backend.duplicateSnapshot();
      expect(initial).toMatchObject({
        ok: true,
        data: {
          profile: { profileVersion: 1, dHashBits: 1024, pHashBits: 64 },
          candidates: [],
        },
      });

      const started = await backend.duplicateScanStart();
      expect(started).toMatchObject({ ok: true, data: { state: "running", totalArtifacts: 2, totalPairs: 1 } });
      await vi.advanceTimersByTimeAsync(45);
      expect(await backend.duplicateSnapshot()).toMatchObject({
        ok: true,
        data: { run: { state: "running", hashedArtifacts: 1, comparedPairs: 0 } },
      });
      const cancelled = await backend.duplicateScanCancel();
      expect(cancelled).toMatchObject({ ok: true, data: { state: "cancelled" } });
      await vi.advanceTimersByTimeAsync(100);
      expect(await backend.duplicateSnapshot()).toMatchObject({
        ok: true,
        data: { run: { state: "cancelled" }, candidates: [] },
      });

      await backend.duplicateScanStart();
      await vi.advanceTimersByTimeAsync(100);
      const complete = await backend.duplicateSnapshot();
      expect(complete).toMatchObject({
        ok: true,
        data: {
          run: { state: "completed", hashedArtifacts: 2, comparedPairs: 1, candidatesFound: 1 },
          candidates: [{
            candidateId: "browser-duplicate-archive-tram",
            relation: "contains",
            confidence: 0.94,
          }],
        },
      });
      expect(events).toEqual(expect.arrayContaining(["cancelled:2", "completed:2"]));

      const reviewResult = await backend.duplicateReviewGet("browser-duplicate-archive-tram");
      if (!reviewResult.ok) throw new Error(reviewResult.error.message);
      expect(reviewResult.data).toMatchObject({
        evidence: expect.arrayContaining([
          expect.objectContaining({ kind: "exact_sha256" }),
          expect.objectContaining({ kind: "visual_hash" }),
          expect.objectContaining({ kind: "sequence_alignment" }),
        ]),
        pagePairs: [
          expect.objectContaining({
            parentSourcePage: 1,
            candidateSourcePage: 3,
            exactSha256: true,
            detailHashDistance: 0,
            edgeSimilarity: 1,
          }),
          expect.any(Object),
          expect.any(Object),
        ],
      });

      const stale = await backend.duplicateDecisionApply({
        candidateId: reviewResult.data.candidate.candidateId,
        expectedRevision: 99,
        action: "hide_parent",
      });
      expect(stale).toMatchObject({
        ok: false,
        error: {
          code: "REVISION_CONFLICT",
          details: { resource: "duplicateCandidate", expectedRevision: 99, actualRevision: 0 },
        },
      });

      const linked = await backend.duplicateDecisionApply({
        candidateId: reviewResult.data.candidate.candidateId,
        expectedRevision: 0,
        action: "series_link",
        seriesName: "Rain sequence",
      });
      if (!linked.ok) throw new Error(linked.error.message);
      expect(linked.data).toMatchObject({
        candidate: { revision: 1 },
        seriesGroups: [{ name: "Rain sequence", members: [{ galleryId: galleryId(4051038) }, { galleryId: galleryId(4050754) }] }],
        decisions: [{ action: "series_link", candidateRevision: 1 }],
      });
      const groupId = linked.data.seriesGroups[0]!.seriesGroupId;

      const unlinked = await backend.duplicateDecisionApply({
        candidateId: linked.data.candidate.candidateId,
        expectedRevision: 1,
        action: "series_unlink",
        targetGalleryId: galleryId(4051038),
        seriesGroupId: groupId,
      });
      if (!unlinked.ok) throw new Error(unlinked.error.message);
      expect(unlinked.data.seriesGroups[0]?.members.map((member) => member.galleryId)).toEqual([galleryId(4050754)]);

      const excluded = await backend.duplicateDecisionApply({
        candidateId: unlinked.data.candidate.candidateId,
        expectedRevision: 2,
        action: "exclude_pair",
      });
      if (!excluded.ok) throw new Error(excluded.error.message);
      expect(excluded.data.decisions.at(-1)).toMatchObject({ action: "exclude_pair", candidateRevision: 3 });
      expect(await backend.duplicateSnapshot()).toMatchObject({ ok: true, data: { candidates: [] } });

      const hiddenCandidate = await backend.duplicateDecisionApply({
        candidateId: excluded.data.candidate.candidateId,
        expectedRevision: 3,
        action: "hide_candidate",
      });
      if (!hiddenCandidate.ok) throw new Error(hiddenCandidate.error.message);
      const hiddenParent = await backend.duplicateDecisionApply({
        candidateId: hiddenCandidate.data.candidate.candidateId,
        expectedRevision: 4,
        action: "hide_parent",
      });
      expect(hiddenParent).toMatchObject({ ok: true, data: { candidate: { revision: 5 } } });

      await backend.favoriteSet({ namespace: "artist", value: "serein" }, true);
      await backend.autoFindRefresh();
      await vi.advanceTimersByTimeAsync(100);
      const autoFind = await backend.autoFindSnapshot();
      if (!autoFind.ok) throw new Error(autoFind.error.message);
      expect(autoFind.data.candidates.some((candidate) =>
        candidate.id === galleryId(4051038) || candidate.id === galleryId(4050754),
      )).toBe(false);
    } finally {
      await backend.favoriteSet({ namespace: "artist", value: "serein" }, false);
      unsubscribe();
      vi.useRealTimers();
    }
  });
});

describe("browser backend internal duplicate contract", () => {
  it("persists exact source-page evidence and keeps quarantine undoable with revision checks", async () => {
    vi.useFakeTimers();
    const events: string[] = [];
    const unsubscribe = await backend.on("internal-duplicate:changed", (run) => events.push(`${run.state}:${run.revision}`));
    try {
      const started = await backend.internalDuplicateScanStart();
      expect(started).toMatchObject({ ok: true, data: { state: "running", totalArtifacts: 1 } });
      await vi.advanceTimersByTimeAsync(90);
      const snapshot = await backend.internalDuplicateSnapshot();
      if (!snapshot.ok) throw new Error(snapshot.error.message);
      expect(snapshot.data).toMatchObject({
        run: { state: "completed", scannedArtifacts: 1, groupsFound: 3 },
        groups: [
          { relation: "exact", pages: [{ sourcePage: 2 }, { sourcePage: 8 }] },
          { relation: "translation_visual", pages: [{ sourcePage: 14 }, { sourcePage: 20 }] },
          { relation: "translation_visual", pages: [{ sourcePage: 15 }, { sourcePage: 21 }] },
        ],
      });
      expect(events).toEqual(expect.arrayContaining(["running:0", "completed:2"]));

      const group = snapshot.data.groups[0]!;
      const stale = await backend.internalRemovalPlan({
        entryId: group.entryId,
        selections: [{
          groupId: group.groupId,
          expectedRevision: 99,
          keepSourcePage: 2,
          removeSourcePages: [8],
        }],
      });
      expect(stale).toMatchObject({ ok: false, error: { code: "REVISION_CONFLICT" } });

      const prepared = await backend.internalRemovalPlan({
        entryId: group.entryId,
        selections: [{
          groupId: group.groupId,
          expectedRevision: group.revision,
          keepSourcePage: 2,
          removeSourcePages: [8],
        }],
      });
      if (!prepared.ok) throw new Error(prepared.error.message);
      expect(prepared.data).toMatchObject({ filesToQuarantine: 1, bytesToQuarantine: 512_000 });
      const applied = await backend.internalRemovalApply({ plan: prepared.data, reason: "test review" });
      if (!applied.ok) throw new Error(applied.error.message);
      expect(applied.data.records).toEqual([
        expect.objectContaining({ sourcePage: 8, state: "quarantined" }),
      ]);
      expect(applied.data.review.groups.some((item) => item.groupId === group.groupId)).toBe(false);

      const restored = await backend.internalRemovalUndo({
        recordIds: applied.data.records.map((record) => record.recordId),
      });
      if (!restored.ok) throw new Error(restored.error.message);
      expect(restored.data.review.groups).toEqual(expect.arrayContaining([
        expect.objectContaining({ groupId: group.groupId, resolved: false }),
      ]));
      expect(restored.data.records).toEqual([
        expect.objectContaining({ sourcePage: 8, state: "restored" }),
      ]);
    } finally {
      unsubscribe();
      vi.useRealTimers();
    }
  });
});
