import type {
  AppConfig,
  BackendConfig,
  BootstrapDto,
  GraphDto,
  NoteDocumentDto,
  NotePropertyDto,
  NoteTemplateDto,
  PollDto,
  Profile,
  ProfileKind,
  SearchHitDto,
  SyncSnapshotDto,
  TrashItemDto,
  VaultScanDto,
  WorkspaceState,
} from './types';

interface ApiEnvelope<T> {
  ok: boolean;
  data?: T;
  error?: string;
}

export async function rpc<T>(endpoint: string, payload: Record<string, unknown> = {}): Promise<T> {
  const res = await fetch(`/api/${endpoint}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(text || `HTTP ${res.status} (${endpoint})`);
  }

  const json = (await res.json()) as ApiEnvelope<T>;
  if (!json.ok) {
    throw new Error(json.error || `요청 처리 중 오류가 발생했습니다 (${endpoint})`);
  }
  return json.data as T;
}

export const api = {
  bootstrap: () => rpc<BootstrapDto>('bootstrap', {}),

  workspaceSave: (workspace: Partial<WorkspaceState>) =>
    rpc<WorkspaceState>('workspace/save', { workspace }),

  openVaultDialog: () =>
    rpc<{ picked: string | null; workspace: WorkspaceState; vault_scan: VaultScanDto | null }>(
      'workspace/open_vault_dialog',
      {}
    ),

  createSampleVault: () =>
    rpc<{ path: string; workspace: WorkspaceState; vault_scan: VaultScanDto }>(
      'workspace/create_sample_vault',
      {}
    ),

  selectVault: (path: string) =>
    rpc<{ workspace: WorkspaceState; vault_scan: VaultScanDto | null }>('workspace/select_vault', {
      path,
    }),

  removeVault: (path: string) =>
    rpc<{ workspace: WorkspaceState; vault_scan: VaultScanDto | null }>('workspace/remove_vault', {
      path,
    }),

  vaultScan: (vault: string) => rpc<VaultScanDto>('vault/scan', { vault }),

  vaultPoll: (vault: string | null) => rpc<PollDto>('vault/poll', { vault }),

  noteRead: (vault: string, path: string) => rpc<NoteDocumentDto>('note/read', { vault, path }),

  noteSave: (params: {
    vault: string;
    path: string;
    icon: string | null;
    cover: string | null;
    properties: NotePropertyDto[];
    body: string;
  }) => rpc<NoteDocumentDto>('note/save', params),

  noteCreate: (params: {
    vault: string;
    folder: string;
    title: string;
    body?: string;
    icon?: string | null;
    properties?: NotePropertyDto[];
  }) => rpc<{ path: string; doc: NoteDocumentDto; scan: VaultScanDto }>('note/create', params),

  noteDuplicate: (vault: string, path: string) =>
    rpc<{ path: string; scan: VaultScanDto }>('note/duplicate', { vault, path }),

  noteDaily: (vault: string, daily_folder: string) =>
    rpc<{ path: string; doc: NoteDocumentDto; scan: VaultScanDto }>('note/daily', {
      vault,
      daily_folder,
    }),

  createFolder: (vault: string, parent: string, name: string) =>
    rpc<{ path: string; scan: VaultScanDto }>('vault/create_folder', { vault, parent, name }),

  renameItem: (vault: string, path: string, new_name: string) =>
    rpc<{ new_path: string; updated_links: number; scan: VaultScanDto }>('vault/rename', {
      vault,
      path,
      new_name,
    }),

  moveItem: (vault: string, path: string, target_folder: string) =>
    rpc<{ new_path: string; scan: VaultScanDto }>('vault/move', { vault, path, target_folder }),

  deleteItem: (vault: string, path: string) =>
    rpc<{ scan: VaultScanDto }>('vault/delete', { vault, path }),

  trashList: (vault: string) =>
    rpc<{ items: TrashItemDto[] }>('vault/trash_list', { vault }),

  trashRestore: (vault: string, trash_name: string) =>
    rpc<{ restored_path: string; scan: VaultScanDto }>('vault/trash_restore', {
      vault,
      trash_name,
    }),

  trashEmpty: (vault: string) => rpc<Record<string, never>>('vault/trash_empty', { vault }),

  linkMention: (vault: string, source_path: string, target_title: string) =>
    rpc<Record<string, never>>('vault/link_mention', { vault, source_path, target_title }),

  searchVault: (vault: string, query: string) =>
    rpc<{ hits: SearchHitDto[] }>('vault/search', { vault, query }),

  vaultGraph: (params: {
    vault: string;
    center_path?: string | null;
    depth?: number;
    include_tags?: boolean;
  }) => rpc<GraphDto>('vault/graph', params),

  vaultTemplates: (vault: string, templates_folder: string) =>
    rpc<{ templates: NoteTemplateDto[] }>('vault/templates', { vault, templates_folder }),

  uploadAttachment: (vault: string, filename: string, data_base64: string) =>
    rpc<{ rel_path: string; media_url: string }>('vault/upload_attachment', {
      vault,
      filename,
      data_base64,
    }),

  pickAttachment: (vault: string) =>
    rpc<{ picked: boolean; rel_path?: string; media_url?: string; filename?: string }>(
      'vault/pick_attachment',
      { vault }
    ),

  // Sync Center RPCs
  syncState: () => rpc<SyncSnapshotDto>('sync/state', {}),

  syncSaveConfig: (cfg: AppConfig, start_after_save: boolean) =>
    rpc<SyncSnapshotDto>('sync/save_config', { cfg, start_after_save }),

  syncStart: () => rpc<SyncSnapshotDto>('sync/start', {}),

  syncStop: () => rpc<SyncSnapshotDto>('sync/stop', {}),

  syncNow: () => rpc<SyncSnapshotDto>('sync/sync_now', {}),

  syncAllowDeletes: (profile_id: string) =>
    rpc<SyncSnapshotDto>('sync/allow_deletes', { profile_id }),

  syncResetProfile: (profile_id: string) =>
    rpc<SyncSnapshotDto>('sync/reset_profile', { profile_id }),

  syncNewProfile: (kind: ProfileKind, copy_synology_from?: BackendConfig | null) =>
    rpc<{ profile: Profile }>('sync/new_profile', {
      kind,
      copy_synology_from: copy_synology_from ?? null,
    }),

  syncTestConnection: (backend: BackendConfig) =>
    rpc<SyncSnapshotDto>('sync/test_connection', { backend }),

  syncSubmitOtp: (backend: BackendConfig, otp: string) =>
    rpc<SyncSnapshotDto>('sync/submit_otp', { backend, otp }),

  syncForgetDevice: (backend: BackendConfig) =>
    rpc<SyncSnapshotDto>('sync/forget_device', { backend }),

  syncNasBrowseOpen: (backend: BackendConfig, initial_folder: string) =>
    rpc<SyncSnapshotDto>('sync/nas_browse_open', { backend, initial_folder }),

  syncNasBrowseStep: (params: {
    backend: BackendConfig;
    path: string;
    create?: string | null;
    otp?: string | null;
  }) => rpc<SyncSnapshotDto>('sync/nas_browse_step', params),

  syncNasBrowseClose: () => rpc<SyncSnapshotDto>('sync/nas_browse_close', {}),

  syncOauthStart: (backend: BackendConfig) =>
    rpc<SyncSnapshotDto>('sync/oauth_start', { backend }),

  syncOauthCancel: () => rpc<SyncSnapshotDto>('sync/oauth_cancel', {}),

  syncOauthLogout: (backend: BackendConfig) =>
    rpc<SyncSnapshotDto>('sync/oauth_logout', { backend }),

  syncSetAutostart: (enable: boolean) =>
    rpc<SyncSnapshotDto>('sync/set_autostart', { enable }),

  syncPickFolder: () => rpc<{ path: string | null }>('sync/pick_folder', {}),

  syncOpenPath: (path: string) => rpc<Record<string, never>>('sync/open_path', { path }),

  syncOpenDataDir: () => rpc<Record<string, never>>('sync/open_data_dir', {}),

  syncClearMessage: () => rpc<SyncSnapshotDto>('sync/clear_message', {}),
};

function safeEncodeUrlComponent(val: string): string {
  return encodeURIComponent(val).replace(/\(/g, '%28').replace(/\)/g, '%29');
}

export function buildVaultMediaUrl(vault: string, relPath: string): string {
  const normalized = relPath
    .trim()
    .replace(/\\/g, '/')
    .replace(/^\.\//, '')
    .replace(/^\/+/, '');
  return `/vault-media?vault=${safeEncodeUrlComponent(vault)}&path=${safeEncodeUrlComponent(normalized)}`;
}

const IMAGE_EXT_RE = /\.(png|jpe?g|gif|webp|svg|bmp|avif|ico)$/i;

/**
 * Converts vault-relative Markdown image links (`![alt](attachments/foo.png)`)
 * and Obsidian WikiLink image embeds (`![[foo.png]]`) into `/vault-media?vault=...&path=...`
 * so TOAST UI Editor renders images seamlessly in both WYSIWYG and Markdown Preview modes.
 */
export function hydrateMarkdownMediaUrls(markdown: string, vault: string | null): string {
  if (!vault || !markdown) return markdown;

  // 1. Convert Obsidian image embeds `![[path|alt]]` where path has an image extension
  let result = markdown.replace(/!\[\[([^\]]+)\]\]/g, (full, inner: string) => {
    const [rawTarget, rawAlt] = inner.split('|');
    const target = (rawTarget || '').trim();
    if (!IMAGE_EXT_RE.test(target)) {
      return full;
    }
    const relPath = target.includes('/') ? target : `attachments/${target}`;
    const alt = (rawAlt || target.split('/').pop() || 'image').trim();
    return `![${alt}](${buildVaultMediaUrl(vault, relPath)})`;
  });

  // 2. Convert standard Markdown relative image URLs `![alt](rel/path.png)`
  result = result.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (full, alt: string, rawTarget: string) => {
    const trimmed = rawTarget.trim();
    // Handle optional title e.g. `attachments/a.png "title"`
    let urlPart = trimmed;
    let titleSuffix = '';
    const quoteIdx = trimmed.search(/\s+["']/);
    if (quoteIdx > 0) {
      urlPart = trimmed.slice(0, quoteIdx).trim();
      titleSuffix = trimmed.slice(quoteIdx);
    }
    if (
      /^(https?:\/\/|data:|blob:|\/vault-media\b|mailto:)/i.test(urlPart) ||
      urlPart.startsWith('#')
    ) {
      return full;
    }
    let decoded = urlPart;
    try {
      decoded = decodeURIComponent(urlPart);
    } catch {
      decoded = urlPart;
    }
    return `![${alt}](${buildVaultMediaUrl(vault, decoded)}${titleSuffix})`;
  });

  return result;
}

/**
 * Converts `/vault-media?vault=...&path=...` URLs emitted by TOAST UI Editor back into
 * clean, portable Obsidian Markdown relative paths (`attachments/foo.png`) before saving to disk.
 */
export function dehydrateMarkdownMediaUrls(markdown: string): string {
  if (!markdown) return markdown;

  const replaceMediaUrl = (rawUrl: string): string => {
    const idx = rawUrl.indexOf('/vault-media?');
    if (idx === -1) return rawUrl;
    const queryStr = rawUrl.slice(idx + '/vault-media?'.length);
    const params = new URLSearchParams(queryStr);
    const relPath = params.get('path');
    if (!relPath) return rawUrl;
    const clean = relPath.replace(/\\/g, '/').replace(/^\/+/, '');
    // Encode spaces so CommonMark parsers (Obsidian, GitHub, TOAST UI) parse filenames with spaces cleanly
    return clean
      .split('/')
      .map((seg) => seg.replace(/ /g, '%20').replace(/\(/g, '%28').replace(/\)/g, '%29'))
      .join('/');
  };

  // Replace in Markdown image syntax `![alt](/vault-media?...)`
  let out = markdown.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (full, alt: string, rawTarget: string) => {
    if (!rawTarget.includes('/vault-media?')) return full;
    const trimmed = rawTarget.trim();
    let urlPart = trimmed;
    let titleSuffix = '';
    const quoteIdx = trimmed.search(/\s+["']/);
    if (quoteIdx > 0) {
      urlPart = trimmed.slice(0, quoteIdx).trim();
      titleSuffix = trimmed.slice(quoteIdx);
    }
    return `![${alt}](${replaceMediaUrl(urlPart)}${titleSuffix})`;
  });

  // Also replace any HTML `<img src="...vault-media?...">` if present
  out = out.replace(/(<img\b[^>]*?\bsrc=["'])([^"']*\/vault-media\?[^"']*)(["'])/gi, (_m, p1, url, p3) => {
    return `${p1}${replaceMediaUrl(url)}${p3}`;
  });

  return out;
}

export function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const res = String(reader.result || '');
      const comma = res.indexOf(',');
      resolve(comma >= 0 ? res.slice(comma + 1) : res);
    };
    reader.onerror = () => reject(reader.error || new Error('파일을 읽지 못했습니다'));
    reader.readAsDataURL(blob);
  });
}
