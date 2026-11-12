export interface VaultRef {
  path: string;
  name: string;
  is_sync_profile: boolean;
  profile_id: string | null;
}

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
}

export interface TreeNodeDto {
  name: string;
  path: string;
  kind: 'folder' | 'note';
  icon: string | null;
  children: TreeNodeDto[];
  modified: number;
}

export interface NoteMetaDto {
  path: string;
  title: string;
  icon: string | null;
  tags: string[];
  updated_at: string;
  word_count: number;
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

export interface BootstrapDto {
  workspace: WorkspaceState;
  vault_scan: VaultScanDto | null;
  sync: SyncSnapshotDto;
  vault_revision: number;
}

export interface PollDto {
  vault_revision: number;
  sync: SyncSnapshotDto;
}

export type SaveStatus = 'saved' | 'saving' | 'dirty' | 'error';
export type ActiveMainView = 'editor' | 'graph' | 'sync';
export type RightInspectorTab = 'backlinks' | 'outline' | 'local_graph';
export type SyncSubTab = 'home' | 'settings' | 'logs';
