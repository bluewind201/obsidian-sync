import { api, isAndroidApp } from './api';
import type {
  AndroidAppInfoDto,
  AppConfig,
  BackendConfig,
  DetectedCloudFolderDto,
  BackendKind,
  ConflictPolicy,
  Profile,
  ProfileExtraDto,
  ProfileKind,
  SyncMode,
  SyncSnapshotDto,
  SyncSubTab,
} from './types';

const SETUP_DOC = 'https://github.com/bluewind201/obsidian-sync/blob/main/docs/SETUP.md';

interface BackendCardMeta {
  kind: BackendKind;
  badge: string;
  color: string;
  name: string;
  desc: string;
}

const BACKEND_CARDS: BackendCardMeta[] = [
  {
    kind: 'synology',
    badge: 'NAS',
    color: '#285AAA',
    name: 'Synology NAS',
    desc: 'DSM 계정으로 로그인 (File Station)',
  },
  {
    kind: 'local_folder',
    badge: '폴더',
    color: '#6E7280',
    name: '폴더 / 공유 드라이브',
    desc: 'NAS 공유폴더(SMB), 외장 디스크 등',
  },
  {
    kind: 'one_drive',
    badge: 'OD',
    color: '#0078D4',
    name: 'OneDrive',
    desc: 'Microsoft 계정으로 로그인',
  },
  {
    kind: 'google_drive',
    badge: 'GD',
    color: '#1E8E3E',
    name: 'Google Drive',
    desc: 'Google 계정으로 로그인',
  },
  {
    kind: 's3',
    badge: 'S3',
    color: '#03C75A',
    name: '네이버 클라우드 / GCS',
    desc: 'S3 호환 스토리지 (액세스 키 필요)',
  },
];

function createDefaultBackend(kind: BackendKind): BackendConfig {
  switch (kind) {
    case 's3':
      return {
        type: 's3',
        endpoint: 'https://kr.object.ncloudstorage.com',
        region: 'kr-standard',
        bucket: '',
        prefix: 'obsidian',
        access_key: '',
        secret_key: '',
      };
    case 'one_drive':
      return {
        type: 'one_drive',
        client_id: '',
        tenant: 'common',
        folder: 'OnyxVault',
      };
    case 'google_drive':
      return {
        type: 'google_drive',
        client_id: '',
        client_secret: '',
        folder: 'OnyxVault',
      };
    case 'local_folder':
      return {
        type: 'local_folder',
        path: '',
      };
    case 'synology':
    default:
      return {
        type: 'synology',
        url: '',
        username: '',
        password: '',
        folder: '/home',
      };
  }
}

function backendHelpUrl(b: BackendConfig): string {
  switch (b.type) {
    case 'synology':
      return `${SETUP_DOC}#synology-nas`;
    case 'local_folder':
      return `${SETUP_DOC}#폴더--공유-드라이브`;
    case 's3':
      return `${SETUP_DOC}#네이버-클라우드--google-cloud-storage`;
    case 'one_drive':
      return `${SETUP_DOC}#onedrive`;
    case 'google_drive':
      return `${SETUP_DOC}#google-drive`;
  }
}

function summarizeBackend(b: BackendConfig): string {
  switch (b.type) {
    case 'synology': {
      const host = (b.url.split('://')[1] || b.url).replace(/\/+$/, '');
      return `Synology · ${host || '(주소 미입력)'} ${b.folder}`;
    }
    case 'local_folder':
      return `폴더 · ${b.path || '(미선택)'}`;
    case 's3':
      return `S3 · ${b.endpoint} / ${b.bucket}/${b.prefix}`;
    case 'one_drive':
      return `OneDrive · /${b.folder}`;
    case 'google_drive':
      return `Google Drive · /${b.folder}`;
  }
}

function profileDisplayName(p: Profile): string {
  if (p.name.trim()) return p.name.trim();
  const trimmed = p.local_path.trim().replace(/[/\\]+$/, '');
  const last = trimmed.split(/[/\\]/).pop() || '';
  return last || '이름 없음';
}

function syncModeLabel(m: SyncMode): string {
  switch (m) {
    case 'two_way':
      return '양방향 동기화';
    case 'upload_only':
      return '올리기만 (백업)';
    case 'download_only':
      return '내려받기만';
  }
}

function isBackendValid(b: BackendConfig): boolean {
  switch (b.type) {
    case 'synology':
      return Boolean(
        b.url.trim() &&
          /^https?:\/\//i.test(b.url.trim()) &&
          b.username.trim() &&
          b.password.trim() &&
          b.folder.trim().startsWith('/') &&
          b.folder.trim().replace(/^\/+|\/+$/g, '').length > 0
      );
    case 'local_folder':
      return Boolean(b.path.trim());
    case 's3':
      return Boolean(
        b.endpoint.trim() &&
          b.region.trim() &&
          b.bucket.trim() &&
          b.access_key.trim() &&
          b.secret_key.trim()
      );
    case 'one_drive':
      return Boolean(b.client_id.trim() && b.folder.trim());
    case 'google_drive':
      return Boolean(b.client_id.trim() && b.client_secret.trim() && b.folder.trim());
  }
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(s: string): string {
  return escapeHtml(s).replace(/'/g, '&#39;');
}

function deepClone<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj)) as T;
}

export interface SyncCenterCallbacks {
  onSnapshotUpdated: (snap: SyncSnapshotDto) => void;
  onOpenVaultInEditor: (vaultPath: string) => void;
  onGetActiveVault?: () => string | null;
  onToast: (msg: string, isError?: boolean) => void;
}

export class SyncCenterController {
  private container: HTMLElement;
  private callbacks: SyncCenterCallbacks;
  private snapshot: SyncSnapshotDto;
  private draftCfg: AppConfig;
  private ignoreTexts = new Map<string, string>();
  private backendCache = new Map<string, BackendConfig>();

  private subTab: SyncSubTab = 'home';
  private selectedIdx = 0;
  private showSecret = false;
  private confirmDeleteProfile = false;
  private confirmDisconnectHomeId: string | null = null;
  private advancedOpen = false;
  private pickerBusy = false;
  private detectedClouds: DetectedCloudFolderDto[] = [];
  private cloudsLoaded = false;
  private oauthApiExpanded = false;
  private s3BucketsCache = new Map<string, string[]>();
  private otpInputValue = '';
  private nasNewFolderName = '';
  private nasOtpValue = '';
  private nasModalEl: HTMLElement | null = null;
  private lastLiveSignature = '';
  private androidInfo: AndroidAppInfoDto | null = null;

  constructor(
    container: HTMLElement,
    initialSnapshot: SyncSnapshotDto,
    callbacks: SyncCenterCallbacks
  ) {
    this.container = container;
    this.snapshot = initialSnapshot;
    this.draftCfg = deepClone(initialSnapshot.cfg);
    this.callbacks = callbacks;
    this.syncIgnoreMapFromDraft();
    if (this.draftCfg.profiles.length === 0) {
      this.subTab = 'settings';
    }
  }

  private syncIgnoreMapFromDraft(): void {
    for (const p of this.draftCfg.profiles) {
      if (!this.ignoreTexts.has(p.id)) {
        this.ignoreTexts.set(p.id, (p.ignore_patterns || []).join('\n'));
      }
    }
  }

  private isDraftDirty(): boolean {
    return (
      this.snapshot.dirty ||
      JSON.stringify(this.draftCfg) !== JSON.stringify(this.snapshot.saved)
    );
  }

  public setSubTab(tab: SyncSubTab, profileIdx?: number): void {
    this.subTab = tab;
    if (typeof profileIdx === 'number') {
      this.selectedIdx = profileIdx;
      this.confirmDeleteProfile = false;
    }
    this.renderFull();
  }

  /**
   * Called whenever background polling or an RPC returns a fresh SyncSnapshotDto.
   * Updates live status widgets without clobbering focused text inputs in Settings!
   */
  public receiveSnapshot(snap: SyncSnapshotDto, forceSyncDraft = false): void {
    const wasDirty = this.isDraftDirty();
    this.snapshot = snap;
    this.callbacks.onSnapshotUpdated(snap);

    // 설정 화면에서 사용자가 폴더를 고르거나 편집 중일 때는 백그라운드 폴링이 draftCfg를 덮어쓰지 못하게 보호
    if (forceSyncDraft || (!wasDirty && !this.pickerBusy && this.subTab !== 'settings')) {
      this.draftCfg = deepClone(snap.cfg);
      this.ignoreTexts.clear();
      this.syncIgnoreMapFromDraft();
    }

    const sig = JSON.stringify({
      running: snap.service_running,
      transitioning: snap.transitioning,
      status: snap.status,
      extras: snap.profile_extras,
      test: snap.test_job,
      oauth: snap.oauth_job,
      nas: snap.nas_browser,
      msg: snap.message,
      autostart: snap.autostart,
    });

    if (forceSyncDraft || ((this.subTab === 'home' || this.subTab === 'logs') && !this.pickerBusy)) {
      this.lastLiveSignature = sig;
      this.renderFull();
    } else if (sig !== this.lastLiveSignature) {
      this.lastLiveSignature = sig;
      this.refreshSettingsLiveZones();
    }

    this.syncNasBrowserModal();
  }

  public renderFull(): void {
    this.container.innerHTML = '';
    const screen = document.createElement('div');
    screen.className = 'sync-screen';

    const wrap = document.createElement('div');
    wrap.className = 'sync-container';
    screen.appendChild(wrap);

    // Header
    const header = document.createElement('div');
    header.className = 'sync-header';
    header.innerHTML = `
      <div>
        <div style="display:flex;align-items:center;gap:10px;">
          <span style="font-size:22px;font-weight:800;">🔄 Onyx 동기화 센터</span>
          <span class="pill-badge">v${escapeHtml(this.snapshot.version || '0.8.0')}</span>
        </div>
        <div style="font-size:13px;color:var(--text-secondary);margin-top:2px;">
          내 Obsidian 볼트를 Synology NAS, 클라우드 드라이브(OneDrive · Google Drive · 네이버 MYBOX/클라우드), 공유 폴더와 실시간 동기화합니다.
        </div>
      </div>
      <div style="display:flex;gap:6px;">
        <button type="button" class="btn btn-sm" data-help-doc>❓ 설정 도움말</button>
      </div>
    `;
    header.querySelector('[data-help-doc]')?.addEventListener('click', () => {
      void api.syncOpenPath(SETUP_DOC);
    });
    wrap.appendChild(header);

    // Message banner slot
    const msgSlot = document.createElement('div');
    msgSlot.id = 'sync-msg-slot';
    wrap.appendChild(msgSlot);

    // Sub-tabs bar
    const tabsBar = document.createElement('div');
    tabsBar.className = 'sync-subtabs';
    const tabs: { id: SyncSubTab; label: string }[] = [
      { id: 'home', label: '🏠 홈 (상태 요약)' },
      { id: 'settings', label: '⚙️ 프로필 및 저장소 설정' },
      { id: 'logs', label: '📜 동기화 기록 (로그)' },
    ];
    for (const t of tabs) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = `sync-subtab${this.subTab === t.id ? ' active' : ''}`;
      b.textContent = t.label;
      b.addEventListener('click', () => {
        this.subTab = t.id;
        this.renderFull();
      });
      tabsBar.appendChild(b);
    }
    wrap.appendChild(tabsBar);

    const content = document.createElement('div');
    wrap.appendChild(content);

    if (this.subTab === 'home') {
      this.renderHomeTab(content);
    } else if (this.subTab === 'settings') {
      this.renderSettingsTab(content);
    } else {
      this.renderLogsTab(content);
    }

    this.container.appendChild(screen);
    this.renderMessageBanner();
    this.syncNasBrowserModal();
  }

  private renderMessageBanner(): void {
    const slot = this.container.querySelector('#sync-msg-slot');
    if (!slot) return;
    const msg = this.snapshot.message;
    if (!msg) {
      slot.innerHTML = '';
      return;
    }
    slot.innerHTML = `
      <div class="sync-card" style="padding:10px 14px;display:flex;align-items:center;justify-content:space-between;gap:10px;border-color:${
        msg.error ? 'var(--danger)' : 'var(--ok)'
      };background:${msg.error ? 'var(--danger-bg)' : 'var(--ok-bg)'};">
        <span style="font-size:13px;font-weight:500;color:${
          msg.error ? 'var(--danger)' : 'var(--ok)'
        };">${escapeHtml(msg.text)}</span>
        <button type="button" class="icon-btn" data-clear-msg title="닫기">✕</button>
      </div>
    `;
    slot.querySelector('[data-clear-msg]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncClearMessage();
        this.receiveSnapshot(snap);
      } catch {
        // ignore
      }
    });
  }

  private renderHomeTab(host: HTMLElement): void {
    const st = this.snapshot.status;
    const running = this.snapshot.service_running;
    const transitioning = this.snapshot.transitioning;

    let heroTitle = '동기화 꺼짐';
    let heroSub = '시작 버튼을 누르면 실시간 동기화가 켜집니다';
    let dotColor = 'var(--text-muted)';

    if (running && st?.syncing) {
      heroTitle = '동기화 중…';
      heroSub = '변경된 파일을 주고받고 있습니다';
      dotColor = 'var(--accent)';
    } else if (running && st?.last_error) {
      heroTitle = '확인이 필요합니다';
      heroSub = st.last_error;
      dotColor = 'var(--danger)';
    } else if (running) {
      heroTitle = '실시간 동기화 켜짐';
      heroSub = st?.last_sync ? `마지막 동기화 ${st.last_sync}` : '첫 동기화를 준비하고 있습니다';
      dotColor = 'var(--ok)';
    } else if (transitioning) {
      heroTitle = '동기화 전환 중…';
      heroSub = '진행 중인 작업을 마친 뒤 요청을 처리합니다';
      dotColor = 'var(--warn)';
    } else if (this.draftCfg.profiles.length === 0) {
      heroTitle = '설정이 필요합니다';
      heroSub = '동기화할 폴더(프로필)를 먼저 추가해 주세요';
      dotColor = 'var(--warn)';
    }

    const pendingDeletes =
      st?.profiles.reduce((acc, p) => acc + (p.pending_deletes || 0), 0) || 0;

    // Hero Card
    const heroCard = document.createElement('div');
    heroCard.className = 'sync-card';
    heroCard.innerHTML = `
      <div class="sync-hero-row">
        <div class="sync-hero-status">
          <span class="sync-hero-dot" style="background:${dotColor};"></span>
          <div>
            <div style="font-size:20px;font-weight:800;">${escapeHtml(heroTitle)}</div>
            <div style="font-size:13px;color:var(--text-secondary);white-space:pre-line;">${escapeHtml(heroSub)}</div>
          </div>
        </div>
        <div style="display:flex;gap:8px;flex-wrap:wrap;" data-hero-actions></div>
      </div>
      <div data-pending-zone></div>
    `;

    const actionsZone = heroCard.querySelector('[data-hero-actions]')!;
    if (!running) {
      const startBtn = document.createElement('button');
      startBtn.type = 'button';
      startBtn.className = 'btn btn-primary';
      startBtn.disabled = transitioning;
      startBtn.textContent = '▶ 동기화 시작';
      startBtn.addEventListener('click', async () => {
        try {
          const snap = this.isDraftDirty()
            ? await api.syncSaveConfig(this.draftCfg, true)
            : await api.syncStart();
          this.receiveSnapshot(snap, true);
          if (!snap.service_running && snap.message?.error) {
            this.subTab = 'settings';
            this.renderFull();
          }
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '동기화 시작 실패', true);
        }
      });
      actionsZone.appendChild(startBtn);
    } else {
      const syncNowBtn = document.createElement('button');
      syncNowBtn.type = 'button';
      syncNowBtn.className = 'btn btn-primary';
      syncNowBtn.disabled = Boolean(st?.syncing);
      syncNowBtn.textContent = '⟳ 지금 동기화';
      syncNowBtn.addEventListener('click', async () => {
        try {
          const snap = await api.syncNow();
          this.receiveSnapshot(snap);
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '동기화 요청 실패', true);
        }
      });

      const pauseBtn = document.createElement('button');
      pauseBtn.type = 'button';
      pauseBtn.className = 'btn';
      pauseBtn.textContent = '⏸ 일시 중지';
      pauseBtn.addEventListener('click', async () => {
        try {
          const snap = await api.syncStop();
          this.receiveSnapshot(snap);
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '중지 요청 실패', true);
        }
      });

      actionsZone.append(syncNowBtn, pauseBtn);
    }

    if (pendingDeletes > 0) {
      const pZone = heroCard.querySelector('[data-pending-zone]')!;
      pZone.innerHTML = `
        <div class="mass-delete-banner">
          <div style="flex:1;min-width:240px;font-size:13px;">
            <strong style="color:var(--warn);">⚠ 대량 삭제를 보류했습니다.</strong><br/>
            모든 프로필의 보류된 삭제 합계(${pendingDeletes}개)입니다. 의도한 삭제인지 확인한 후 진행하세요.
          </div>
          <button type="button" class="btn btn-primary" data-allow-del>삭제 진행 (${pendingDeletes}개)</button>
        </div>
      `;
      pZone.querySelector('[data-allow-del]')?.addEventListener('click', async () => {
        try {
          const snap = await api.syncAllowDeletes('');
          this.receiveSnapshot(snap);
          this.callbacks.onToast('보류된 삭제 작업을 승인했습니다.');
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '삭제 승인 실패', true);
        }
      });
    }
    host.appendChild(heroCard);

    // Profiles Card
    const profCard = document.createElement('div');
    profCard.className = 'sync-card';
    profCard.innerHTML = `
      <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;">
        <span style="font-weight:700;font-size:15px;">연결된 Obsidian 볼트 (${this.draftCfg.profiles.length})</span>
        <div style="display:flex;gap:6px;">
          <button type="button" class="btn btn-sm" data-add-obs>＋ Obsidian 볼트 추가</button>
        </div>
      </div>
      <div data-prof-list></div>
      <div style="margin-top:8px;font-size:12px;color:var(--text-muted);" data-watch-summary></div>
    `;

    profCard.querySelector('[data-add-obs]')?.addEventListener('click', () => {
      void this.handleCreateProfile('obsidian');
    });

    const listEl = profCard.querySelector('[data-prof-list]')!;
    if (this.draftCfg.profiles.length === 0) {
      listEl.innerHTML = `<div style="padding:18px;text-align:center;color:var(--text-muted);">아직 등록된 동기화 프로필이 없습니다. 위의 버튼으로 폴더를 추가해 보세요.</div>`;
    } else {
      this.draftCfg.profiles.forEach((p, idx) => {
        const pst = st?.profiles.find((x) => x.id === p.id);
        const extra = this.snapshot.profile_extras.find((x) => x.id === p.id);
        let dotClass = 'muted';
        let statusLine = '대기';
        if (!p.enabled) {
          dotClass = 'muted';
          statusLine = '꺼짐';
        } else if (pst?.syncing) {
          dotClass = 'syncing';
          statusLine = '동기화 중…';
        } else if (pst?.last_error) {
          dotClass = 'danger';
          statusLine = pst.last_error.split('\n')[0] || pst.last_error;
        } else if (pst) {
          dotClass = 'ok';
          statusLine = pst.last_sync
            ? `${pst.last_sync.slice(5)} · ${pst.last_summary}`
            : '준비 중';
        }

        const item = document.createElement('div');
        item.className = 'profile-item-card';
        const summary = extra?.summary || summarizeBackend(p.backend);
        item.innerHTML = `
          <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;flex-wrap:wrap;">
            <div style="display:flex;align-items:center;gap:8px;">
              <span class="status-dot ${dotClass}"></span>
              <span style="font-weight:700;font-size:14px;">${escapeHtml(profileDisplayName(p))}</span>
              <span style="font-size:12px;color:var(--text-muted);">
                ${'Obsidian'} · ${syncModeLabel(p.mode)}
              </span>
            </div>
            <div style="display:flex;gap:6px;" data-row-btns></div>
          </div>
          <div style="font-size:12.5px;color:var(--text-secondary);margin-top:5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
            ${escapeHtml(p.local_path || '(내 PC 폴더 미선택)')}  ⇄  ${escapeHtml(summary)}
          </div>
          <div style="font-size:12px;margin-top:3px;color:${
            dotClass === 'danger' ? 'var(--danger)' : 'var(--text-muted)'
          };">${escapeHtml(statusLine)}</div>
        `;

        const btns = item.querySelector('[data-row-btns]')!;
        if (p.kind === 'obsidian' && p.local_path.trim()) {
          const openVaultBtn = document.createElement('button');
          openVaultBtn.type = 'button';
          openVaultBtn.className = 'btn btn-sm btn-primary';
          openVaultBtn.textContent = '📝 노트 열기';
          openVaultBtn.addEventListener('click', () => {
            this.callbacks.onOpenVaultInEditor(p.local_path.trim());
          });
          btns.appendChild(openVaultBtn);
        }

        if (p.local_path.trim()) {
          const openDirBtn = document.createElement('button');
          openDirBtn.type = 'button';
          openDirBtn.className = 'btn btn-sm';
          openDirBtn.textContent = '폴더 열기';
          openDirBtn.addEventListener('click', () => {
            void api.syncOpenPath(p.local_path.trim());
          });
          btns.appendChild(openDirBtn);
        }

        const cfgBtn = document.createElement('button');
        cfgBtn.type = 'button';
        cfgBtn.className = 'btn btn-sm';
        cfgBtn.textContent = '변경 / 설정';
        cfgBtn.addEventListener('click', () => {
          this.selectedIdx = idx;
          this.confirmDeleteProfile = false;
          this.subTab = 'settings';
          this.renderFull();
        });
        btns.appendChild(cfgBtn);

        const discBtn = document.createElement('button');
        discBtn.type = 'button';
        discBtn.className = 'btn btn-sm btn-danger';
        discBtn.textContent = '연결 해제';
        discBtn.addEventListener('click', () => {
          this.confirmDisconnectHomeId = this.confirmDisconnectHomeId === p.id ? null : p.id;
          this.renderFull();
        });
        btns.appendChild(discBtn);

        if (this.confirmDisconnectHomeId === p.id) {
          const confirmBar = document.createElement('div');
          confirmBar.style.cssText =
            'margin-top:8px;padding:8px 10px;border-radius:7px;background:var(--danger-bg);display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:6px;';
          confirmBar.innerHTML = `
            <span style="font-size:12px;color:var(--danger);font-weight:600;">동기화 연결만 해제합니다 (로컬 .md 파일은 그대로 유지됩니다).</span>
            <div style="display:flex;gap:6px;">
              <button type="button" class="btn btn-sm btn-danger" data-home-disc-keep>연결만 해제</button>
              <button type="button" class="btn btn-sm btn-danger" data-home-disc-close>연결 해제 + 볼트 닫기</button>
              <button type="button" class="btn btn-sm" data-home-disc-cancel>취소</button>
            </div>
          `;
          confirmBar.querySelector('[data-home-disc-keep]')?.addEventListener('click', async () => {
            try {
              const res = await api.syncDisconnectProfile(p.id, false);
              this.confirmDisconnectHomeId = null;
              this.receiveSnapshot(res.sync, true);
              this.callbacks.onToast('동기화 연결을 해제했습니다.');
            } catch (err: any) {
              this.callbacks.onToast(err?.message || '연결 해제 실패', true);
            }
          });
          confirmBar.querySelector('[data-home-disc-close]')?.addEventListener('click', async () => {
            try {
              const res = await api.syncDisconnectProfile(p.id, true);
              this.confirmDisconnectHomeId = null;
              this.receiveSnapshot(res.sync, true);
              this.callbacks.onToast('동기화 연결을 해제하고 볼트 목록에서도 닫았습니다.');
            } catch (err: any) {
              this.callbacks.onToast(err?.message || '연결 해제 실패', true);
            }
          });
          confirmBar.querySelector('[data-home-disc-cancel]')?.addEventListener('click', () => {
            this.confirmDisconnectHomeId = null;
            this.renderFull();
          });
          item.appendChild(confirmBar);
        }

        listEl.appendChild(item);
      });
    }

    const watchSummary = profCard.querySelector('[data-watch-summary]')!;
    if (this.draftCfg.watch_local) {
      watchSummary.textContent =
        this.draftCfg.quick_check_secs > 0
          ? `저장 즉시 업로드 · 다른 기기 변경 ${Math.max(2, this.draftCfg.quick_check_secs)}초마다 확인`
          : `저장 즉시 업로드 · ${this.draftCfg.poll_interval_secs}초마다 원격 확인`;
    } else {
      watchSummary.textContent = `${this.draftCfg.poll_interval_secs}초마다 확인`;
    }
    host.appendChild(profCard);

    // Recent 10 Logs Card
    const logsCard = document.createElement('div');
    logsCard.className = 'sync-card';
    const recentLogs = (st?.logs || []).slice(-10).reverse();
    logsCard.innerHTML = `
      <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
        <span style="font-weight:700;font-size:15px;">최근 활동</span>
        <button type="button" class="btn btn-sm" data-view-all-logs>전체 기록 보기</button>
      </div>
      <div class="log-list-box" data-recent-logs></div>
    `;
    logsCard.querySelector('[data-view-all-logs]')?.addEventListener('click', () => {
      this.subTab = 'logs';
      this.renderFull();
    });
    const rLogsEl = logsCard.querySelector('[data-recent-logs]')!;
    if (recentLogs.length === 0) {
      rLogsEl.innerHTML = `<div style="color:var(--text-muted);font-family:inherit;font-size:13px;">아직 기록이 없습니다.</div>`;
    } else {
      for (const l of recentLogs) {
        const row = document.createElement('div');
        row.className = 'log-row';
        row.innerHTML = `
          <span class="log-time">${escapeHtml(l.time)}</span>
          <span class="log-text${l.error ? ' error' : ''}">${escapeHtml(l.text)}</span>
        `;
        rLogsEl.appendChild(row);
      }
    }
    host.appendChild(logsCard);

    const hint = document.createElement('div');
    hint.style.cssText = 'font-size:12px;color:var(--text-muted);padding:2px 4px;';
    hint.textContent =
      '충돌이 나면 두 버전을 모두 남깁니다 (예: 노트 (충돌 PC 2025-01-01 120000).md). 다른 곳에서 지워진 파일은 폴더 안 .trash 로 옮겨집니다.';
    host.appendChild(hint);
  }

  private async handleCreateProfile(kind: ProfileKind): Promise<void> {
    try {
      const currentProf = this.draftCfg.profiles[this.selectedIdx];
      const copyFrom =
        currentProf?.backend.type === 'synology' ? currentProf.backend : null;
      const res = await api.syncNewProfile(kind, copyFrom);
      this.draftCfg.profiles.push(res.profile);
      this.ignoreTexts.set(res.profile.id, (res.profile.ignore_patterns || []).join('\n'));
      this.selectedIdx = this.draftCfg.profiles.length - 1;
      this.confirmDeleteProfile = false;
      this.subTab = 'settings';
      this.renderFull();
    } catch (err: any) {
      this.callbacks.onToast(err?.message || '프로필 추가 실패', true);
    }
  }

  private renderSettingsTab(host: HTMLElement): void {
    // 1. Profile Selector Card
    const selectorCard = document.createElement('div');
    selectorCard.className = 'sync-card';
    selectorCard.innerHTML = `
      <div style="font-weight:700;font-size:15px;">Obsidian 볼트 동기화 프로필</div>
      <div style="font-size:12.5px;color:var(--text-secondary);margin-top:2px;">
        프로필 하나가 '내 Obsidian 볼트 폴더 ↔ 저장소 폴더' 1:1 연결입니다. 폴더 변경 또는 연결 해제를 자유롭게 수행할 수 있습니다.
      </div>
      <div class="profile-pills-bar" data-pills></div>
    `;
    const pillsBar = selectorCard.querySelector('[data-pills]')!;
    this.draftCfg.profiles.forEach((p, idx) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `profile-pill-btn${idx === this.selectedIdx ? ' active' : ''}`;
      if (!p.enabled) btn.style.opacity = '0.6';
      btn.textContent = profileDisplayName(p);
      btn.addEventListener('click', () => {
        if (this.selectedIdx !== idx) {
          this.selectedIdx = idx;
          this.confirmDeleteProfile = false;
          this.otpInputValue = '';
          this.renderFull();
        }
      });
      pillsBar.appendChild(btn);
    });

    const refreshClouds = () => {
      if (this.cloudsLoaded) return;
      this.cloudsLoaded = true;
      void api
        .syncDetectCloudFolders()
        .then((res) => {
          this.detectedClouds = res.folders || [];
          if (this.subTab === 'settings' && !this.pickerBusy) {
            const fieldsZone = this.container.querySelector('[data-backend-fields]') as HTMLElement | null;
            const prof = this.draftCfg.profiles[this.selectedIdx];
            if (fieldsZone && prof) {
              this.populateBackendFields(fieldsZone, prof);
            }
          }
        })
        .catch(() => {});
    };
    refreshClouds();

    const addObsBtn = document.createElement('button');
    addObsBtn.type = 'button';
    addObsBtn.className = 'btn btn-sm';
    addObsBtn.textContent = '＋ Obsidian 볼트 추가';
    addObsBtn.addEventListener('click', () => void this.handleCreateProfile('obsidian'));

    pillsBar.append(addObsBtn);
    host.appendChild(selectorCard);

    if (this.draftCfg.profiles.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'sync-card';
      empty.textContent = "위의 '＋' 버튼으로 동기화할 폴더를 추가하세요.";
      host.appendChild(empty);
    } else {
      this.selectedIdx = Math.min(this.selectedIdx, this.draftCfg.profiles.length - 1);
      this.renderProfileSteps(host, this.draftCfg.profiles[this.selectedIdx]);
    }

    // Save Bar
    const saveCard = document.createElement('div');
    saveCard.className = 'sync-card';
    saveCard.id = 'sync-save-bar-zone';
    host.appendChild(saveCard);
    this.populateSaveBar(saveCard);

    // Collapsible Advanced Settings
    const advCard = document.createElement('div');
    advCard.className = 'sync-card';
    advCard.innerHTML = `
      <button type="button" class="btn" style="width:100%;justify-content:space-between;border:none;padding:4px 2px;font-weight:700;font-size:14px;" data-toggle-adv>
        <span>⚙ 고급 설정 (모든 프로필 공통)</span>
        <span>${this.advancedOpen ? '▲' : '▼'}</span>
      </button>
      <div data-adv-body style="display:${this.advancedOpen ? 'block' : 'none'};margin-top:14px;"></div>
    `;
    advCard.querySelector('[data-toggle-adv]')?.addEventListener('click', () => {
      this.advancedOpen = !this.advancedOpen;
      this.renderFull();
    });
    if (this.advancedOpen) {
      this.populateAdvancedSettings(advCard.querySelector('[data-adv-body]')!);
    }
    host.appendChild(advCard);
  }

  private renderProfileSteps(host: HTMLElement, p: Profile): void {
    const extra: ProfileExtraDto | undefined = this.snapshot.profile_extras.find(
      (x) => x.id === p.id
    );

    p.kind = 'obsidian';
    // ① 이름
    const step1 = document.createElement('div');
    step1.className = 'sync-card';
    const step1Done = Boolean(p.name.trim());
    step1.innerHTML = `
      <div class="step-header">
        <span class="step-num ${step1Done ? 'done' : ''}">${step1Done ? '✓' : '1'}</span>
        <span style="font-weight:700;font-size:15px;">Obsidian 볼트 프로필</span>
      </div>
      <div class="form-grid">
        <div class="form-label">볼트 이름</div>
        <div>
          <input type="text" class="text-input" data-p-name placeholder="예: 내 업무 볼트, 개인 노트" value="${escapeAttr(p.name)}" />
        </div>
        <div class="form-label">동기화 대상</div>
        <div style="font-size:12.5px;color:var(--text-secondary);">
          <span class="pill-badge">Obsidian 볼트 (.md · .obsidian)</span>
        </div>
        <div class="form-label">활성화</div>
        <div>
          <label style="display:inline-flex;align-items:center;gap:6px;cursor:pointer;">
            <input type="checkbox" data-p-enabled ${p.enabled ? 'checked' : ''} />
            이 볼트 실시간 동기화 사용
          </label>
        </div>
      </div>
    `;
    const nameInput = step1.querySelector('[data-p-name]') as HTMLInputElement;
    nameInput.addEventListener('input', () => {
      p.name = nameInput.value;
      this.refreshSaveBarOnly();
    });
    const enabledChk = step1.querySelector('[data-p-enabled]') as HTMLInputElement;
    enabledChk.addEventListener('change', () => {
      p.enabled = enabledChk.checked;
      this.refreshSaveBarOnly();
    });
    host.appendChild(step1);

    // ② 내 PC / 기기 볼트 폴더
    const step2 = document.createElement('div');
    step2.className = 'sync-card';
    const step2Done = Boolean(p.local_path.trim());
    const savedProf = this.snapshot.saved.profiles.find((sp) => sp.id === p.id);
    const activeVault = this.callbacks.onGetActiveVault?.() || null;
    const folderChanged = Boolean(
      savedProf &&
        savedProf.local_path.trim() &&
        savedProf.local_path.trim() !== p.local_path.trim()
    );
    step2.innerHTML = `
      <div class="step-header">
        <span class="step-num ${step2Done ? 'done' : ''}">${step2Done ? '✓' : '2'}</span>
        <span style="font-weight:700;font-size:15px;">내 PC 볼트 폴더</span>
      </div>
      <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:8px;">
        동기화할 Obsidian 볼트 폴더를 고르세요. <code>.obsidian</code> 폴더가 들어 있거나 새 기기라면 빈 폴더를 선택해도 됩니다.
      </div>
      <div style="display:flex;gap:8px;flex-wrap:wrap;">
        <input type="text" class="text-input" style="flex:1;min-width:220px;" data-local-path placeholder="C:\\Users\\나\\Documents\\OnyxVault" value="${escapeAttr(p.local_path)}" />
        <button type="button" class="btn" data-pick-local>📂 볼트 폴더 변경…</button>
        ${
          activeVault && activeVault.trim() !== p.local_path.trim()
            ? `<button type="button" class="btn btn-sm" data-use-active-vault>📌 현재 열린 볼트 지정</button>`
            : ''
        }
      </div>
      ${
        folderChanged
          ? `<div style="margin-top:8px;padding:8px 10px;border-radius:7px;background:rgba(35,131,226,0.08);border:1px solid var(--accent);display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;font-size:12px;">
              <span>🔁 폴더가 변경되었습니다 (이전: <code>${escapeHtml(savedProf!.local_path)}</code>)</span>
              <button type="button" class="btn btn-sm btn-primary" data-apply-folder-now>💾 폴더 변경 지금 적용</button>
            </div>`
          : ''
      }
    `;
    const localInput = step2.querySelector('[data-local-path]') as HTMLInputElement;
    localInput.addEventListener('input', () => {
      p.local_path = localInput.value;
      this.refreshSaveBarOnly();
    });
    const profId = p.id;
    step2.querySelector('[data-pick-local]')?.addEventListener('click', async () => {
      this.pickerBusy = true;
      try {
        const res = await api.syncPickFolder();
        if (res.path) {
          const target = this.draftCfg.profiles.find((x) => x.id === profId);
          if (target) {
            target.local_path = res.path;
            if (!target.name.trim() || target.name === '새 폴더' || target.name === 'Obsidian') {
              target.name = res.path.replace(/[/\\]+$/, '').split(/[/\\]/).pop() || target.name;
            }
            this.renderFull();
          }
        }
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '폴더 선택 실패', true);
      } finally {
        this.pickerBusy = false;
      }
    });
    step2.querySelector('[data-use-active-vault]')?.addEventListener('click', () => {
      if (!activeVault) return;
      p.local_path = activeVault;
      if (!p.name.trim() || p.name === '새 폴더') {
        p.name = activeVault.replace(/[/\\]+$/, '').split(/[/\\]/).pop() || p.name;
      }
      this.renderFull();
    });
    step2.querySelector('[data-apply-folder-now]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncSaveConfig(this.draftCfg, true);
        this.receiveSnapshot(snap, true);
        this.callbacks.onToast('새 동기화 폴더로 변경·적용했습니다.');
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '폴더 변경 저장 실패', true);
      }
    });
    host.appendChild(step2);

    // ③ 저장소
    const step3 = document.createElement('div');
    step3.className = 'sync-card';
    const backendOk = isBackendValid(p.backend);
    step3.innerHTML = `
      <div class="step-header">
        <span class="step-num ${backendOk ? 'done' : ''}">${backendOk ? '✓' : '3'}</span>
        <span style="font-weight:700;font-size:15px;">저장소</span>
      </div>
      <div class="backend-grid" data-backend-cards></div>
      <div data-backend-fields></div>
      <div style="margin-top:12px;display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:10px;">
        <label style="display:inline-flex;align-items:center;gap:6px;font-size:12.5px;cursor:pointer;">
          <input type="checkbox" data-show-secret ${this.showSecret ? 'checked' : ''} />
          비밀번호 / 시크릿 키 표시
        </label>
        <button type="button" class="ghost-action-btn" style="color:var(--accent);" data-backend-help>
          ❓ 이 저장소 설정 방법 보기
        </button>
      </div>
    `;

    const cardsGrid = step3.querySelector('[data-backend-cards]')!;
    for (const cardMeta of BACKEND_CARDS) {
      const cardBtn = document.createElement('button');
      cardBtn.type = 'button';
      cardBtn.className = `backend-choice-card${p.backend.type === cardMeta.kind ? ' selected' : ''}`;
      cardBtn.innerHTML = `
        <span class="backend-badge" style="background:${cardMeta.color};">${escapeHtml(cardMeta.badge)}</span>
        <div>
          <div style="font-weight:700;font-size:13.5px;">${escapeHtml(cardMeta.name)}</div>
          <div style="font-size:11.5px;color:var(--text-secondary);margin-top:2px;">${escapeHtml(cardMeta.desc)}</div>
        </div>
      `;
      cardBtn.addEventListener('click', () => {
        if (p.backend.type === cardMeta.kind) return;
        // Cache current backend fields for this profile
        this.backendCache.set(`${p.id}:${p.backend.type}`, deepClone(p.backend));
        const restored =
          this.backendCache.get(`${p.id}:${cardMeta.kind}`) ||
          createDefaultBackend(cardMeta.kind);
        p.backend = deepClone(restored);
        this.renderFull();
      });
      cardsGrid.appendChild(cardBtn);
    }

    this.populateBackendFields(step3.querySelector('[data-backend-fields]')!, p);

    const secretChk = step3.querySelector('[data-show-secret]') as HTMLInputElement;
    secretChk.addEventListener('change', () => {
      this.showSecret = secretChk.checked;
      this.renderFull();
    });
    step3.querySelector('[data-backend-help]')?.addEventListener('click', () => {
      const url = extra?.help_url || backendHelpUrl(p.backend);
      void api.syncOpenPath(url);
    });
    host.appendChild(step3);

    // ④ 연결 확인
    const step4 = document.createElement('div');
    step4.className = 'sync-card';
    step4.id = 'sync-conn-check-zone';
    host.appendChild(step4);
    this.populateConnectionCheckZone(step4, p);

    // ⑤ 동기화 방식
    const step5 = document.createElement('div');
    step5.className = 'sync-card';
    step5.innerHTML = `
      <div class="step-header">
        <span class="step-num done">5</span>
        <span style="font-weight:700;font-size:15px;">동기화 방식</span>
      </div>
      <div style="font-weight:600;font-size:13px;margin-bottom:6px;">동기화 방향</div>
      <div style="display:flex;flex-direction:column;gap:6px;margin-bottom:12px;">
        <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="radio" name="p_mode_${escapeAttr(p.id)}" value="two_way" ${p.mode === 'two_way' ? 'checked' : ''} />
          <span><strong>양방향</strong> — 어느 쪽에서 바꿔도 서로 맞춥니다</span>
        </label>
        <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="radio" name="p_mode_${escapeAttr(p.id)}" value="upload_only" ${p.mode === 'upload_only' ? 'checked' : ''} />
          <span><strong>올리기만 (백업)</strong> — 이 PC의 변경만 저장소로 올립니다</span>
        </label>
        <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="radio" name="p_mode_${escapeAttr(p.id)}" value="download_only" ${p.mode === 'download_only' ? 'checked' : ''} />
          <span><strong>내려받기만</strong> — 저장소의 변경만 이 PC로 받습니다</span>
        </label>
      </div>
      <div data-mode-options></div>
      ${
        p.kind === 'obsidian'
          ? `
        <div style="margin-top:10px;">
          <label style="display:inline-flex;align-items:center;gap:7px;cursor:pointer;">
            <input type="checkbox" data-sync-obs-dir ${p.sync_obsidian_folder ? 'checked' : ''} />
            <code>.obsidian</code> 설정 폴더도 동기화 (플러그인, 테마, 단축키)
          </label>
        </div>
      `
          : ''
      }
      <div style="margin-top:12px;">
        <label style="display:block;font-size:13px;color:var(--text-secondary);margin-bottom:4px;">
          동기화하지 않을 파일/폴더 (한 줄에 하나, 예: <code>Private/**</code>, <code>*.mp4</code>)
        </label>
        <textarea class="textarea-input" rows="3" data-ignore-textarea placeholder="Private/**&#10;*.tmp"></textarea>
      </div>
      <div style="margin-top:14px;padding-top:12px;border-top:1px solid var(--border-subtle);display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
        <button type="button" class="btn btn-sm" data-reset-prof ${this.snapshot.transitioning ? 'disabled' : ''}>
          이 프로필 동기화 기록 초기화
        </button>
        <div style="display:flex;align-items:center;gap:6px;" data-del-prof-zone></div>
      </div>
    `;

    step5.querySelectorAll(`input[name="p_mode_${p.id}"]`).forEach((r) => {
      r.addEventListener('change', (ev) => {
        p.mode = (ev.target as HTMLInputElement).value as SyncMode;
        this.renderFull();
      });
    });

    const modeOpts = step5.querySelector('[data-mode-options]')!;
    if (p.mode === 'two_way') {
      modeOpts.innerHTML = `
        <label style="display:inline-flex;align-items:center;gap:7px;cursor:pointer;margin-bottom:10px;">
          <input type="checkbox" data-prop-del ${p.propagate_deletes ? 'checked' : ''} />
          삭제도 동기화 (지워진 파일은 폴더 안 <code>.trash</code> 로 이동)
        </label>
        <div style="font-weight:600;font-size:13px;margin-bottom:6px;">데이터 기준 (양쪽이 모두 바뀌었을 때)</div>
        <div style="display:flex;flex-direction:column;gap:5px;">
          <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
            <input type="radio" name="p_conflict_${escapeAttr(p.id)}" value="keep_both" ${p.conflict_policy === 'keep_both' ? 'checked' : ''} />
            <span>둘 다 보존 — 이 PC 쪽은 <code>(충돌 …)</code> 사본으로 남깁니다 (권장)</span>
          </label>
          <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
            <input type="radio" name="p_conflict_${escapeAttr(p.id)}" value="prefer_local" ${p.conflict_policy === 'prefer_local' ? 'checked' : ''} />
            <span>이 PC 기준 — 이 PC 내용으로 저장소를 덮어씁니다</span>
          </label>
          <label style="display:flex;align-items:center;gap:8px;cursor:pointer;">
            <input type="radio" name="p_conflict_${escapeAttr(p.id)}" value="prefer_remote" ${p.conflict_policy === 'prefer_remote' ? 'checked' : ''} />
            <span>저장소 기준 — 저장소 내용으로 덮어씁니다 (이전 내용은 .trash 에 보관)</span>
          </label>
        </div>
      `;
      const propChk = modeOpts.querySelector('[data-prop-del]') as HTMLInputElement;
      propChk.addEventListener('change', () => {
        p.propagate_deletes = propChk.checked;
        this.refreshSaveBarOnly();
      });
      modeOpts.querySelectorAll(`input[name="p_conflict_${p.id}"]`).forEach((cr) => {
        cr.addEventListener('change', (ev) => {
          p.conflict_policy = (ev.target as HTMLInputElement).value as ConflictPolicy;
          this.refreshSaveBarOnly();
        });
      });
    } else if (p.mode === 'upload_only') {
      modeOpts.innerHTML = `
        <label style="display:inline-flex;align-items:center;gap:7px;cursor:pointer;">
          <input type="checkbox" data-del-target ${p.delete_in_target ? 'checked' : ''} />
          이 PC에서 지운 파일은 저장소에서도 지우기 (끄면 저장소에 남음)
        </label>
      `;
      const chk = modeOpts.querySelector('[data-del-target]') as HTMLInputElement;
      chk.addEventListener('change', () => {
        p.delete_in_target = chk.checked;
        this.refreshSaveBarOnly();
      });
    } else {
      modeOpts.innerHTML = `
        <label style="display:inline-flex;align-items:center;gap:7px;cursor:pointer;">
          <input type="checkbox" data-del-target ${p.delete_in_target ? 'checked' : ''} />
          저장소에서 지운 파일은 이 PC에서도 지우기 (.trash 로 이동, 끄면 이 PC에 남음)
        </label>
        <div style="font-size:12px;color:var(--text-muted);margin-top:4px;">
          처음 연결할 때 내용이 다른 파일은 저장소 기준으로 맞추고, 이 PC 파일은 .trash 에 보관합니다.
        </div>
      `;
      const chk = modeOpts.querySelector('[data-del-target]') as HTMLInputElement;
      chk.addEventListener('change', () => {
        p.delete_in_target = chk.checked;
        this.refreshSaveBarOnly();
      });
    }

    const syncObsChk = step5.querySelector('[data-sync-obs-dir]') as HTMLInputElement | null;
    syncObsChk?.addEventListener('change', () => {
      p.sync_obsidian_folder = syncObsChk.checked;
      this.refreshSaveBarOnly();
    });

    const ignoreArea = step5.querySelector('[data-ignore-textarea]') as HTMLTextAreaElement;
    ignoreArea.value = this.ignoreTexts.get(p.id) ?? p.ignore_patterns.join('\n');
    ignoreArea.addEventListener('input', () => {
      this.ignoreTexts.set(p.id, ignoreArea.value);
      p.ignore_patterns = ignoreArea.value
        .split('\n')
        .map((l) => l.trim())
        .filter((l) => l.length > 0);
      this.refreshSaveBarOnly();
    });

    step5.querySelector('[data-reset-prof]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncResetProfile(p.id);
        this.receiveSnapshot(snap);
        this.callbacks.onToast('동기화 기록 초기화를 요청했습니다.');
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '초기화 실패', true);
      }
    });

    const delZone = step5.querySelector('[data-del-prof-zone]')!;
    if (!this.confirmDeleteProfile) {
      const delBtn = document.createElement('button');
      delBtn.type = 'button';
      delBtn.className = 'btn btn-sm btn-danger';
      delBtn.textContent = '🔌 이 볼트 연결 해제';
      delBtn.addEventListener('click', () => {
        this.confirmDeleteProfile = true;
        this.renderFull();
      });
      delZone.appendChild(delBtn);
    } else {
      delZone.innerHTML = `
        <span style="font-size:12.5px;color:var(--danger);">동기화 연결을 즉시 해제할까요? (로컬 .md 파일은 삭제되지 않습니다)</span>
        <button type="button" class="btn btn-sm btn-danger" data-confirm-disc-keep>연결만 해제 (볼트 유지)</button>
        <button type="button" class="btn btn-sm btn-danger" data-confirm-disc-close>해제 + 볼트 목록 닫기</button>
        <button type="button" class="btn btn-sm" data-cancel-del>취소</button>
      `;
      delZone.querySelector('[data-confirm-disc-keep]')?.addEventListener('click', async () => {
        try {
          const isSaved = this.snapshot.saved.profiles.some((sp) => sp.id === p.id);
          if (isSaved) {
            const res = await api.syncDisconnectProfile(p.id, false);
            this.confirmDeleteProfile = false;
            this.selectedIdx = Math.max(0, this.selectedIdx - 1);
            this.receiveSnapshot(res.sync, true);
          } else {
            this.draftCfg.profiles.splice(this.selectedIdx, 1);
            this.ignoreTexts.delete(p.id);
            this.confirmDeleteProfile = false;
            this.selectedIdx = Math.max(0, this.selectedIdx - 1);
            this.renderFull();
          }
          this.callbacks.onToast('동기화 연결을 해제했습니다.');
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '연결 해제 실패', true);
        }
      });
      delZone.querySelector('[data-confirm-disc-close]')?.addEventListener('click', async () => {
        try {
          const isSaved = this.snapshot.saved.profiles.some((sp) => sp.id === p.id);
          if (isSaved) {
            const res = await api.syncDisconnectProfile(p.id, true);
            this.confirmDeleteProfile = false;
            this.selectedIdx = Math.max(0, this.selectedIdx - 1);
            this.receiveSnapshot(res.sync, true);
          } else {
            this.draftCfg.profiles.splice(this.selectedIdx, 1);
            this.ignoreTexts.delete(p.id);
            this.confirmDeleteProfile = false;
            this.selectedIdx = Math.max(0, this.selectedIdx - 1);
            this.renderFull();
          }
          this.callbacks.onToast('동기화 연결을 해제하고 볼트 목록에서도 닫았습니다.');
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '연결 해제 실패', true);
        }
      });
      delZone.querySelector('[data-cancel-del]')?.addEventListener('click', () => {
        this.confirmDeleteProfile = false;
        this.renderFull();
      });
    }

    host.appendChild(step5);
  }

  private populateBackendFields(container: HTMLElement, p: Profile): void {
    const pwType = this.showSecret ? 'text' : 'password';
    const b = p.backend;

    if (b.type === 'synology') {
      container.innerHTML = `
        <div class="form-grid">
          <div class="form-label">NAS 주소</div>
          <input type="text" class="text-input" data-syn-url placeholder="https://내NAS주소:5001" value="${escapeAttr(b.url)}" />
          <div class="form-label">아이디</div>
          <input type="text" class="text-input" data-syn-user placeholder="DSM 로그인 아이디" value="${escapeAttr(b.username)}" />
          <div class="form-label">비밀번호</div>
          <input type="${pwType}" class="text-input" data-syn-pw value="${escapeAttr(b.password)}" />
          <div class="form-label">NAS 폴더</div>
          <div style="display:flex;gap:8px;">
            <input type="text" class="text-input" data-syn-folder placeholder="${
              p.kind === 'obsidian' ? '/home/Obsidian' : '/photo/PC 사진'
            }" value="${escapeAttr(b.folder)}" />
            <button type="button" class="btn" data-syn-browse>NAS에서 고르기…</button>
          </div>
          <div></div>
          <div style="font-size:12px;color:var(--text-muted);">
            [NAS에서 고르기…]로 목록에서 고르면 오타가 없습니다. 직접 적을 때는 공유 폴더 이름부터 적습니다 (예: <code>/home/Obsidian</code>).
          </div>
        </div>
      `;
      const urlIn = container.querySelector('[data-syn-url]') as HTMLInputElement;
      const userIn = container.querySelector('[data-syn-user]') as HTMLInputElement;
      const pwIn = container.querySelector('[data-syn-pw]') as HTMLInputElement;
      const folIn = container.querySelector('[data-syn-folder]') as HTMLInputElement;
      const browseBtn = container.querySelector('[data-syn-browse]') as HTMLButtonElement;

      const syncBtnState = () => {
        browseBtn.disabled = !b.url.trim() || !b.username.trim();
        this.refreshSaveBarOnly();
      };
      syncBtnState();

      urlIn.addEventListener('input', () => {
        b.url = urlIn.value;
        syncBtnState();
      });
      userIn.addEventListener('input', () => {
        b.username = userIn.value;
        syncBtnState();
      });
      pwIn.addEventListener('input', () => {
        b.password = pwIn.value;
        syncBtnState();
      });
      folIn.addEventListener('input', () => {
        b.folder = folIn.value;
        syncBtnState();
      });
      browseBtn.addEventListener('click', async () => {
        try {
          const snap = await api.syncNasBrowseOpen(b, b.folder || '/');
          this.receiveSnapshot(snap);
        } catch (err: any) {
          this.callbacks.onToast(err?.message || 'NAS 폴더 탐색기를 열지 못했습니다', true);
        }
      });
    } else if (b.type === 'local_folder') {
      const vaultSubName =
        (profileDisplayName(p) || 'MyVault').replace(/[\\/:*?"<>|]/g, '-').trim() || 'MyVault';
      const cloudChipsHtml =
        this.detectedClouds.length > 0
          ? `<div class="cloud-quick-card" style="margin-bottom:10px;">
              <div style="font-weight:700;font-size:12.5px;">⚡ 감지된 PC 클라우드 / 드라이브 폴더 (API 키 없이 1클릭 연결)</div>
              <div style="display:flex;flex-wrap:wrap;gap:6px;">
                ${this.detectedClouds
                  .map(
                    (c, i) =>
                      `<button type="button" class="btn btn-sm" data-cloud-quick-idx="${i}">☁️ ${escapeHtml(c.label)}</button>`
                  )
                  .join('')}
              </div>
              <div style="font-size:11.5px;color:var(--text-muted);">
                클라우드를 클릭하면 해당 드라이브 안에 <code>OnyxVault/${escapeHtml(vaultSubName)}</code> 폴더를 자동 준비하여 즉시 연결합니다.
              </div>
            </div>`
          : '';
      container.innerHTML = `
        ${cloudChipsHtml}
        <div class="form-grid">
          <div class="form-label">저장 위치</div>
          <div style="display:flex;gap:8px;">
            <input type="text" class="text-input" data-lf-path placeholder="\\\\NAS\\home\\Obsidian 또는 OneDrive\\OnyxVault" value="${escapeAttr(b.path)}" />
            <button type="button" class="btn" data-lf-pick>폴더 선택…</button>
          </div>
        </div>
      `;
      container.querySelectorAll('[data-cloud-quick-idx]').forEach((btn) => {
        btn.addEventListener('click', async () => {
          const idx = Number(btn.getAttribute('data-cloud-quick-idx'));
          const cloud = this.detectedClouds[idx];
          if (!cloud) return;
          try {
            const prep = await api.syncPrepareFolderTarget(
              cloud.path,
              `OnyxVault/${vaultSubName}`
            );
            b.path = prep.path;
            this.callbacks.onToast(`${cloud.label} 동기화 폴더를 준비했습니다: ${prep.path}`);
            this.renderFull();
          } catch (err: any) {
            this.callbacks.onToast(err?.message || '클라우드 폴더 생성 실패', true);
          }
        });
      });
      const pathIn = container.querySelector('[data-lf-path]') as HTMLInputElement;
      pathIn.addEventListener('input', () => {
        b.path = pathIn.value;
        this.refreshSaveBarOnly();
      });
      const profId = p.id;
      container.querySelector('[data-lf-pick]')?.addEventListener('click', async () => {
        this.pickerBusy = true;
        try {
          const res = await api.syncPickFolder();
          if (res.path) {
            const prof = this.draftCfg.profiles.find((x) => x.id === profId);
            if (prof && prof.backend.type === 'local_folder') {
              prof.backend.path = res.path;
              this.renderFull();
            }
          }
        } finally {
          this.pickerBusy = false;
        }
      });
    } else if (b.type === 's3') {
      const vaultSubName =
        (profileDisplayName(p) || 'MyVault').replace(/[\\/:*?"<>|]/g, '-').trim() || 'MyVault';
      const mybox = this.detectedClouds.find((c) => c.provider === 'naver_mybox');
      const cacheKey = `${b.endpoint}|${b.region}|${b.access_key}`;
      const cachedBuckets = this.s3BucketsCache.get(cacheKey) || [];
      container.innerHTML = `
        ${
          mybox
            ? `<div class="cloud-quick-card" style="margin-bottom:10px;border-color:#03c75a;background:rgba(3,199,90,0.08);">
                <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
                  <div>
                    <div style="font-weight:700;font-size:12.5px;">🟢 네이버 MYBOX PC 폴더 발견 (${escapeHtml(mybox.label)})</div>
                    <div style="font-size:11.5px;color:var(--text-secondary);">복잡한 S3 API 키 없이 MYBOX 폴더로 1클릭 자동 동기화할 수 있습니다.</div>
                  </div>
                  <button type="button" class="btn btn-sm btn-primary" data-mybox-quick>⚡ MYBOX 간편 연결</button>
                </div>
              </div>`
            : ''
        }
        <div style="display:flex;gap:6px;margin-bottom:10px;align-items:center;flex-wrap:wrap;">
          <span style="font-size:12px;color:var(--text-muted);">빠른 프리셋:</span>
          <button type="button" class="btn btn-sm" data-preset-ncp>네이버 클라우드 Object Storage</button>
          <button type="button" class="btn btn-sm" data-preset-gcs>Google Cloud Storage</button>
        </div>
        <div class="form-grid">
          <div class="form-label">Access Key</div>
          <input type="text" class="text-input" data-s3-ak placeholder="네이버 클라우드 인증키 관리에서 발급한 Access Key" value="${escapeAttr(b.access_key)}" />
          <div class="form-label">Secret Key</div>
          <input type="${pwType}" class="text-input" data-s3-sk placeholder="Secret Key" value="${escapeAttr(b.secret_key)}" />
          <div class="form-label">버킷 선택 / 생성</div>
          <div style="display:flex;flex-direction:column;gap:6px;">
            <div style="display:flex;gap:6px;flex-wrap:wrap;">
              <input type="text" class="text-input" style="flex:1;min-width:140px;" data-s3-bucket placeholder="my-obsidian-vault" value="${escapeAttr(b.bucket)}" />
              <button type="button" class="btn btn-sm" data-s3-load-buckets>🔄 내 버킷 불러오기</button>
              <button type="button" class="btn btn-sm" data-s3-create-bucket>＋ 새 버킷 생성</button>
            </div>
            ${
              cachedBuckets.length > 0
                ? `<select class="text-input" data-s3-bucket-select>
                    <option value="">-- 조회된 버킷에서 고르기 (${cachedBuckets.length}개) --</option>
                    ${cachedBuckets
                      .map(
                        (bk) =>
                          `<option value="${escapeAttr(bk)}" ${bk === b.bucket ? 'selected' : ''}>🪣 ${escapeHtml(bk)}</option>`
                      )
                      .join('')}
                  </select>`
                : ''
            }
          </div>
          <div class="form-label">엔드포인트</div>
          <input type="text" class="text-input" data-s3-ep placeholder="https://kr.object.ncloudstorage.com" value="${escapeAttr(b.endpoint)}" />
          <div class="form-label">리전</div>
          <input type="text" class="text-input" data-s3-reg placeholder="kr-standard" value="${escapeAttr(b.region)}" />
          <div class="form-label">저장 폴더(Prefix)</div>
          <input type="text" class="text-input" data-s3-prefix placeholder="obsidian" value="${escapeAttr(b.prefix)}" />
        </div>
      `;
      container.querySelector('[data-mybox-quick]')?.addEventListener('click', async () => {
        if (!mybox) return;
        try {
          const prep = await api.syncPrepareFolderTarget(mybox.path, `OnyxVault/${vaultSubName}`);
          const nextBackend: BackendConfig = { type: 'local_folder', path: prep.path };
          p.backend = nextBackend;
          this.backendCache.set(`${p.id}:local_folder`, nextBackend);
          this.callbacks.onToast(`네이버 MYBOX 폴더로 연결했습니다: ${prep.path}`);
          this.renderFull();
        } catch (err: any) {
          this.callbacks.onToast(err?.message || 'MYBOX 연결 실패', true);
        }
      });
      container.querySelector('[data-preset-ncp]')?.addEventListener('click', () => {
        b.endpoint = 'https://kr.object.ncloudstorage.com';
        b.region = 'kr-standard';
        this.renderFull();
      });
      container.querySelector('[data-preset-gcs]')?.addEventListener('click', () => {
        b.endpoint = 'https://storage.googleapis.com';
        b.region = 'auto';
        this.renderFull();
      });
      container.querySelector('[data-s3-load-buckets]')?.addEventListener('click', async () => {
        try {
          const res = await api.syncS3ListBuckets({
            endpoint: b.endpoint,
            region: b.region,
            access_key: b.access_key,
            secret_key: b.secret_key,
          });
          this.s3BucketsCache.set(`${b.endpoint}|${b.region}|${b.access_key}`, res.buckets || []);
          if (!b.bucket && res.buckets.length > 0) {
            b.bucket = res.buckets[0];
          }
          this.callbacks.onToast(`버킷 ${res.buckets.length}개를 불러왔습니다.`);
          this.renderFull();
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '버킷 목록 조회 실패 (키/엔드포인트 확인)', true);
        }
      });
      container.querySelector('[data-s3-create-bucket]')?.addEventListener('click', async () => {
        const wanted =
          b.bucket.trim() ||
          `onyx-vault-${Math.floor(1000 + Math.random() * 9000)}`;
        try {
          const res = await api.syncS3ListBuckets({
            endpoint: b.endpoint,
            region: b.region,
            access_key: b.access_key,
            secret_key: b.secret_key,
            create_bucket: wanted,
          });
          b.bucket = wanted;
          this.s3BucketsCache.set(`${b.endpoint}|${b.region}|${b.access_key}`, res.buckets || []);
          this.callbacks.onToast(`새 버킷 '${wanted}' 생성 및 선택 완료`);
          this.renderFull();
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '새 버킷 생성 실패', true);
        }
      });
      const bkSel = container.querySelector('[data-s3-bucket-select]') as HTMLSelectElement | null;
      bkSel?.addEventListener('change', () => {
        if (bkSel.value) {
          b.bucket = bkSel.value;
          const bIn = container.querySelector('[data-s3-bucket]') as HTMLInputElement;
          if (bIn) bIn.value = b.bucket;
          this.refreshSaveBarOnly();
        }
      });
      const bind = (sel: string, fn: (v: string) => void) => {
        const el = container.querySelector(sel) as HTMLInputElement;
        el.addEventListener('input', () => {
          fn(el.value);
          this.refreshSaveBarOnly();
        });
      };
      bind('[data-s3-ep]', (v) => (b.endpoint = v));
      bind('[data-s3-reg]', (v) => (b.region = v));
      bind('[data-s3-bucket]', (v) => (b.bucket = v));
      bind('[data-s3-prefix]', (v) => (b.prefix = v));
      bind('[data-s3-ak]', (v) => (b.access_key = v));
      bind('[data-s3-sk]', (v) => (b.secret_key = v));
    } else if (b.type === 'one_drive') {
      const vaultSubName =
        (profileDisplayName(p) || 'MyVault').replace(/[\\/:*?"<>|]/g, '-').trim() || 'MyVault';
      const odLocal = this.detectedClouds.find((c) => c.provider === 'one_drive');
      const apiOpen = this.oauthApiExpanded || Boolean(b.client_id.trim());
      container.innerHTML = `
        <div class="cloud-quick-card" style="margin-bottom:10px;">
          <div style="font-weight:700;font-size:13px;">
            ${
              odLocal
                ? `⚡ 간편 연결 (권장) — PC OneDrive 폴더 자동 감지됨`
                : `⚡ 간편 연결 안내 — PC OneDrive 동기화 앱 폴더 사용`
            }
          </div>
          <div style="font-size:12px;color:var(--text-secondary);line-height:1.5;">
            ${
              odLocal
                ? `감지된 폴더(<code>${escapeHtml(odLocal.path)}</code>)에 <code>OnyxVault/${escapeHtml(vaultSubName)}</code>를 자동 연결하면 <strong>Microsoft Azure Client ID 발급 없이</strong> 바로 동기화됩니다.`
                : `PC에 OneDrive 앱이 설치되어 있다면 별도 API 키를 발급받지 않고 <strong>동기화 폴더 지정</strong>만으로 바로 연동할 수 있습니다.`
            }
          </div>
          <div style="display:flex;gap:8px;flex-wrap:wrap;">
            ${
              odLocal
                ? `<button type="button" class="btn btn-sm btn-primary" data-od-easy-connect>⚡ OneDrive 폴더로 1클릭 연결</button>`
                : `<button type="button" class="btn btn-sm btn-primary" data-od-pick-folder>📂 OneDrive 폴더 직접 고르기…</button>`
            }
            <button type="button" class="btn btn-sm" data-toggle-oauth-api>
              ${apiOpen ? '▲ 직접 OAuth API 접기' : '🔐 고급: 직접 OAuth Client ID 입력 (모바일/원격용)'}
            </button>
          </div>
        </div>
        <div data-od-api-fields style="display:${apiOpen ? 'block' : 'none'};">
          <div class="form-grid">
            <div class="form-label">앱 Client ID</div>
            <input type="text" class="text-input" data-od-cid placeholder="Azure 앱 등록에서 받은 애플리케이션 ID" value="${escapeAttr(b.client_id)}" />
            <div class="form-label">테넌트(Tenant)</div>
            <input type="text" class="text-input" data-od-tenant placeholder="common" value="${escapeAttr(b.tenant || 'common')}" />
            <div class="form-label">OneDrive 폴더</div>
            <input type="text" class="text-input" data-od-folder placeholder="OnyxVault" value="${escapeAttr(b.folder)}" />
          </div>
        </div>
      `;
      container.querySelector('[data-od-easy-connect]')?.addEventListener('click', async () => {
        if (!odLocal) return;
        try {
          const prep = await api.syncPrepareFolderTarget(
            odLocal.path,
            `OnyxVault/${vaultSubName}`
          );
          const nextBackend: BackendConfig = { type: 'local_folder', path: prep.path };
          p.backend = nextBackend;
          this.backendCache.set(`${p.id}:local_folder`, nextBackend);
          this.callbacks.onToast(`OneDrive 폴더로 간편 연결했습니다: ${prep.path}`);
          this.renderFull();
        } catch (err: any) {
          this.callbacks.onToast(err?.message || 'OneDrive 간편 연결 실패', true);
        }
      });
      container.querySelector('[data-od-pick-folder]')?.addEventListener('click', async () => {
        this.pickerBusy = true;
        try {
          const res = await api.syncPickFolder();
          if (res.path) {
            const nextBackend: BackendConfig = { type: 'local_folder', path: res.path };
            p.backend = nextBackend;
            this.backendCache.set(`${p.id}:local_folder`, nextBackend);
            this.callbacks.onToast(`폴더 기반 연결로 전환했습니다: ${res.path}`);
            this.renderFull();
          }
        } finally {
          this.pickerBusy = false;
        }
      });
      container.querySelector('[data-toggle-oauth-api]')?.addEventListener('click', () => {
        this.oauthApiExpanded = !this.oauthApiExpanded;
        this.renderFull();
      });
      const cidIn = container.querySelector('[data-od-cid]') as HTMLInputElement;
      const tenIn = container.querySelector('[data-od-tenant]') as HTMLInputElement;
      const folIn = container.querySelector('[data-od-folder]') as HTMLInputElement;
      cidIn.addEventListener('input', () => {
        b.client_id = cidIn.value;
        this.refreshSettingsLiveZones();
      });
      tenIn.addEventListener('input', () => {
        b.tenant = tenIn.value;
        this.refreshSaveBarOnly();
      });
      folIn.addEventListener('input', () => {
        b.folder = folIn.value;
        this.refreshSettingsLiveZones();
      });
    } else if (b.type === 'google_drive') {
      const vaultSubName =
        (profileDisplayName(p) || 'MyVault').replace(/[\\/:*?"<>|]/g, '-').trim() || 'MyVault';
      const gdLocal = this.detectedClouds.find((c) => c.provider === 'google_drive');
      const apiOpen =
        this.oauthApiExpanded || Boolean(b.client_id.trim()) || Boolean(b.client_secret.trim());
      container.innerHTML = `
        <div class="cloud-quick-card" style="margin-bottom:10px;border-color:#1e8e3e;background:rgba(30,142,62,0.07);">
          <div style="font-weight:700;font-size:13px;">
            ${
              gdLocal
                ? `⚡ 간편 연결 (권장) — Google Drive 데스크톱 폴더 자동 감지됨`
                : `⚡ 간편 연결 안내 — Google Drive 데스크톱 폴더 사용`
            }
          </div>
          <div style="font-size:12px;color:var(--text-secondary);line-height:1.5;">
            ${
              gdLocal
                ? `감지된 Google Drive(<code>${escapeHtml(gdLocal.path)}</code>)에 <code>OnyxVault/${escapeHtml(vaultSubName)}</code>를 연결하면 <strong>복잡한 Google Cloud OAuth 키 발급 없이</strong> 바로 동기화됩니다.`
                : `PC에 Google Drive 데스크톱 앱이 있다면 API 키 없이 내 드라이브 폴더 지정만으로 바로 동기화할 수 있습니다.`
            }
          </div>
          <div style="display:flex;gap:8px;flex-wrap:wrap;">
            ${
              gdLocal
                ? `<button type="button" class="btn btn-sm btn-primary" data-gd-easy-connect>⚡ Google Drive 폴더 1클릭 연결</button>`
                : `<button type="button" class="btn btn-sm btn-primary" data-gd-pick-folder>📂 Google Drive 폴더 직접 고르기…</button>`
            }
            <button type="button" class="btn btn-sm" data-toggle-oauth-api>
              ${apiOpen ? '▲ 직접 OAuth API 접기' : '🔐 고급: 직접 OAuth Client ID/Secret (모바일/원격용)'}
            </button>
          </div>
        </div>
        <div data-gd-api-fields style="display:${apiOpen ? 'block' : 'none'};">
          <div class="form-grid">
            <div class="form-label">Client ID</div>
            <input type="text" class="text-input" data-gd-cid placeholder="OAuth 클라이언트 ID (TV 및 입력 제한 기기)" value="${escapeAttr(b.client_id)}" />
            <div class="form-label">Client Secret</div>
            <input type="${pwType}" class="text-input" data-gd-sec value="${escapeAttr(b.client_secret)}" />
            <div class="form-label">Drive 폴더</div>
            <input type="text" class="text-input" data-gd-folder placeholder="OnyxVault" value="${escapeAttr(b.folder)}" />
          </div>
        </div>
      `;
      container.querySelector('[data-gd-easy-connect]')?.addEventListener('click', async () => {
        if (!gdLocal) return;
        try {
          const prep = await api.syncPrepareFolderTarget(
            gdLocal.path,
            `OnyxVault/${vaultSubName}`
          );
          const nextBackend: BackendConfig = { type: 'local_folder', path: prep.path };
          p.backend = nextBackend;
          this.backendCache.set(`${p.id}:local_folder`, nextBackend);
          this.callbacks.onToast(`Google Drive 폴더로 간편 연결했습니다: ${prep.path}`);
          this.renderFull();
        } catch (err: any) {
          this.callbacks.onToast(err?.message || 'Google Drive 간편 연결 실패', true);
        }
      });
      container.querySelector('[data-gd-pick-folder]')?.addEventListener('click', async () => {
        this.pickerBusy = true;
        try {
          const res = await api.syncPickFolder();
          if (res.path) {
            const nextBackend: BackendConfig = { type: 'local_folder', path: res.path };
            p.backend = nextBackend;
            this.backendCache.set(`${p.id}:local_folder`, nextBackend);
            this.callbacks.onToast(`폴더 기반 연결로 전환했습니다: ${res.path}`);
            this.renderFull();
          }
        } finally {
          this.pickerBusy = false;
        }
      });
      container.querySelector('[data-toggle-oauth-api]')?.addEventListener('click', () => {
        this.oauthApiExpanded = !this.oauthApiExpanded;
        this.renderFull();
      });
      const cidIn = container.querySelector('[data-gd-cid]') as HTMLInputElement;
      const secIn = container.querySelector('[data-gd-sec]') as HTMLInputElement;
      const folIn = container.querySelector('[data-gd-folder]') as HTMLInputElement;
      cidIn.addEventListener('input', () => {
        b.client_id = cidIn.value;
        this.refreshSettingsLiveZones();
      });
      secIn.addEventListener('input', () => {
        b.client_secret = secIn.value;
        this.refreshSettingsLiveZones();
      });
      folIn.addEventListener('input', () => {
        b.folder = folIn.value;
        this.refreshSettingsLiveZones();
      });
    }
  }

  private populateConnectionCheckZone(zone: HTMLElement, p: Profile): void {
    const extra = this.snapshot.profile_extras.find((x) => x.id === p.id);
    const needsOauth = p.backend.type === 'one_drive' || p.backend.type === 'google_drive';
    const oauthLoggedIn = !needsOauth || Boolean(extra?.oauth_logged_in);
    const backendOk = isBackendValid(p.backend);
    const testJob = this.snapshot.test_job;
    const oauthJob = this.snapshot.oauth_job;
    const step4Done = oauthLoggedIn && testJob?.status === 'ok';

    zone.innerHTML = `
      <div class="step-header">
        <span class="step-num ${step4Done ? 'done' : ''}">${step4Done ? '✓' : '4'}</span>
        <span style="font-weight:700;font-size:15px;">연결 확인</span>
      </div>
      <div data-oauth-box></div>
      <div data-synology-device-box></div>
      <div style="display:flex;align-items:center;gap:10px;margin-top:8px;">
        <button type="button" class="btn" data-test-conn ${
          !backendOk || !oauthLoggedIn || testJob?.status === 'running' ? 'disabled' : ''
        }>🔌 연결 테스트</button>
        ${
          testJob?.status === 'running'
            ? `<span style="display:inline-flex;align-items:center;gap:6px;font-size:13px;color:var(--text-secondary);"><span class="spinner"></span> 확인하는 중…</span>`
            : ''
        }
      </div>
      <div data-test-result style="margin-top:8px;"></div>
    `;

    // OAuth Device Login Box
    const oauthBox = zone.querySelector('[data-oauth-box]')!;
    if (needsOauth) {
      if (oauthJob && oauthJob.status === 'requesting') {
        oauthBox.innerHTML = `
          <div style="display:flex;align-items:center;gap:8px;padding:8px 0;">
            <span class="spinner"></span> <span>로그인 코드 요청 중…</span>
          </div>
        `;
      } else if (oauthJob && oauthJob.status === 'code') {
        oauthBox.innerHTML = `
          <div class="oauth-code-box">
            <div style="font-size:13px;margin-bottom:6px;">브라우저가 열리면 아래 코드를 입력하고 계정을 승인해 주세요:</div>
            <div style="display:flex;align-items:center;gap:10px;margin-bottom:8px;">
              <span class="oauth-user-code">${escapeHtml(oauthJob.user_code || '')}</span>
              <button type="button" class="btn btn-sm" data-copy-code>📋 복사</button>
            </div>
            ${
              oauthJob.uri
                ? `<div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:8px;">
                    브라우저가 안 열리면: <a href="#" data-open-uri style="color:var(--accent);">${escapeHtml(oauthJob.uri)}</a>
                  </div>`
                : ''
            }
            <div style="display:flex;align-items:center;gap:10px;">
              <span class="spinner"></span>
              <span style="font-size:12.5px;color:var(--text-secondary);">승인을 기다리는 중…</span>
              <button type="button" class="btn btn-sm" data-cancel-oauth>취소</button>
            </div>
          </div>
        `;
        oauthBox.querySelector('[data-copy-code]')?.addEventListener('click', () => {
          if (oauthJob.user_code) {
            void navigator.clipboard.writeText(oauthJob.user_code);
            this.callbacks.onToast('인증 코드를 클립보드에 복사했습니다.');
          }
        });
        oauthBox.querySelector('[data-open-uri]')?.addEventListener('click', (ev) => {
          ev.preventDefault();
          if (oauthJob.uri) void api.syncOpenPath(oauthJob.uri);
        });
        oauthBox.querySelector('[data-cancel-oauth]')?.addEventListener('click', async () => {
          const snap = await api.syncOauthCancel();
          this.receiveSnapshot(snap);
        });
      } else if (oauthLoggedIn) {
        oauthBox.innerHTML = `
          <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px;">
            <span class="status-dot ok"></span>
            <span style="font-weight:600;font-size:13px;">계정 연결됨</span>
            <button type="button" class="btn btn-sm" data-relogin>다시 로그인</button>
            <button type="button" class="btn btn-sm" data-logout>로그아웃</button>
          </div>
        `;
        oauthBox.querySelector('[data-relogin]')?.addEventListener('click', async () => {
          const snap = await api.syncOauthStart(p.backend);
          this.receiveSnapshot(snap);
        });
        oauthBox.querySelector('[data-logout]')?.addEventListener('click', async () => {
          const snap = await api.syncOauthLogout(p.backend);
          this.receiveSnapshot(snap);
        });
      } else {
        oauthBox.innerHTML = `
          <div style="display:flex;align-items:center;gap:10px;margin-bottom:8px;">
            <button type="button" class="btn btn-primary" data-start-oauth ${!backendOk ? 'disabled' : ''}>🔑 로그인</button>
            <span style="font-size:12.5px;color:var(--text-secondary);">
              ${backendOk ? '브라우저에서 계정을 승인합니다' : '위 저장소 항목을 먼저 입력하세요'}
            </span>
          </div>
        `;
        oauthBox.querySelector('[data-start-oauth]')?.addEventListener('click', async () => {
          try {
            const snap = await api.syncOauthStart(p.backend);
            this.receiveSnapshot(snap);
          } catch (err: any) {
            this.callbacks.onToast(err?.message || '로그인 시작 실패', true);
          }
        });
      }
    }

    // Synology remembered 2FA device indicator
    const synBox = zone.querySelector('[data-synology-device-box]')!;
    if (p.backend.type === 'synology' && extra?.synology_device_remembered) {
      synBox.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px;margin-bottom:6px;">
          <span class="status-dot ok"></span>
          <span style="font-size:12.5px;">2단계 인증 완료 (이 PC 기억됨)</span>
          <button type="button" class="btn btn-sm" data-forget-dev>기억 해제</button>
        </div>
      `;
      synBox.querySelector('[data-forget-dev]')?.addEventListener('click', async () => {
        const snap = await api.syncForgetDevice(p.backend);
        this.receiveSnapshot(snap);
      });
    }

    // Test Connection button
    zone.querySelector('[data-test-conn]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncTestConnection(p.backend);
        this.receiveSnapshot(snap);
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '연결 테스트 요청 실패', true);
      }
    });

    // Test Connection Result / OTP Box
    const resBox = zone.querySelector('[data-test-result]')!;
    if (testJob?.status === 'ok') {
      resBox.innerHTML = `<div style="color:var(--ok);font-weight:600;font-size:13px;">✔ ${escapeHtml(testJob.message)}</div>`;
    } else if (testJob?.status === 'failed') {
      resBox.innerHTML = `<div style="color:var(--danger);font-size:13px;white-space:pre-line;">✖ ${escapeHtml(testJob.message)}</div>`;
    } else if (testJob?.status === 'need_otp') {
      resBox.innerHTML = `
        <div class="oauth-code-box">
          <div style="font-weight:700;margin-bottom:4px;">🔐 Synology 2단계 인증 (OTP)</div>
          <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:8px;">
            OTP 앱(Synology Secure SignIn, Google Authenticator 등)의 6자리 코드를 입력하세요. 한 번 인증하면 이 PC를 기억합니다.
          </div>
          <div style="display:flex;gap:8px;align-items:center;">
            <input type="text" class="text-input" style="width:150px;" data-otp-in placeholder="123456" maxlength="8" value="${escapeAttr(this.otpInputValue)}" />
            <button type="button" class="btn btn-primary" data-otp-submit>인증</button>
          </div>
        </div>
      `;
      const otpIn = resBox.querySelector('[data-otp-in]') as HTMLInputElement;
      otpIn.addEventListener('input', () => {
        this.otpInputValue = otpIn.value;
      });
      const submitOtp = async () => {
        const code = otpIn.value.trim();
        if (!code) return;
        this.otpInputValue = '';
        const snap = await api.syncSubmitOtp(p.backend, code);
        this.receiveSnapshot(snap);
      };
      otpIn.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter') {
          ev.preventDefault();
          void submitOtp();
        }
      });
      resBox.querySelector('[data-otp-submit]')?.addEventListener('click', () => void submitOtp());
    }
  }

  private populateSaveBar(saveCard: HTMLElement): void {
    const dirty = this.isDraftDirty();
    const startLabel = this.snapshot.service_running ? '저장 후 다시 시작' : '저장하고 동기화 시작';
    saveCard.innerHTML = `
      <div style="display:flex;align-items:center;gap:10px;flex-wrap:wrap;">
        <button type="button" class="btn btn-primary" data-save-start ${this.snapshot.transitioning ? 'disabled' : ''}>
          ${escapeHtml(startLabel)}
        </button>
        <button type="button" class="btn" data-save-only ${!dirty ? 'disabled' : ''}>
          저장만 하기
        </button>
        ${
          dirty
            ? `<span style="font-size:12.5px;color:var(--warn);font-weight:600;">● 저장하지 않은 변경 사항이 있습니다</span>`
            : `<span style="font-size:12.5px;color:var(--text-muted);">모든 설정이 저장되어 있습니다</span>`
        }
      </div>
    `;

    saveCard.querySelector('[data-save-start]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncSaveConfig(this.draftCfg, true);
        this.receiveSnapshot(snap, true);
        if (snap.service_running && !snap.message?.error) {
          this.subTab = 'home';
          this.renderFull();
        }
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '설정 저장 실패', true);
      }
    });

    saveCard.querySelector('[data-save-only]')?.addEventListener('click', async () => {
      try {
        const snap = await api.syncSaveConfig(this.draftCfg, false);
        this.receiveSnapshot(snap, true);
        this.callbacks.onToast('설정을 저장했습니다.');
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '설정 저장 실패', true);
      }
    });
  }

  private refreshSaveBarOnly(): void {
    const saveZone = this.container.querySelector('#sync-save-bar-zone') as HTMLElement | null;
    if (saveZone) {
      this.populateSaveBar(saveZone);
    }
  }

  private refreshSettingsLiveZones(): void {
    this.renderMessageBanner();
    const activeOtpFocused =
      document.activeElement?.hasAttribute('data-otp-in') ?? false;
    if (!activeOtpFocused) {
      const connZone = this.container.querySelector('#sync-conn-check-zone') as HTMLElement | null;
      const prof = this.draftCfg.profiles[this.selectedIdx];
      if (connZone && prof) {
        this.populateConnectionCheckZone(connZone, prof);
      }
    }
    this.refreshSaveBarOnly();
  }

  private populateAdvancedSettings(body: HTMLElement): void {
    const isAndroid = isAndroidApp();
    const autostartLabel = isAndroid
      ? 'Android 부팅 시 자동 실행 및 백그라운드 동기화 유지'
      : 'Windows 시작 시 자동 실행 (트레이에서 조용히 실행)';

    body.innerHTML = `
      <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:14px;">
        <label style="display:inline-flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="checkbox" data-adv-watch ${this.draftCfg.watch_local ? 'checked' : ''} />
          저장하자마자 바로 업로드 (실시간 파일 감시)
        </label>
        <label style="display:inline-flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="checkbox" data-adv-launch ${this.draftCfg.start_on_launch ? 'checked' : ''} />
          앱을 켜면 자동으로 동기화 시작
        </label>
        <label style="display:inline-flex;align-items:center;gap:8px;cursor:pointer;">
          <input type="checkbox" data-adv-autostart ${this.snapshot.autostart ? 'checked' : ''} />
          ${escapeHtml(autostartLabel)}
        </label>
      </div>
      <div class="form-grid" style="max-width:520px;">
        <div class="form-label">다른 기기 변경 확인(초)</div>
        <input type="number" min="0" max="600" class="text-input" data-adv-quick value="${this.draftCfg.quick_check_secs}" title="작은 신호 파일 하나만 확인합니다 (0 = 끔)" />
        <div class="form-label">전체 목록 확인 주기(초)</div>
        <input type="number" min="10" max="3600" class="text-input" data-adv-poll value="${this.draftCfg.poll_interval_secs}" />
        <div class="form-label">저장 후 대기(ms)</div>
        <input type="number" min="200" max="60000" class="text-input" data-adv-debounce value="${this.draftCfg.debounce_ms}" />
        <div class="form-label">기기 이름</div>
        <input type="text" class="text-input" data-adv-devname placeholder="${escapeAttr(this.snapshot.default_device_name || (isAndroid ? 'Android' : 'PC'))}" value="${escapeAttr(this.draftCfg.device_name)}" />
      </div>
      ${
        isAndroid
          ? `<div id="android-native-settings-zone" style="margin-top:16px;padding-top:14px;border-top:1px solid var(--border-subtle);"></div>`
          : `<div style="margin-top:14px;">
              <button type="button" class="btn btn-sm" data-open-datadir>📂 설정 데이터 폴더 열기</button>
            </div>`
      }
    `;

    if (isAndroid) {
      const az = body.querySelector('#android-native-settings-zone') as HTMLElement | null;
      if (az) this.renderAndroidNativeSettings(az);
    }

    const watchChk = body.querySelector('[data-adv-watch]') as HTMLInputElement;
    watchChk.addEventListener('change', () => {
      this.draftCfg.watch_local = watchChk.checked;
      this.refreshSaveBarOnly();
    });

    const launchChk = body.querySelector('[data-adv-launch]') as HTMLInputElement;
    launchChk.addEventListener('change', () => {
      this.draftCfg.start_on_launch = launchChk.checked;
      this.refreshSaveBarOnly();
    });

    const autoChk = body.querySelector('[data-adv-autostart]') as HTMLInputElement;
    autoChk.addEventListener('change', async () => {
      try {
        const snap = await api.syncSetAutostart(autoChk.checked);
        this.receiveSnapshot(snap);
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '자동 실행 설정 변경 실패', true);
      }
    });

    const quickIn = body.querySelector('[data-adv-quick]') as HTMLInputElement;
    quickIn.addEventListener('input', () => {
      this.draftCfg.quick_check_secs = Math.max(0, Number(quickIn.value) || 0);
      this.refreshSaveBarOnly();
    });

    const pollIn = body.querySelector('[data-adv-poll]') as HTMLInputElement;
    pollIn.addEventListener('input', () => {
      this.draftCfg.poll_interval_secs = Math.max(10, Number(pollIn.value) || 60);
      this.refreshSaveBarOnly();
    });

    const debIn = body.querySelector('[data-adv-debounce]') as HTMLInputElement;
    debIn.addEventListener('input', () => {
      this.draftCfg.debounce_ms = Math.max(200, Number(debIn.value) || 1000);
      this.refreshSaveBarOnly();
    });

    const devIn = body.querySelector('[data-adv-devname]') as HTMLInputElement;
    devIn.addEventListener('input', () => {
      this.draftCfg.device_name = devIn.value;
      this.refreshSaveBarOnly();
    });

    body.querySelector('[data-open-datadir]')?.addEventListener('click', () => {
      void api.syncOpenDataDir();
    });
  }

  private renderAndroidNativeSettings(zone: HTMLElement): void {
    const paint = (info: AndroidAppInfoDto | null) => {
      if (!info) {
        zone.innerHTML = `<div style="font-size:12.5px;color:var(--text-muted);">Android 기기 정보를 불러오는 중…</div>`;
        return;
      }
      zone.innerHTML = `
        <div style="font-weight:700;font-size:13.5px;margin-bottom:8px;">📱 Android 권한 및 앱 업데이트</div>
        <div style="display:flex;flex-direction:column;gap:8px;font-size:13px;">
          <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;padding:8px 10px;border-radius:8px;background:var(--bg-inset);">
            <span>
              모든 파일 접근 권한:
              <strong style="color:${info.all_files_granted ? 'var(--ok)' : 'var(--warn)'};">
                ${info.all_files_granted ? '허용됨' : '허용 필요'}
              </strong>
            </span>
            <button type="button" class="btn btn-sm" data-android-files-perm>
              ${info.all_files_granted ? '권한 설정 보기' : '🔓 권한 허용하기'}
            </button>
          </div>
          <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;padding:8px 10px;border-radius:8px;background:var(--bg-inset);">
            <div>
              <div>앱 버전: <strong>v${escapeHtml(info.version || '0.8.0')}</strong>${
                info.available_update_tag
                  ? ` · <span style="color:var(--accent);font-weight:600;">새 버전 ${escapeHtml(info.available_update_tag)} 출시</span>`
                  : ''
              }</div>
              ${
                info.last_update_message
                  ? `<div style="font-size:12px;color:var(--text-muted);margin-top:2px;">${escapeHtml(info.last_update_message)}</div>`
                  : ''
              }
            </div>
            <button type="button" class="btn btn-sm ${info.available_update_tag ? 'btn-primary' : ''}" data-android-update-now>
              ${info.available_update_tag ? '📲 지금 업데이트' : '업데이트 확인'}
            </button>
          </div>
          <label style="display:inline-flex;align-items:center;gap:8px;cursor:pointer;">
            <input type="checkbox" data-android-auto-update ${info.auto_update ? 'checked' : ''} />
            새 버전이 나오면 자동으로 받아서 설치
          </label>
        </div>
      `;

      zone.querySelector('[data-android-files-perm]')?.addEventListener('click', () => {
        void api.androidGrantFilesAccess();
      });
      zone.querySelector('[data-android-update-now]')?.addEventListener('click', async () => {
        try {
          const res = await api.androidUpdateNow();
          this.callbacks.onToast(res.message);
          this.androidInfo = await api.androidInfo();
          paint(this.androidInfo);
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '업데이트 확인 실패', true);
        }
      });
      const autoUpChk = zone.querySelector('[data-android-auto-update]') as HTMLInputElement | null;
      autoUpChk?.addEventListener('change', async () => {
        try {
          const res = await api.androidSetAutoUpdate(autoUpChk.checked);
          if (this.androidInfo) this.androidInfo.auto_update = res.auto_update;
        } catch (err: any) {
          this.callbacks.onToast(err?.message || '자동 업데이트 설정 실패', true);
        }
      });
    };

    paint(this.androidInfo);
    void api
      .androidInfo()
      .then((info) => {
        this.androidInfo = info;
        if (document.body.contains(zone)) paint(info);
      })
      .catch(() => {});
  }

  private renderLogsTab(host: HTMLElement): void {
    const logs = this.snapshot.status?.logs || [];
    const card = document.createElement('div');
    card.className = 'sync-card';
    card.innerHTML = `
      <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;">
        <span style="font-weight:700;">동기화 기록 (${logs.length}건)</span>
        <button type="button" class="btn btn-sm" data-copy-logs ${logs.length === 0 ? 'disabled' : ''}>📋 전체 복사</button>
      </div>
      <div class="log-list-box" data-logs-box></div>
    `;

    card.querySelector('[data-copy-logs]')?.addEventListener('click', () => {
      const text = logs.map((l) => `${l.time} ${l.text}`).join('\n');
      void navigator.clipboard.writeText(text);
      this.callbacks.onToast('동기화 기록 전체를 복사했습니다.');
    });

    const box = card.querySelector('[data-logs-box]')!;
    if (logs.length === 0) {
      box.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);font-family:inherit;">동기화를 시작하면 여기에 기록이 표시됩니다.</div>`;
    } else {
      for (const l of logs) {
        const row = document.createElement('div');
        row.className = 'log-row';
        row.innerHTML = `
          <span class="log-time">${escapeHtml(l.time)}</span>
          <span class="log-text${l.error ? ' error' : ''}">${escapeHtml(l.text)}</span>
        `;
        box.appendChild(row);
      }
      setTimeout(() => {
        box.scrollTop = box.scrollHeight;
      }, 0);
    }

    host.appendChild(card);
  }

  /**
   * Renders & updates the interactive Synology NAS Folder Browser Modal (`nas_browser`)
   */
  private syncNasBrowserModal(): void {
    const nb = this.snapshot.nas_browser;
    const prof = this.draftCfg.profiles[this.selectedIdx];
    if (!nb || !prof || prof.backend.type !== 'synology') {
      if (this.nasModalEl) {
        this.nasModalEl.remove();
        this.nasModalEl = null;
      }
      return;
    }

    // Do not clobber focused input inside the NAS modal while user is typing OTP or folder name
    if (
      this.nasModalEl &&
      this.nasModalEl.contains(document.activeElement) &&
      (document.activeElement?.hasAttribute('data-nas-new-folder') ||
        document.activeElement?.hasAttribute('data-nas-otp')) &&
      nb.status !== 'loading'
    ) {
      return;
    }

    if (!this.nasModalEl) {
      this.nasModalEl = document.createElement('div');
      this.nasModalEl.className = 'modal-backdrop';
      document.body.appendChild(this.nasModalEl);
    }

    const backend = prof.backend;
    const atRoot = nb.path === '/' || !nb.path;
    const ready = nb.status === 'ready';

    this.nasModalEl.innerHTML = `
      <div class="modal-card" style="width:480px;">
        <div class="modal-header">
          <span>📁 Synology NAS 폴더 선택</span>
          <button type="button" class="icon-btn" data-nas-close>✕</button>
        </div>
        <div class="modal-body">
          <div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
            <button type="button" class="btn btn-sm" data-nas-up ${atRoot ? 'disabled' : ''}>⬆ 위로</button>
            <span style="font-weight:700;font-size:13.5px;">${escapeHtml(atRoot ? '공유 폴더 목록 (/)' : nb.path)}</span>
          </div>
          <div style="border:1px solid var(--border-subtle);border-radius:8px;min-height:220px;max-height:280px;overflow-y:auto;padding:6px;" data-nas-list></div>
          ${
            !atRoot
              ? `
            <div style="display:flex;gap:6px;margin-top:10px;">
              <input type="text" class="text-input" data-nas-new-folder placeholder="새 폴더 이름" value="${escapeAttr(this.nasNewFolderName)}" />
              <button type="button" class="btn" data-nas-create ${!ready ? 'disabled' : ''}>＋ 새 폴더</button>
            </div>
          `
              : `<div style="margin-top:8px;font-size:12px;color:var(--text-muted);">공유 폴더를 눌러 들어간 뒤 동기화할 폴더를 고르세요.</div>`
          }
        </div>
        <div class="modal-footer">
          <button type="button" class="btn" data-nas-cancel>취소</button>
          <button type="button" class="btn btn-primary" data-nas-choose ${atRoot || !ready ? 'disabled' : ''}>
            이 폴더 사용: ${escapeHtml(nb.path)}
          </button>
        </div>
      </div>
    `;

    const listEl = this.nasModalEl.querySelector('[data-nas-list]')!;
    if (nb.status === 'loading') {
      listEl.innerHTML = `<div style="padding:24px;display:flex;align-items:center;justify-content:center;gap:8px;color:var(--text-secondary);"><span class="spinner"></span> NAS 폴더를 불러오는 중…</div>`;
    } else if (nb.status === 'failed') {
      listEl.innerHTML = `
        <div style="padding:16px;color:var(--danger);font-size:13px;">
          ✖ ${escapeHtml(nb.message || '불러오지 못했습니다')}
          <div style="margin-top:8px;"><button type="button" class="btn btn-sm" data-nas-retry>다시 시도</button></div>
        </div>
      `;
      listEl.querySelector('[data-nas-retry]')?.addEventListener('click', async () => {
        const snap = await api.syncNasBrowseStep({ backend, path: nb.path });
        this.receiveSnapshot(snap);
      });
    } else if (nb.status === 'need_otp') {
      listEl.innerHTML = `
        <div style="padding:14px;">
          <div style="font-weight:700;margin-bottom:4px;">🔐 2단계 인증 필요</div>
          <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:10px;">
            OTP 앱의 6자리 코드를 입력하세요.
          </div>
          <div style="display:flex;gap:6px;">
            <input type="text" class="text-input" style="width:140px;" data-nas-otp placeholder="123456" value="${escapeAttr(this.nasOtpValue)}" />
            <button type="button" class="btn btn-primary" data-nas-otp-btn>인증</button>
          </div>
        </div>
      `;
      const otpIn = listEl.querySelector('[data-nas-otp]') as HTMLInputElement;
      otpIn.addEventListener('input', () => {
        this.nasOtpValue = otpIn.value;
      });
      listEl.querySelector('[data-nas-otp-btn]')?.addEventListener('click', async () => {
        const code = otpIn.value.trim();
        if (!code) return;
        this.nasOtpValue = '';
        const snap = await api.syncNasBrowseStep({ backend, path: nb.path, otp: code });
        this.receiveSnapshot(snap);
      });
    } else {
      if (nb.folders.length === 0) {
        listEl.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);">하위 폴더가 없습니다.</div>`;
      } else {
        for (const f of nb.folders) {
          const item = document.createElement('button');
          item.type = 'button';
          item.className = 'context-menu-item';
          item.style.justifyContent = 'space-between';
          item.innerHTML = `<span>📁 ${escapeHtml(f.name)}</span><span style="color:var(--text-muted);">›</span>`;
          item.addEventListener('click', async () => {
            const snap = await api.syncNasBrowseStep({ backend, path: f.path });
            this.receiveSnapshot(snap);
          });
          listEl.appendChild(item);
        }
      }
    }

    this.nasModalEl.querySelector('[data-nas-up]')?.addEventListener('click', async () => {
      const idx = nb.path.lastIndexOf('/');
      const up = idx <= 0 ? '/' : nb.path.slice(0, idx);
      const snap = await api.syncNasBrowseStep({ backend, path: up });
      this.receiveSnapshot(snap);
    });

    const newFolderIn = this.nasModalEl.querySelector(
      '[data-nas-new-folder]'
    ) as HTMLInputElement | null;
    newFolderIn?.addEventListener('input', () => {
      this.nasNewFolderName = newFolderIn.value;
    });

    this.nasModalEl.querySelector('[data-nas-create]')?.addEventListener('click', async () => {
      const name = (newFolderIn?.value || '').trim();
      if (!name) return;
      this.nasNewFolderName = '';
      const snap = await api.syncNasBrowseStep({ backend, path: nb.path, create: name });
      this.receiveSnapshot(snap);
    });

    const closeModal = async () => {
      const snap = await api.syncNasBrowseClose();
      this.receiveSnapshot(snap);
    };

    this.nasModalEl.querySelector('[data-nas-close]')?.addEventListener('click', () => void closeModal());
    this.nasModalEl.querySelector('[data-nas-cancel]')?.addEventListener('click', () => void closeModal());
    this.nasModalEl.querySelector('[data-nas-choose]')?.addEventListener('click', async () => {
      backend.folder = nb.path;
      const snap = await api.syncNasBrowseClose();
      this.receiveSnapshot(snap);
      this.renderFull();
    });
  }

  public destroy(): void {
    if (this.nasModalEl) {
      this.nasModalEl.remove();
      this.nasModalEl = null;
    }
  }
}
