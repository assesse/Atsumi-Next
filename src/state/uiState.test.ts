import { describe, expect, it } from "vitest";
import { galleryId } from "../core/types";
import { initialUiState, uiReducer } from "./uiState";

const ids = [galleryId(1), galleryId(2), galleryId(3), galleryId(4)];

describe("uiReducer selection", () => {
  it("clears the sole selection and its anchor when the same card is plain-clicked again", () => {
    const selected = uiReducer(initialUiState, {
      type: "selection.click",
      id: ids[1]!,
      visibleIds: ids,
      ctrl: false,
      shift: false,
    });
    expect([...selected.selection.ids]).toEqual([ids[1]]);

    const reselected = uiReducer(selected, {
      type: "selection.click",
      id: ids[1]!,
      visibleIds: ids,
      ctrl: false,
      shift: false,
    });
    expect([...reselected.selection.ids]).toEqual([]);
    expect(reselected.selection.anchorId).toBeNull();
  });

  it("replaces a multiple selection with the plain-clicked card", () => {
    const first = uiReducer(initialUiState, {
      type: "selection.click",
      id: ids[0]!,
      visibleIds: ids,
      ctrl: true,
      shift: false,
    });
    const multiple = uiReducer(first, {
      type: "selection.click",
      id: ids[2]!,
      visibleIds: ids,
      ctrl: true,
      shift: false,
    });
    const replaced = uiReducer(multiple, {
      type: "selection.click",
      id: ids[1]!,
      visibleIds: ids,
      ctrl: false,
      shift: false,
    });

    expect([...replaced.selection.ids]).toEqual([ids[1]]);
    expect(replaced.selection.anchorId).toBe(ids[1]);
  });

  it("toggles with control and adds an anchored range with shift", () => {
    const anchored = uiReducer(initialUiState, {
      type: "selection.click",
      id: ids[0]!,
      visibleIds: ids,
      ctrl: true,
      shift: false,
    });
    const ranged = uiReducer(anchored, {
      type: "selection.click",
      id: ids[2]!,
      visibleIds: ids,
      ctrl: false,
      shift: true,
    });
    expect([...ranged.selection.ids]).toEqual(ids.slice(0, 3));
  });

  it("drops hidden selections and a stale anchor when the visible projection changes", () => {
    const selected = uiReducer(initialUiState, {
      type: "selection.click",
      id: ids[0]!,
      visibleIds: ids,
      ctrl: false,
      shift: false,
    });
    const anchoredElsewhere = uiReducer(selected, {
      type: "selection.click",
      id: ids[1]!,
      visibleIds: ids,
      ctrl: true,
      shift: false,
    });
    const deselectedAnchor = uiReducer(anchoredElsewhere, {
      type: "selection.click",
      id: ids[1]!,
      visibleIds: ids,
      ctrl: true,
      shift: false,
    });

    const retained = uiReducer(deselectedAnchor, { type: "selection.retain", ids: [ids[0]!, ids[2]!] });
    expect([...retained.selection.ids]).toEqual([ids[0]]);
    expect(retained.selection.anchorId).toBeNull();
  });
});

describe("uiReducer detail tabs", () => {
  it("inserts a child immediately after its parent and deduplicates tabs", () => {
    const first = uiReducer(initialUiState, { type: "detail.open", id: ids[0]! });
    const second = uiReducer(first, { type: "detail.open", id: ids[2]! });
    const child = uiReducer(second, { type: "detail.open", id: ids[1]!, parentId: ids[0]! });
    const duplicate = uiReducer(child, { type: "detail.open", id: ids[0]! });

    expect(duplicate.detail.tabs).toEqual([ids[0], ids[1], ids[2]]);
    expect(duplicate.detail.activeId).toBe(ids[0]);
  });
});
