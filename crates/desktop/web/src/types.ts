export interface VaultRef {
  path: string;
  name: string;
  is_sync_profile: boolean;
  profile_id: string | null;
}

export type PageLayoutMode = 'a4' | 'full';
export type PageOrientation = 'portrait' | 'landscape';

export interface WorkspaceState {
  vaults: VaultRef[];
  active_vault: string | null;
  open_tabs: string[];
  active_tab: string | null;
  split_tab: string | null;
  favorites: string[];
  recent_notes: string[];
  theme: 'light' | 'dark';
  editor_mode: 'wysiwyg' | 'markdown';
  daily_folder: string;
  templates_folder: string;
  show_tags: boolean;
  page_layout: PageLayoutMode;
  page_orientation: PageOrientation;
  margin_top_cm: number;
  margin_bottom_cm: number;
  margin_left_cm: number;
  margin_right_cm: number;
  auto_fit_a4: boolean;
  show_page_guides: boolean;
  print_include_title: boolean;
  print_include_properties: boolean;
  editor_zoom: number;
  daily_folders_by_vault: Record<string, string>;
}

export interface TreeNodeDto {
  name: string;
  path: string;
  kind: 'folder' | 'note';
  icon: string | null;
  color?: string | null;
  children: TreeNodeDto[];
  modified: number;
  encrypted?: boolean;
  locked?: boolean;
}

export interface NoteMetaDto {
  path: string;
  title: string;
  icon: string | null;
  tags: string[];
  updated_at: string;
  word_count: number;
  encrypted?: boolean;
  locked?: boolean;
}

export interface TagCountDto {
  tag: string;
  count: number;
}

export interface VaultScanDto {
  tree: TreeNodeDto[];
  notes: NoteMetaDto[];
  tags: TagCountDto[];
  total_notes: number;
  total_folders: number;
  revision: number;
  encrypted_notes?: number;
  vault_auto_encrypt?: boolean;
  vault_unlocked?: boolean;
  vault_has_crypto_config?: boolean;
  vault_crypto_hint?: string | null;
}

export type PropertyType = 'tags' | 'date' | 'checkbox' | 'url' | 'number' | 'text' | string;

export interface NotePropertyDto {
  key: string;
  value: string;
  prop_type: PropertyType;
}

export interface NoteHeadingDto {
  level: number;
  text: string;
  line: number;
}

export interface BacklinkDto {
  path: string;
  title: string;
  icon: string | null;
  snippet: string;
  linked: boolean;
}

export interface NoteDocumentDto {
  path: string;
  title: string;
  icon: string | null;
  cover: string | null;
  properties: NotePropertyDto[];
  raw_frontmatter: string;
  body: string;
  headings: NoteHeadingDto[];
  outgoing_links: string[];
  backlinks: BacklinkDto[];
  tags: string[];
  word_count: number;
  char_count: number;
  updated_at: string;
  encrypted?: boolean;
  locked?: boolean;
  encryption_hint?: string | null;
}

export interface VaultBatchCryptoResult {
  encrypted_count: number;
  decrypted_count: number;
  unlocked_count: number;
  skipped_locked_count: number;
  total_notes: number;
}

export interface PickedFileAttachmentDto {
  picked: boolean;
  rel_path?: string | null;
  media_url?: string | null;
  file_name?: string | null;
  is_image: boolean;
}

export interface TrashItemDto {
  trash_name: string;
  display_name: string;
  modified_at: string;
  is_dir: boolean;
}

export interface SearchMatchDto {
  line: number;
  snippet: string;
}

export interface SearchHitDto {
  path: string;
  title: string;
  icon?: string | null;
  matches: SearchMatchDto[];
  score: number;
}

export interface GraphNodeDto {
  id: string;
  label: string;
  path?: string | null;
  kind: 'note' | 'ghost' | 'tag';
  group: string;
  degree: number;
  folder?: string;
  icon?: string | null;
  inbound_count?: number;
  outbound_count?: number;
  tags?: string[];
  excerpt?: string;
}

export interface DetectedCloudFolderDto {
  provider: string;
  label: string;
  path: string;
  has_onyx_vault: boolean;
}

export interface GraphEdgeDto {
  source: string;
  target: string;
}

export interface GraphDto {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
}

export interface NoteTemplateDto {
  id: string;
  name: string;
  icon: string;
  description: string;
  body: string;
  properties: NotePropertyDto[];
}

export type ProfileKind = 'obsidian' | 'folder';
export type SyncMode = 'two_way' | 'upload_only' | 'download_only';
export type ConflictPolicy = 'keep_both' | 'prefer_local' | 'prefer_remote';

export type BackendConfig =
  | {
      type: 'synology';
      url: string;
      username: string;
      password: string;
      folder: string;
    }
  | {
      type: 'local_folder';
      path: string;
    }
  | {
      type: 's3';
      endpoint: string;
      region: string;
      bucket: string;
      prefix: string;
      access_key: string;
      secret_key: string;
    }
  | {
      type: 'one_drive';
      client_id: string;
      tenant: string;
      folder: string;
    }
  | {
      type: 'google_drive';
      client_id: string;
      client_secret: string;
      folder: string;
    };

export type BackendKind = BackendConfig['type'];

export interface Profile {
  id: string;
  name: string;
  enabled: boolean;
  kind: ProfileKind;
  local_path: string;
  backend: BackendConfig;
  mode: SyncMode;
  propagate_deletes: boolean;
  delete_in_target: boolean;
  conflict_policy: ConflictPolicy;
  sync_obsidian_folder: boolean;
  ignore_patterns: string[];
}

export interface AppConfig {
  profiles: Profile[];
  poll_interval_secs: number;
  quick_check_secs: number;
  debounce_ms: number;
  watch_local: boolean;
  start_on_launch: boolean;
  device_name: string;
}

export interface LogLine {
  time: string;
  text: string;
  error: boolean;
}

export interface ProfileStatus {
  id: string;
  name: string;
  active: boolean;
  syncing: boolean;
  last_sync: string | null;
  last_summary: string;
  last_error: string | null;
  pending_deletes: number | null;
}

export interface ServiceStatus {
  running: boolean;
  syncing: boolean;
  last_sync: string | null;
  last_summary: string;
  last_error: string | null;
  logs: LogLine[];
  profiles: ProfileStatus[];
}

export interface ProfileExtraDto {
  id: string;
  oauth_logged_in: boolean;
  synology_device_remembered: boolean;
  help_url: string;
  summary: string;
  valid: boolean;
  validation_error: string | null;
}

export interface TestJobDto {
  status: 'running' | 'ok' | 'need_otp' | 'failed';
  message: string;
}

export interface OAuthJobDto {
  status: 'requesting' | 'code' | 'done' | 'failed';
  user_code: string | null;
  uri: string | null;
  message: string | null;
}

export interface NasFolderDto {
  name: string;
  path: string;
}

export interface NasBrowserDto {
  path: string;
  status: 'loading' | 'ready' | 'need_otp' | 'failed';
  folders: NasFolderDto[];
  message: string | null;
}

export interface SyncSnapshotDto {
  version: string;
  cfg: AppConfig;
  saved: AppConfig;
  dirty: boolean;
  autostart: boolean;
  service_running: boolean;
  transitioning: boolean;
  status: ServiceStatus | null;
  profile_extras: ProfileExtraDto[];
  test_job: TestJobDto | null;
  oauth_job: OAuthJobDto | null;
  nas_browser: NasBrowserDto | null;
  message: { text: string; error: boolean } | null;
  default_device_name: string;
}

export interface UpdateStatusDto {
  supported: boolean;
  current_version: string;
  auto_update: boolean;
  checking: boolean;
  downloading: boolean;
  ready_to_install: boolean;
  applying: boolean;
  available_tag: string | null;
  release_url: string | null;
  release_notes: string | null;
  downloaded_bytes: number;
  total_bytes: number;
  last_checked_at: string | null;
  message: string;
  error: string | null;
}

export interface BootstrapDto {
  workspace: WorkspaceState;
  vault_scan: VaultScanDto | null;
  sync: SyncSnapshotDto;
  vault_revision: number;
  update?: UpdateStatusDto;
}

export interface PollDto {
  vault_revision: number;
  sync: SyncSnapshotDto;
  update?: UpdateStatusDto;
}

export type SaveStatus = 'saved' | 'saving' | 'dirty' | 'error';
export type ActiveMainView = 'editor' | 'graph' | 'sync';
export type RightInspectorTab = 'backlinks' | 'outline' | 'local_graph';
export type SyncSubTab = 'home' | 'settings' | 'logs';

export interface AndroidAppInfoDto {
  version: string;
  auto_update: boolean;
  all_files_granted: boolean;
  available_update_tag: string | null;
  last_update_message: string;
}
