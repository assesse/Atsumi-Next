import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ExitConfirmDialog } from "./ExitConfirmDialog";

afterEach(() => vi.unstubAllGlobals());

describe("ExitConfirmDialog", () => {
  it("shows a clear active-work state, cancellation X, and the tray or quit choices", async () => {
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => window.setTimeout(() => callback(0), 0));
    const previousShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value() { this.setAttribute("open", ""); } });
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    try {
      const onClose = vi.fn();
      await act(async () => root.render(<ExitConfirmDialog open activeDownloads={2} statusError={false} actionPending={false} onClose={onClose} onMinimizeToTray={vi.fn()} onQuit={vi.fn()} />));
      expect(container).toHaveTextContent("작업 진행 중 · 다운로드 2개");
      expect(container.querySelector(".eyebrow")).toBeNull();
      expect(container).toHaveTextContent("트레이로 보내기");
      expect(container).toHaveTextContent("종료");
      expect(container.querySelectorAll(".exit-choice")).toHaveLength(2);
      const cancel = container.querySelector<HTMLButtonElement>("[aria-label='종료 취소']");
      expect(cancel).not.toBeNull();
      await act(async () => cancel?.click());
      expect(onClose).toHaveBeenCalledOnce();
    } finally {
      await act(async () => root.unmount());
      container.remove();
      if (previousShowModal) Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      else delete (HTMLDialogElement.prototype as unknown as { showModal?: unknown }).showModal;
    }
  });
});
