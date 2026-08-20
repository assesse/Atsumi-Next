import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { SettingsSnapshot } from "../api/contracts";
import { SettingsDialog } from "./SettingsDialog";

const settings: SettingsSnapshot = {
  revision: 1,
  downloadRoot: "C:\\Atsumi",
  folderNameTemplate: "[{artist}] {title} [{group}] {id}",
  autoFindHistoryMode: "include_all_history",
  maxColumns: 3,
  previewWidth: 220,
  cacheLimitGb: 5,
  concurrentImageRequests: 5,
  requestStartIntervalMs: 25,
};

describe("SettingsDialog operational boundaries", () => {
  it("exposes preset sizing and only safe, implemented reset operations", async () => {
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
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onSave = vi.fn(async () => false);
    const onPreviewFolderName = vi.fn(async () => ({
      ok: true,
      data: "[작가] 작품 제목 [그룹] 4113714",
    } as const));
    const onClearCache = vi.fn(async () => ({
      ok: true,
      data: { successEntriesRemoved: 2, successBytesRemoved: 10, negativeEntriesRemoved: 1 },
    } as const));
    const onResetExplorationData = vi.fn(async () => ({
      ok: true,
      data: {
        favoritesRemoved: 1,
        searchHistoryRemoved: 2,
        autoFindRunsRemoved: 1,
        autoFindCandidatesRemoved: 3,
        autoFindExclusionsRemoved: 1,
      },
    } as const));

    try {
      await act(async () => root.render(
        <SettingsDialog
          open
          settings={settings}
          loading={false}
          error={null}
          onClose={vi.fn()}
          onSave={onSave}
          onPreviewLayout={vi.fn()}
          onPreviewFolderName={onPreviewFolderName}
          onClearCache={onClearCache}
          onResetExplorationData={onResetExplorationData}
        />,
      ));
      await act(async () => {
        await new Promise((resolve) => window.setTimeout(resolve, 140));
      });

      expect(container.querySelector(".settings-nav")).toBeNull();
      expect(container.textContent).not.toContain("다음 단계");

      const destructive = [...container.querySelectorAll<HTMLButtonElement>(".danger-zone button")];
      expect(destructive).toHaveLength(3);
      expect(destructive.every((button) => !button.disabled)).toBe(true);
      expect(container.textContent).toContain("다운로드 DB와 파일은 보존");

      await act(async () => destructive[0]?.click());
      expect(onClearCache).toHaveBeenCalledTimes(1);
      await act(async () => destructive[2]?.click());
      expect(confirm).toHaveBeenCalledWith(expect.stringContaining("다운로드 DB와 파일은 삭제되지 않습니다"));
      expect(onResetExplorationData).toHaveBeenCalledTimes(1);

      const template = container.querySelector<HTMLInputElement>('[aria-label="갤러리 폴더 이름 템플릿"]');
      expect(template?.value).toBe("[{artist}] {title} [{group}] {id}");
      const historyMode = container.querySelector<HTMLSelectElement>('[aria-label="Auto Find 기록 기준"]');
      expect(historyMode?.value).toBe("include_all_history");
      const previewRange = container.querySelector<HTMLInputElement>('[aria-label="앨범 미리보기 크기"]');
      expect(previewRange?.min).toBe("0");
      expect(previewRange?.max).toBe("6");
      expect(previewRange?.value).toBe("2");
      expect(container.textContent).toContain("사용가능 인자 : {artist}, {title}, {group}, {id}");
      expect(container.textContent).toContain("미리보기 : [작가] 작품 제목 [그룹] 4113714");
      expect(container.textContent).not.toContain("{id}는 필수입니다");
      expect(container.textContent).not.toContain("사용할 수 있으며");
      expect(onPreviewFolderName).toHaveBeenCalledTimes(1);
      await act(async () => {
        if (!template) throw new Error("template input missing");
        template.value = "{title} {id}";
        template.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await act(async () => {
        [...container.querySelectorAll<HTMLButtonElement>("button")]
          .find((button) => button.textContent === "기본값 복원")
          ?.click();
      });
      await act(async () => {
        [...container.querySelectorAll<HTMLButtonElement>("button")]
          .find((button) => button.textContent === "저장")
          ?.click();
      });
      expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
        folderNameTemplate: "[{artist}] {title} [{group}] {id}",
        autoFindHistoryMode: "include_all_history",
      }));
    } finally {
      await act(async () => root.unmount());
      container.remove();
      if (previousShowModal) {
        Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      } else {
        Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      }
      confirm.mockRestore();
    }
  });
});
