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
});
