import { useEffect, useRef, useState } from "react";
import type { ApiError, SettingsPatch, SettingsSnapshot } from "../api/contracts";
import { FluentIcon } from "./FluentIcon";

type SettingsDialogProps = {
  open: boolean;
  settings: SettingsSnapshot;
  loading: boolean;
  error: ApiError | null;
  onClose: () => void;
  onSave: (patch: SettingsPatch) => Promise<boolean>;
  onNotice: (message: string) => void;
  onPreviewLayout: (layout: { maxColumns: number; previewWidth: number } | null) => void;
};

const sections = ["일반", "탐색", "다운로드", "네트워크", "중복 검사", "저장 공간"];

export function SettingsDialog({ open, settings, loading, error, onClose, onSave, onNotice, onPreviewLayout }: SettingsDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const wasOpen = useRef(false);
  const [activeSection, setActiveSection] = useState("일반");
  const [draft, setDraft] = useState<SettingsSnapshot>(settings);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open && !wasOpen.current) {
      setDraft(settings);
      onPreviewLayout({ maxColumns: settings.maxColumns, previewWidth: settings.previewWidth });
      if (!dialog.current?.open) dialog.current?.showModal();
    } else if (!open && wasOpen.current && dialog.current?.open) {
      dialog.current.close();
      onPreviewLayout(null);
    }
    wasOpen.current = open;
  }, [open, onPreviewLayout, settings]);

  const patch = <K extends keyof SettingsSnapshot>(key: K, value: SettingsSnapshot[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const previewLayout = (maxColumns: number, previewWidth: number) => {
    onPreviewLayout({ maxColumns, previewWidth });
  };

  const close = () => {
    onPreviewLayout(null);
    onClose();
  };

  const save = async () => {
    setSaving(true);
    const success = await onSave({
      downloadRoot: draft.downloadRoot,
      maxColumns: draft.maxColumns,
      previewWidth: draft.previewWidth,
      cacheLimitGb: draft.cacheLimitGb,
      concurrentImageRequests: draft.concurrentImageRequests,
      requestStartIntervalMs: draft.requestStartIntervalMs,
    });
    setSaving(false);
    if (success) close();
  };

  return (
    <dialog
      className="settings-dialog"
      ref={dialog}
      aria-labelledby="settings-dialog-title"
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onClose={onClose}
    >
      <div className="settings-form">
        <header className="dialog-header">
          <div>
            <span className="eyebrow">SETTINGS</span>
            <h2 id="settings-dialog-title">설정</h2>
          </div>
          <div className="dialog-header-actions">
            <button type="button" className="text-button primary" disabled={loading || saving} onClick={() => void save()}>
              {saving ? "저장 중" : "저장"}
            </button>
            <button type="button" className="icon-button small" title="닫기" aria-label="닫기" onClick={close}>
              <FluentIcon glyph="\uE711" />
            </button>
          </div>
        </header>
        <div className="settings-layout">
          <nav className="settings-nav" aria-label="설정 분류">
            {sections.map((section) => (
              <button key={section} type="button" aria-current={activeSection === section ? "page" : undefined} className={activeSection === section ? "is-active" : ""} onClick={() => setActiveSection(section)}>
                {section}
              </button>
            ))}
          </nav>
          <section className="settings-content">
            {activeSection !== "일반" ? (
              <div className="settings-placeholder">
                <span className="eyebrow">FOUNDATION</span>
                <h3>{activeSection}</h3>
                <p>이 분류는 실제 기능 계약과 함께 다음 단계에서 연결됩니다.</p>
              </div>
            ) : (
              <>
                {error ? <div className="inline-error" role="alert">{error.message}</div> : null}
                <div className="setting-row">
                  <div><strong>다운로드 폴더</strong><span>완료된 갤러리를 저장할 위치</span></div>
                  <input value={draft.downloadRoot} placeholder="폴더를 선택하세요" aria-label="다운로드 폴더" onChange={(event) => patch("downloadRoot", event.target.value)} />
                </div>
                <div className="setting-row">
                  <div><strong>앨범 카드 최대 열 수</strong><span>창이 넓어도 설정한 열 수를 넘지 않습니다</span></div>
                  <div className="range-wrap"><input id="settings-max-columns" aria-label="앨범 카드 최대 열 수" type="range" min="1" max="4" step="1" value={draft.maxColumns} onChange={(event) => { const value = Number(event.target.value); patch("maxColumns", value); previewLayout(value, draft.previewWidth); }} /><output htmlFor="settings-max-columns">{draft.maxColumns}열</output></div>
                </div>
                <div className="setting-row">
                  <div><strong>앨범 미리보기 크기</strong><span>Explore와 Downloads에 함께 적용</span></div>
                  <div className="range-wrap"><input id="settings-preview-width" aria-label="앨범 미리보기 크기" type="range" min="160" max="360" value={draft.previewWidth} onChange={(event) => { const value = Number(event.target.value); patch("previewWidth", value); previewLayout(draft.maxColumns, value); }} /><output htmlFor="settings-preview-width">{draft.previewWidth}px</output></div>
                </div>
                <div className="setting-row">
                  <div><strong>캐시 한도</strong><span>썸네일과 재생성 가능한 자료</span></div>
                  <div className="range-wrap"><input id="settings-cache-limit" aria-label="캐시 한도" type="range" min="1" max="30" value={draft.cacheLimitGb} onChange={(event) => patch("cacheLimitGb", Number(event.target.value))} /><output htmlFor="settings-cache-limit">{draft.cacheLimitGb}GB</output></div>
                </div>
                <div className="setting-row">
                  <div><strong>동시 이미지 요청</strong><span>Classic 실측 안정 기본값 5</span></div>
                  <input type="number" min="1" max="30" value={draft.concurrentImageRequests} aria-label="동시 이미지 요청" onChange={(event) => patch("concurrentImageRequests", Number(event.target.value))} />
                </div>
                <div className="setting-row">
                  <div><strong>요청 시작 간격</strong><span>Classic 실측 안정 기본값 25ms</span></div>
                  <input type="number" min="0" max="5000" value={draft.requestStartIntervalMs} aria-label="요청 시작 간격" onChange={(event) => patch("requestStartIntervalMs", Number(event.target.value))} />
                </div>
                <div className="danger-zone">
                  <strong>저장 데이터 관리</strong>
                  <p>삭제 범위와 undo 계약이 확정되기 전에는 실제 파일을 변경하지 않습니다.</p>
                  <div>
                    <button type="button" className="text-button" onClick={() => onNotice("캐시 제거 계획 화면은 다음 단계에서 연결합니다.")}>캐시 제거</button>
                    <button type="button" className="text-button warning-button" onClick={() => onNotice("데이터 제거는 dry-run 보고서부터 제공합니다.")}>데이터 제거</button>
                    <button type="button" className="text-button danger-button" onClick={() => onNotice("모든 파일 제거는 현재 비활성화되어 있습니다.")}>모든 파일 제거</button>
                  </div>
                </div>
              </>
            )}
          </section>
        </div>
      </div>
    </dialog>
  );
}
