import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { ApiResult, MaintenanceAction, MaintenanceResult, SettingsSnapshot } from "../api/contracts";
import { SettingsDialog } from "./SettingsDialog";

const settings: SettingsSnapshot = {
  revision: 1,
  downloadRoot: "C:\\Atsumi",
  folderNameTemplate: "[{artist}] {title} [{group}] {id}",
  autoFindHistoryMode: "include_all_history",
  maxColumns: 3,
  previewWidth: 220,
  relatedPreviewWidth: 240,
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
    const onMaintenance = vi.fn(async (action: MaintenanceAction): Promise<ApiResult<MaintenanceResult>> => ({
      ok: true,
      data: { action, completedSteps: ["done"], warnings: [], restartRequired: false },
    }));

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
          onMaintenance={onMaintenance}
        />,
      ));
      await act(async () => {
        await new Promise((resolve) => window.setTimeout(resolve, 140));
      });

      expect(container.querySelector(".settings-nav")).toBeNull();
      expect(container.textContent).not.toContain("다음 단계");

      const maintenance = [...container.querySelectorAll<HTMLButtonElement>(".maintenance-actions button")];
      expect(maintenance).toHaveLength(3);
      expect(maintenance.map((button) => button.textContent)).toEqual(["빠른 복구", "라이브러리 검사 및 재구축", "앱 데이터 완전 초기화"]);
      await act(async () => maintenance[0]?.click());
      expect(onMaintenance).toHaveBeenCalledWith({ kind: "quickRepair" });
      await act(async () => maintenance[2]?.click());
      expect(confirm).toHaveBeenCalledWith(expect.stringContaining("외부 다운로드 원본 파일은 유지"));

      const template = container.querySelector<HTMLInputElement>('[aria-label="갤러리 폴더 이름 템플릿"]');
      expect(template?.value).toBe("[{artist}] {title} [{group}] {id}");
      const historyMode = container.querySelector<HTMLSelectElement>('[aria-label="Auto Find 기록 기준"]');
      expect(historyMode?.value).toBe("include_all_history");
      const previewRange = container.querySelector<HTMLInputElement>('[aria-label="앨범 미리보기 크기"]');
      expect(previewRange?.min).toBe("0");
      expect(previewRange?.max).toBe("6");
      expect(previewRange?.value).toBe("2");
      const relatedPreviewRange = container.querySelector<HTMLInputElement>('[aria-label="Related galleries 미리보기 크기"]');
      expect(relatedPreviewRange?.min).toBe("180");
      expect(relatedPreviewRange?.max).toBe("320");
      expect(relatedPreviewRange?.value).toBe("240");
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
        relatedPreviewWidth: 240,
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
