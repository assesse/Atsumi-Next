import { useEffect, useRef } from "react";
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
        <div className="exit-dialog-header">
          <h2 id="exit-dialog-title">앱을 닫을까요?</h2>
          <button type="button" className="exit-dialog-close" aria-label="종료 취소" title="종료 취소" disabled={actionPending} onClick={onClose}>×</button>
        </div>
        <p className={`exit-download-status${statusError ? " is-error" : (activeDownloads ?? 0) > 0 ? " is-working" : ""}`} role="status">
          {statusError
            ? "다운로드 상태 확인 불가"
            : activeDownloads === null
              ? "다운로드 상태 확인 중"
              : activeDownloads > 0
                ? `작업 진행 중 · 다운로드 ${activeDownloads}개`
                : "진행 중인 다운로드 없음"}
        </p>
        <div className="exit-dialog-actions">
          <button ref={trayButton} type="button" className="exit-choice primary-choice" disabled={actionPending} onClick={onMinimizeToTray}>
            트레이로 보내기
          </button>
          <button type="button" className="exit-choice quit-choice" title={(activeDownloads ?? 0) > 0 ? "진행 중인 다운로드를 중단하고 종료" : "Atsumi Next 완전히 종료"} disabled={actionPending || (activeDownloads === null && !statusError)} onClick={onQuit}>
            종료
          </button>
        </div>
      </div>
    </dialog>
  );
}
