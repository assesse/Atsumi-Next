import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { SettingsSnapshot } from "../api/contracts";
import { SettingsDialog } from "./SettingsDialog";

const settings: SettingsSnapshot = {
  revision: 1,
  downloadRoot: "C:\\Atsumi",
  maxColumns: 3,
  previewWidth: 220,
  cacheLimitGb: 5,
  concurrentImageRequests: 5,
  requestStartIntervalMs: 25,
};

describe("SettingsDialog operational boundaries", () => {
  it("only exposes implemented sections and disables destructive operations with reasons", async () => {
    const previousShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value: vi.fn(function (this: HTMLDialogElement) {
        this.setAttribute("open", "");
      }),
    });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    try {
      await act(async () => root.render(
        <SettingsDialog
          open
          settings={settings}
          loading={false}
          error={null}
          onClose={vi.fn()}
          onSave={vi.fn(async () => true)}
          onClassicImport={vi.fn()}
          onPreviewLayout={vi.fn()}
        />,
      ));

      const sectionLabels = [...container.querySelectorAll<HTMLButtonElement>(".settings-nav button")]
        .map((button) => button.textContent);
      expect(sectionLabels).toEqual(["일반", "저장 공간"]);
      expect(container.textContent).not.toContain("다음 단계");

      const destructive = [...container.querySelectorAll<HTMLButtonElement>(".danger-zone button")];
      expect(destructive).toHaveLength(3);
      expect(destructive.every((button) => button.disabled && Boolean(button.title))).toBe(true);
    } finally {
      await act(async () => root.unmount());
      container.remove();
      if (previousShowModal) {
        Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      } else {
        Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      }
    }
  });
});
