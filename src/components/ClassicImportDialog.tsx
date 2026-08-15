import { useEffect, useMemo, useRef, useState } from "react";
import { backend } from "../api/backend";
import type { ClassicImportReport } from "../api/contracts";
import { FluentIcon } from "./FluentIcon";

type ClassicImportDialogProps = {
  open: boolean;
  onClose: () => void;
  onChanged: () => void;
};

const stateLabels: Record<ClassicImportReport["state"], string> = {
  dry_run: "검토 대기",
  applying: "적용 중",
  applied: "적용 완료",
  rolling_back: "되돌리는 중",
  rolled_back: "되돌림 완료",
  failed: "작업 중단",
};

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${unit}`;
};

export function ClassicImportDialog({ open, onClose, onChanged }: ClassicImportDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const closingInternally = useRef(false);
  const [dataRoot, setDataRoot] = useState("");
  const [downloadRoot, setDownloadRoot] = useState("");
  const [report, setReport] = useState<ClassicImportReport | null>(null);
  const [accepted, setAccepted] = useState<ReadonlySet<string>>(() => new Set());
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const node = dialog.current;
    if (!node) return;
    if (open && !node.open) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      node.showModal();
      window.requestAnimationFrame(() => closeButton.current?.focus());
    } else if (!open && node.open) {
      closingInternally.current = true;
      node.close();
      const target = opener.current;
      opener.current = null;
      window.requestAnimationFrame(() => {
        if (target?.isConnected && !target.closest("dialog:not([open])")) target.focus();
        else document.querySelector<HTMLElement>('[aria-label="설정"]')?.focus();
      });
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setApproved(false);
  }, [open]);

  const requiredAcknowledgements = useMemo(
    () => report?.conflicts.filter((conflict) => conflict.requiresAcknowledgement) ?? [],
    [report],
  );
  const allAcknowledged = requiredAcknowledgements.every((conflict) => accepted.has(conflict.conflictId));

  const pickFolder = async (target: "data" | "downloads") => {
    setBusy(true);
    setError(null);
    try {
      const result = await backend.classicImportPickFolder();
      if (!result.ok) {
        setError(result.error.message);
      } else if (result.data) {
        if (target === "data") setDataRoot(result.data);
        else setDownloadRoot(result.data);
        setReport(null);
        setAccepted(new Set());
        setApproved(false);
      }
    } catch {
      setError("Windows 폴더 선택기를 열지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  const runDryRun = async () => {
    if (!dataRoot.trim()) {
      setError("Classic 데이터 폴더를 먼저 선택하세요.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await backend.classicImportDryRun({
        dataRoot: dataRoot.trim(),
        ...(downloadRoot.trim() ? { downloadRoot: downloadRoot.trim() } : {}),
      });
      if (!result.ok) setError(result.error.message);
      else {
        setReport(result.data);
        setAccepted(new Set());
        setApproved(false);
      }
    } catch {
      setError("Classic 폴더를 검사하지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  const reload = async () => {
    if (!report) return;
    setBusy(true);
    setError(null);
    try {
      const result = await backend.classicImportGet(report.importId);
      if (!result.ok) setError(result.error.message);
      else setReport(result.data);
    } catch {
      setError("저장된 가져오기 보고서를 다시 불러오지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!report) return;
    setBusy(true);
    setError(null);
    try {
      const result = await backend.classicImportApply({
        importId: report.importId,
        expectedRevision: report.revision,
        acceptedConflictIds: [...accepted],
      });
      if (!result.ok) {
        setError(result.error.message);
        if (result.error.code === "REVISION_CONFLICT") await reload();
      } else {
        setReport(result.data.report);
        onChanged();
      }
    } catch {
      setError("Classic 가져오기를 안전하게 완료하지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  const rollback = async () => {
    if (!report) return;
    setBusy(true);
    setError(null);
    try {
      const result = await backend.classicImportRollback({
        importId: report.importId,
        expectedRevision: report.revision,
      });
      if (!result.ok) {
        setError(result.error.message);
        if (result.error.code === "REVISION_CONFLICT") await reload();
      } else {
        setReport(result.data);
        onChanged();
      }
    } catch {
      setError("가져온 Next 데이터를 되돌리지 못했습니다.");
    } finally {
      setBusy(false);
    }
  };

  const close = () => {
    if (!busy) onClose();
  };

  return (
    <dialog
      ref={dialog}
      className="classic-import-dialog"
      aria-labelledby="classic-import-title"
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onClose={() => {
        if (closingInternally.current) {
          closingInternally.current = false;
          return;
        }
        onClose();
      }}
    >
      <div className="classic-import-shell">
        <header className="dialog-header">
          <div>
            <span className="eyebrow">CLASSIC IMPORT</span>
            <h2 id="classic-import-title">Classic 데이터 가져오기</h2>
          </div>
          <button ref={closeButton} type="button" className="icon-button small" aria-label="닫기" title="닫기" disabled={busy} onClick={close}>
            <FluentIcon glyph="\uE711" />
          </button>
        </header>

        <div className="classic-readonly-notice">
          <FluentIcon glyph="\uE72E" />
          <div><strong>Classic 원본은 읽기 전용입니다.</strong><span>파일을 이동하거나 수정하지 않고, 승인 후 검증된 페이지만 Next 저장 폴더에 복사합니다.</span></div>
        </div>

        <section className="classic-paths" aria-label="Classic 위치">
          <label>
            <span>Classic 데이터 폴더 <small>state.json · atsumi_cache.sqlite</small></span>
            <div><input value={dataRoot} readOnly placeholder="AtsumiData 폴더를 선택하세요" /><button type="button" className="text-button" disabled={busy} onClick={() => void pickFolder("data")}>선택</button></div>
          </label>
          <label>
            <span>Classic 다운로드 폴더 <small>선택 사항 · 앨범 파일 가져오기</small></span>
            <div><input value={downloadRoot} readOnly placeholder="다운로드 폴더를 선택하세요" /><button type="button" className="text-button" disabled={busy} onClick={() => void pickFolder("downloads")}>선택</button></div>
          </label>
          <button type="button" className="text-button primary" disabled={busy || !dataRoot.trim()} onClick={() => void runDryRun()}>
            {busy ? "검사 중…" : "읽기 전용 검사"}
          </button>
        </section>

        {error ? <div className="inline-error" role="alert">{error}</div> : null}

        {report ? (
          <div className="classic-report">
            <header>
              <div><span className={`classic-state is-${report.state}`}>{stateLabels[report.state]}</span><strong>{report.dataRootLabel}</strong>{report.downloadRootLabel ? <span>+ {report.downloadRootLabel}</span> : null}</div>
              <button type="button" className="text-button" disabled={busy} onClick={() => void reload()}>보고서 다시 읽기</button>
            </header>
            <dl className="classic-counts">
              <div><dt>가져올 앨범</dt><dd>{report.counts.galleriesEligible} / {report.counts.galleriesDiscovered}</dd></div>
              <div><dt>페이지</dt><dd>{report.counts.pageFiles}</dd></div>
              <div><dt>복사 예정</dt><dd>{formatBytes(report.counts.plannedCopyBytes)}</dd></div>
              <div><dt>즐겨찾기</dt><dd>{report.counts.favorites}</dd></div>
              <div><dt>검색 기록</dt><dd>{report.counts.searchHistory}</dd></div>
              <div><dt>검토 항목</dt><dd>{report.counts.conflicts}</dd></div>
            </dl>

            {report.conflicts.length ? (
              <section className="classic-conflicts" aria-labelledby="classic-conflicts-title">
                <h3 id="classic-conflicts-title">충돌 및 보존 규칙</h3>
                {report.conflicts.map((conflict) => (
                  <label key={conflict.conflictId} className={`classic-conflict is-${conflict.severity}`}>
                    {conflict.requiresAcknowledgement ? (
                      <input
                        type="checkbox"
                        checked={accepted.has(conflict.conflictId)}
                        disabled={busy || report.state !== "dry_run"}
                        onChange={(event) => setAccepted((current) => {
                          const next = new Set(current);
                          if (event.target.checked) next.add(conflict.conflictId);
                          else next.delete(conflict.conflictId);
                          return next;
                        })}
                      />
                    ) : <FluentIcon glyph={conflict.severity === "blocking" ? "\uE783" : "\uE946"} />}
                    <span><strong>{conflict.galleryId ? `#${conflict.galleryId} · ` : ""}{conflict.code}</strong><small>{conflict.message}</small></span>
                  </label>
                ))}
              </section>
            ) : <p className="classic-clean-report">충돌 없이 가져올 수 있습니다.</p>}

            {report.errorMessage ? <div className="inline-error" role="alert">{report.errorMessage}</div> : null}

            {report.state === "dry_run" ? (
              <div className="classic-approval">
                <label><input type="checkbox" checked={approved} onChange={(event) => setApproved(event.target.checked)} /><span>검사 결과와 경고를 확인했습니다. Classic 원본을 그대로 두고 Next에 복사·등록하는 작업을 승인합니다.</span></label>
                <button type="button" className="text-button primary" disabled={busy || !approved || !allAcknowledged || !report.canApply} onClick={() => void apply()}>승인하고 안전한 항목 가져오기</button>
              </div>
            ) : null}

            {(report.state === "applied" || report.state === "failed") ? (
              <div className="classic-rollback">
                <div><strong>Next 변경만 되돌리기</strong><span>가져온 Next 파일은 관리 격리 폴더로 이동합니다. Classic 원본은 그대로 유지됩니다.</span></div>
                <button type="button" className="text-button warning-button" disabled={busy} onClick={() => void rollback()}>가져오기 되돌리기</button>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </dialog>
  );
}
