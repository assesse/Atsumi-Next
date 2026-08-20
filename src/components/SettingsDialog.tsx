import { useEffect, useRef, useState } from "react";
import type {
  ApiError,
  ApiResult,
  ExplorationDataResetResult,
  SettingsPatch,
  SettingsSnapshot,
  ThumbnailCacheClearResult,
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
  onClearCache: () => Promise<ApiResult<ThumbnailCacheClearResult>>;
  onResetExplorationData: () => Promise<ApiResult<ExplorationDataResetResult>>;
};

const DEFAULT_FOLDER_NAME_TEMPLATE = "[{artist}] {title} [{group}] {id}";

export function SettingsDialog({ open, settings, loading, error, onClose, onSave, onPreviewLayout, onPreviewFolderName, onClearCache, onResetExplorationData }: SettingsDialogProps) {
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
  const [maintenanceBusy, setMaintenanceBusy] = useState<"cache" | "exploration" | null>(null);
  const [maintenanceMessage, setMaintenanceMessage] = useState("");

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
      concurrentImageRequests: 5,
      requestStartIntervalMs: 25,
    }));
    previewLayout(maxColumns, previewWidth);
    setMaintenanceMessage("화면·네트워크 설정을 기본값으로 되돌렸습니다. 저장을 눌러 적용하세요.");
  };

  const clearCache = async () => {
    setMaintenanceBusy("cache");
    const result = await onClearCache();
    setMaintenanceBusy(null);
    setMaintenanceMessage(result.ok
      ? `재생성 가능한 미리보기 캐시 ${result.data.successEntriesRemoved + result.data.negativeEntriesRemoved}개를 정리했습니다.`
      : result.error.message);
  };

  const resetExplorationData = async () => {
    if (!window.confirm("즐겨찾기, 검색 이력, Auto Find 결과와 제외 기록을 초기화할까요? 다운로드 DB와 파일은 삭제되지 않습니다.")) return;
    setMaintenanceBusy("exploration");
    const result = await onResetExplorationData();
    setMaintenanceBusy(null);
    setMaintenanceMessage(result.ok
      ? `탐색 데이터 ${result.data.favoritesRemoved + result.data.searchHistoryRemoved + result.data.autoFindRunsRemoved + result.data.autoFindCandidatesRemoved + result.data.autoFindExclusionsRemoved}건을 초기화했습니다.`
      : result.error.message);
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
                  <div><strong>동시 이미지 요청</strong><span>Classic 실측 안정 기본값 5</span></div>
                  <input type="number" min="1" max="30" value={draft.concurrentImageRequests} aria-label="동시 이미지 요청" onChange={(event) => patch("concurrentImageRequests", Number(event.target.value))} />
                </div>
                <div className="setting-row">
                  <div><strong>요청 시작 간격</strong><span>Classic 실측 안정 기본값 25ms</span></div>
                  <input type="number" min="0" max="5000" value={draft.requestStartIntervalMs} aria-label="요청 시작 간격" onChange={(event) => patch("requestStartIntervalMs", Number(event.target.value))} />
                </div>
                <div className="danger-zone">
                  <strong>저장 데이터 관리</strong>
                  <p>캐시는 재생성 가능한 미리보기만 지웁니다. 탐색 데이터 초기화는 즐겨찾기·검색 이력·Auto Find 기록만 지우며 다운로드 DB와 파일은 보존합니다.</p>
                  {maintenanceMessage ? <p className="maintenance-message" role="status">{maintenanceMessage}</p> : null}
                  <div>
                    <button type="button" className="text-button" disabled={maintenanceBusy !== null} onClick={() => void clearCache()}>{maintenanceBusy === "cache" ? "정리 중" : "캐시 초기화"}</button>
                    <button type="button" className="text-button" disabled={maintenanceBusy !== null} onClick={restorePreferenceDefaults}>설정 기본값</button>
                    <button type="button" className="text-button warning-button" disabled={maintenanceBusy !== null} onClick={() => void resetExplorationData()}>{maintenanceBusy === "exploration" ? "초기화 중" : "탐색 데이터 초기화"}</button>
                  </div>
                  <p>다운로드 기록·artifact·격리 파일의 일괄 영구 삭제는 복구 계획이 없으므로 제공하지 않습니다.</p>
                </div>
              </>
          </section>
        </div>
      </div>
    </dialog>
  );
}
