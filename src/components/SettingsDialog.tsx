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
  onClassicImport: () => void;
  onPreviewLayout: (layout: { maxColumns: number; previewWidth: number } | null) => void;
};

const sections = ["일반", "저장 공간"];
const DEFAULT_FOLDER_NAME_TEMPLATE = "[{artist}] {title} [{group}] {id}";

const previewFolderName = (template: string) => template
  .replaceAll("{artist}", "작가")
  .replaceAll("{title}", "작품 제목")
  .replaceAll("{group}", "그룹")
  .replaceAll("{id}", "4113714")
  .replace(/[<>:\"/\\|?*\u0000-\u001f\u007f-\u009f]/g, "_")
  .replace(/\s+/g, " ")
  .trim()
  .replace(/[ .]+$/g, "");

export function SettingsDialog({ open, settings, loading, error, onClose, onSave, onClassicImport, onPreviewLayout }: SettingsDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const closingInternally = useRef(false);
  const wasOpen = useRef(false);
  const [activeSection, setActiveSection] = useState("일반");
  const [draft, setDraft] = useState<SettingsSnapshot>(settings);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (open && !wasOpen.current) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setDraft(settings);
      onPreviewLayout({ maxColumns: settings.maxColumns, previewWidth: settings.previewWidth });
      if (!dialog.current?.open) dialog.current?.showModal();
      window.requestAnimationFrame(() => closeButton.current?.focus());
    } else if (!open && wasOpen.current && dialog.current?.open) {
      closingInternally.current = true;
      dialog.current.close();
      onPreviewLayout(null);
      const target = opener.current;
      opener.current = null;
      window.requestAnimationFrame(() => {
        if (target?.isConnected) target.focus();
        else document.querySelector<HTMLElement>('[aria-label="설정"]')?.focus();
      });
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
      folderNameTemplate: draft.folderNameTemplate,
      autoFindHistoryMode: draft.autoFindHistoryMode,
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
      onClose={() => {
        if (closingInternally.current) {
          closingInternally.current = false;
          return;
        }
        onClose();
      }}
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
            <button ref={closeButton} type="button" className="icon-button small" title="닫기" aria-label="닫기" onClick={close}>
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
            {activeSection === "저장 공간" ? (
              <div className="settings-storage">
                <span className="eyebrow">MIGRATION</span>
                <h3>Classic 데이터</h3>
                <p>Classic 원본은 읽기 전용으로 조사하고, dry-run 보고서를 승인한 뒤에만 Next 저장소로 복사합니다.</p>
                <button type="button" className="text-button primary" onClick={() => { close(); onClassicImport(); }}>Classic 가져오기 열기</button>
              </div>
            ) : (
              <>
                {error ? <div className="inline-error" role="alert">{error.message}</div> : null}
                <div className="setting-row">
                  <div><strong>다운로드 폴더</strong><span>완료된 갤러리를 저장할 위치</span></div>
                  <input value={draft.downloadRoot} placeholder="폴더를 선택하세요" aria-label="다운로드 폴더" onChange={(event) => patch("downloadRoot", event.target.value)} />
                </div>
                <div className="setting-row">
                  <div>
                    <strong>갤러리 폴더 이름</strong>
                    <span>{"{artist}, {title}, {group}, {id}를 사용할 수 있으며 {id}는 필수입니다."}</span>
                    <span aria-live="polite">미리보기: {previewFolderName(draft.folderNameTemplate) || "(유효한 이름 없음)"}</span>
                  </div>
                  <div>
                    <input
                      value={draft.folderNameTemplate}
                      aria-label="갤러리 폴더 이름 템플릿"
                      maxLength={512}
                      onChange={(event) => patch("folderNameTemplate", event.target.value)}
                    />
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => patch("folderNameTemplate", DEFAULT_FOLDER_NAME_TEMPLATE)}
                    >
                      기본값 복원
                    </button>
                  </div>
                </div>
                <div className="setting-row">
                  <div>
                    <strong>Auto Find 기록 기준</strong>
                    <span>변경한 기준은 다음 Auto Find 실행부터 적용됩니다.</span>
                    <span>최신 기준은 검증 완료·격리된 소유 작품의 gallery ID 이후만 후보로 봅니다.</span>
                  </div>
                  <select
                    aria-label="Auto Find 기록 기준"
                    value={draft.autoFindHistoryMode}
                    onChange={(event) => patch("autoFindHistoryMode", event.target.value as SettingsSnapshot["autoFindHistoryMode"])}
                  >
                    <option value="include_all_history">전체 기록 포함</option>
                    <option value="newer_than_oldest_downloaded">가장 오래된 소유 작품 이후</option>
                  </select>
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
                  <p id="destructive-settings-unavailable">캐시 정리와 영구 삭제는 안전한 계획·검토·undo 계약이 아직 없으므로 사용할 수 없습니다. 다운로드 격리와 복원은 Downloads에서 이용할 수 있습니다.</p>
                  <div>
                    <button type="button" className="text-button" disabled aria-describedby="destructive-settings-unavailable" title="안전한 캐시 제거 계획이 아직 제공되지 않습니다">캐시 제거</button>
                    <button type="button" className="text-button warning-button" disabled aria-describedby="destructive-settings-unavailable" title="dry-run과 undo를 제공하기 전에는 사용할 수 없습니다">데이터 제거</button>
                    <button type="button" className="text-button danger-button" disabled aria-describedby="destructive-settings-unavailable" title="영구 삭제는 지원하지 않습니다">모든 파일 제거</button>
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
