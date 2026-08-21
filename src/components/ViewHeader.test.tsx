import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { ViewHeader } from "./ViewHeader";

describe("ViewHeader language filter", () => {
  it("uses an active state without a numeric badge and preserves the activity count", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    try {
      await act(async () => root.render(
        <ViewHeader
          view="explore"
          search={{ draft: "", committed: "", languages: ["korean", "english"], suggestionsOpen: false, activeSuggestion: null }}
          suggestions={[]}
          activityCount={3}
          activityOpen={false}
          onDraft={vi.fn()}
          onSuggestions={vi.fn()}
          onCommit={vi.fn()}
          onSelectSuggestion={vi.fn()}
          onCompleteSuggestion={vi.fn()}
          onLanguages={vi.fn()}
          onRefresh={vi.fn()}
          onActivity={vi.fn()}
          onSettings={vi.fn()}
        />,
      ));
      const languageButton = container.querySelector('button[aria-label="언어 필터"]');
      expect(languageButton?.querySelector(".icon-dot")).toBeNull();
      expect(container.querySelector(".activity-count")).toHaveTextContent("3");
    } finally {
      await act(async () => root.unmount());
      container.remove();
    }
  });

  it("completes only the active token on Tab and submits it on Enter", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    const onSelectSuggestion = vi.fn();
    const onCompleteSuggestion = vi.fn();
    const suggestion = { type: "TAG" as const, token: "tag:full_color", label: "full color", extra: "태그" };
    try {
      await act(async () => root.render(
        <ViewHeader view="explore" search={{ draft: "artist:mizuno tag:full", committed: "", languages: [], suggestionsOpen: true, activeSuggestion: 0 }} suggestions={[suggestion]} activityCount={0} activityOpen={false} onDraft={vi.fn()} onSuggestions={vi.fn()} onCommit={vi.fn()} onSelectSuggestion={onSelectSuggestion} onCompleteSuggestion={onCompleteSuggestion} onLanguages={vi.fn()} onRefresh={vi.fn()} onActivity={vi.fn()} onSettings={vi.fn()} />,
      ));
      const input = container.querySelector<HTMLInputElement>('input[aria-label="검색"]');
      if (!input) throw new Error("search input missing");
      input.setSelectionRange(23, 23);
      await act(async () => input.dispatchEvent(new KeyboardEvent("keyup", { key: "End", bubbles: true })));
      await act(async () => container.querySelector("form")?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
      expect(onSelectSuggestion).toHaveBeenCalledWith(suggestion, "artist:mizuno tag:full_color");
      onSelectSuggestion.mockClear();
      await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
      expect(onCompleteSuggestion).toHaveBeenCalledWith("artist:mizuno tag:full_color");
      expect(onSelectSuggestion).not.toHaveBeenCalled();
    } finally {
      await act(async () => root.unmount());
      container.remove();
    }
  });
});
