import { useEffect, useRef } from "react";
import { FluentIcon } from "./FluentIcon";

type ExitConfirmDialogProps = {
  open: boolean;
  activeDownloads: number | null;
  statusError: boolean;
  onClose: () => void;
  onMinimizeToTray: () => void;
  onQuit: () => void;
  actionPending: boolean;
};

export function ExitConfirmDialog({ open, activeDownloads, statusError, onClose, onMinimizeToTray, onQuit, actionPending }: ExitConfirmDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const trayButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const closingInternally = useRef(false);

  useEffect(() => {
    const node = dialog.current;
    if (!node) return;
    const wasOpen = node.open;
    if (open && !wasOpen) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      node.showModal();
      window.requestAnimationFrame(() => trayButton.current?.focus());
    } else if (!open && wasOpen) {
      closingInternally.current = true;
      node.close();
      const target = opener.current;
      opener.current = null;
      window.requestAnimationFrame(() => {
        if (target?.isConnected) target.focus();
        else document.querySelector<HTMLElement>(".view-header input")?.focus();
      });
    }
  }, [open]);

  return (
    <dialog
      className="exit-dialog"
      ref={dialog}
      aria-labelledby="exit-dialog-title"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      onClose={() => {
        if (closingInternally.current) {
          closingInternally.current = false;
          return;
        }
        onClose();
      }}
    >
      <div className="exit-dialog-body">
        <header className="dialog-header">
          <div>
            <span className="eyebrow">APP CONTROL</span>
            <h2 id="exit-dialog-title">Atsumi Next를 닫을까요?</h2>
          </div>
          <button type="button" className="icon-button small" title="취소" aria-label="종료 선택 취소" onClick={onClose}>
            <FluentIcon glyph="\uE711" />
          </button>
        </header>
        <div className={`exit-download-notice${statusError || (activeDownloads ?? 0) > 0 ? " is-active" : ""}`} role="status">
          <FluentIcon glyph={statusError ? "\uE7BA" : (activeDownloads ?? 0) > 0 ? "\uE896" : "\uE73E"} />
          <div>
            <strong>{statusError ? "다운로드 상태를 확인하지 못했습니다." : activeDownloads === null ? "다운로드 상태를 확인하는 중입니다." : activeDownloads > 0 ? `${activeDownloads}개 다운로드 작업이 진행 중입니다.` : "진행 중인 다운로드가 없습니다."}</strong>
            <span>
              {statusError
                ? "프로그램을 종료하면 확인하지 못한 작업이 중단될 수 있습니다. 트레이 최소화를 권장합니다."
                : activeDownloads === null
                  ? "잠시만 기다려 주세요."
                  : activeDownloads > 0
                ? "트레이로 최소화하면 작업을 계속 볼 수 있습니다. 프로그램 종료를 선택하면 진행 작업은 중단됨 상태로 복구됩니다."
                : "트레이로 최소화해 백그라운드에 둘 수도 있고, 프로그램을 완전히 종료할 수도 있습니다."}
            </span>
          </div>
        </div>
        <div className="exit-dialog-actions">
          <button ref={trayButton} type="button" className="exit-choice primary-choice" disabled={actionPending} onClick={onMinimizeToTray}>
            <FluentIcon glyph="\uE921" />
            <span><strong>트레이로 최소화</strong><small>백그라운드에서 Atsumi Next 유지</small></span>
          </button>
          <button type="button" className="exit-choice quit-choice" disabled={actionPending || (activeDownloads === null && !statusError)} onClick={onQuit}>
            <FluentIcon glyph="\uE8BB" />
            <span><strong>프로그램 종료</strong><small>{statusError || (activeDownloads ?? 0) > 0 ? "진행 작업을 중단하고 종료" : "Atsumi Next 완전히 종료"}</small></span>
          </button>
        </div>
      </div>
    </dialog>
  );
}
