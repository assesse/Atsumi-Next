import { describe, expect, it } from "vitest";
import { mockGalleries } from "../data/mockGalleries";
import { initialUiState, uiReducer } from "./uiState";
import { visibleGalleries } from "./selectors";

describe("gallery selectors", () => {
  it("understands namespace-prefixed artist searches", () => {
    const searched = uiReducer(initialUiState, {
      type: "search.commit",
      view: "explore",
      value: "artist:serein",
    });
    expect(visibleGalleries(searched, mockGalleries).map((gallery) => gallery.artist)).toEqual([
      "serein",
      "serein",
    ]);
  });

  it("keeps each view's search independently", () => {
    const downloads = uiReducer(initialUiState, { type: "navigate", view: "downloads" });
    const searched = uiReducer(downloads, {
      type: "search.commit",
      view: "downloads",
      value: "paperlane",
    });
    expect(searched.search.explore.committed).toBe("");
    expect(searched.search.downloads.committed).toBe("paperlane");
    expect(visibleGalleries(searched, mockGalleries)).toHaveLength(1);
  });

  it("understands optional group searches", () => {
    const searched = uiReducer(initialUiState, {
      type: "search.commit",
      view: "explore",
      value: "group:paper studio",
    });
    expect(visibleGalleries(searched, mockGalleries).map((gallery) => gallery.title)).toEqual([
      "The Green Window",
    ]);
  });
});
