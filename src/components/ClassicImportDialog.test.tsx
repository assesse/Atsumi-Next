import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { backend } from "../api/backend";
import type { ClassicImportReport } from "../api/contracts";
import { galleryId } from "../core/types";
import { ClassicImportDialog } from "./ClassicImportDialog";

const report: ClassicImportReport = {
  importId: "classic-import-test",
  revision: 0,
  state: "dry_run",
  dataRootLabel: "AtsumiData",
  downloadRootLabel: "Downloads",
  sourceFingerprint: "fixture",
  counts: {
    favorites: 1,
    searchHistory: 1,
    exclusions: 0,
    hiddenGalleries: 0,
    pairExclusions: 0,
    seriesGroups: 0,
    galleriesDiscovered: 1,
    galleriesEligible: 1,
    pageFiles: 2,
    legacyHashRows: 0,
    plannedCopyBytes: 2048,
    conflicts: 1,
  },
  conflicts: [{
    conflictId: "folder-without-state:123",
    code: "folder_without_state",
    severity: "warning",
    galleryId: galleryId(123),
    message: "Classic UI에는 없지만 유효한 manifest가 있습니다.",
    requiresAcknowledgement: true,
  }],
  galleries: [{
    galleryId: galleryId(123),
    title: "Classic fixture",
    sourceFolder: "fixture",
    expectedPages: 2,
    pages: [],
    plannedBytes: 2048,
    eligible: true,
    conflictIds: ["folder-without-state:123"],
  }],
  canApply: true,
  createdAt: "2026-08-16T00:00:00.000Z",
};

afterEach(() => vi.restoreAllMocks());

describe("ClassicImportDialog", () => {
  it("shows the read-only guarantee and requires both conflict review and explicit approval", async () => {
    vi.spyOn(backend, "classicImportPickFolder")
      .mockResolvedValueOnce({ ok: true, data: "C:\\Classic\\AtsumiData" })
      .mockResolvedValueOnce({ ok: true, data: "C:\\Classic\\Downloads" });
    vi.spyOn(backend, "classicImportDryRun").mockResolvedValue({ ok: true, data: report });
    const apply = vi.spyOn(backend, "classicImportApply").mockResolvedValue({
      ok: true,
      data: {
        report: { ...report, revision: 1, state: "applied" },
        importedGalleryIds: [galleryId(123)],
        copiedFiles: 2,
        copiedBytes: 2048,
      },
    });
    const onChanged = vi.fn();
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);
    await act(async () => root.render(
      <ClassicImportDialog open={false} onClose={vi.fn()} onChanged={onChanged} />,
    ));

    expect(container).toHaveTextContent("Classic 원본은 읽기 전용입니다");
    const choose = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .filter((button) => button.textContent === "선택");
    await act(async () => choose[0]?.click());
    await act(async () => choose[1]?.click());
    const inspect = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("읽기 전용 검사"));
    await act(async () => inspect?.click());
    expect(backend.classicImportDryRun).toHaveBeenCalledWith({
      dataRoot: "C:\\Classic\\AtsumiData",
      downloadRoot: "C:\\Classic\\Downloads",
    });

    const applyButton = [...container.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent?.includes("승인하고 안전한 항목 가져오기"));
    expect(applyButton).toBeDisabled();
    const checkboxes = [...container.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    await act(async () => checkboxes[0]?.click());
    expect(applyButton).toBeDisabled();
    await act(async () => checkboxes[1]?.click());
    expect(applyButton).toBeEnabled();
    await act(async () => applyButton?.click());
    expect(apply).toHaveBeenCalledWith({
      importId: report.importId,
      expectedRevision: 0,
      acceptedConflictIds: ["folder-without-state:123"],
    });
    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(container).toHaveTextContent("Classic 원본은 그대로 유지됩니다");

    await act(async () => root.unmount());
    container.remove();
  });
});
