import { useEffect, useRef, useState } from "react";
import type {
  ApiError,
  ApiResult,
  MaintenanceAction,
  MaintenanceResult,
  SettingsPatch,
  SettingsSnapshot,
} from "../api/contracts";
import {
  GALLERY_PREVIEW_PRESETS,
  galleryPreviewPresetIndex,
} from "../layout/galleryPreviewPresets";
import { FluentIcon } from "./FluentIcon";

type SettingsDialogProps = {
  open: boolean;
  settings: SettingsSnapshot;
  loading: boolean;
  error: ApiError | null;
  onClose: () => void;
  onSave: (patch: SettingsPatch) => Promise<boolean>;
  onPreviewLayout: (layout: { maxColumns: number; previewWidth: number } | null) => void;
  onPreviewFolderName: (template: string) => Promise<ApiResult<string>>;
  onMaintenance: (action: MaintenanceAction) => Promise<ApiResult<MaintenanceResult>>;
};

const DEFAULT_FOLDER_NAME_TEMPLATE = "[{artist}] {title} [{group}] {id}";

export function SettingsDialog({ open, settings, loading, error, onClose, onSave, onPreviewLayout, onPreviewFolderName, onMaintenance }: SettingsDialogProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const closingInternally = useRef(false);
  const wasOpen = useRef(false);
  const [draft, setDraft] = useState<SettingsSnapshot>(settings);
  const [saving, setSaving] = useState(false);
  const [folderPreview, setFolderPreview] = useState("");
  const [folderPreviewError, setFolderPreviewError] = useState("");
  const folderPreviewRequest = useRef(0);
  const [maintenanceBusy, setMaintenanceBusy] = useState<MaintenanceAction["kind"] | null>(null);
  const [maintenanceMessage, setMaintenanceMessage] = useState("");
  const [rebuildOptions, setRebuildOptions] = useState({ thumbnail: true, duplicate: false, internal: false, autoFind: false });

  useEffect(() => {
    if (open && !wasOpen.current) {
      opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      setDraft(settings);
      setMaintenanceMessage("");
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

  useEffect(() => {
    if (!open) return undefined;
    const request = ++folderPreviewRequest.current;
    const timer = window.setTimeout(() => {
      void onPreviewFolderName(draft.folderNameTemplate).then((result) => {
        if (folderPreviewRequest.current !== request) return;
        if (result.ok) {
          setFolderPreview(result.data);
          setFolderPreviewError("");
        } else {
          setFolderPreview("");
          setFolderPreviewError(result.error.message);
        }
      }).catch(() => {
        if (folderPreviewRequest.current !== request) return;
        setFolderPreview("");
        setFolderPreviewError("미리보기를 만들 수 없습니다.");
      });
    }, 125);
    return () => window.clearTimeout(timer);
  }, [draft.folderNameTemplate, onPreviewFolderName, open]);

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

  const restorePreferenceDefaults = () => {
    const maxColumns = 3;
    const previewWidth = 220;
    setDraft((current) => ({
      ...current,
      autoFindHistoryMode: "include_all_history",
      maxColumns,
      previewWidth,
      relatedPreviewWidth: 240,
      concurrentImageRequests: 5,
      requestStartIntervalMs: 25,
    }));
    previewLayout(maxColumns, previewWidth);
    setMaintenanceMessage("화면·네트워크 설정을 기본값으로 되돌렸습니다. 저장을 눌러 적용하세요.");
  };

  const runMaintenance = async (action: MaintenanceAction) => {
    if (action.kind === "factoryReset" && !window.confirm("앱 데이터 전체를 초기화하고 앱을 다시 시작할까요? 외부 다운로드 원본 파일은 유지됩니다.")) return;
    setMaintenanceBusy(action.kind);
    const result = await onMaintenance(action);
    setMaintenanceBusy(null);
    setMaintenanceMessage(result.ok ? result.data.completedSteps.join(" · ") : result.error.message);
  };

  const save = async () => {
    setSaving(true);
    const success = await onSave({
      downloadRoot: draft.downloadRoot,
      folderNameTemplate: draft.folderNameTemplate,
      autoFindHistoryMode: draft.autoFindHistoryMode,
      maxColumns: draft.maxColumns,
      previewWidth: draft.previewWidth,
      relatedPreviewWidth: draft.relatedPreviewWidth,
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
        <div className="settings-layout settings-layout-single">
          <section className="settings-content">
              <>
                {error ? <div className="inline-error" role="alert">{error.message}</div> : null}
                <div className="setting-row">
                  <div><strong>다운로드 폴더</strong><span>완료된 갤러리를 저장할 위치</span></div>
                  <input value={draft.downloadRoot} placeholder="폴더를 선택하세요" aria-label="다운로드 폴더" onChange={(event) => patch("downloadRoot", event.target.value)} />
                </div>
                <div className="setting-row">
                  <div>
                    <strong>갤러리 폴더 이름</strong>
                    <span>{"사용가능 인자 : {artist}, {title}, {group}, {id}"}</span>
                    <span aria-live="polite">미리보기 : {folderPreview || "(확인 중)"}</span>
                    {folderPreviewError ? <span className="setting-validation-error" role="alert">{folderPreviewError}</span> : null}
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
                  <div className="range-wrap"><input id="settings-preview-width" aria-label="앨범 미리보기 크기" type="range" min="0" max={GALLERY_PREVIEW_PRESETS.length - 1} step="1" value={galleryPreviewPresetIndex(draft.previewWidth)} onChange={(event) => { const preset = GALLERY_PREVIEW_PRESETS[Number(event.target.value)] ?? GALLERY_PREVIEW_PRESETS[2]!; patch("previewWidth", preset.width); previewLayout(draft.maxColumns, preset.width); }} /><output htmlFor="settings-preview-width">{draft.previewWidth}px</output></div>
                </div>
                <div className="setting-row">
                  <div><strong>Related galleries 미리보기 크기</strong><span>Floating Detail 안의 Related galleries에만 적용</span></div>
                  <div className="range-wrap"><input id="settings-related-preview-width" aria-label="Related galleries 미리보기 크기" type="range" min="180" max="320" step="20" value={draft.relatedPreviewWidth} onChange={(event) => patch("relatedPreviewWidth", Number(event.target.value))} /><output htmlFor="settings-related-preview-width">{draft.relatedPreviewWidth}px</output></div>
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
                  <p>원본 파일과 사용자 판정을 보존하는 복구·검사 작업과, 외부 원본을 보존하는 앱 데이터 초기화를 제공합니다.</p>
                  {maintenanceMessage ? <p className="maintenance-message" role="status">{maintenanceMessage}</p> : null}
                  <div>
                    <button type="button" className="text-button" disabled={maintenanceBusy !== null} onClick={restorePreferenceDefaults}>설정 기본값</button>
                  </div>
                  <div className="maintenance-actions">
                    <button type="button" className="text-button" disabled={maintenanceBusy !== null} onClick={() => void runMaintenance({ kind: "quickRepair" })}>{maintenanceBusy === "quickRepair" ? "복구 중" : "빠른 복구"}</button>
                    <p>다운로드, 검색 또는 미리보기가 멈출 때 cache와 중단된 작업 상태를 정리합니다. 저장된 앨범과 원본 파일은 유지됩니다.</p>
                    <button type="button" className="text-button" disabled={maintenanceBusy !== null} onClick={() => void runMaintenance({ kind: "rebuildLibrary", rebuildThumbnailData: rebuildOptions.thumbnail, rebuildDuplicateAnalysis: rebuildOptions.duplicate, rebuildInternalAnalysis: rebuildOptions.internal, rebuildAutoFindResults: rebuildOptions.autoFind })}>{maintenanceBusy === "rebuildLibrary" ? "검사 중" : "라이브러리 검사 및 재구축"}</button>
                    <p>DB, manifest와 실제 파일을 검사하고 필요한 파생 데이터를 다시 만듭니다. 원본과 사용자 판정은 유지됩니다.</p>
                    <label><input type="checkbox" checked={rebuildOptions.thumbnail} onChange={(event) => setRebuildOptions((current) => ({ ...current, thumbnail: event.target.checked }))} /> 미리보기 cache 재생성</label>
                    <label><input type="checkbox" checked={rebuildOptions.duplicate} onChange={(event) => setRebuildOptions((current) => ({ ...current, duplicate: event.target.checked }))} /> 작품 중복 분석 재실행</label>
                    <label><input type="checkbox" checked={rebuildOptions.internal} onChange={(event) => setRebuildOptions((current) => ({ ...current, internal: event.target.checked }))} /> 내부 중복 분석 재실행</label>
                    <label><input type="checkbox" checked={rebuildOptions.autoFind} onChange={(event) => setRebuildOptions((current) => ({ ...current, autoFind: event.target.checked }))} /> Auto Find 결과 갱신</label>
                    <button type="button" className="text-button warning-button" disabled={maintenanceBusy !== null} onClick={() => void runMaintenance({ kind: "factoryReset", confirmation: "RESET_ALL_APP_DATA" })}>{maintenanceBusy === "factoryReset" ? "초기화 준비 중" : "앱 데이터 완전 초기화"}</button>
                    <p>첫 실행 상태로 돌아갑니다. 외부 다운로드 원본 파일과 quarantine/recovery 파일은 유지됩니다.</p>
                  </div>
                </div>
              </>
          </section>
        </div>
      </div>
    </dialog>
  );
}
