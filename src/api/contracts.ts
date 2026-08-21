import type { DownloadState, GalleryId, Language, SearchSort } from "../core/types";

export type ApiErrorAction = "retry" | "review" | "reconnect" | "reveal" | "none";

export type ApiError = {
  code: string;
  message: string;
  retryable: boolean;
  action?: ApiErrorAction;
  details?: Record<string, unknown>;
};

export type ApiResult<T> = { ok: true; data: T } | { ok: false; error: ApiError };

export type SettingsSnapshot = {
  revision: number;
  downloadRoot: string;
  folderNameTemplate: string;
  autoFindHistoryMode: AutoFindHistoryMode;
  maxColumns: number;
  previewWidth: number;
  relatedPreviewWidth: number;
  cacheLimitGb: number;
  concurrentImageRequests: number;
  requestStartIntervalMs: number;
};

export type SettingsPatch = Partial<Omit<SettingsSnapshot, "revision">>;

export type ThumbnailCacheClearResult = {
  successEntriesRemoved: number;
  successBytesRemoved: number;
  negativeEntriesRemoved: number;
};

export type MaintenanceAction =
  | { kind: "quickRepair" }
  | {
    kind: "rebuildLibrary";
    rebuildThumbnailData: boolean;
    rebuildDuplicateAnalysis: boolean;
    rebuildInternalAnalysis: boolean;
    rebuildAutoFindResults: boolean;
  }
  | { kind: "factoryReset"; confirmation: string };

export type MaintenancePreview = {
  previewId: string;
  action: MaintenanceAction;
  originalFilesDeleted: boolean;
  userDecisionsPreserved: boolean;
  restartRequired: boolean;
  warnings: string[];
  steps: string[];
};

export type MaintenanceResult = {
  action: MaintenanceAction;
  completedSteps: string[];
  warnings: string[];
  restartRequired: boolean;
};

export type ExplorationDataResetRequest = {
  confirmation: "RESET_EXPLORATION_DATA";
};

export type ExplorationDataResetResult = {
  favoritesRemoved: number;
  searchHistoryRemoved: number;
  autoFindRunsRemoved: number;
  autoFindCandidatesRemoved: number;
  autoFindExclusionsRemoved: number;
};

export type WindowPlacementSnapshot = {
  revision: number;
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  maximized: boolean;
};

export type WindowPlacement = Omit<WindowPlacementSnapshot, "revision">;

export type JobRef = {
  jobId: string;
  reused: boolean;
};

export type BackendThumbnailKey =
  | { kind: "galleryCover"; galleryId: number }
  | { kind: "galleryPage"; galleryId: number; sourcePage: number }
  | { kind: "artifactPage"; entryId: string; sourcePage: number };

export type ThumbnailRequestDto = {
  key: BackendThumbnailKey;
  consumer: "explore" | "downloads" | "detail" | "review";
  priority: "critical" | "visible" | "prefetch";
};

export type ThumbnailRequestToken = {
  requestId: string;
  key: BackendThumbnailKey;
};

export type ThumbnailInvalidation = {
  key: BackendThumbnailKey;
  successCacheRemoved: boolean;
  negativeCacheRemoved: boolean;
};

export type ResolvedThumbnail = {
  contentType: string;
  bytes: number[];
  width: number;
  height: number;
  sourceRevision?: string;
};

export type ThumbnailDelivery = {
  key: BackendThumbnailKey;
  thumbnail: ResolvedThumbnail;
  cacheStatus: "resolved" | "memory";
};

export type ThumbnailFailure = {
  key: BackendThumbnailKey;
  code:
    | "cancelled"
    | "notFound"
    | "candidatesExhausted"
    | "responseInvalid"
    | "decodeFailed"
    | "temporarilyUnavailable"
    | "unauthorized"
    | "invalidData"
    | "resolver"
    | "coordinatorClosed";
  message: string;
  retryable: boolean;
  negativeCacheHit: boolean;
};

export type ThumbnailCompletionEvent = {
  requestId: string;
  key: BackendThumbnailKey;
  outcome:
    | { status: "ready"; delivery: ThumbnailDelivery }
    | { status: "failed"; failure: ThumbnailFailure };
};

export type ThumbnailWorkerStats = {
  workerCount: number;
  concurrencyLimit: number;
  requestStartIntervalMs: number;
  activeWorkers: number;
  queuedKeys: number;
  inFlightKeys: number;
  subscriberCount: number;
  successCacheEntries: number;
  successCacheBytes: number;
  negativeCacheEntries: number;
  requestsTotal: number;
  successCacheHits: number;
  negativeCacheHits: number;
  joinedInFlight: number;
  resolvedSuccess: number;
  resolvedFailure: number;
  cancelledSubscribers: number;
  cancelledWork: number;
};

export type JobEvent = {
  jobId: string;
  galleryId?: number;
  revision: number;
  state: DownloadState;
  completedUnits?: number;
  totalUnits?: number;
  message?: string;
};

export type DownloadChangedEvent = {
  entryId: string;
  galleryId: number;
  revision: number;
  state: DownloadState;
  progress?: number;
  attempt?: number;
  errorCode?: string;
  errorMessage?: string;
  errorRetryable?: boolean;
};

export type SearchRequest = {
  text: string;
  includeTags: string[];
  excludeTags: string[];
  languages: Language[];
  sort: SearchSort;
  pageSize: number;
};

export type TagNamespace = "tag" | "female" | "male";
export type TagCatalogStatus = {
  revision: number;
  entryCount: number;
  neutralCount: number;
  femaleCount: number;
  maleCount: number;
  lastAttemptAt?: string;
  lastSuccessAt?: string;
  lastErrorCode?: string;
  lastErrorMessage?: string;
};
export type TagSuggestionRequest = { query: string; namespace?: TagNamespace; limit: number };
export type TagSuggestion = { namespace: TagNamespace; name: string; token: string; galleryCount: number; favorite: boolean };

export type GallerySummary = {
  id: GalleryId;
  title: string;
  artist: string;
  group?: string;
  pages: number;
  language: Language;
  tags: string[];
  series: string[];
  characters: string[];
  publishedRank: number;
  popularity: number;
  thumbnailKey?: string;
  thumbnailWidth: number;
  thumbnailHeight: number;
};

export type GalleryPage = {
  page: number;
  totalPages: number;
  items: GallerySummary[];
};

export type SearchSubmission = {
  queryId: string;
  firstPage: GalleryPage;
};

export type FavoriteNamespace = "artist" | "group" | "series" | "character" | "tag";

export type FavoriteKey = {
  namespace: FavoriteNamespace;
  value: string;
};

export type FavoriteRecord = FavoriteKey & {
  revision: number;
  createdAt: string;
  updatedAt: string;
};

export type FavoriteMutationResult = {
  enabled: boolean;
  favorite?: FavoriteRecord;
};

export type SearchHistoryEntry = {
  historyId: number;
  text: string;
  includeTags: string[];
  excludeTags: string[];
  languages: Language[];
  sort: SearchSort;
  pageSize: number;
  useCount: number;
  lastUsedAt: string;
};

export type AutoFindRunState = "running" | "completed" | "failed" | "cancelled";

export type AutoFindHistoryMode = "include_all_history" | "newer_than_oldest_downloaded";

export type AutoFindCutoffEvidence = {
  artist: string;
  oldestOwnedGalleryId?: GalleryId;
  qualifiedOwnedCount: number;
  source: "verified_owned_artifact";
  policyVersion: 1;
};

export type AutoFindTruncation = {
  artist: string;
  reason: "candidate_limit_after_cutoff";
  eligibleCount: number;
  limit: number;
};

export type AutoFindRun = {
  runId: string;
  revision: number;
  state: AutoFindRunState;
  totalFavorites: number;
  completedFavorites: number;
  candidatesFound: number;
  startedAt: string;
  updatedAt: string;
  finishedAt?: string;
  errorCode?: string;
  errorMessage?: string;
  historyMode: AutoFindHistoryMode;
};

export type AutoFindCandidate = GallerySummary & {
  runId: string;
  matchedFavorite: FavoriteKey;
  discoveredAt: string;
};

export type AutoFindSnapshot = {
  run?: AutoFindRun;
  candidates: AutoFindCandidate[];
  cutoffEvidence: AutoFindCutoffEvidence[];
  truncations: AutoFindTruncation[];
};

export type AutoFindExclusionResult = {
  excludedGalleryIds: GalleryId[];
  snapshot: AutoFindSnapshot;
};

export type HashProfile = {
  profileVersion: number;
  algorithmVersion: number;
  dHashBits: number;
  pHashBits: number;
  visualMatchThreshold: number;
  lowInformationStdDevThreshold: number;
};

export type DuplicateScanState = "running" | "completed" | "failed" | "cancelled";

export type DuplicateScanRun = {
  runId: string;
  revision: number;
  state: DuplicateScanState;
  totalArtifacts: number;
  hashedArtifacts: number;
  totalPairs: number;
  comparedPairs: number;
  candidatesFound: number;
  startedAt: string;
  updatedAt: string;
  finishedAt?: string;
  errorCode?: string;
  errorMessage?: string;
};

export type DuplicateRelation = "exact" | "contains" | "partial" | "translation_visual";

export type DuplicateGalleryRef = {
  galleryId: GalleryId;
  entryId: string;
  title: string;
  artist?: string;
  group?: string;
  pageCount: number;
};

export type DuplicateCandidate = {
  candidateId: string;
  revision: number;
  parent: DuplicateGalleryRef;
  candidate: DuplicateGalleryRef;
  relation: DuplicateRelation;
  confidence: number;
  matchedPages: number;
  parentCoverage: number;
  candidateCoverage: number;
  createdAt: string;
  updatedAt: string;
};

export type DuplicateEvidenceKind =
  | "exact_sha256"
  | "visual_hash"
  | "sequence_alignment"
  | "e_hentai_relation";

export type DuplicateEvidence = {
  evidenceId: string;
  kind: DuplicateEvidenceKind;
  confidence: number;
  matchedPages: number;
  description: string;
};

export type DuplicatePagePair = {
  parentSourcePage: number;
  candidateSourcePage: number;
  exactSha256: boolean;
  dHashDistance: number;
  pHashDistance: number;
  detailHashDistance: number;
  edgeSimilarity: number;
  visualSimilarity: number;
  lowInformation: boolean;
};

export type DuplicateDecisionAction =
  | "hide_parent"
  | "hide_candidate"
  | "series_link"
  | "series_unlink"
  | "exclude_pair";

export type DuplicateDecisionRequest = {
  candidateId: string;
  expectedRevision: number;
  action: DuplicateDecisionAction;
  targetGalleryId?: GalleryId;
  seriesGroupId?: string;
  seriesName?: string;
};

export type DuplicateDecisionHistory = {
  decisionId: string;
  candidateId: string;
  candidateRevision: number;
  action: DuplicateDecisionAction;
  targetGalleryId?: GalleryId;
  seriesGroupId?: string;
  createdAt: string;
};

export type SeriesGroup = {
  seriesGroupId: string;
  name: string;
  revision: number;
  members: DuplicateGalleryRef[];
  createdAt: string;
  updatedAt: string;
};

export type DuplicateReview = {
  candidate: DuplicateCandidate;
  evidence: DuplicateEvidence[];
  pagePairs: DuplicatePagePair[];
  decisions: DuplicateDecisionHistory[];
  seriesGroups: SeriesGroup[];
};

export type DuplicateSnapshot = {
  profile: HashProfile;
  run?: DuplicateScanRun;
  candidates: DuplicateCandidate[];
};

export type InternalScanState = "running" | "completed" | "failed" | "cancelled";

export type InternalScanRun = {
  runId: string;
  revision: number;
  state: InternalScanState;
  totalArtifacts: number;
  scannedArtifacts: number;
  totalPages: number;
  comparedPairs: number;
  groupsFound: number;
  algorithmVersion: number;
  skippedArtifacts: number;
  skippedPages: number;
  startedAt: string;
  updatedAt: string;
  finishedAt?: string;
  errorCode?: string;
  errorMessage?: string;
};

export type InternalScanSkip = {
  entryId: string;
  galleryId: GalleryId;
  title: string;
  pageCount: number;
  reason: "page_limit";
};

export type InternalMatchKind = "exact" | "translation_visual";

export type InternalPageEvidence = {
  sourcePage: number;
  exactSha256: boolean;
  visualSimilarity: number;
  detailHashDistance: number;
  lowInformation: boolean;
};

export type InternalDuplicateGroup = {
  groupId: string;
  blockId: string;
  sequenceIndex: number;
  revision: number;
  entryId: string;
  galleryId: GalleryId;
  relation: InternalMatchKind;
  confidence: number;
  recommendedKeepSourcePage: number;
  pages: InternalPageEvidence[];
  resolved: boolean;
  createdAt: string;
  updatedAt: string;
};

export type PageQuarantineState =
  | "pending_quarantine"
  | "quarantined"
  | "pending_restore"
  | "restored";

export type PageQuarantineRecord = {
  recordId: string;
  planId: string;
  entryId: string;
  galleryId: GalleryId;
  sourcePage: number;
  originalRelativePath: string;
  quarantineRelativePath: string;
  reason: string;
  state: PageQuarantineState;
  createdAt: string;
  updatedAt: string;
};

export type InternalDuplicateSnapshot = {
  run?: InternalScanRun;
  groups: InternalDuplicateGroup[];
  quarantineRecords: PageQuarantineRecord[];
  skips: InternalScanSkip[];
};

export type InternalDuplicateReview = {
  entryId: string;
  galleryId: GalleryId;
  title: string;
  groups: InternalDuplicateGroup[];
  quarantineRecords: PageQuarantineRecord[];
};

export type InternalRemovalSelection = {
  groupId: string;
  expectedRevision: number;
  keepSourcePage: number;
  removeSourcePages: number[];
};

export type InternalRemovalPlanRequest = {
  entryId: string;
  selections: InternalRemovalSelection[];
};

export type InternalRemovalPlan = InternalRemovalPlanRequest & {
  planId: string;
  filesToQuarantine: number;
  bytesToQuarantine: number;
  expiresAt: string;
};

export type InternalRemovalApplyRequest = {
  plan: InternalRemovalPlan;
  reason: string;
};

export type InternalRemovalUndoRequest = {
  recordIds: string[];
};

export type InternalRemovalResult = {
  review: InternalDuplicateReview;
  records: PageQuarantineRecord[];
};

export type GalleryDetail = GallerySummary & {
  related: GallerySummary[];
};

export type DownloadEntry = {
  entryId: string;
  galleryId: GalleryId;
  revision: number;
  state: DownloadState;
  progress?: number;
  attempt?: number;
  errorCode?: string;
  errorMessage?: string;
  errorRetryable?: boolean;
  reviewKind?: "gallery_duplicate" | "internal_pages";
  reviewId?: string;
};

export type DownloadListRequest = {
  state?: DownloadState;
  query?: string;
  page: number;
  pageSize: number;
};

export type DownloadPage = {
  page: number;
  totalItems: number;
  entries: DownloadEntry[];
};

export type ReconcileIssue = {
  entryId: string;
  code: string;
  message: string;
  recoverable: boolean;
};

export type ReconcileReport = {
  inspectedArtifacts: number;
  verifiedArtifacts: number;
  resumedJobs: number;
  issues: ReconcileIssue[];
};

export type RemovalPlan = {
  planId: string;
  entryIds: string[];
  filesToQuarantine: number;
  bytesToQuarantine: number;
  expiresAt: string;
};

export type OpenResult = { opened: boolean; path?: string };

export type ActivityItem = {
  id: string;
  label: string;
  detail: string;
  severity: "neutral" | "info" | "warning" | "danger" | "success";
  progress?: number;
};
