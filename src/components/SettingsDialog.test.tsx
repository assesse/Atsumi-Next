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
  privacyMode: false,
  cacheLimitGb: 5,
  concurrentImageRequests: 5,
  requestStartIntervalMs: 25,
  collapsedGroupKeys: [],
};

describe("SettingsDialog operational boundaries", () => {
  it("exposes preset sizing and only safe, implemented reset operations", async () => {
    const previousShowModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "showModal");
    const previousClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
    const writeText = vi.fn<(value: string) => Promise<void>>(async (_value) => undefined);
    Object.defineProperty(HTMLDialogElement.prototype, "showModal", {
      configurable: true,
      value: vi.fn(function (this: HTMLDialogElement) {
        this.setAttribute("open", "");
      }),
    });
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
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
      expect(container.querySelectorAll('[data-settings-scroll-root="true"]')).toHaveLength(1);
      expect(container.querySelector(".settings-dialog > .settings-form")).not.toBeNull();
      const about = container.querySelector<HTMLElement>(".settings-about-panel");
      expect(about).toHaveTextContent("Atsumi Next");
      expect(about).toHaveTextContent("assesse · Atsumi Next contributors");
      expect(about).toHaveTextContent("앨범 제목, 태그, 파일 경로, 데이터베이스 내용이 포함되지 않습니다");
      const aboutButtons = [...about?.querySelectorAll<HTMLButtonElement>("button") ?? []];
      await act(async () => aboutButtons.find((button) => button.textContent === "피드백 주소 복사")?.click());
      expect(writeText).toHaveBeenLastCalledWith("https://github.com/assesse/Atsumi-Next/issues/new/choose");
      await act(async () => aboutButtons.find((button) => button.textContent === "진단 정보 복사")?.click());
      expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("privateDataIncluded=false"));
      expect(writeText.mock.calls.at(-1)?.[0]).not.toMatch(/C:\\|앨범|tag=/i);

      expect(container.querySelector(".settings-reset-row")).not.toBeNull();
      expect(container.querySelector(".maintenance-panel .settings-reset-row")).toBeNull();
      const maintenanceItems = [...container.querySelectorAll<HTMLElement>(".maintenance-item")];
      expect(maintenanceItems).toHaveLength(3);
      const [quickRepair, rebuild, factoryReset] = maintenanceItems;
      expect(quickRepair).toHaveTextContent("빠른 복구");
      expect(quickRepair?.querySelectorAll("button")).toHaveLength(1);
      expect(rebuild).toHaveTextContent("라이브러리 검사 및 재구축");
      expect(rebuild?.querySelectorAll('input[type="checkbox"]')).toHaveLength(4);
      expect([...rebuild?.querySelectorAll<HTMLInputElement>('input[type="checkbox"]') ?? []].map((input) => input.checked)).toEqual([true, false, false, false]);
      expect(factoryReset).toHaveTextContent("앱 데이터 완전 초기화");
      expect(factoryReset).toHaveTextContent("외부 다운로드 원본 파일과 quarantine/recovery 파일은 유지됩니다.");
      expect(factoryReset).toHaveClass("maintenance-item--factory-reset");
      const maintenance = [...container.querySelectorAll<HTMLButtonElement>(".maintenance-item > button")];
      expect(maintenance.map((button) => button.textContent)).toEqual(["빠른 복구", "라이브러리 검사 및 재구축", "앱 데이터 완전 초기화"]);
      await act(async () => maintenance[0]?.click());
      expect(onMaintenance).toHaveBeenCalledWith({ kind: "quickRepair" });
      const duplicateOption = rebuild?.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1];
      await act(async () => duplicateOption?.click());
      await act(async () => maintenance[1]?.click());
      expect(onMaintenance).toHaveBeenLastCalledWith({
        kind: "rebuildLibrary",
        rebuildThumbnailData: true,
        rebuildDuplicateAnalysis: true,
        rebuildInternalAnalysis: false,
        rebuildAutoFindResults: false,
      });
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
      const privacyMode = container.querySelector<HTMLInputElement>('[aria-label="개인정보 보호 모드"]');
      expect(privacyMode).not.toBeChecked();
      await act(async () => privacyMode?.click());
      expect(privacyMode).toBeChecked();
      await act(async () => {
        [...container.querySelectorAll<HTMLButtonElement>("button")]
          .find((button) => button.textContent === "설정 기본값")
          ?.click();
      });
      expect(privacyMode).not.toBeChecked();
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
        privacyMode?.click();
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
        privacyMode: true,
      }));
    } finally {
      await act(async () => root.unmount());
      container.remove();
      if (previousShowModal) {
        Object.defineProperty(HTMLDialogElement.prototype, "showModal", previousShowModal);
      } else {
        Reflect.deleteProperty(HTMLDialogElement.prototype, "showModal");
      }
      if (previousClipboard) {
        Object.defineProperty(navigator, "clipboard", previousClipboard);
      } else {
        Reflect.deleteProperty(navigator, "clipboard");
      }
      confirm.mockRestore();
    }
  });
});
