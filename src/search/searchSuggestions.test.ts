import { describe, expect, it } from "vitest";
import type { Gallery } from "../core/types";
import { galleryId } from "../core/types";
import { buildSearchSuggestionCatalog, filterSearchSuggestions, historyDisplayToken } from "./searchSuggestions";

const gallery: Gallery = { id: galleryId(1), title: "Rain Archive", subtitle: "", artist: "Mizuno", group: "Paper Studio", pages: 1, score: 0, publishedAt: "", coverIndex: 0, language: "korean", tags: ["full color", "female:glasses", "male:business suit"], series: ["Rain Archives"], characters: ["Mira Lane"] };

describe("search suggestion catalog", () => {
  it("merges history, favorites, metadata and deduplicates observed values", () => {
    const catalog = buildSearchSuggestionCatalog({ history: [{ historyId: 1, text: "", includeTags: ["story arc"], excludeTags: [], languages: [], sort: "recent", pageSize: 50, useCount: 3, lastUsedAt: "2026-08-21" }], favorites: [{ namespace: "tag", value: "full color", revision: 1, createdAt: "", updatedAt: "" }], galleries: [gallery, { ...gallery, id: galleryId(2) }] });
    expect(historyDisplayToken({ historyId: 2, text: "", includeTags: [], excludeTags: ["webtoon"], languages: [], sort: "recent", pageSize: 50, useCount: 1, lastUsedAt: "" })).toBe("-tag:webtoon");
    expect(catalog.find((item) => item.token === "tag:full_color")).toMatchObject({ favorite: true, observedCount: 2 });
  });

  it("ranks exact before prefix/substrings, restricts explicit prefixes, and makes a synthetic exact suggestion", () => {
    const catalog = buildSearchSuggestionCatalog({ history: [], favorites: [], galleries: [gallery] });
    expect(filterSearchSuggestions(catalog, "tag:full", 8).every((item) => item.type === "TAG")).toBe(true);
    expect(filterSearchSuggestions(catalog, "artist:mi", 9)).toContainEqual(expect.objectContaining({ type: "ARTIST", token: "artist:mizuno" }));
    expect(filterSearchSuggestions(catalog, "tag:cyberpunk", 13)[0]).toMatchObject({ token: "tag:cyberpunk", extra: "입력한 태그로 전역 검색" });
    expect(filterSearchSuggestions(catalog, "", 0)).toHaveLength(0);
  });
});
