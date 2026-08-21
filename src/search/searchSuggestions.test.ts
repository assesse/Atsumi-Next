import { describe, expect, it } from "vitest";
import { buildSearchSuggestionCatalog, catalogSuggestion, historyDisplayToken } from "./searchSuggestions";

describe("search suggestion catalog", () => {
  it("keeps structured history only when the field is empty", () => {
    const catalog = buildSearchSuggestionCatalog([{ historyId: 1, text: "", includeTags: ["story arc"], excludeTags: [], languages: [], sort: "recent", pageSize: 50, useCount: 3, lastUsedAt: "2026-08-21" }]);
    expect(historyDisplayToken({ historyId: 2, text: "", includeTags: [], excludeTags: ["webtoon"], languages: [], sort: "recent", pageSize: 50, useCount: 1, lastUsedAt: "" })).toBe("-tag:webtoon");
    expect(catalog).toHaveLength(1);
    expect(catalog[0]?.token).toBe("tag:story_arc");
  });
  it("adapts only SQLite tag suggestions and never creates synthetic candidates", () => {
    expect(catalogSuggestion({ namespace: "female", name: "big balls", token: "female:big_balls", galleryCount: 4822, favorite: true })).toMatchObject({ type: "FEMALE", label: "big balls", favorite: true, galleryCount: 4822 });
  });
});
