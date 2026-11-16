import './styles/notion.css';
import { api, hydrateMarkdownMediaUrls, isAndroidApp } from './api';
import { renderPrintableHtml, renderSplitViewer, StudioEditorController } from './editor';
import { ForceGraphCanvas } from './graph';
import {
  closeContextMenu,
  collectFoldersFromTree,
  ContextMenuEntry,
  openCommandPaletteModal,
  openCoverPickerModal,
  openDailyNoteFolderModal,
  openEmojiPickerModal,
  openNoteCryptoModal,
  openPageSetupModal,
  openTemplatesModal,
  openTrashModal,
  openVaultCryptoModal,
  openWikiLinkPickerModal,
  PageSetupValues,
  QuickCommandItem,
  showContextMenu,
  showFolderMoveModal,
  showTextPromptModal,
} from './modals';
import { SyncCenterController } from './syncCenter';
import type {
  ActiveMainView,
  AndroidAppInfoDto,
  NoteDocumentDto,
  NoteMetaDto,
  NotePropertyDto,
  PropertyType,
  RightInspectorTab,
  SaveStatus,
  SyncSnapshotDto,
  TreeNodeDto,
  VaultScanDto,
  WorkspaceState,
} from './types';

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

function parentFolderOf(relPath: string): string {
  const idx = relPath.lastIndexOf('/');
  return idx > 0 ? relPath.slice(0, idx) : '';
}

function stripMdExt(name: string): string {
  return name.replace(/\.md$/i, '');
}

class ObsidianSyncStudioApp {
  private root: HTMLElement;

  // Core state
  private workspace!: WorkspaceState;
  private scan: VaultScanDto | null = null;
  private syncSnap!: SyncSnapshotDto;
  private vaultRevision = 0;

  // UI view state
  private mainView: ActiveMainView = 'editor';
  private sidebarCollapsed = false;
  private inspectorOpen = true;
  private inspectorTab: RightInspectorTab = 'backlinks';
  private localGraphDepth = 2;
  private showRawYaml = false;
  private expandedFolders = new Set<string>();
  private suppressTreeClickOnce = false;
  private dragAutoExpandTimer: number | null = null;
  private lastHoverFolderPath: string | null = null;

  // Active note editing state
  private currentDoc: NoteDocumentDto | null = null;
  private draftBody = '';
  private draftIcon: string | null = null;
  private draftCover: string | null = null;
  private draftProperties: NotePropertyDto[] = [];
  private saveStatus: SaveStatus = 'saved';
  private autoSaveTimer: number | null = null;
  private savingPromise: Promise<void> | null = null;

  // Split pane state
  private splitDoc: NoteDocumentDto | null = null;
  private splitViewerCleanup: (() => void) | null = null;

  // Controllers
  private editorCtrl: StudioEditorController | null = null;
  private mountedNotePath: string | null = null;
  private localGraphCtrl: ForceGraphCanvas | null = null;
  private globalGraphCtrl: ForceGraphCanvas | null = null;
  private globalGraphIncludeTags = true;
  private globalGraphLocalOnly = false;
  private globalGraphSearch = '';
  private globalGraphLayoutMode: 'cluster' | 'radial' | 'hierarchy' = 'cluster';
  private globalGraphHideOrphans = false;
  private globalGraphFolderFilter = '';
  private syncCtrl: SyncCenterController | null = null;

  // Persistent DOM containers
  private sidebarEl!: HTMLElement;
  private sidebarBackdropEl: HTMLElement | null = null;
  private tabsStripEl!: HTMLElement;
  private topbarEl!: HTMLElement;
  private editorViewWrapEl!: HTMLElement;
  private editorToolbarDockEl!: HTMLElement;
  private editorPaneEl!: HTMLElement;
  private splitPaneEl!: HTMLElement;
  private inspectorEl!: HTMLElement;
  private graphViewWrapEl!: HTMLElement;
  private syncViewWrapEl!: HTMLElement;
  private toastStackEl!: HTMLElement;
  private printRootEl: HTMLElement | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private layoutRafPending = false;
  private androidInfo: AndroidAppInfoDto | null = null;

  private pollTimer: number | null = null;

  constructor(root: HTMLElement) {
    this.root = root;
  }

  private isMobileViewport(): boolean {
    return window.innerWidth <= 860;
  }

  private isCompactPhone(): boolean {
    return window.innerWidth <= 680;
  }

  private syncSidebarBackdrop(): void {
    if (this.sidebarEl) {
      this.sidebarEl.classList.toggle('collapsed', this.sidebarCollapsed);
    }
    if (this.sidebarBackdropEl) {
      this.sidebarBackdropEl.classList.toggle(
        'visible',
        !this.sidebarCollapsed && this.isMobileViewport()
      );
    }
  }

  private setSidebarCollapsed(collapsed: boolean): void {
    this.sidebarCollapsed = collapsed;
    this.syncSidebarBackdrop();
  }

  private handleAndroidBackPress(): boolean {
    const backdrops = document.querySelectorAll('.modal-backdrop');
    if (backdrops.length > 0) {
      const topModal = backdrops[backdrops.length - 1] as HTMLElement;
      const closeBtn = topModal.querySelector(
        '.modal-header .icon-btn, [data-close], [data-nas-close], [data-cmd-close]'
      ) as HTMLElement | null;
      if (closeBtn) closeBtn.click();
      else topModal.remove();
      return true;
    }
    if (document.querySelector('.context-menu')) {
      closeContextMenu();
      return true;
    }
    if (this.isMobileViewport() && !this.sidebarCollapsed) {
      this.setSidebarCollapsed(true);
      return true;
    }
    if (window.innerWidth <= 960 && this.inspectorOpen) {
      this.inspectorOpen = false;
      this.renderInspector();
      this.renderTopbar();
      return true;
    }
    if (this.splitDoc) {
      void this.closeSplitNote();
      return true;
    }
    if (this.mainView !== 'editor') {
      void this.switchMainView('editor');
      return true;
    }
    return false;
  }

  public async init(): Promise<void> {
    this.printRootEl = document.getElementById('print-root');
    this.buildShellDom();
    window.setTimeout(() => {
      if (!document.getElementById('pretendard-webfont')) {
        const link = document.createElement('link');
        link.id = 'pretendard-webfont';
        link.rel = 'stylesheet';
        link.href =
          'https://cdn.jsdelivr.net/gh/orioncactus/pretendard@v1.3.9/dist/web/static/pretendard.min.css';
        document.head.appendChild(link);
      }
    }, 350);
    this.bindGlobalShortcuts();
    this.setupResizeObserver();
    window.addEventListener('afterprint', () => {
      if (this.printRootEl) this.printRootEl.innerHTML = '';
    });
    window.addEventListener('resize', () => {
      this.syncSidebarBackdrop();
    });
    if (isAndroidApp()) {
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') {
          void api
            .androidInfo()
            .then((info) => {
              this.androidInfo = info;
              if (this.workspace && !this.workspace.active_vault && this.mainView === 'editor') {
                this.renderEditorMainArea(true);
              }
            })
            .catch(() => {});
        }
      });
    }
    window.__handleAndroidBack = () => this.handleAndroidBackPress();

    try {
      const boot = await api.bootstrap();
      this.workspace = boot.workspace;
      this.scan = boot.vault_scan;
      this.syncSnap = boot.sync;
      this.vaultRevision = boot.vault_revision;

      if (window.innerWidth <= 960) {
        this.inspectorOpen = false;
      }
      if (this.isMobileViewport()) {
        this.setSidebarCollapsed(true);
      }

      if (isAndroidApp()) {
        void api
          .androidInfo()
          .then((info) => {
            this.androidInfo = info;
            if (info.available_update_tag) {
              this.showToast(
                `새 버전 ${info.available_update_tag} 출시! 동기화 센터 → 고급 설정에서 업데이트할 수 있습니다.`
              );
            }
            if (!this.workspace.active_vault) {
              this.renderEditorMainArea(true);
            }
          })
          .catch(() => {});
      }

      this.applyTheme(this.workspace.theme || 'light');
      this.applyPageLayoutVariables();
      this.expandInitialFolders();

      this.renderChrome();

      if (this.workspace.active_vault && this.scan) {
        const initialPath =
          this.workspace.active_tab ||
          this.workspace.open_tabs[0] ||
          this.scan.notes[0]?.path ||
          null;
        if (initialPath) {
          await this.openNote(initialPath);
        } else {
          this.renderEditorMainArea(true);
        }
        if (this.workspace.split_tab) {
          await this.openSplitNote(this.workspace.split_tab);
        }
      } else {
        this.renderEditorMainArea(true);
      }

      this.scheduleNextPoll();
    } catch (err: any) {
      this.showToast(err?.message || '초기화 중 오류가 발생했습니다.', true);
    }
  }

  private buildShellDom(): void {
    this.root.innerHTML = `
      <div class="app-shell">
        <div class="sidebar-backdrop" data-sidebar-backdrop></div>
        <aside class="sidebar${window.innerWidth <= 860 ? ' collapsed' : ''}" id="app-sidebar"></aside>
        <div class="main-area">
          <div class="tabs-strip" id="app-tabs-strip"></div>
          <div class="topbar" id="app-topbar"></div>
          <div class="workspace-body" id="view-editor">
            <div class="editor-column">
              <div class="editor-mount-wrap editor-toolbar-dock" id="app-editor-toolbar-dock" style="display:none;"></div>
              <div class="panes-container">
                <div class="editor-pane" id="app-editor-pane"></div>
                <div class="split-pane" id="app-split-pane" style="display:none;"></div>
              </div>
            </div>
            <aside class="inspector collapsed" id="app-inspector"></aside>
          </div>
          <div class="graph-view-screen" id="view-graph" style="display:none;"></div>
          <div style="flex:1;min-height:0;display:none;overflow:hidden;" id="view-sync"></div>
        </div>
      </div>
      <div class="toast-stack" id="app-toasts"></div>
    `;

    this.sidebarEl = this.root.querySelector('#app-sidebar')!;
    this.sidebarBackdropEl = this.root.querySelector('[data-sidebar-backdrop]');
    this.sidebarBackdropEl?.addEventListener('click', () => {
      this.setSidebarCollapsed(true);
    });
    this.tabsStripEl = this.root.querySelector('#app-tabs-strip')!;
    this.topbarEl = this.root.querySelector('#app-topbar')!;
    this.editorViewWrapEl = this.root.querySelector('#view-editor')!;
    this.editorToolbarDockEl = this.root.querySelector('#app-editor-toolbar-dock')!;
    this.editorPaneEl = this.root.querySelector('#app-editor-pane')!;
    this.splitPaneEl = this.root.querySelector('#app-split-pane')!;
    this.inspectorEl = this.root.querySelector('#app-inspector')!;
    this.graphViewWrapEl = this.root.querySelector('#view-graph')!;
    this.syncViewWrapEl = this.root.querySelector('#view-sync')!;
    this.toastStackEl = this.root.querySelector('#app-toasts')!;

    // Convert vertical mouse wheel over overflowing top bars into horizontal scroll
    const bindHorizontalWheel = (host: HTMLElement, selector: string) => {
      host.addEventListener(
        'wheel',
        (ev: WheelEvent) => {
          const scroller = (ev.target as HTMLElement | null)?.closest(selector) as HTMLElement | null;
          if (!scroller || scroller.scrollWidth <= scroller.clientWidth + 1) return;
          if (Math.abs(ev.deltaY) > Math.abs(ev.deltaX) && ev.deltaY !== 0) {
            scroller.scrollLeft += ev.deltaY;
            ev.preventDefault();
          }
        },
        { passive: false }
      );
    };
    bindHorizontalWheel(this.tabsStripEl, '.tabs-scroll');
    bindHorizontalWheel(this.topbarEl, '.topbar-right');
  }

  private expandInitialFolders(): void {
    if (!this.scan) return;
    for (const n of this.scan.tree.slice(0, 6)) {
      if (n.kind === 'folder') {
        this.expandedFolders.add(n.path);
      }
    }
    if (this.workspace.active_tab) {
      this.expandAncestorsOf(this.workspace.active_tab);
    }
  }

  private expandAncestorsOf(relPath: string): void {
    const parts = relPath.split('/');
    let acc = '';
    for (let i = 0; i < parts.length - 1; i++) {
      acc = acc ? `${acc}/${parts[i]}` : parts[i];
      this.expandedFolders.add(acc);
    }
  }

  public showToast(msg: string, isError = false): void {
    const el = document.createElement('div');
    el.className = `toast-msg${isError ? ' error' : ''}`;
    el.innerHTML = `<span>${isError ? '⚠️' : '✓'}</span><span>${escapeHtml(msg)}</span>`;
    this.toastStackEl.appendChild(el);
    setTimeout(() => {
      el.remove();
    }, 3400);
  }

  private applyTheme(theme: 'light' | 'dark'): void {
    document.documentElement.setAttribute('data-theme', theme);
    this.editorCtrl?.setTheme(theme);
    this.localGraphCtrl?.setTheme(theme);
    this.globalGraphCtrl?.setTheme(theme);
    if (this.splitDoc) {
      this.renderSplitPane();
    }
  }

  private async persistWorkspace(patch: Partial<WorkspaceState>): Promise<void> {
    Object.assign(this.workspace, patch);
    try {
      this.workspace = await api.workspaceSave(patch);
    } catch {
      // ignore transient save error
    }
  }

  private bindGlobalShortcuts(): void {
    window.addEventListener('keydown', (ev) => {
      const mod = ev.ctrlKey || ev.metaKey;
      const key = ev.key.toLowerCase();

      if (mod && key === 'k') {
        ev.preventDefault();
        closeContextMenu();
        this.openCommandPalette();
        return;
      }
      if (mod && key === 'p') {
        ev.preventDefault();
        closeContextMenu();
        if (!ev.shiftKey && this.mainView === 'editor' && this.currentDoc && !this.currentDoc.locked) {
          void this.handlePrintNote();
        } else {
          this.openCommandPalette();
        }
        return;
      }
      if (mod && key === 's') {
        ev.preventDefault();
        void this.flushSave(true);
        return;
      }
      if (mod && key === 'd') {
        ev.preventDefault();
        void this.openTodayDailyNote();
        return;
      }
      if (mod && key === 'g') {
        ev.preventDefault();
        void this.switchMainView(this.mainView === 'graph' ? 'editor' : 'graph');
        return;
      }
      if (mod && ev.key === '\\') {
        ev.preventDefault();
        this.setSidebarCollapsed(!this.sidebarCollapsed);
        requestAnimationFrame(() => this.applyPageLayoutVariables());
      }
    });
  }

  // ───────────────────────── v0.6.0 A4 / Daily Folder / Print / Crypto Helpers ─────────────────────────
  private getActiveDailyFolder(): string {
    const vault = this.workspace?.active_vault;
    const map = this.workspace?.daily_folders_by_vault;
    if (vault && map && Object.prototype.hasOwnProperty.call(map, vault)) {
      return map[vault];
    }
    return this.workspace?.daily_folder ?? 'Daily';
  }

  private setupResizeObserver(): void {
    if (typeof ResizeObserver === 'undefined') return;
    this.resizeObserver?.disconnect();
    this.resizeObserver = new ResizeObserver(() => {
      if (this.layoutRafPending) return;
      this.layoutRafPending = true;
      window.requestAnimationFrame(() => {
        this.layoutRafPending = false;
        this.applyPageLayoutVariables();
      });
    });
    if (this.editorPaneEl) {
      this.resizeObserver.observe(this.editorPaneEl);
    }
  }

  private applyPageLayoutVariables(): void {
    if (!this.workspace) return;
    const isLand = this.workspace.page_orientation === 'landscape';
    const pageWidthMm = isLand ? 297 : 210;
    const pageHeightMm = isLand ? 210 : 297;

    const mTop = Number.isFinite(this.workspace.margin_top_cm) ? this.workspace.margin_top_cm : 2.0;
    const mBottom = Number.isFinite(this.workspace.margin_bottom_cm)
      ? this.workspace.margin_bottom_cm
      : 1.0;
    const mLeft = Number.isFinite(this.workspace.margin_left_cm) ? this.workspace.margin_left_cm : 1.5;
    const mRight = Number.isFinite(this.workspace.margin_right_cm)
      ? this.workspace.margin_right_cm
      : 1.5;

    const rootStyle = document.documentElement.style;
    rootStyle.setProperty('--page-width', `${pageWidthMm}mm`);
    rootStyle.setProperty('--page-min-height', `${pageHeightMm}mm`);
    rootStyle.setProperty('--page-margin-top', `${mTop}cm`);
    rootStyle.setProperty('--page-margin-bottom', `${mBottom}cm`);
    rootStyle.setProperty('--page-margin-left', `${mLeft}cm`);
    rootStyle.setProperty('--page-margin-right', `${mRight}cm`);

    const zoomFactor = Math.min(1.5, Math.max(0.5, (this.workspace.editor_zoom || 100) / 100));
    const sheetEl = document.getElementById('a4-paper-sheet') as HTMLElement | null;
    const scaleOuter = document.getElementById('a4-scale-outer') as HTMLElement | null;

    if (this.isCompactPhone()) {
      rootStyle.setProperty('--a4-scale', '1');
      if (scaleOuter) {
        scaleOuter.style.width = '';
        scaleOuter.style.height = '';
      }
      return;
    }

    let finalScale = zoomFactor;
    if (
      sheetEl &&
      this.workspace.page_layout !== 'full' &&
      this.workspace.auto_fit_a4 !== false &&
      this.editorPaneEl
    ) {
      const availWidth = Math.max(240, this.editorPaneEl.clientWidth - 48);
      const naturalWidth = sheetEl.offsetWidth || pageWidthMm * 3.7795275591;
      if (naturalWidth > 0 && naturalWidth * zoomFactor > availWidth) {
        finalScale = Math.max(0.45, availWidth / naturalWidth);
      }
    }

    rootStyle.setProperty('--a4-scale', finalScale.toFixed(4));

    if (sheetEl) {
      // Avoid a scale(1) transform layer at 100% zoom so hit-testing & text stay pixel-sharp.
      sheetEl.style.transform = Math.abs(finalScale - 1) < 0.002 ? 'none' : '';
    }

    if (sheetEl && scaleOuter) {
      const naturalW = sheetEl.offsetWidth;
      const naturalH = sheetEl.offsetHeight;
      if (naturalW > 0) {
        const nextW = `${Math.round(naturalW * finalScale)}px`;
        const nextH = `${Math.round(naturalH * finalScale)}px`;
        if (scaleOuter.style.width !== nextW) scaleOuter.style.width = nextW;
        if (scaleOuter.style.height !== nextH) scaleOuter.style.height = nextH;
      }
    }
  }

  private getPageSetupValues(): PageSetupValues {
    return {
      page_layout: this.workspace.page_layout || 'a4',
      page_orientation: this.workspace.page_orientation || 'portrait',
      margin_top_cm: this.workspace.margin_top_cm ?? 2.0,
      margin_bottom_cm: this.workspace.margin_bottom_cm ?? 1.0,
      margin_left_cm: this.workspace.margin_left_cm ?? 1.5,
      margin_right_cm: this.workspace.margin_right_cm ?? 1.5,
      auto_fit_a4: this.workspace.auto_fit_a4 ?? true,
      show_page_guides: this.workspace.show_page_guides ?? true,
      editor_zoom: this.workspace.editor_zoom ?? 100,
      print_include_title: this.workspace.print_include_title ?? true,
      print_include_properties: this.workspace.print_include_properties ?? false,
    };
  }

  private openPageSetupDialog(): void {
    const applyPatch = async (patch: PageSetupValues) => {
      const prevMode = this.workspace.page_layout;
      const prevGuides = this.workspace.show_page_guides;
      await this.persistWorkspace(patch);
      this.applyPageLayoutVariables();
      if (prevMode !== patch.page_layout || prevGuides !== patch.show_page_guides) {
        this.renderEditorMainArea(true);
      } else {
        this.renderEditorMainArea(false);
      }
      this.renderTopbar();
    };

    openPageSetupModal({
      initial: this.getPageSetupValues(),
      onApply: async (patch) => {
        await applyPatch(patch);
        this.showToast('용지 및 여백 설정을 적용했습니다.');
      },
      onPrintNow: async (patch) => {
        await applyPatch(patch);
        await this.handlePrintNote();
      },
      onExportPdfNow: async (patch) => {
        await applyPatch(patch);
        await this.handleExportPdf();
      },
    });
  }

  private openDailyNoteFolderDialog(): void {
    const vault = this.workspace.active_vault;
    if (!vault) {
      this.showToast('먼저 볼트 폴더를 열어 주세요.', true);
      return;
    }
    const folders = this.scan ? collectFoldersFromTree(this.scan.tree) : [];
    const currentFolder = this.getActiveDailyFolder();

    openDailyNoteFolderModal({
      vault,
      folders,
      currentFolder,
      onSelect: async (folder) => {
        const nextMap = { ...(this.workspace.daily_folders_by_vault || {}), [vault]: folder };
        await this.persistWorkspace({
          daily_folders_by_vault: nextMap,
          daily_folder: folder || this.workspace.daily_folder,
        });
        this.renderSidebar();
        this.showToast(
          folder
            ? `일일 문서 저장 폴더가 '${folder}'(으)로 설정되었습니다.`
            : '일일 문서 저장 폴더가 볼트 최상위 루트로 설정되었습니다.'
        );
      },
      onFolderCreated: async () => {
        this.scan = await api.vaultScan(vault);
        this.vaultRevision = this.scan.revision;
        this.renderSidebar();
      },
      onToast: (m, err) => this.showToast(m, err),
    });
  }

  private async openNoteCryptoDialog(targetPath?: string): Promise<void> {
    const vault = this.workspace.active_vault;
    if (!vault) return;
    const notePath = targetPath || this.currentDoc?.path;
    if (!notePath) return;

    await this.flushSave(false);
    const meta = this.scan?.notes.find((n) => n.path === notePath);
    const isCurrent = this.currentDoc?.path === notePath;
    const encrypted = isCurrent ? Boolean(this.currentDoc?.encrypted) : Boolean(meta?.encrypted);
    const locked = isCurrent ? Boolean(this.currentDoc?.locked) : Boolean(meta?.locked);
    const title = isCurrent
      ? this.currentDoc!.title
      : meta?.title || stripMdExt(notePath.split('/').pop() || notePath);

    openNoteCryptoModal({
      vault,
      notePath,
      noteTitle: title,
      encrypted,
      locked,
      hint: isCurrent ? this.currentDoc?.encryption_hint : null,
      onCompleted: async () => {
        this.scan = await api.vaultScan(vault);
        this.vaultRevision = this.scan.revision;
        if (this.currentDoc?.path === notePath) {
          this.mountedNotePath = null;
          await this.openNote(notePath);
        } else {
          this.renderSidebar();
          this.renderTabsStrip();
        }
      },
      onToast: (m, err) => this.showToast(m, err),
    });
  }

  private async openVaultCryptoDialog(): Promise<void> {
    const vault = this.workspace.active_vault;
    if (!vault) {
      this.showToast('먼저 볼트를 열어 주세요.', true);
      return;
    }
    await this.flushSave(false);
    try {
      const freshScan = await api.vaultScan(vault);
      this.scan = freshScan;
      this.vaultRevision = freshScan.revision;
      const encryptedCount =
        freshScan.encrypted_notes ?? freshScan.notes.filter((n) => n.encrypted).length;
      const unlockedCount = freshScan.notes.filter((n) => n.encrypted && !n.locked).length;
      const status = {
        vault_encrypted: Boolean(freshScan.vault_has_crypto_config || encryptedCount > 0),
        vault_unlocked: Boolean(freshScan.vault_unlocked),
        total_notes: freshScan.total_notes,
        encrypted_notes: encryptedCount,
        unlocked_notes: unlockedCount,
        auto_encrypt_new_notes: Boolean(freshScan.vault_auto_encrypt),
        hint: freshScan.vault_crypto_hint ?? null,
      };
      openVaultCryptoModal({
        vault,
        status,
        onCompleted: async () => {
          this.scan = await api.vaultScan(vault);
          this.vaultRevision = this.scan.revision;
          if (this.currentDoc) {
            const p = this.currentDoc.path;
            this.mountedNotePath = null;
            await this.openNote(p);
          } else {
            this.renderChrome();
          }
        },
        onToast: (m, err) => this.showToast(m, err),
      });
    } catch (err: any) {
      this.showToast(err?.message || '암호화 상태를 불러오지 못했습니다.', true);
    }
  }

  private async toggleShowTags(): Promise<void> {
    const next = !this.workspace.show_tags;
    await this.persistWorkspace({ show_tags: next });
    this.renderSidebar();
    this.renderPropertiesRows();
    this.renderTopbar();
  }

  private async preparePrintDom(): Promise<boolean> {
    if (!this.currentDoc || !this.workspace.active_vault) {
      this.showToast('인쇄할 문서를 먼저 열어 주세요.', true);
      return false;
    }
    if (this.currentDoc.locked) {
      this.showToast('잠긴 문서는 먼저 잠금 해제 후 인쇄 또는 PDF 출력할 수 있습니다.', true);
      return false;
    }
    await this.flushSave(false);
    this.applyPageLayoutVariables();

    // @page 규칙은 CSS var()를 지원하지 않으므로 실제 cm / A4 용지 크기를 <style>로 직접 주입
    const mTop = Number.isFinite(this.workspace.margin_top_cm) ? this.workspace.margin_top_cm : 2.0;
    const mBot = Number.isFinite(this.workspace.margin_bottom_cm)
      ? this.workspace.margin_bottom_cm
      : 1.0;
    const mLeft = Number.isFinite(this.workspace.margin_left_cm)
      ? this.workspace.margin_left_cm
      : 1.5;
    const mRight = Number.isFinite(this.workspace.margin_right_cm)
      ? this.workspace.margin_right_cm
      : 1.5;
    const isLand = this.workspace.page_orientation === 'landscape';
    let pageStyleEl = document.getElementById('onyx-dynamic-page-style') as HTMLStyleElement | null;
    if (!pageStyleEl) {
      pageStyleEl = document.createElement('style');
      pageStyleEl.id = 'onyx-dynamic-page-style';
      document.head.appendChild(pageStyleEl);
    }
    pageStyleEl.textContent = `@page { size: ${isLand ? '297mm 210mm' : '210mm 297mm'}; margin: ${mTop}cm ${mRight}cm ${mBot}cm ${mLeft}cm; }`;

    const printRoot = this.printRootEl || document.getElementById('print-root');
    if (!printRoot) return false;
    this.printRootEl = printRoot;

    const md = this.editorCtrl ? this.editorCtrl.getCleanMarkdown() : this.draftBody;
    const bodyHtml = renderPrintableHtml(md, this.workspace.active_vault);

    let headerHtml = '';
    const incTitle = this.workspace.print_include_title !== false;
    const incProps = Boolean(this.workspace.print_include_properties);
    if (incTitle || (incProps && this.draftProperties.length > 0)) {
      let propsTable = '';
      if (incProps && this.draftProperties.length > 0) {
        const rows = this.draftProperties
          .filter((p) => p.key.trim())
          .map(
            (p) =>
              `<tr><th>${escapeHtml(p.key)}</th><td>${escapeHtml(p.value)}</td></tr>`
          )
          .join('');
        if (rows) {
          propsTable = `<table class="print-props-table">${rows}</table>`;
        }
      }
      headerHtml = `
        <div class="print-doc-header">
          ${incTitle ? `<h1 class="print-doc-title">${this.draftIcon ? `${escapeHtml(this.draftIcon)} ` : ''}${escapeHtml(this.currentDoc.title)}</h1>` : ''}
          ${propsTable}
        </div>
      `;
    }

    printRoot.innerHTML = `${headerHtml}${bodyHtml}`;

    const imgs = Array.from(printRoot.querySelectorAll('img'));
    if (imgs.length > 0) {
      await Promise.all(
        imgs.map((img) => {
          if (img.complete) return Promise.resolve();
          return new Promise<void>((resolve) => {
            const timer = window.setTimeout(resolve, 1500);
            const done = () => {
              window.clearTimeout(timer);
              resolve();
            };
            img.addEventListener('load', done, { once: true });
            img.addEventListener('error', done, { once: true });
          });
        })
      );
    }
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
    );
    return true;
  }

  private async handlePrintNote(): Promise<void> {
    if (isAndroidApp()) {
      await this.handleExportPdf();
      return;
    }
    const ok = await this.preparePrintDom();
    if (!ok) return;
    window.print();
  }

  private async handleExportPdf(): Promise<void> {
    const ok = await this.preparePrintDom();
    if (!ok || !this.currentDoc) return;
    let delegatedToSystemPrint = false;
    try {
      const safeName = `${this.currentDoc.title.replace(/[\\/:*?"<>|]/g, '_') || 'document'}.pdf`;
      const res = await api.exportPdf({
        default_filename: safeName,
        landscape: this.workspace.page_orientation === 'landscape',
        margin_top_cm: this.workspace.margin_top_cm ?? 2.0,
        margin_bottom_cm: this.workspace.margin_bottom_cm ?? 1.0,
        margin_left_cm: this.workspace.margin_left_cm ?? 1.5,
        margin_right_cm: this.workspace.margin_right_cm ?? 1.5,
      });
      delegatedToSystemPrint = Boolean(res.delegated_to_system_print);
      if (res.exported && res.path) {
        this.showToast(`PDF로 저장했습니다: ${res.path}`);
      } else if (delegatedToSystemPrint) {
        this.showToast('인쇄/PDF 저장 화면을 열었습니다.');
      }
    } catch (err: any) {
      this.showToast(err?.message || 'PDF 출력에 실패했습니다.', true);
    } finally {
      if (!delegatedToSystemPrint && this.printRootEl) {
        this.printRootEl.innerHTML = '';
      }
    }
  }

  // ───────────────────────── Background Polling ─────────────────────────
  private scheduleNextPoll(): void {
    if (this.pollTimer !== null) window.clearTimeout(this.pollTimer);
    const hasActiveSyncJob =
      this.syncSnap?.test_job?.status === 'running' ||
      this.syncSnap?.oauth_job?.status === 'requesting' ||
      this.syncSnap?.oauth_job?.status === 'code' ||
      this.syncSnap?.nas_browser?.status === 'loading' ||
      this.syncSnap?.transitioning;
    const interval = hasActiveSyncJob ? 850 : 2000;

    this.pollTimer = window.setTimeout(async () => {
      try {
        const res = await api.vaultPoll(this.workspace.active_vault);
        this.syncSnap = res.sync;
        this.updateSidebarSyncIndicators();
        if (this.mainView === 'sync' && this.syncCtrl) {
          this.syncCtrl.receiveSnapshot(res.sync);
        }
        if (
          this.workspace.active_vault &&
          res.vault_revision !== this.vaultRevision &&
          !this.savingPromise
        ) {
          this.vaultRevision = res.vault_revision;
          const freshScan = await api.vaultScan(this.workspace.active_vault);
          this.scan = freshScan;
          this.renderSidebar();
        }
      } catch {
        // ignore transient poll errors
      } finally {
        this.scheduleNextPoll();
      }
    }, interval);
  }

  // ───────────────────────── Main View Switching ─────────────────────────
  private async switchMainView(view: ActiveMainView): Promise<void> {
    if (this.isMobileViewport()) {
      this.setSidebarCollapsed(true);
    }
    if (this.mainView === 'editor' && view !== 'editor') {
      await this.flushSave(false);
    }
    this.mainView = view;

    this.editorViewWrapEl.style.display = view === 'editor' ? 'flex' : 'none';
    this.graphViewWrapEl.style.display = view === 'graph' ? 'block' : 'none';
    this.syncViewWrapEl.style.display = view === 'sync' ? 'flex' : 'none';

    if (view !== 'graph' && this.globalGraphCtrl) {
      this.globalGraphCtrl.destroy();
      this.globalGraphCtrl = null;
    }

    this.renderChrome();

    if (view === 'graph') {
      await this.mountGlobalGraphView();
    } else if (view === 'sync') {
      this.mountSyncCenterView();
    }
  }

  private renderChrome(): void {
    this.renderSidebar();
    this.renderTabsStrip();
    this.renderTopbar();
    this.renderInspector();
  }

  // ───────────────────────── Sidebar ─────────────────────────
  private getSyncStatusSummary(): { dotClass: string; label: string; pendingDeletes: number } {
    const s = this.syncSnap;
    const st = s?.status;
    const pendingDeletes =
      st?.profiles.reduce((sum, p) => sum + (p.pending_deletes || 0), 0) || 0;

    if (s?.service_running && st?.syncing) {
      return { dotClass: 'syncing', label: '동기화 중…', pendingDeletes };
    }
    if (s?.service_running && st?.last_error) {
      return { dotClass: 'danger', label: '확인 필요', pendingDeletes };
    }
    if (s?.service_running) {
      return { dotClass: 'ok', label: '동기화 켜짐', pendingDeletes };
    }
    if (s?.transitioning) {
      return { dotClass: 'warn', label: '전환 중…', pendingDeletes };
    }
    return { dotClass: 'muted', label: '동기화 꺼짐', pendingDeletes };
  }

  private updateSidebarSyncIndicators(): void {
    const { dotClass, label, pendingDeletes } = this.getSyncStatusSummary();
    const navDot = this.sidebarEl.querySelector('[data-sync-nav-dot]');
    if (navDot) navDot.className = `status-dot ${dotClass}`;

    const navBadge = this.sidebarEl.querySelector('[data-sync-pending-badge]') as HTMLElement | null;
    if (navBadge) {
      if (pendingDeletes > 0) {
        navBadge.style.display = 'inline-block';
        navBadge.textContent = `삭제보류 ${pendingDeletes}`;
      } else {
        navBadge.style.display = 'none';
      }
    }

    const footerTxt = this.sidebarEl.querySelector('[data-sync-footer-label]');
    if (footerTxt) footerTxt.textContent = label;
    const footerDot = this.sidebarEl.querySelector('[data-sync-footer-dot]');
    if (footerDot) footerDot.className = `status-dot ${dotClass}`;
  }

  private renderSidebar(): void {
    this.syncSidebarBackdrop();
    const activeVaultPath = this.workspace.active_vault;
    const activeVaultRef = this.workspace.vaults.find((v) => v.path === activeVaultPath);
    const vaultName = activeVaultRef
      ? activeVaultRef.name
      : activeVaultPath
        ? activeVaultPath.replace(/[/\\]+$/, '').split(/[/\\]/).pop() || '볼트'
        : '볼트 선택…';
    const avatarChar = (vaultName[0] || 'O').toUpperCase();
    const { dotClass, label: syncLabel, pendingDeletes } = this.getSyncStatusSummary();

    this.sidebarEl.innerHTML = `
      <div class="sidebar-vault-header">
        <button type="button" class="vault-switcher-btn" data-vault-switcher title="볼트 전환 / 새 폴더 열기">
          <span class="vault-avatar">${escapeHtml(avatarChar)}</span>
          <span class="vault-switcher-name">${escapeHtml(vaultName)}</span>
          <span style="font-size:11px;color:var(--text-muted);">▾</span>
        </button>
        <button type="button" class="icon-btn" data-quick-new-note title="새 노트 작성">✏️</button>
      </div>

      <div class="sidebar-nav">
        <button type="button" class="nav-item" data-cmd-palette>
          <span class="nav-icon">🔍</span>
          <span class="nav-label">빠른 검색 / 명령어</span>
          <span class="kbd-hint">Ctrl+K</span>
        </button>
        <div style="display:flex;align-items:center;gap:2px;">
          <button type="button" class="nav-item" style="flex:1;" data-daily-note title="저장 폴더: ${escapeAttr(this.getActiveDailyFolder() || '(루트)')}">
            <span class="nav-icon">📅</span>
            <span class="nav-label">오늘의 일일 노트</span>
            <span class="kbd-hint">Ctrl+D</span>
          </button>
          <button type="button" class="icon-btn" data-daily-note-settings title="일일 문서 저장 폴더 설정 (${escapeAttr(this.getActiveDailyFolder() || '루트')})">⚙️</button>
        </div>
        <button type="button" class="nav-item ${this.mainView === 'graph' ? 'active' : ''}" data-open-graph>
          <span class="nav-icon">🕸️</span>
          <span class="nav-label">그래프 뷰</span>
          <span class="kbd-hint">Ctrl+G</span>
        </button>
        <button type="button" class="nav-item ${this.mainView === 'sync' ? 'active' : ''}" data-open-sync>
          <span class="nav-icon">🔄</span>
          <span class="nav-label">동기화 센터</span>
          <span class="pill-badge warn" data-sync-pending-badge style="display:${
            pendingDeletes > 0 ? 'inline-block' : 'none'
          };">삭제보류 ${pendingDeletes}</span>
          <span class="status-dot ${dotClass}" data-sync-nav-dot></span>
        </button>
      </div>

      <div class="sidebar-scroll" data-sidebar-scroll></div>

      <div class="sidebar-footer">
        <button type="button" class="ghost-action-btn" data-vault-crypto ${!activeVaultPath ? 'disabled' : ''} title="볼트 전체 암호화/잠금 관리">
          🔐 암호화
        </button>
        <button type="button" class="ghost-action-btn" data-open-trash ${!activeVaultPath ? 'disabled' : ''}>
          🗑️ 휴지통
        </button>
        <button type="button" class="ghost-action-btn" data-footer-sync>
          <span class="status-dot ${dotClass}" data-sync-footer-dot></span>
          <span data-sync-footer-label>${escapeHtml(syncLabel)}</span>
        </button>
      </div>
    `;

    // Vault Switcher Dropdown
    const vaultBtn = this.sidebarEl.querySelector('[data-vault-switcher]') as HTMLElement;
    vaultBtn.addEventListener('click', (ev) => {
      const rect = vaultBtn.getBoundingClientRect();
      const entries: ContextMenuEntry[] = this.workspace.vaults.map((v) => ({
        icon: v.path === activeVaultPath ? '✓' : v.is_sync_profile ? '🔄' : '📁',
        label: `${v.name} (${v.path})`,
        onClick: async () => {
          if (v.path === activeVaultPath) return;
          await this.selectVaultPath(v.path);
        },
      }));
      if (entries.length > 0) {
        entries.push({ separator: true } as any);
      }
      entries.push(
        {
          icon: '📂',
          label: '폴더 열기… (기존 볼트 추가)',
          onClick: async () => {
            await this.handleOpenVaultDialog();
          },
        },
        {
          icon: '✨',
          label: '샘플 볼트 시작하기',
          onClick: async () => {
            await this.handleCreateSampleVault();
          },
        }
      );
      if (activeVaultPath) {
        const normActive = activeVaultPath.trim().replace(/\\/g, '/').toLowerCase();
        const linkedProf = this.syncSnap?.cfg.profiles.find(
          (p) => p.local_path.trim().replace(/\\/g, '/').toLowerCase() === normActive
        );
        entries.push(
          { separator: true } as any,
          {
            icon: '💻',
            label: '탐색기에서 볼트 폴더 열기',
            onClick: () => {
              void api.syncOpenPath(activeVaultPath);
            },
          },
          ...(linkedProf
            ? [
                {
                  icon: '🔌',
                  label: '이 볼트 동기화 연결 해제',
                  danger: true,
                  onClick: async () => {
                    try {
                      const res = await api.syncDisconnectProfile(linkedProf.id, false);
                      this.syncSnap = res.sync;
                      this.syncCtrl?.receiveSnapshot(res.sync, true);
                      this.updateSidebarSyncIndicators();
                      this.showToast(
                        '동기화 연결을 해제했습니다 (로컬 볼트 파일은 그대로 유지됩니다).'
                      );
                    } catch (err: any) {
                      this.showToast(err?.message || '동기화 연결 해제 실패', true);
                    }
                  },
                },
              ]
            : []),
          {
            icon: '✕',
            label: '목록에서 현재 볼트 닫기',
            danger: true,
            onClick: async () => {
              await this.flushSave(false);
              const res = await api.removeVault(activeVaultPath);
              this.workspace = res.workspace;
              this.scan = res.vault_scan;
              this.currentDoc = null;
              this.mountedNotePath = null;
              this.renderChrome();
              this.renderEditorMainArea(true);
            },
          }
        );
      }
      showContextMenu(rect.left, rect.bottom + 4, entries);
      ev.stopPropagation();
    });

    this.sidebarEl.querySelector('[data-quick-new-note]')?.addEventListener('click', () => {
      void this.promptCreateNote('');
    });
    this.sidebarEl.querySelector('[data-cmd-palette]')?.addEventListener('click', () => {
      this.openCommandPalette();
    });
    this.sidebarEl.querySelector('[data-daily-note]')?.addEventListener('click', () => {
      void this.openTodayDailyNote();
    });
    this.sidebarEl.querySelector('[data-daily-note-settings]')?.addEventListener('click', () => {
      this.openDailyNoteFolderDialog();
    });
    this.sidebarEl.querySelector('[data-vault-crypto]')?.addEventListener('click', () => {
      void this.openVaultCryptoDialog();
    });
    this.sidebarEl.querySelector('[data-open-graph]')?.addEventListener('click', () => {
      void this.switchMainView(this.mainView === 'graph' ? 'editor' : 'graph');
    });
    this.sidebarEl.querySelector('[data-open-sync]')?.addEventListener('click', () => {
      void this.switchMainView('sync');
    });
    this.sidebarEl.querySelector('[data-footer-sync]')?.addEventListener('click', () => {
      void this.switchMainView('sync');
    });
    this.sidebarEl.querySelector('[data-open-trash]')?.addEventListener('click', () => {
      if (!this.workspace.active_vault) return;
      void openTrashModal({
        vault: this.workspace.active_vault,
        onRestored: async (restoredPath) => {
          if (!this.workspace.active_vault) return;
          this.scan = await api.vaultScan(this.workspace.active_vault);
          this.renderSidebar();
          if (restoredPath.endsWith('.md')) {
            await this.openNote(restoredPath);
          }
        },
        onEmptied: () => {},
        onToast: (m, err) => this.showToast(m, err),
      });
    });

    // Populate Scrollable Tree + Favorites + Tags
    const scrollEl = this.sidebarEl.querySelector('[data-sidebar-scroll]')!;
    if (!activeVaultPath || !this.scan) {
      scrollEl.innerHTML = `
        <div style="padding:18px 10px;text-align:center;color:var(--text-muted);font-size:12.5px;">
          볼트 폴더를 열거나 샘플 볼트를 만들어 노트를 시작하세요.
        </div>
      `;
      return;
    }

    // 1. Favorites Section
    const favPaths = this.workspace.favorites.filter((fp) =>
      this.scan!.notes.some((n) => n.path === fp)
    );
    if (favPaths.length > 0) {
      const favSec = document.createElement('div');
      favSec.className = 'sidebar-section';
      favSec.innerHTML = `
        <div class="sidebar-section-header">
          <span>⭐ 즐겨찾기</span>
        </div>
        <div data-fav-list></div>
      `;
      const favList = favSec.querySelector('[data-fav-list]')!;
      for (const fp of favPaths) {
        const meta = this.scan.notes.find((n) => n.path === fp);
        if (!meta) continue;
        const row = document.createElement('div');
        row.className = `tree-row${
          this.mainView === 'editor' && this.currentDoc?.path === fp ? ' active' : ''
        }`;
        row.innerHTML = `
          <span class="tree-icon">${escapeHtml(meta.icon || '📄')}</span>
          <span class="tree-label">${escapeHtml(meta.title)}</span>
          <span class="tree-hover-actions">
            <button type="button" class="mini-icon-btn" title="즐겨찾기 해제">★</button>
          </span>
        `;
        row.addEventListener('click', () => {
          void this.openNote(fp);
        });
        row.querySelector('.mini-icon-btn')?.addEventListener('click', (ev) => {
          ev.stopPropagation();
          void this.toggleFavorite(fp);
        });
        favList.appendChild(row);
      }
      scrollEl.appendChild(favSec);
    }

    // 2. Pages Hierarchical Tree
    const pagesSec = document.createElement('div');
    pagesSec.className = 'sidebar-section';
    pagesSec.innerHTML = `
      <div class="sidebar-section-header">
        <span>📄 페이지 (${this.scan.total_notes})</span>
        <div class="sidebar-section-actions">
          <button type="button" class="mini-icon-btn" data-root-new-note title="새 노트">＋</button>
          <button type="button" class="mini-icon-btn" data-root-new-folder title="새 폴더">📁</button>
        </div>
      </div>
      <div data-tree-root></div>
    `;
    pagesSec.querySelector('[data-root-new-note]')?.addEventListener('click', () => {
      void this.promptCreateNote('');
    });
    pagesSec.querySelector('[data-root-new-folder]')?.addEventListener('click', () => {
      void this.promptCreateFolder('');
    });

    const treeRoot = pagesSec.querySelector('[data-tree-root]') as HTMLElement;
    if (this.scan.tree.length === 0) {
      treeRoot.innerHTML = `<div style="padding:8px 10px;font-size:12.5px;color:var(--text-muted);">아직 노트가 없습니다. ＋ 버튼을 눌러 첫 노트를 작성하세요.</div>`;
    } else {
      for (const node of this.scan.tree) {
        this.appendTreeNodeDom(treeRoot, node, 0);
      }
    }
    scrollEl.appendChild(pagesSec);

    // 3. Tags Section (hidden by default; toggle on demand)
    if (this.scan.tags.length > 0) {
      const tagsSec = document.createElement('div');
      tagsSec.className = 'sidebar-section';
      const showTags = Boolean(this.workspace.show_tags);
      tagsSec.innerHTML = `
        <div class="sidebar-section-header sidebar-tags-head">
          <span>🏷️ 태그 (${this.scan.tags.length})</span>
          <button type="button" class="tag-toggle-chip${showTags ? ' active' : ''}" data-toggle-sidebar-tags>
            ${showTags ? '숨기기' : '보기'}
          </button>
        </div>
        ${showTags ? '<div class="tag-cloud" data-tags-cloud></div>' : ''}
      `;
      tagsSec.querySelector('[data-toggle-sidebar-tags]')?.addEventListener('click', () => {
        void this.toggleShowTags();
      });
      if (showTags) {
        const cloud = tagsSec.querySelector('[data-tags-cloud]')!;
        for (const t of this.scan.tags) {
          const cleanTag = t.tag.replace(/^#+/, '');
          const chip = document.createElement('button');
          chip.type = 'button';
          chip.className = 'tag-chip';
          chip.innerHTML = `<span>#${escapeHtml(cleanTag)}</span><span class="tag-count">${t.count}</span>`;
          chip.addEventListener('click', () => {
            this.openCommandPalette(`tag:#${cleanTag}`);
          });
          cloud.appendChild(chip);
        }
      }
      scrollEl.appendChild(tagsSec);
    }
  }

  private appendTreeNodeDom(parentEl: HTMLElement, node: TreeNodeDto, depth: number): void {
    const isFolder = node.kind === 'folder';
    const isExpanded = isFolder && this.expandedFolders.has(node.path);
    const isActive =
      !isFolder && this.mainView === 'editor' && this.currentDoc?.path === node.path;

    const row = document.createElement('div');
    row.className = `tree-row${isActive ? ' active' : ''}`;
    row.style.paddingLeft = `${6 + depth * 14}px`;
    row.dataset.nodePath = node.path;
    row.dataset.nodeKind = node.kind;

    const displayLabel = isFolder ? node.name : stripMdExt(node.name);
    const lockPrefix = !isFolder && node.encrypted ? (node.locked ? '🔒 ' : '🔓 ') : '';
    const icon = isFolder ? (isExpanded ? '📂' : '📁') : node.icon || '📄';

    row.innerHTML = `
      <span class="tree-chevron ${isFolder ? (isExpanded ? 'expanded' : '') : 'spacer'}">▶</span>
      <span class="tree-icon">${escapeHtml(icon)}</span>
      <span class="tree-label">${escapeHtml(lockPrefix + displayLabel)}</span>
      <span class="tree-hover-actions">
        ${isFolder ? `<button type="button" class="mini-icon-btn" data-add-child title="이 폴더에 새 노트">＋</button>` : ''}
        <button type="button" class="mini-icon-btn" data-more-menu title="더보기">⋯</button>
      </span>
    `;

    row.addEventListener('click', () => {
      if (this.suppressTreeClickOnce) {
        this.suppressTreeClickOnce = false;
        return;
      }
      if (isFolder) {
        if (this.expandedFolders.has(node.path)) {
          this.expandedFolders.delete(node.path);
        } else {
          this.expandedFolders.add(node.path);
        }
        this.renderSidebar();
      } else {
        void this.openNote(node.path);
      }
    });

    row.addEventListener('pointerdown', (ev: PointerEvent) => {
      if (ev.button !== 0 || ev.pointerType !== 'mouse') return;
      if ((ev.target as HTMLElement | null)?.closest('button')) return;

      const startX = ev.clientX;
      const startY = ev.clientY;
      let started = false;
      let ghostEl: HTMLElement | null = null;
      let activeDropEl: HTMLElement | null = null;
      let dropTargetFolder: string | null = null;

      const clearDropHighlights = () => {
        if (activeDropEl) {
          activeDropEl.classList.remove('drop-target-folder', 'drop-target-root');
          activeDropEl = null;
        }
      };

      const clearExpandTimer = () => {
        if (this.dragAutoExpandTimer !== null) {
          window.clearTimeout(this.dragAutoExpandTimer);
          this.dragAutoExpandTimer = null;
        }
        this.lastHoverFolderPath = null;
      };

      const isInvalidTarget = (targetFolder: string): boolean => {
        if (targetFolder === parentFolderOf(node.path)) return true;
        if (isFolder && (targetFolder === node.path || targetFolder.startsWith(`${node.path}/`))) {
          return true;
        }
        return false;
      };

      const onMove = (me: PointerEvent) => {
        const dx = me.clientX - startX;
        const dy = me.clientY - startY;
        if (!started) {
          if (Math.hypot(dx, dy) < 6) return;
          started = true;
          this.suppressTreeClickOnce = true;
          row.classList.add('dragging');
          ghostEl = document.createElement('div');
          ghostEl.className = 'tree-drag-ghost';
          ghostEl.textContent = `${isFolder ? '📁' : '📄'} ${displayLabel}`;
          document.body.appendChild(ghostEl);
        }

        if (ghostEl) {
          ghostEl.style.left = `${me.clientX + 12}px`;
          ghostEl.style.top = `${me.clientY + 12}px`;
        }

        const hit = document.elementFromPoint(me.clientX, me.clientY) as HTMLElement | null;
        const hitRow = hit?.closest('.tree-row[data-node-path]') as HTMLElement | null;
        const hitTreeRoot = hit?.closest('[data-tree-root]') as HTMLElement | null;

        let nextTarget: string | null = null;
        let nextDropEl: HTMLElement | null = null;

        if (hitRow) {
          const hitPath = hitRow.dataset.nodePath || '';
          const hitKind = hitRow.dataset.nodeKind || '';
          if (hitKind === 'folder') {
            nextTarget = hitPath;
            nextDropEl = hitRow;
            if (!this.expandedFolders.has(hitPath) && hitPath !== this.lastHoverFolderPath) {
              clearExpandTimer();
              this.lastHoverFolderPath = hitPath;
              this.dragAutoExpandTimer = window.setTimeout(() => {
                this.dragAutoExpandTimer = null;
                this.expandedFolders.add(hitPath);
                this.renderSidebar();
                const draggedEl = this.sidebarEl.querySelector(
                  `.tree-row[data-node-path="${CSS.escape(node.path)}"]`
                );
                draggedEl?.classList.add('dragging');
              }, 550);
            }
          } else if (hitPath) {
            clearExpandTimer();
            nextTarget = parentFolderOf(hitPath);
            if (nextTarget) {
              nextDropEl = this.sidebarEl.querySelector(
                `.tree-row[data-node-kind="folder"][data-node-path="${CSS.escape(nextTarget)}"]`
              );
            } else if (hitTreeRoot) {
              nextDropEl = hitTreeRoot;
            }
          }
        } else if (hitTreeRoot) {
          clearExpandTimer();
          nextTarget = '';
          nextDropEl = hitTreeRoot;
        } else {
          clearExpandTimer();
        }

        if (nextTarget !== null && isInvalidTarget(nextTarget)) {
          nextTarget = null;
          nextDropEl = null;
        }

        dropTargetFolder = nextTarget;
        if (activeDropEl !== nextDropEl) {
          clearDropHighlights();
          if (nextDropEl) {
            nextDropEl.classList.add(
              nextDropEl.hasAttribute('data-tree-root') ? 'drop-target-root' : 'drop-target-folder'
            );
            activeDropEl = nextDropEl;
          }
        }
        if (ghostEl) {
          ghostEl.textContent =
            dropTargetFolder !== null
              ? `${isFolder ? '📁' : '📄'} ${displayLabel} → ${dropTargetFolder || '루트'}`
              : `${isFolder ? '📁' : '📄'} ${displayLabel}`;
        }
      };

      const onUp = () => {
        window.removeEventListener('pointermove', onMove, true);
        window.removeEventListener('pointerup', onUp, true);
        window.removeEventListener('pointercancel', onUp, true);
        clearExpandTimer();
        clearDropHighlights();
        this.sidebarEl
          .querySelectorAll('.tree-row.dragging')
          .forEach((el) => el.classList.remove('dragging'));
        ghostEl?.remove();
        ghostEl = null;

        if (started) {
          window.setTimeout(() => {
            this.suppressTreeClickOnce = false;
          }, 80);
          if (dropTargetFolder !== null) {
            void this.executeMoveVaultItem(node.path, dropTargetFolder);
          }
        }
      };

      window.addEventListener('pointermove', onMove, true);
      window.addEventListener('pointerup', onUp, true);
      window.addEventListener('pointercancel', onUp, true);
    });

    row.addEventListener('contextmenu', (ev) => {
      ev.preventDefault();
      this.openTreeNodeContextMenu(ev.clientX, ev.clientY, node);
    });

    row.querySelector('[data-add-child]')?.addEventListener('click', (ev) => {
      ev.stopPropagation();
      this.expandedFolders.add(node.path);
      void this.promptCreateNote(node.path);
    });

    row.querySelector('[data-more-menu]')?.addEventListener('click', (ev) => {
      ev.stopPropagation();
      const r = (ev.currentTarget as HTMLElement).getBoundingClientRect();
      this.openTreeNodeContextMenu(r.left, r.bottom + 2, node);
    });

    parentEl.appendChild(row);

    if (isFolder && isExpanded) {
      for (const child of node.children) {
        this.appendTreeNodeDom(parentEl, child, depth + 1);
      }
    }
  }

  private openTreeNodeContextMenu(x: number, y: number, node: TreeNodeDto): void {
    const isFolder = node.kind === 'folder';
    const isFav = this.workspace.favorites.includes(node.path);

    if (isFolder) {
      showContextMenu(x, y, [
        {
          icon: '📝',
          label: '새 노트 만들기',
          onClick: () => this.promptCreateNote(node.path),
        },
        {
          icon: '📁',
          label: '새 하위 폴더 만들기',
          onClick: () => this.promptCreateFolder(node.path),
        },
        { separator: true },
        {
          icon: '✏️',
          label: '이름 변경',
          onClick: () => this.promptRenameItem(node.path, node.name, true),
        },
        {
          icon: '📦',
          label: '폴더 이동',
          onClick: () => this.promptMoveItem(node.path, node.name),
        },
        { separator: true },
        {
          icon: '🗑️',
          label: '휴지통으로 이동',
          danger: true,
          onClick: () => this.deleteVaultItem(node.path),
        },
      ]);
    } else {
      const baseTitle = stripMdExt(node.name);
      showContextMenu(x, y, [
        {
          icon: isFav ? '☆' : '⭐',
          label: isFav ? '즐겨찾기 해제' : '즐겨찾기에 고정',
          onClick: () => this.toggleFavorite(node.path),
        },
        {
          icon: '◫',
          label: '우측 분할 보기로 열기',
          onClick: () => this.openSplitNote(node.path),
        },
        { separator: true },
        {
          icon: '✏️',
          label: '이름 변경 (위키링크 자동 갱신)',
          onClick: () => this.promptRenameItem(node.path, baseTitle, false),
        },
        {
          icon: '📑',
          label: '노트 복제',
          onClick: () => this.duplicateNoteItem(node.path),
        },
        {
          icon: '📦',
          label: '폴더 이동',
          onClick: () => this.promptMoveItem(node.path, baseTitle),
        },
        {
          icon: '📝',
          label: '같은 폴더에 새 노트',
          onClick: () => this.promptCreateNote(parentFolderOf(node.path)),
        },
        {
          icon: '🔐',
          label: '암호화 / 잠금 설정…',
          onClick: () => this.openNoteCryptoDialog(node.path),
        },
        { separator: true },
        {
          icon: '🗑️',
          label: '휴지통으로 이동',
          danger: true,
          onClick: () => this.deleteVaultItem(node.path),
        },
      ]);
    }
  }

  // ───────────────────────── Tabs Strip & Topbar ─────────────────────────
  private renderTabsStrip(): void {
    this.tabsStripEl.innerHTML = '';

    const toggleSidebarBtn = document.createElement('button');
    toggleSidebarBtn.type = 'button';
    toggleSidebarBtn.className = 'icon-btn';
    toggleSidebarBtn.title = '사이드바 접기/펼치기 (Ctrl+\\)';
    toggleSidebarBtn.textContent = '☰';
    toggleSidebarBtn.addEventListener('click', () => {
      this.setSidebarCollapsed(!this.sidebarCollapsed);
      requestAnimationFrame(() => this.applyPageLayoutVariables());
    });
    this.tabsStripEl.appendChild(toggleSidebarBtn);

    const scroll = document.createElement('div');
    scroll.className = 'tabs-scroll';

    for (const tabPath of this.workspace.open_tabs) {
      const meta = this.scan?.notes.find((n) => n.path === tabPath);
      const title = meta?.title || stripMdExt(tabPath.split('/').pop() || tabPath);
      const icon = meta?.icon || '📄';
      const lockBadge = meta?.encrypted ? (meta.locked ? '🔒 ' : '🔓 ') : '';
      const active = this.mainView === 'editor' && this.currentDoc?.path === tabPath;

      const pill = document.createElement('div');
      pill.className = `tab-pill${active ? ' active' : ''}`;
      pill.innerHTML = `
        <span>${escapeHtml(icon)}</span>
        <span class="tab-pill-title">${escapeHtml(lockBadge + title)}</span>
        <button type="button" class="tab-close" title="탭 닫기">✕</button>
      `;
      pill.addEventListener('click', () => {
        void this.openNote(tabPath);
      });
      pill.addEventListener('auxclick', (ev) => {
        if (ev.button === 1) {
          ev.preventDefault();
          void this.closeTab(tabPath);
        }
      });
      pill.querySelector('.tab-close')?.addEventListener('click', (ev) => {
        ev.stopPropagation();
        void this.closeTab(tabPath);
      });
      scroll.appendChild(pill);
    }

    const newTabBtn = document.createElement('button');
    newTabBtn.type = 'button';
    newTabBtn.className = 'icon-btn';
    newTabBtn.title = '새 노트 만들기';
    newTabBtn.textContent = '＋';
    newTabBtn.addEventListener('click', () => {
      const folder = this.currentDoc ? parentFolderOf(this.currentDoc.path) : '';
      void this.promptCreateNote(folder);
    });

    this.tabsStripEl.append(scroll, newTabBtn);
  }

  private renderTopbar(): void {
    this.topbarEl.innerHTML = '';

    // Left: Breadcrumbs
    const bc = document.createElement('div');
    bc.className = 'breadcrumbs';
    if (this.mainView === 'graph') {
      bc.innerHTML = `<span class="breadcrumb-seg current">🕸️ 전체 볼트 지식 그래프</span>`;
    } else if (this.mainView === 'sync') {
      bc.innerHTML = `<span class="breadcrumb-seg current">🔄 실시간 동기화 센터</span>`;
    } else if (this.currentDoc) {
      const segs = this.currentDoc.path.split('/');
      segs.forEach((seg, idx) => {
        if (idx > 0) {
          const sep = document.createElement('span');
          sep.className = 'breadcrumb-sep';
          sep.textContent = '›';
          bc.appendChild(sep);
        }
        const span = document.createElement('span');
        const isLast = idx === segs.length - 1;
        span.className = `breadcrumb-seg${isLast ? ' current' : ''}`;
        span.textContent = isLast
          ? `${this.draftIcon || '📄'} ${stripMdExt(seg)}`
          : `📁 ${seg}`;
        bc.appendChild(span);
      });
    } else {
      bc.innerHTML = `<span class="breadcrumb-seg">노트를 선택하거나 새로 작성하세요</span>`;
    }
    this.topbarEl.appendChild(bc);

    // Right controls
    const right = document.createElement('div');
    right.className = 'topbar-right';

    if (this.mainView === 'editor' && this.currentDoc) {
      // Save status badge
      const saveBadge = document.createElement('span');
      saveBadge.id = 'topbar-save-badge';
      this.applySaveBadgeContent(saveBadge);
      right.appendChild(saveBadge);

      if (!this.currentDoc.locked) {
        // Prominent Editor Mode Switcher
        const modeGroup = document.createElement('div');
        modeGroup.className = 'mode-switcher';
        const wwBtn = document.createElement('button');
        wwBtn.type = 'button';
        wwBtn.className = `mode-btn${this.workspace.editor_mode === 'wysiwyg' ? ' active' : ''}`;
        wwBtn.textContent = '위지윅 모드';
        wwBtn.title = 'Notion처럼 바로 편집';
        wwBtn.addEventListener('click', () => void this.changeEditorMode('wysiwyg'));

        const mdBtn = document.createElement('button');
        mdBtn.type = 'button';
        mdBtn.className = `mode-btn${this.workspace.editor_mode === 'markdown' ? ' active' : ''}`;
        mdBtn.textContent = '마크다운 분할';
        mdBtn.title = '좌측 마크다운 + 우측 실시간 프리뷰';
        mdBtn.addEventListener('click', () => void this.changeEditorMode('markdown'));

        modeGroup.append(wwBtn, mdBtn);
        right.appendChild(modeGroup);

        // Page Setup (A4 / Margins)
        const pageSetupBtn = document.createElement('button');
        pageSetupBtn.type = 'button';
        pageSetupBtn.className = `icon-btn${this.workspace.page_layout !== 'full' ? ' active' : ''}`;
        pageSetupBtn.title = 'A4 용지 및 인쇄/PDF 여백 설정';
        pageSetupBtn.textContent = '📐';
        pageSetupBtn.addEventListener('click', () => this.openPageSetupDialog());
        right.appendChild(pageSetupBtn);

        // Print & PDF
        const printBtn = document.createElement('button');
        printBtn.type = 'button';
        printBtn.className = 'icon-btn';
        printBtn.title = '인쇄 (Ctrl+P)';
        printBtn.textContent = '🖨️';
        printBtn.addEventListener('click', () => void this.handlePrintNote());
        right.appendChild(printBtn);

        const pdfBtn = document.createElement('button');
        pdfBtn.type = 'button';
        pdfBtn.className = 'icon-btn';
        pdfBtn.title = 'PDF 문서로 내보내기';
        pdfBtn.textContent = '📄PDF';
        pdfBtn.style.fontSize = '11px';
        pdfBtn.style.fontWeight = '700';
        pdfBtn.addEventListener('click', () => void this.handleExportPdf());
        right.appendChild(pdfBtn);
      }

      // Note Crypto Lock / Encrypt button
      const cryptoBtn = document.createElement('button');
      cryptoBtn.type = 'button';
      cryptoBtn.className = `icon-btn${this.currentDoc.encrypted ? ' active' : ''}`;
      cryptoBtn.title = this.currentDoc.encrypted
        ? this.currentDoc.locked
          ? '노트 잠김 (클릭하여 잠금 해제/관리)'
          : '노트 암호화됨 · 세션 해제 (클릭하여 잠그기/관리)'
        : '현재 노트 암호화';
      cryptoBtn.textContent = this.currentDoc.encrypted
        ? this.currentDoc.locked
          ? '🔒'
          : '🔓'
        : '🔐';
      cryptoBtn.addEventListener('click', () => void this.openNoteCryptoDialog());
      right.appendChild(cryptoBtn);

      // Split View toggle
      const splitBtn = document.createElement('button');
      splitBtn.type = 'button';
      splitBtn.className = `icon-btn${this.workspace.split_tab ? ' active' : ''}`;
      splitBtn.title = '우측 분할 보기 토글';
      splitBtn.textContent = '◫';
      splitBtn.addEventListener('click', () => {
        if (this.workspace.split_tab) {
          void this.closeSplitNote();
        } else {
          const fallback =
            this.workspace.open_tabs.find((t) => t !== this.currentDoc?.path) ||
            this.currentDoc?.path;
          if (fallback) void this.openSplitNote(fallback);
        }
      });
      right.appendChild(splitBtn);
    }

    // Tag visibility toggle
    if (this.mainView === 'editor') {
      const tagToggleBtn = document.createElement('button');
      tagToggleBtn.type = 'button';
      tagToggleBtn.className = `icon-btn${this.workspace.show_tags ? ' active' : ''}`;
      tagToggleBtn.title = this.workspace.show_tags
        ? '태그 숨기기'
        : '태그 보기 (사이드바 및 문서 속성)';
      tagToggleBtn.textContent = '🏷️';
      tagToggleBtn.addEventListener('click', () => void this.toggleShowTags());
      right.appendChild(tagToggleBtn);
    }

    // Theme Toggle
    const themeBtn = document.createElement('button');
    themeBtn.type = 'button';
    themeBtn.className = 'icon-btn';
    themeBtn.title = '라이트 / 다크 테마 전환';
    themeBtn.textContent = this.workspace.theme === 'dark' ? '☀️' : '🌙';
    themeBtn.addEventListener('click', async () => {
      const next = this.workspace.theme === 'dark' ? 'light' : 'dark';
      this.applyTheme(next);
      await this.persistWorkspace({ theme: next });
      this.renderTopbar();
    });
    right.appendChild(themeBtn);

    // Inspector Toggle
    if (this.mainView === 'editor') {
      const inspBtn = document.createElement('button');
      inspBtn.type = 'button';
      inspBtn.className = `icon-btn${this.inspectorOpen ? ' active' : ''}`;
      inspBtn.title = '우측 인스펙터 패널 (백링크·목차·로컬 그래프)';
      inspBtn.textContent = '◨';
      inspBtn.addEventListener('click', () => {
        this.inspectorOpen = !this.inspectorOpen;
        this.renderInspector();
        this.renderTopbar();
        requestAnimationFrame(() => this.applyPageLayoutVariables());
      });
      right.appendChild(inspBtn);
    }

    this.topbarEl.appendChild(right);
  }

  private applySaveBadgeContent(el: HTMLElement): void {
    if (this.saveStatus === 'saving') {
      el.className = 'save-badge saving';
      el.textContent = '저장 중…';
    } else if (this.saveStatus === 'dirty') {
      el.className = 'save-badge dirty';
      el.textContent = '● 변경됨';
    } else if (this.saveStatus === 'error') {
      el.className = 'save-badge error';
      el.textContent = '⚠ 저장 오류';
    } else {
      el.className = 'save-badge';
      el.textContent = '✓ 저장됨';
    }
  }

  private updateSaveStatus(status: SaveStatus): void {
    this.saveStatus = status;
    const badge = document.getElementById('topbar-save-badge');
    if (badge) {
      this.applySaveBadgeContent(badge);
    }
  }

  private async changeEditorMode(mode: 'wysiwyg' | 'markdown'): Promise<void> {
    if (this.workspace.editor_mode === mode) return;
    await this.persistWorkspace({ editor_mode: mode });
    this.editorCtrl?.setMode(mode);
    const col = this.editorPaneEl.querySelector('.notion-page-column');
    const mountWrap = this.editorPaneEl.querySelector('.editor-mount-wrap');
    col?.classList.toggle('wide-md', mode === 'markdown');
    mountWrap?.classList.toggle('wide-md', mode === 'markdown');
    this.renderTopbar();
    requestAnimationFrame(() => this.applyPageLayoutVariables());
  }

  // ───────────────────────── Vault & Note Operations ─────────────────────────
  private async selectVaultPath(path: string): Promise<void> {
    await this.flushSave(false);
    try {
      const res = await api.selectVault(path);
      this.workspace = res.workspace;
      this.scan = res.vault_scan;
      this.currentDoc = null;
      this.mountedNotePath = null;
      this.expandedFolders.clear();
      this.expandInitialFolders();
      await this.switchMainView('editor');

      const first =
        this.workspace.active_tab ||
        this.workspace.open_tabs[0] ||
        this.scan?.notes[0]?.path ||
        null;
      if (first) {
        await this.openNote(first);
      } else {
        this.renderEditorMainArea(true);
      }
    } catch (err: any) {
      this.showToast(err?.message || '볼트 열기 실패', true);
    }
  }

  private async handleOpenVaultDialog(): Promise<void> {
    await this.flushSave(false);
    try {
      const res = await api.openVaultDialog();
      if (!res.picked) return;
      this.workspace = res.workspace;
      this.scan = res.vault_scan;
      this.currentDoc = null;
      this.mountedNotePath = null;
      this.expandedFolders.clear();
      this.expandInitialFolders();
      await this.switchMainView('editor');

      const first = this.scan?.notes[0]?.path || null;
      if (first) {
        await this.openNote(first);
      } else {
        this.renderEditorMainArea(true);
      }
      this.showToast(`볼트를 열었습니다: ${res.picked}`);
    } catch (err: any) {
      this.showToast(err?.message || '볼트 폴더 열기 실패', true);
    }
  }

  private async handleCreateSampleVault(): Promise<void> {
    await this.flushSave(false);
    try {
      const res = await api.createSampleVault();
      this.workspace = res.workspace;
      this.scan = res.vault_scan;
      this.currentDoc = null;
      this.mountedNotePath = null;
      this.expandedFolders.clear();
      this.expandInitialFolders();
      await this.switchMainView('editor');

      const first =
        this.workspace.active_tab || this.scan?.notes[0]?.path || null;
      if (first) {
        await this.openNote(first);
      } else {
        this.renderEditorMainArea(true);
      }
      this.showToast('샘플 볼트가 생성되었습니다! 바로 편집해 보세요.');
    } catch (err: any) {
      this.showToast(err?.message || '샘플 볼트 생성 실패', true);
    }
  }

  public async openNote(relPath: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    if (this.isMobileViewport()) {
      this.setSidebarCollapsed(true);
    }
    if (this.mainView !== 'editor') {
      await this.switchMainView('editor');
    }
    if (this.currentDoc?.path === relPath && this.mountedNotePath === relPath) {
      return;
    }

    await this.flushSave(false);

    try {
      const doc = await api.noteRead(this.workspace.active_vault, relPath);
      this.currentDoc = doc;
      this.draftBody = doc.body;
      this.draftIcon = doc.icon;
      this.draftCover = doc.cover;
      this.draftProperties = doc.properties.map((p) => ({ ...p }));
      this.saveStatus = 'saved';

      this.expandAncestorsOf(relPath);

      const openTabs = [...this.workspace.open_tabs];
      if (!openTabs.includes(relPath)) {
        openTabs.push(relPath);
      }
      const recent = [relPath, ...this.workspace.recent_notes.filter((x) => x !== relPath)].slice(
        0,
        20
      );
      await this.persistWorkspace({
        open_tabs: openTabs,
        active_tab: relPath,
        recent_notes: recent,
      });

      this.renderChrome();
      this.renderEditorMainArea(true);
      this.renderInspector();
    } catch (err: any) {
      this.showToast(err?.message || `노트를 열 수 없습니다: ${relPath}`, true);
    }
  }

  private async closeTab(relPath: string): Promise<void> {
    if (this.currentDoc?.path === relPath) {
      await this.flushSave(false);
    }
    const idx = this.workspace.open_tabs.indexOf(relPath);
    const nextTabs = this.workspace.open_tabs.filter((t) => t !== relPath);
    let nextActive = this.workspace.active_tab;

    if (this.currentDoc?.path === relPath) {
      nextActive = nextTabs[Math.max(0, idx - 1)] || nextTabs[0] || null;
    }

    await this.persistWorkspace({
      open_tabs: nextTabs,
      active_tab: nextActive,
    });

    if (nextActive && nextActive !== this.currentDoc?.path) {
      await this.openNote(nextActive);
    } else if (!nextActive) {
      this.currentDoc = null;
      this.mountedNotePath = null;
      this.editorCtrl?.destroy();
      this.editorCtrl = null;
      this.renderChrome();
      this.renderEditorMainArea(true);
      this.renderInspector();
    } else {
      this.renderTabsStrip();
    }
  }

  private scheduleAutoSave(): void {
    if (!this.currentDoc || this.currentDoc.locked) return;
    this.updateSaveStatus('dirty');
    if (this.autoSaveTimer !== null) {
      window.clearTimeout(this.autoSaveTimer);
    }
    this.autoSaveTimer = window.setTimeout(() => {
      this.autoSaveTimer = null;
      void this.flushSave(false);
    }, 900);
  }

  private async flushSave(manualToast: boolean): Promise<void> {
    if (this.autoSaveTimer !== null) {
      window.clearTimeout(this.autoSaveTimer);
      this.autoSaveTimer = null;
    }
    if (!this.workspace.active_vault || !this.currentDoc || this.currentDoc.locked) return;
    if (this.saveStatus !== 'dirty' && !manualToast) return;

    if (this.savingPromise) {
      await this.savingPromise;
    }

    const vault = this.workspace.active_vault;
    const path = this.currentDoc.path;
    const body = this.editorCtrl ? this.editorCtrl.getCleanMarkdown() : this.draftBody;
    this.draftBody = body;

    this.updateSaveStatus('saving');

    this.savingPromise = (async () => {
      try {
        const updatedDoc = await api.noteSave({
          vault,
          path,
          icon: this.draftIcon,
          cover: this.draftCover,
          properties: this.draftProperties,
          body,
        });
        if (this.currentDoc && this.currentDoc.path === path) {
          this.currentDoc = updatedDoc;
          this.updatePageFooterStats();
          if (this.showRawYaml) {
            this.renderRawYamlPreview();
          }
          // Refresh scan tags/icons subtly & update inspector without remounting editor!
          const freshScan = await api.vaultScan(vault);
          this.scan = freshScan;
          this.vaultRevision = freshScan.revision;
          this.renderSidebar();
          this.renderTabsStrip();
          this.renderInspector();
        }
        this.updateSaveStatus('saved');
        if (manualToast) {
          this.showToast('저장되었습니다.');
        }
      } catch (err: any) {
        this.updateSaveStatus('error');
        this.showToast(err?.message || '노트 저장 실패', true);
      } finally {
        this.savingPromise = null;
      }
    })();

    await this.savingPromise;
  }

  private async openWikiLinkByTitle(rawTitle: string): Promise<void> {
    if (!this.workspace.active_vault || !this.scan) return;
    const clean = rawTitle.split('#')[0].trim();
    if (!clean) return;

    const exact =
      this.scan.notes.find(
        (n) =>
          n.title.toLowerCase() === clean.toLowerCase() ||
          stripMdExt(n.path).toLowerCase() === clean.toLowerCase()
      ) || null;

    if (exact) {
      await this.openNote(exact.path);
      return;
    }

    // Create the linked note automatically if it doesn't exist yet (Obsidian ghost-link behavior!)
    try {
      const folder = this.currentDoc ? parentFolderOf(this.currentDoc.path) : '';
      const created = await api.noteCreate({
        vault: this.workspace.active_vault,
        folder,
        title: clean,
      });
      this.scan = created.scan;
      this.vaultRevision = created.scan.revision;
      await this.openNote(created.path);
      this.showToast(`새 연결 노트를 생성했습니다: ${clean}`);
    } catch (err: any) {
      this.showToast(err?.message || '연결된 노트 생성 실패', true);
    }
  }

  private async openTodayDailyNote(): Promise<void> {
    if (this.isMobileViewport()) {
      this.setSidebarCollapsed(true);
    }
    if (!this.workspace.active_vault) {
      this.showToast('먼저 볼트 폴더를 열어 주세요.', true);
      return;
    }
    try {
      const dailyFolder = this.getActiveDailyFolder();
      const res = await api.noteDaily(this.workspace.active_vault, dailyFolder);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;
      if (dailyFolder) {
        this.expandedFolders.add(dailyFolder);
      }
      await this.openNote(res.path);
    } catch (err: any) {
      this.showToast(err?.message || '오늘의 일일 노트 열기 실패', true);
    }
  }

  private async promptCreateNote(folder: string): Promise<void> {
    if (!this.workspace.active_vault) {
      this.showToast('먼저 볼트 폴더를 열어 주세요.', true);
      return;
    }
    const title = await showTextPromptModal({
      title: folder ? `📝 새 노트 (${folder})` : '📝 새 노트 만들기',
      label: '새 노트 제목을 입력하세요',
      placeholder: '예: 프로젝트 아이디어',
      confirmText: '만들기',
    });
    if (!title) return;

    try {
      const res = await api.noteCreate({
        vault: this.workspace.active_vault,
        folder,
        title,
      });
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;
      if (folder) this.expandedFolders.add(folder);
      await this.openNote(res.path);
    } catch (err: any) {
      this.showToast(err?.message || '노트 생성 실패', true);
    }
  }

  private async promptCreateFolder(parent: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    const name = await showTextPromptModal({
      title: parent ? `📁 새 하위 폴더 (${parent})` : '📁 새 폴더 만들기',
      label: '폴더 이름을 입력하세요',
      placeholder: '예: Projects',
      confirmText: '만들기',
    });
    if (!name) return;

    try {
      const res = await api.createFolder(this.workspace.active_vault, parent, name);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;
      if (parent) this.expandedFolders.add(parent);
      this.expandedFolders.add(res.path);
      this.renderSidebar();
      this.showToast(`폴더 생성됨: ${res.path}`);
    } catch (err: any) {
      this.showToast(err?.message || '폴더 생성 실패', true);
    }
  }

  private async promptRenameItem(
    oldPath: string,
    currentName: string,
    isFolder: boolean
  ): Promise<void> {
    const newName = await showTextPromptModal({
      title: isFolder ? '✏️ 폴더 이름 변경' : '✏️ 노트 이름 변경 (위키링크 자동 갱신)',
      label: '새 이름을 입력하세요',
      initialValue: currentName,
      confirmText: '변경',
    });
    if (!newName || newName === currentName) return;
    await this.executeRename(oldPath, newName);
  }

  private async executeRename(oldPath: string, newName: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    await this.flushSave(false);
    try {
      const res = await api.renameItem(this.workspace.active_vault, oldPath, newName);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;

      // Update tabs & favorites if renamed
      const remap = (p: string) =>
        p === oldPath
          ? res.new_path
          : p.startsWith(`${oldPath}/`)
            ? `${res.new_path}${p.slice(oldPath.length)}`
            : p;

      const openTabs = this.workspace.open_tabs.map(remap);
      const favorites = this.workspace.favorites.map(remap);
      const activeTab = this.workspace.active_tab ? remap(this.workspace.active_tab) : null;
      const splitTab = this.workspace.split_tab ? remap(this.workspace.split_tab) : null;

      await this.persistWorkspace({
        open_tabs: openTabs,
        favorites,
        active_tab: activeTab,
        split_tab: splitTab,
      });

      if (this.currentDoc && (this.currentDoc.path === oldPath || this.currentDoc.path.startsWith(`${oldPath}/`))) {
        const updatedPath = remap(this.currentDoc.path);
        this.currentDoc.path = updatedPath;
        this.currentDoc.title = stripMdExt(updatedPath.split('/').pop() || newName);
        this.mountedNotePath = updatedPath;
      }

      this.renderChrome();
      this.renderInspector();
      const linkNotice =
        res.updated_links > 0 ? ` (연결된 위키링크 ${res.updated_links}곳 자동 갱신됨)` : '';
      this.showToast(`이름을 변경했습니다.${linkNotice}`);
    } catch (err: any) {
      this.showToast(err?.message || '이름 변경 실패', true);
    }
  }

  private async duplicateNoteItem(path: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    await this.flushSave(false);
    try {
      const res = await api.noteDuplicate(this.workspace.active_vault, path);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;
      await this.openNote(res.path);
      this.showToast(`노트를 복제했습니다: ${res.path}`);
    } catch (err: any) {
      this.showToast(err?.message || '노트 복제 실패', true);
    }
  }

  private async promptMoveItem(path: string, displayName: string): Promise<void> {
    if (!this.workspace.active_vault || !this.scan) return;
    const folders = collectFoldersFromTree(this.scan.tree).filter(
      (f) => f !== path && !f.startsWith(`${path}/`)
    );
    const target = await showFolderMoveModal({
      itemName: displayName,
      folders,
      currentFolder: parentFolderOf(path),
    });
    if (target === null || target === parentFolderOf(path)) return;
    await this.executeMoveVaultItem(path, target);
  }

  private async executeMoveVaultItem(path: string, targetFolder: string): Promise<void> {
    if (!this.workspace.active_vault || !this.scan) return;
    if (targetFolder === parentFolderOf(path)) return;
    if (targetFolder === path || targetFolder.startsWith(`${path}/`)) return;

    await this.flushSave(false);
    try {
      const res = await api.moveItem(this.workspace.active_vault, path, targetFolder);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;
      if (targetFolder) this.expandedFolders.add(targetFolder);

      const remap = (p: string): string => {
        if (p === path) return res.new_path;
        if (p.startsWith(`${path}/`)) return `${res.new_path}${p.slice(path.length)}`;
        return p;
      };

      const nextExpanded = new Set<string>();
      for (const f of this.expandedFolders) {
        nextExpanded.add(remap(f));
      }
      this.expandedFolders = nextExpanded;

      const openTabs = this.workspace.open_tabs.map(remap);
      const activeTab = this.workspace.active_tab ? remap(this.workspace.active_tab) : null;
      const splitTab = this.workspace.split_tab ? remap(this.workspace.split_tab) : null;
      const favorites = this.workspace.favorites.map(remap);
      const recentNotes = this.workspace.recent_notes.map(remap);

      await this.persistWorkspace({
        open_tabs: openTabs,
        active_tab: activeTab,
        split_tab: splitTab,
        favorites,
        recent_notes: recentNotes,
      });

      if (this.currentDoc) {
        this.currentDoc.path = remap(this.currentDoc.path);
      }
      if (this.mountedNotePath) {
        this.mountedNotePath = remap(this.mountedNotePath);
      }
      if (this.splitDoc) {
        this.splitDoc.path = remap(this.splitDoc.path);
      }
      this.renderChrome();
      this.showToast(`이동 완료: ${res.new_path}`);
    } catch (err: any) {
      this.showToast(err?.message || '폴더 이동 실패', true);
    }
  }

  private async deleteVaultItem(path: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    try {
      const res = await api.deleteItem(this.workspace.active_vault, path);
      this.scan = res.scan;
      this.vaultRevision = res.scan.revision;

      const matches = (p: string) => p === path || p.startsWith(`${path}/`);
      const nextTabs = this.workspace.open_tabs.filter((t) => !matches(t));
      const nextFavs = this.workspace.favorites.filter((f) => !matches(f));
      const nextRecents = this.workspace.recent_notes.filter((r) => !matches(r));
      const nextSplit =
        this.workspace.split_tab && matches(this.workspace.split_tab)
          ? null
          : this.workspace.split_tab;

      if (this.currentDoc && matches(this.currentDoc.path)) {
        const nextActive = nextTabs[0] || null;
        await this.persistWorkspace({
          open_tabs: nextTabs,
          active_tab: nextActive,
          favorites: nextFavs,
          recent_notes: nextRecents,
          split_tab: nextSplit,
        });
        if (nextActive) {
          await this.openNote(nextActive);
        } else {
          this.currentDoc = null;
          this.mountedNotePath = null;
          this.editorCtrl?.destroy();
          this.editorCtrl = null;
          this.renderChrome();
          this.renderEditorMainArea(true);
          this.renderInspector();
        }
      } else {
        await this.persistWorkspace({
          open_tabs: nextTabs,
          favorites: nextFavs,
          recent_notes: nextRecents,
          split_tab: nextSplit,
        });
        this.renderSidebar();
        this.renderTabsStrip();
      }
      this.showToast(`휴지통(.trash)으로 이동했습니다: ${path}`);
    } catch (err: any) {
      this.showToast(err?.message || '삭제 실패', true);
    }
  }

  private async toggleFavorite(path: string): Promise<void> {
    const exists = this.workspace.favorites.includes(path);
    const next = exists
      ? this.workspace.favorites.filter((x) => x !== path)
      : [...this.workspace.favorites, path];
    await this.persistWorkspace({ favorites: next });
    this.renderSidebar();
    if (this.currentDoc?.path === path) {
      this.renderNotionHeaderZone();
    }
  }

  private hideEditorToolbarDock(): void {
    if (this.editorToolbarDockEl) {
      this.editorToolbarDockEl.style.display = 'none';
      this.editorToolbarDockEl.innerHTML = '';
      this.editorToolbarDockEl.classList.remove('toastui-editor-dark');
    }
  }

  // ───────────────────────── Notion Page Header + Editor Rendering ─────────────────────────
  private renderEditorMainArea(remountEditor: boolean): void {
    if (!this.workspace.active_vault) {
      this.editorCtrl?.destroy();
      this.editorCtrl = null;
      this.mountedNotePath = null;
      this.hideEditorToolbarDock();
      const isAndroid = isAndroidApp();
      const needsFilesPerm = isAndroid && this.androidInfo && !this.androidInfo.all_files_granted;
      this.editorPaneEl.innerHTML = `
        <div class="empty-vault-hero">
          <div style="font-size:48px;margin-bottom:10px;">📓</div>
          <h2 style="margin:0 0 8px;font-size:22px;">Onyx Studio에 오신 것을 환영합니다</h2>
          <p style="color:var(--text-secondary);font-size:14px;margin:0 0 22px;line-height:1.6;">
            노션처럼 직관적인 위지윅 편집기, 마크다운 실시간 분할 프리뷰, 위키링크 &amp; 그래프 뷰,
            그리고 6종 클라우드/NAS 실시간 동기화를 한 곳에서 사용하세요.
          </p>
          ${
            needsFilesPerm
              ? `<div style="margin:0 0 18px;padding:10px 14px;border-radius:8px;background:var(--warn-bg);color:var(--warn);font-size:13px;line-height:1.5;">
                  ⚠ 기기 내 Obsidian 볼트 폴더를 읽고 쓰려면 <strong>'모든 파일 접근 권한'</strong> 허용이 필요합니다.<br/>
                  <button type="button" class="btn btn-sm" style="margin-top:8px;" data-hero-files-perm>🔓 모든 파일 접근 권한 허용하기</button>
                </div>`
              : ''
          }
          <div style="display:flex;justify-content:center;gap:10px;flex-wrap:wrap;">
            <button type="button" class="btn btn-primary" data-hero-sample>✨ 샘플 볼트 시작하기</button>
            <button type="button" class="btn" data-hero-open>${isAndroid ? '📂 휴대폰 볼트 폴더 열기…' : '📂 내 PC 볼트 폴더 열기…'}</button>
            <button type="button" class="btn" data-hero-sync>🔄 동기화 센터 설정</button>
          </div>
        </div>
      `;
      this.editorPaneEl.querySelector('[data-hero-files-perm]')?.addEventListener('click', () => {
        void api.androidGrantFilesAccess();
      });
      this.editorPaneEl.querySelector('[data-hero-sample]')?.addEventListener('click', () => {
        void this.handleCreateSampleVault();
      });
      this.editorPaneEl.querySelector('[data-hero-open]')?.addEventListener('click', () => {
        void this.handleOpenVaultDialog();
      });
      this.editorPaneEl.querySelector('[data-hero-sync]')?.addEventListener('click', () => {
        void this.switchMainView('sync');
      });
      return;
    }

    if (!this.currentDoc) {
      this.editorCtrl?.destroy();
      this.editorCtrl = null;
      this.mountedNotePath = null;
      this.hideEditorToolbarDock();
      this.editorPaneEl.innerHTML = `
        <div class="empty-vault-hero">
          <div style="font-size:42px;margin-bottom:8px;">✍️</div>
          <h3 style="margin:0 0 8px;">열려 있는 노트가 없습니다</h3>
          <p style="color:var(--text-secondary);font-size:13.5px;margin:0 0 18px;">
            왼쪽 페이지 트리에서 노트를 선택하거나 새 노트를 만들어 보세요.
          </p>
          <div style="display:flex;justify-content:center;gap:8px;">
            <button type="button" class="btn btn-primary" data-empty-new>＋ 새 노트 만들기</button>
            <button type="button" class="btn" data-empty-daily>📅 오늘의 일일 노트</button>
          </div>
        </div>
      `;
      this.editorPaneEl.querySelector('[data-empty-new]')?.addEventListener('click', () => {
        void this.promptCreateNote('');
      });
      this.editorPaneEl.querySelector('[data-empty-daily]')?.addEventListener('click', () => {
        void this.openTodayDailyNote();
      });
      return;
    }

    if (this.currentDoc.locked) {
      this.editorCtrl?.destroy();
      this.editorCtrl = null;
      this.mountedNotePath = null;
      this.hideEditorToolbarDock();
      const hint = this.currentDoc.encryption_hint;
      this.editorPaneEl.innerHTML = `
        <div class="locked-note-card">
          <div class="locked-note-icon">🔒</div>
          <div class="locked-note-title">${escapeHtml(this.currentDoc.title)}</div>
          <div class="locked-note-desc">
            이 노트는 <strong>AES-256-GCM</strong>으로 암호화되어 잠겨 있습니다.<br/>
            암호를 입력하면 앱을 실행하는 동안 안전하게 열람·편집할 수 있습니다.
          </div>
          <div class="locked-note-form">
            ${hint ? `<div class="locked-note-hint">💡 암호 힌트: ${escapeHtml(hint)}</div>` : ''}
            <input type="password" class="text-input" data-lock-pw placeholder="암호 입력 후 Enter…" />
            <button type="button" class="btn btn-primary" data-lock-unlock>🔓 이 노트 잠금 해제</button>
            <div style="display:flex;gap:8px;justify-content:center;margin-top:4px;">
              <button type="button" class="btn btn-sm" data-lock-vault>🔐 볼트 전체 잠금 해제</button>
              <button type="button" class="btn btn-sm" data-lock-manage>🗝️ 암호화 관리</button>
            </div>
          </div>
        </div>
      `;

      const pwIn = this.editorPaneEl.querySelector('[data-lock-pw]') as HTMLInputElement;
      const submitUnlock = async () => {
        if (!this.workspace.active_vault || !this.currentDoc) return;
        if (!pwIn.value) {
          this.showToast('암호를 입력해 주세요.', true);
          return;
        }
        try {
          await api.cryptoUnlockNote(this.workspace.active_vault, this.currentDoc.path, pwIn.value);
          this.scan = await api.vaultScan(this.workspace.active_vault);
          this.vaultRevision = this.scan.revision;
          this.mountedNotePath = null;
          await this.openNote(this.currentDoc.path);
          this.showToast('노트 잠금이 해제되었습니다.');
        } catch (err: any) {
          this.showToast(err?.message || '암호가 올바르지 않습니다.', true);
        }
      };

      this.editorPaneEl.querySelector('[data-lock-unlock]')?.addEventListener('click', () => {
        void submitUnlock();
      });
      pwIn.addEventListener('keydown', (ev) => {
        if (ev.key === 'Enter') {
          ev.preventDefault();
          void submitUnlock();
        }
      });
      this.editorPaneEl.querySelector('[data-lock-vault]')?.addEventListener('click', () => {
        void this.openVaultCryptoDialog();
      });
      this.editorPaneEl.querySelector('[data-lock-manage]')?.addEventListener('click', () => {
        void this.openNoteCryptoDialog();
      });
      setTimeout(() => pwIn.focus(), 10);
      return;
    }

    const isA4 = (this.workspace.page_layout || 'a4') !== 'full';

    if (remountEditor || this.mountedNotePath !== this.currentDoc.path) {
      const isWideMd = !isA4 && this.workspace.editor_mode === 'markdown';
      const orientText = this.workspace.page_orientation === 'landscape' ? 'A4 가로' : 'A4 세로';
      const mTop = this.workspace.margin_top_cm ?? 2.0;
      const mBot = this.workspace.margin_bottom_cm ?? 1.0;
      const mLeft = this.workspace.margin_left_cm ?? 1.5;
      const mRight = this.workspace.margin_right_cm ?? 1.5;
      const zoom = this.workspace.editor_zoom ?? 100;

      const innerDocHtml = `
        <div id="notion-cover-zone"></div>
        <div class="notion-page-column ${isWideMd ? 'wide-md' : ''}" id="notion-header-zone"></div>
        <div class="editor-mount-wrap ${isWideMd ? 'wide-md' : ''}" id="toast-editor-host"></div>
        <div class="page-status-footer" id="notion-page-footer"></div>
      `;

      if (isA4) {
        this.editorPaneEl.innerHTML = `
          <div class="a4-desk-stage">
            <div class="a4-ruler-badge">
              <span>📄 <strong>${orientText}</strong> · 여백 상 ${mTop} / 하 ${mBot} / 좌 ${mLeft} / 우 ${mRight} cm · 배율 <strong data-ruler-zoom>${zoom}%</strong></span>
              <button type="button" class="mini-icon-btn" data-ruler-pagesetup title="용지·여백·인쇄 설정" style="width:auto;padding:0 6px;font-size:11px;">📐 여백</button>
              <button type="button" class="mini-icon-btn" data-ruler-toggle-full title="전체 화면 폭으로 전환" style="width:auto;padding:0 6px;font-size:11px;">🖥️ 전체폭</button>
            </div>
            <div class="a4-scale-outer" id="a4-scale-outer">
              <div class="a4-paper-sheet${this.workspace.show_page_guides !== false ? ' show-page-guides' : ''}" id="a4-paper-sheet">
                ${innerDocHtml}
              </div>
            </div>
          </div>
        `;
      } else {
        this.editorPaneEl.innerHTML = `
          <div style="padding:10px 24px 0;display:flex;justify-content:center;">
            <div class="a4-ruler-badge" style="margin-bottom:0;">
              <span>🖥️ <strong>전체 화면 폭 모드</strong> · 인쇄/PDF 여백 (상 ${mTop}/하 ${mBot}/좌 ${mLeft}/우 ${mRight}cm)</span>
              <button type="button" class="mini-icon-btn" data-ruler-pagesetup style="width:auto;padding:0 6px;font-size:11px;">📐 설정</button>
              <button type="button" class="mini-icon-btn" data-ruler-toggle-a4 style="width:auto;padding:0 6px;font-size:11px;">📄 A4 용지 보기</button>
            </div>
          </div>
          <div class="notion-full-stage${this.workspace.show_page_guides !== false ? ' show-page-guides' : ''}">
            ${innerDocHtml}
          </div>
        `;
      }

      this.editorPaneEl.querySelector('[data-ruler-pagesetup]')?.addEventListener('click', () => {
        this.openPageSetupDialog();
      });
      this.editorPaneEl.querySelector('[data-ruler-toggle-full]')?.addEventListener('click', async () => {
        await this.persistWorkspace({ page_layout: 'full' });
        this.renderEditorMainArea(true);
        this.renderTopbar();
      });
      this.editorPaneEl.querySelector('[data-ruler-toggle-a4]')?.addEventListener('click', async () => {
        await this.persistWorkspace({ page_layout: 'a4' });
        this.renderEditorMainArea(true);
        this.renderTopbar();
      });

      this.renderNotionCoverZone();
      this.renderNotionHeaderZone();
      this.updatePageFooterStats();

      const host = this.editorPaneEl.querySelector('#toast-editor-host') as HTMLElement;
      this.editorCtrl?.destroy();
      this.editorCtrl = new StudioEditorController(host, {
        getActiveVault: () => this.workspace.active_vault,
        onMarkdownChange: (cleanMd) => {
          this.draftBody = cleanMd;
          this.scheduleAutoSave();
          requestAnimationFrame(() => this.applyPageLayoutVariables());
        },
        onRequestWikiLinkPicker: () => {
          openWikiLinkPickerModal({
            notes: this.scan?.notes || [],
            onPick: (title) => {
              this.editorCtrl?.insertWikiLink(title);
            },
          });
        },
        onOpenWikiLink: (targetTitle) => {
          void this.openWikiLinkByTitle(targetTitle);
        },
        onSaveShortcut: () => {
          void this.flushSave(true);
        },
        onPickFileAttachment: async () => {
          const activeVault = this.workspace.active_vault;
          if (!activeVault) {
            this.showToast('먼저 볼트를 열어 주세요.', true);
            return;
          }
          try {
            const picked = await api.pickFileAttachment(activeVault);
            if (picked.picked && picked.media_url) {
              this.editorCtrl?.insertAttachmentLink(
                picked.file_name || '첨부파일',
                picked.media_url,
                picked.is_image
              );
              this.showToast(`파일을 첨부했습니다 (${picked.rel_path || picked.file_name})`);
            }
          } catch (err: any) {
            this.showToast(err?.message || '파일을 첨부하지 못했습니다.', true);
          }
        },
        onZoomDelta: (deltaPct) => {
          const nextZoom = Math.min(
            150,
            Math.max(50, (this.workspace.editor_zoom ?? 100) + deltaPct)
          );
          void this.persistWorkspace({ editor_zoom: nextZoom });
          this.applyPageLayoutVariables();
          const zEl = this.editorPaneEl.querySelector('[data-ruler-zoom]');
          if (zEl) zEl.textContent = `${nextZoom}%`;
        },
        onToast: (m, err) => this.showToast(m, err),
      });

      this.editorCtrl.mount({
        initialMarkdown: this.draftBody,
        mode: this.workspace.editor_mode,
        theme: this.workspace.theme,
        toolbarDockEl: this.editorToolbarDockEl,
      });
      this.mountedNotePath = this.currentDoc.path;

      const sheetEl = document.getElementById('a4-paper-sheet');
      if (sheetEl && this.resizeObserver) {
        this.resizeObserver.observe(sheetEl);
      }
      requestAnimationFrame(() => this.applyPageLayoutVariables());
    } else {
      this.renderNotionCoverZone();
      this.renderNotionHeaderZone();
      this.updatePageFooterStats();
      requestAnimationFrame(() => this.applyPageLayoutVariables());
    }
  }

  private renderNotionCoverZone(): void {
    const coverZone = this.editorPaneEl.querySelector('#notion-cover-zone') as HTMLElement | null;
    const headerCol = this.editorPaneEl.querySelector('#notion-header-zone') as HTMLElement | null;
    if (!coverZone || !headerCol) return;

    const cover = this.draftCover;
    headerCol.classList.toggle('has-cover', Boolean(cover));

    if (!cover) {
      coverZone.innerHTML = '';
      return;
    }

    const bgStyle =
      cover.startsWith('linear-gradient') || cover.startsWith('#') || cover.startsWith('rgb')
        ? `background: ${cover};`
        : `background-image: url('${escapeAttr(cover)}');`;

    coverZone.innerHTML = `
      <div class="notion-cover" style="${bgStyle}">
        <div class="notion-cover-actions">
          <button type="button" class="glass-btn" data-change-cover>커버 변경</button>
          <button type="button" class="glass-btn" data-remove-cover>커버 제거</button>
        </div>
      </div>
    `;

    coverZone.querySelector('[data-change-cover]')?.addEventListener('click', () => {
      this.openCoverPicker();
    });
    coverZone.querySelector('[data-remove-cover]')?.addEventListener('click', () => {
      this.draftCover = null;
      this.renderNotionCoverZone();
      this.renderNotionHeaderZone();
      this.scheduleAutoSave();
    });
  }

  private openCoverPicker(): void {
    openCoverPickerModal({
      vault: this.workspace.active_vault,
      currentCover: this.draftCover,
      onSelect: (cover) => {
        this.draftCover = cover;
        this.renderNotionCoverZone();
        this.renderNotionHeaderZone();
        this.scheduleAutoSave();
      },
      onToast: (m, err) => this.showToast(m, err),
    });
  }

  private openIconPicker(): void {
    openEmojiPickerModal({
      currentIcon: this.draftIcon,
      onSelect: (emoji) => {
        this.draftIcon = emoji;
        this.renderNotionHeaderZone();
        this.renderTopbar();
        this.scheduleAutoSave();
      },
    });
  }

  private renderNotionHeaderZone(): void {
    const headerZone = this.editorPaneEl.querySelector('#notion-header-zone') as HTMLElement | null;
    if (!headerZone || !this.currentDoc) return;

    const isFav = this.workspace.favorites.includes(this.currentDoc.path);

    headerZone.innerHTML = `
      ${
        this.draftIcon
          ? `<div class="notion-icon-wrapper">
              <button type="button" class="notion-page-icon-btn" data-pick-icon title="아이콘 변경">${escapeHtml(this.draftIcon)}</button>
            </div>`
          : ''
      }
      <div class="notion-hover-controls">
        ${!this.draftIcon ? `<button type="button" class="ghost-action-btn" data-add-icon>😀 아이콘 추가</button>` : ''}
        ${!this.draftCover ? `<button type="button" class="ghost-action-btn" data-add-cover>🖼️ 커버 추가</button>` : ''}
        <button type="button" class="ghost-action-btn" data-apply-tpl>✨ 템플릿 적용</button>
        <button type="button" class="ghost-action-btn" data-toggle-fav>${isFav ? '⭐ 즐겨찾기됨' : '☆ 즐겨찾기'}</button>
        <button type="button" class="ghost-action-btn" data-hover-crypto>${this.currentDoc.encrypted ? '🔓 암호화 관리' : '🔐 암호화'}</button>
      </div>
      <input type="text" class="notion-title-input" data-note-title placeholder="제목 없음" value="${escapeAttr(this.currentDoc.title)}" />
      <div class="notion-properties" id="notion-props-zone"></div>
    `;

    headerZone.querySelector('[data-pick-icon]')?.addEventListener('click', () => this.openIconPicker());
    headerZone.querySelector('[data-add-icon]')?.addEventListener('click', () => this.openIconPicker());
    headerZone.querySelector('[data-add-cover]')?.addEventListener('click', () => this.openCoverPicker());
    headerZone.querySelector('[data-toggle-fav]')?.addEventListener('click', () => {
      if (this.currentDoc) void this.toggleFavorite(this.currentDoc.path);
    });
    headerZone.querySelector('[data-hover-crypto]')?.addEventListener('click', () => {
      void this.openNoteCryptoDialog();
    });
    headerZone.querySelector('[data-apply-tpl]')?.addEventListener('click', () => {
      if (!this.workspace.active_vault) return;
      void openTemplatesModal({
        vault: this.workspace.active_vault,
        templatesFolder: this.workspace.templates_folder || 'Templates',
        onApplyTemplate: (tpl) => {
          if (!this.draftIcon && tpl.icon) {
            this.draftIcon = tpl.icon;
          }
          for (const tp of tpl.properties) {
            if (!this.draftProperties.some((p) => p.key === tp.key)) {
              this.draftProperties.push({ ...tp });
            }
          }
          const currentMd = this.editorCtrl ? this.editorCtrl.getCleanMarkdown() : this.draftBody;
          const merged = currentMd.trim() ? `${currentMd.trim()}\n\n${tpl.body}` : tpl.body;
          this.draftBody = merged;
          this.editorCtrl?.setMarkdownContent(merged);
          this.renderNotionHeaderZone();
          this.scheduleAutoSave();
          this.showToast('템플릿을 적용했습니다.');
        },
        onToast: (m, err) => this.showToast(m, err),
      });
    });

    // Title rename on Enter or Blur
    const titleInput = headerZone.querySelector('[data-note-title]') as HTMLInputElement;
    const commitTitleRename = async () => {
      if (!this.currentDoc) return;
      const nextTitle = titleInput.value.trim();
      if (!nextTitle || nextTitle === this.currentDoc.title) {
        titleInput.value = this.currentDoc.title;
        return;
      }
      await this.executeRename(this.currentDoc.path, nextTitle);
    };
    titleInput.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        titleInput.blur();
      }
    });
    titleInput.addEventListener('blur', () => {
      void commitTitleRename();
    });

    this.renderPropertiesRows();
  }

  private renderPropertiesRows(): void {
    const propsZone = this.editorPaneEl.querySelector('#notion-props-zone') as HTMLElement | null;
    if (!propsZone) return;
    propsZone.innerHTML = '';

    const typeIcons: Record<string, string> = {
      tags: '🏷️',
      date: '📅',
      checkbox: '☑️',
      url: '🔗',
      number: '🔢',
      text: '≡',
    };

    const showTags = Boolean(this.workspace.show_tags);
    const tagCount = this.draftProperties
      .filter((p) => p.prop_type === 'tags' || p.key.toLowerCase() === 'tags')
      .reduce(
        (sum, p) =>
          sum +
          p.value
            .split(',')
            .map((t) => t.trim())
            .filter(Boolean).length,
        0
      );

    this.draftProperties.forEach((prop, idx) => {
      const isTagProp = prop.prop_type === 'tags' || prop.key.toLowerCase() === 'tags';
      if (isTagProp && !showTags) {
        return;
      }
      const row = document.createElement('div');
      row.className = 'prop-row';

      // Type selector
      const typeSel = document.createElement('select');
      typeSel.className = 'prop-type-select';
      typeSel.title = '속성 유형 변경';
      for (const [tKey, tIcon] of Object.entries(typeIcons)) {
        const opt = document.createElement('option');
        opt.value = tKey;
        opt.textContent = tIcon;
        if (prop.prop_type === tKey) opt.selected = true;
        typeSel.appendChild(opt);
      }
      typeSel.addEventListener('change', () => {
        prop.prop_type = typeSel.value as PropertyType;
        if (prop.prop_type === 'checkbox' && prop.value !== 'true' && prop.value !== 'false') {
          prop.value = 'false';
        }
        this.renderPropertiesRows();
        this.scheduleAutoSave();
      });

      // Key input
      const keyIn = document.createElement('input');
      keyIn.type = 'text';
      keyIn.className = 'prop-key-input';
      keyIn.placeholder = '속성 이름';
      keyIn.value = prop.key;
      keyIn.addEventListener('input', () => {
        prop.key = keyIn.value;
        this.scheduleAutoSave();
      });

      // Value cell
      const valCell = document.createElement('div');
      valCell.className = 'prop-value-cell';

      if (prop.prop_type === 'tags' || prop.key.toLowerCase() === 'tags') {
        const tags = prop.value
          .split(',')
          .map((t) => t.trim())
          .filter(Boolean);
        for (const t of tags) {
          const pill = document.createElement('span');
          pill.className = 'prop-tag-pill';
          pill.innerHTML = `#${escapeHtml(t.replace(/^#+/, ''))} <button type="button" class="prop-tag-remove">✕</button>`;
          pill.querySelector('.prop-tag-remove')?.addEventListener('click', () => {
            prop.value = tags.filter((x) => x !== t).join(', ');
            this.renderPropertiesRows();
            this.scheduleAutoSave();
          });
          valCell.appendChild(pill);
        }
        const tagAddIn = document.createElement('input');
        tagAddIn.type = 'text';
        tagAddIn.className = 'prop-val-input';
        tagAddIn.style.minWidth = '90px';
        tagAddIn.placeholder = '+ 태그 입력 후 Enter';
        const commitTag = () => {
          const v = tagAddIn.value.trim().replace(/^#+/, '');
          if (!v) return;
          const next = [...tags, v];
          prop.value = next.join(', ');
          this.renderPropertiesRows();
          this.scheduleAutoSave();
        };
        tagAddIn.addEventListener('keydown', (ev) => {
          if (ev.key === 'Enter' || ev.key === ',') {
            ev.preventDefault();
            commitTag();
          }
        });
        tagAddIn.addEventListener('blur', commitTag);
        valCell.appendChild(tagAddIn);
      } else if (prop.prop_type === 'checkbox') {
        const chk = document.createElement('input');
        chk.type = 'checkbox';
        chk.checked = prop.value === 'true';
        chk.style.cursor = 'pointer';
        chk.addEventListener('change', () => {
          prop.value = chk.checked ? 'true' : 'false';
          this.scheduleAutoSave();
        });
        valCell.appendChild(chk);
      } else if (prop.prop_type === 'date') {
        const dateIn = document.createElement('input');
        dateIn.type = 'date';
        dateIn.className = 'prop-val-input';
        dateIn.style.maxWidth = '180px';
        dateIn.value = prop.value.slice(0, 10);
        dateIn.addEventListener('input', () => {
          prop.value = dateIn.value;
          this.scheduleAutoSave();
        });
        valCell.appendChild(dateIn);
      } else if (prop.prop_type === 'number') {
        const numIn = document.createElement('input');
        numIn.type = 'number';
        numIn.className = 'prop-val-input';
        numIn.value = prop.value;
        numIn.addEventListener('input', () => {
          prop.value = numIn.value;
          this.scheduleAutoSave();
        });
        valCell.appendChild(numIn);
      } else if (prop.prop_type === 'url') {
        const urlIn = document.createElement('input');
        urlIn.type = 'url';
        urlIn.className = 'prop-val-input';
        urlIn.placeholder = 'https://...';
        urlIn.value = prop.value;
        urlIn.addEventListener('input', () => {
          prop.value = urlIn.value;
          this.scheduleAutoSave();
        });
        const openBtn = document.createElement('button');
        openBtn.type = 'button';
        openBtn.className = 'mini-icon-btn';
        openBtn.title = '브라우저에서 열기';
        openBtn.textContent = '↗';
        openBtn.addEventListener('click', () => {
          if (prop.value.trim()) void api.syncOpenPath(prop.value.trim());
        });
        valCell.append(urlIn, openBtn);
      } else {
        const txtIn = document.createElement('input');
        txtIn.type = 'text';
        txtIn.className = 'prop-val-input';
        txtIn.placeholder = '비어 있음';
        txtIn.value = prop.value;
        txtIn.addEventListener('input', () => {
          prop.value = txtIn.value;
          this.scheduleAutoSave();
        });
        valCell.appendChild(txtIn);
      }

      const delBtn = document.createElement('button');
      delBtn.type = 'button';
      delBtn.className = 'prop-delete-btn';
      delBtn.title = '속성 삭제';
      delBtn.textContent = '✕';
      delBtn.addEventListener('click', () => {
        this.draftProperties.splice(idx, 1);
        this.renderPropertiesRows();
        this.scheduleAutoSave();
      });

      row.append(typeSel, keyIn, valCell, delBtn);
      propsZone.appendChild(row);
    });

    // Footer bar (+ 속성 추가 & 태그 보기/숨기기 & 원본 YAML 보기)
    const footer = document.createElement('div');
    footer.className = 'prop-footer-bar';
    footer.innerHTML = `
      <button type="button" class="ghost-action-btn" data-add-prop>＋ 속성 추가</button>
      <button type="button" class="tag-toggle-chip${showTags ? ' active' : ''}" data-toggle-prop-tags>
        🏷️ ${showTags ? '태그 숨기기' : tagCount > 0 ? `태그 보기 (${tagCount})` : '태그 보기'}
      </button>
      <button type="button" class="ghost-action-btn" data-toggle-yaml>
        ${this.showRawYaml ? '▲ 원본 YAML 접기' : '▼ 원본 YAML 보기'}
      </button>
    `;
    footer.querySelector('[data-add-prop]')?.addEventListener('click', () => {
      const hasTags = this.draftProperties.some((p) => p.key === 'tags');
      this.draftProperties.push(
        hasTags
          ? { key: `속성${this.draftProperties.length + 1}`, value: '', prop_type: 'text' }
          : { key: 'tags', value: '', prop_type: 'tags' }
      );
      if (!hasTags && !this.workspace.show_tags) {
        void this.persistWorkspace({ show_tags: true }).then(() => {
          this.renderSidebar();
          this.renderPropertiesRows();
          this.renderTopbar();
        });
      } else {
        this.renderPropertiesRows();
      }
      this.scheduleAutoSave();
    });
    footer.querySelector('[data-toggle-prop-tags]')?.addEventListener('click', () => {
      void this.toggleShowTags();
    });
    footer.querySelector('[data-toggle-yaml]')?.addEventListener('click', () => {
      this.showRawYaml = !this.showRawYaml;
      this.renderPropertiesRows();
    });
    propsZone.appendChild(footer);

    if (this.showRawYaml) {
      const yamlBox = document.createElement('div');
      yamlBox.className = 'raw-yaml-box';
      yamlBox.id = 'raw-yaml-preview-box';
      propsZone.appendChild(yamlBox);
      this.renderRawYamlPreview();
    }
  }

  private renderRawYamlPreview(): void {
    const box = document.getElementById('raw-yaml-preview-box');
    if (!box || !this.currentDoc) return;
    box.textContent = this.currentDoc.raw_frontmatter?.trim()
      ? `---\n${this.currentDoc.raw_frontmatter.trim()}\n---`
      : '(저장된 Frontmatter YAML 없음)';
  }

  private updatePageFooterStats(): void {
    const footer = this.editorPaneEl.querySelector('#notion-page-footer') as HTMLElement | null;
    if (!footer || !this.currentDoc) return;
    footer.innerHTML = `
      <span>💡 팁: <code>[[</code> 입력으로 다른 노트 연결 · <code>Ctrl+클릭</code>으로 연결 노트 열기</span>
      <span>단어 ${this.currentDoc.word_count} · 글자 ${this.currentDoc.char_count} · 수정 ${escapeHtml(this.currentDoc.updated_at)}</span>
    `;
  }

  // ───────────────────────── Split View Reader ─────────────────────────
  private async openSplitNote(relPath: string): Promise<void> {
    if (!this.workspace.active_vault) return;
    try {
      this.splitDoc = await api.noteRead(this.workspace.active_vault, relPath);
      await this.persistWorkspace({ split_tab: relPath });
      this.renderSplitPane();
      this.renderTopbar();
    } catch (err: any) {
      this.showToast(err?.message || '분할 보기 노트를 열지 못했습니다', true);
    }
  }

  private async closeSplitNote(): Promise<void> {
    this.splitViewerCleanup?.();
    this.splitViewerCleanup = null;
    this.splitDoc = null;
    this.splitPaneEl.style.display = 'none';
    await this.persistWorkspace({ split_tab: null });
    this.renderTopbar();
  }

  private renderSplitPane(): void {
    this.splitViewerCleanup?.();
    this.splitViewerCleanup = null;

    if (!this.splitDoc) {
      this.splitPaneEl.style.display = 'none';
      return;
    }

    this.splitPaneEl.style.display = 'flex';
    this.splitPaneEl.innerHTML = `
      <div class="split-pane-header">
        <select class="select-input" style="padding:3px 8px;font-size:12.5px;max-width:220px;" data-split-select></select>
        <div style="display:flex;gap:4px;">
          <button type="button" class="btn btn-sm" data-split-promote title="왼쪽 편집기에서 열기">편집기로 열기</button>
          <button type="button" class="icon-btn" data-split-close title="분할 보기 닫기">✕</button>
        </div>
      </div>
      <div class="split-pane-scroll">
        <h2 style="margin:0 0 14px;font-size:24px;">
          ${escapeHtml(this.splitDoc.icon || '📄')} ${escapeHtml(this.splitDoc.title)}
        </h2>
        <div id="split-viewer-mount"></div>
      </div>
    `;

    const sel = this.splitPaneEl.querySelector('[data-split-select]') as HTMLSelectElement;
    for (const n of this.scan?.notes || []) {
      const opt = document.createElement('option');
      opt.value = n.path;
      opt.textContent = `${n.icon || '📄'} ${n.title}`;
      if (n.path === this.splitDoc.path) opt.selected = true;
      sel.appendChild(opt);
    }
    sel.addEventListener('change', () => {
      void this.openSplitNote(sel.value);
    });

    this.splitPaneEl.querySelector('[data-split-promote]')?.addEventListener('click', () => {
      if (this.splitDoc) void this.openNote(this.splitDoc.path);
    });
    this.splitPaneEl.querySelector('[data-split-close]')?.addEventListener('click', () => {
      void this.closeSplitNote();
    });

    const mount = this.splitPaneEl.querySelector('#split-viewer-mount') as HTMLElement;
    if (this.splitDoc.locked) {
      mount.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);font-size:13px;">🔒 암호화되어 잠긴 노트입니다. 왼쪽 편집기에서 열어 잠금을 해제해 주세요.</div>`;
    } else {
      const viewerHandle = renderSplitViewer({
        container: mount,
        markdown: hydrateMarkdownMediaUrls(this.splitDoc.body, this.workspace.active_vault),
        vault: this.workspace.active_vault,
        theme: this.workspace.theme,
        onOpenWikiLink: (t) => void this.openWikiLinkByTitle(t),
      });
      this.splitViewerCleanup = viewerHandle.destroy;
    }
  }

  // ───────────────────────── Right Inspector Panel ─────────────────────────
  private renderInspector(): void {
    this.localGraphCtrl?.destroy();
    this.localGraphCtrl = null;

    this.inspectorEl.classList.toggle('collapsed', !this.inspectorOpen);
    if (!this.inspectorOpen) return;

    this.inspectorEl.innerHTML = `
      <div class="inspector-tabs">
        <button type="button" class="inspector-tab ${this.inspectorTab === 'backlinks' ? 'active' : ''}" data-itab="backlinks">🔗 백링크</button>
        <button type="button" class="inspector-tab ${this.inspectorTab === 'outline' ? 'active' : ''}" data-itab="outline">📑 목차</button>
        <button type="button" class="inspector-tab ${this.inspectorTab === 'local_graph' ? 'active' : ''}" data-itab="local_graph">🕸️ 로컬 그래프</button>
        <button type="button" class="icon-btn" data-iclose title="인스펙터 닫기" style="width:24px;height:24px;font-size:12px;">✕</button>
      </div>
      <div class="inspector-body" data-ibody></div>
    `;

    this.inspectorEl.querySelector('[data-iclose]')?.addEventListener('click', () => {
      this.inspectorOpen = false;
      this.renderInspector();
      this.renderTopbar();
    });

    this.inspectorEl.querySelectorAll('[data-itab]').forEach((btn) => {
      btn.addEventListener('click', () => {
        this.inspectorTab = btn.getAttribute('data-itab') as RightInspectorTab;
        this.renderInspector();
      });
    });

    const body = this.inspectorEl.querySelector('[data-ibody]') as HTMLElement;
    if (!this.currentDoc || !this.workspace.active_vault) {
      body.innerHTML = `<div style="padding:20px;text-align:center;color:var(--text-muted);font-size:12.5px;">열려 있는 노트가 없습니다.</div>`;
      return;
    }

    if (this.inspectorTab === 'backlinks') {
      const linked = this.currentDoc.backlinks.filter((b) => b.linked);
      const unlinked = this.currentDoc.backlinks.filter((b) => !b.linked);
      const outgoing = this.currentDoc.outgoing_links || [];

      // 1. Linked Mentions
      const lTitle = document.createElement('div');
      lTitle.className = 'inspector-group-title';
      lTitle.innerHTML = `<span>연결된 언급 (Linked)</span><span>${linked.length}</span>`;
      body.appendChild(lTitle);

      if (linked.length === 0) {
        const empty = document.createElement('div');
        empty.style.cssText = 'font-size:12px;color:var(--text-muted);margin-bottom:10px;';
        empty.textContent = '이 노트를 [[링크]]한 다른 노트가 아직 없습니다.';
        body.appendChild(empty);
      } else {
        for (const bl of linked) {
          const card = document.createElement('div');
          card.className = 'backlink-card';
          card.innerHTML = `
            <div class="backlink-card-header">
              <span>${escapeHtml(bl.icon || '📄')} ${escapeHtml(bl.title)}</span>
            </div>
            <div class="backlink-snippet">${escapeHtml(bl.snippet)}</div>
          `;
          card.addEventListener('click', () => void this.openNote(bl.path));
          body.appendChild(card);
        }
      }

      // 2. Unlinked Mentions
      const uTitle = document.createElement('div');
      uTitle.className = 'inspector-group-title';
      uTitle.innerHTML = `<span>연결되지 않은 언급</span><span>${unlinked.length}</span>`;
      body.appendChild(uTitle);

      if (unlinked.length === 0) {
        const empty = document.createElement('div');
        empty.style.cssText = 'font-size:12px;color:var(--text-muted);margin-bottom:10px;';
        empty.textContent = '연결되지 않은 언급이 없습니다.';
        body.appendChild(empty);
      } else {
        for (const bl of unlinked) {
          const card = document.createElement('div');
          card.className = 'backlink-card';
          card.innerHTML = `
            <div class="backlink-card-header">
              <span>${escapeHtml(bl.icon || '📄')} ${escapeHtml(bl.title)}</span>
              <button type="button" class="btn btn-sm" data-link-mention>[[연결]]</button>
            </div>
            <div class="backlink-snippet">${escapeHtml(bl.snippet)}</div>
          `;
          card.addEventListener('click', () => void this.openNote(bl.path));
          card.querySelector('[data-link-mention]')?.addEventListener('click', async (ev) => {
            ev.stopPropagation();
            if (!this.workspace.active_vault || !this.currentDoc) return;
            try {
              await api.linkMention(
                this.workspace.active_vault,
                bl.path,
                this.currentDoc.title
              );
              const refreshed = await api.noteRead(
                this.workspace.active_vault,
                this.currentDoc.path
              );
              this.currentDoc = refreshed;
              this.renderInspector();
              this.showToast(`"${bl.title}" 노트에 [[${this.currentDoc.title}]] 링크를 연결했습니다.`);
            } catch (err: any) {
              this.showToast(err?.message || '링크 연결 실패', true);
            }
          });
          body.appendChild(card);
        }
      }

      // 3. Outgoing Links
      const oTitle = document.createElement('div');
      oTitle.className = 'inspector-group-title';
      oTitle.innerHTML = `<span>나가는 링크 (Outgoing)</span><span>${outgoing.length}</span>`;
      body.appendChild(oTitle);

      if (outgoing.length === 0) {
        const empty = document.createElement('div');
        empty.style.cssText = 'font-size:12px;color:var(--text-muted);';
        empty.textContent = '본문에 [[연결된 노트]]가 없습니다.';
        body.appendChild(empty);
      } else {
        for (const out of outgoing) {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = 'outline-item';
          btn.textContent = `↗ [[${out}]]`;
          btn.addEventListener('click', () => void this.openWikiLinkByTitle(out));
          body.appendChild(btn);
        }
      }
    } else if (this.inspectorTab === 'outline') {
      const headings = this.currentDoc.headings || [];
      if (headings.length === 0) {
        body.innerHTML = `<div style="padding:16px 4px;font-size:12.5px;color:var(--text-muted);">제목(H1~H6)을 추가하면 목차가 자동으로 생성됩니다.</div>`;
        return;
      }
      for (const h of headings) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'outline-item';
        btn.style.paddingLeft = `${6 + Math.max(0, h.level - 1) * 12}px`;
        if (h.level === 1) btn.style.fontWeight = '700';
        btn.textContent = h.text;
        btn.addEventListener('click', () => {
          this.editorCtrl?.scrollToHeading(h.text);
        });
        body.appendChild(btn);
      }
    } else if (this.inspectorTab === 'local_graph') {
      body.innerHTML = `
        <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
          <span style="font-size:12px;color:var(--text-secondary);">연결 깊이</span>
          <div style="display:flex;gap:4px;">
            <button type="button" class="btn btn-sm ${this.localGraphDepth === 1 ? 'btn-primary' : ''}" data-depth="1">1단계</button>
            <button type="button" class="btn btn-sm ${this.localGraphDepth === 2 ? 'btn-primary' : ''}" data-depth="2">2단계</button>
          </div>
        </div>
        <div class="local-graph-box" id="local-graph-canvas-host"></div>
        <div style="margin-top:8px;font-size:11.5px;color:var(--text-muted);">
          노드를 클릭하면 해당 노트로 바로 이동합니다. 마우스 휠로 확대/축소할 수 있습니다.
        </div>
      `;
      body.querySelectorAll('[data-depth]').forEach((b) => {
        b.addEventListener('click', () => {
          this.localGraphDepth = Number(b.getAttribute('data-depth')) || 2;
          this.renderInspector();
        });
      });

      const host = body.querySelector('#local-graph-canvas-host') as HTMLElement;
      const vault = this.workspace.active_vault;
      const centerPath = this.currentDoc.path;
      void api
        .vaultGraph({
          vault,
          center_path: centerPath,
          depth: this.localGraphDepth,
          include_tags: true,
        })
        .then((gData) => {
          if (!document.body.contains(host)) return;
          this.localGraphCtrl = new ForceGraphCanvas(host, gData, {
            centerPath,
            theme: this.workspace.theme,
            compact: true,
            onNodeClick: (node) => {
              if (node.kind === 'note' && node.path) {
                void this.openNote(node.path);
              } else if (node.kind === 'tag') {
                this.openCommandPalette(`tag:${node.label}`);
              } else if (node.kind === 'ghost') {
                void this.openWikiLinkByTitle(node.label);
              }
            },
          });
        })
        .catch(() => {});
    }
  }

  // ───────────────────────── Global Graph View ─────────────────────────
  private async mountGlobalGraphView(): Promise<void> {
    this.globalGraphCtrl?.destroy();
    this.globalGraphCtrl = null;

    if (!this.workspace.active_vault) {
      this.graphViewWrapEl.innerHTML = `
        <div class="empty-vault-hero">
          <h3>열려 있는 볼트가 없습니다</h3>
          <button type="button" class="btn btn-primary" data-back-ed>에디터로 돌아가기</button>
        </div>
      `;
      this.graphViewWrapEl.querySelector('[data-back-ed]')?.addEventListener('click', () => {
        void this.switchMainView('editor');
      });
      return;
    }

    const folders = this.scan ? collectFoldersFromTree(this.scan.tree) : [];
    const folderOptionsHtml = [
      `<option value="">전체 폴더</option>`,
      `<option value="__root__" ${this.globalGraphFolderFilter === '__root__' ? 'selected' : ''}>📄 루트 노트만</option>`,
      ...folders.map(
        (f) =>
          `<option value="${escapeAttr(f)}" ${this.globalGraphFolderFilter === f ? 'selected' : ''}>📁 ${escapeHtml(f)}</option>`
      ),
    ].join('');

    this.graphViewWrapEl.innerHTML = `
      <div style="width:100%;height:100%;" id="global-graph-canvas-wrap"></div>
      <div class="graph-controls-panel">
        <div style="display:flex;align-items:center;justify-content:space-between;gap:8px;">
          <span style="font-weight:700;font-size:14px;">🕸️ 지식 그래프 탐색</span>
          <button type="button" class="btn btn-sm" data-graph-reset>⊡ 화면 맞춤</button>
        </div>
        <div class="graph-mode-tabs">
          <button type="button" class="graph-mode-tab${this.globalGraphLayoutMode === 'cluster' ? ' active' : ''}" data-graph-mode="cluster">🫧 클러스터</button>
          <button type="button" class="graph-mode-tab${this.globalGraphLayoutMode === 'radial' ? ' active' : ''}" data-graph-mode="radial">🎯 방사형</button>
          <button type="button" class="graph-mode-tab${this.globalGraphLayoutMode === 'hierarchy' ? ' active' : ''}" data-graph-mode="hierarchy">🌿 계층도</button>
        </div>
        <input type="text" class="text-input" data-graph-search placeholder="노드 이름 검색 하이라이트…" value="${escapeAttr(this.globalGraphSearch)}" />
        <select class="text-input" data-graph-folder style="padding:5px 8px;font-size:12.5px;">
          ${folderOptionsHtml}
        </select>
        <label style="display:flex;align-items:center;gap:7px;font-size:12.5px;cursor:pointer;">
          <input type="checkbox" data-graph-orphans ${this.globalGraphHideOrphans ? 'checked' : ''} />
          고립 노트(연결 0) 숨기기
        </label>
        <label style="display:flex;align-items:center;gap:7px;font-size:12.5px;cursor:pointer;">
          <input type="checkbox" data-graph-tags ${this.globalGraphIncludeTags ? 'checked' : ''} />
          #태그 노드 함께 표시
        </label>
        <label style="display:flex;align-items:center;gap:7px;font-size:12.5px;cursor:pointer;">
          <input type="checkbox" data-graph-local ${this.globalGraphLocalOnly ? 'checked' : ''} ${!this.currentDoc ? 'disabled' : ''} />
          현재 노트 주변(2단계)만 보기
        </label>
        <button type="button" class="btn btn-primary btn-sm" data-back-editor>← 노트 편집기로 돌아가기</button>
      </div>
      <div class="graph-detail-card" id="graph-detail-card" style="display:none;"></div>
      <div class="graph-legend">
        <span><span class="status-dot" style="background:#2383e2;"></span> 노트(폴더별)</span>
        <span><span class="status-dot" style="background:#e03e3e;"></span> 현재 노트</span>
        <span><span class="status-dot" style="background:#3b82f6;"></span> 나가는 링크</span>
        <span><span class="status-dot" style="background:#10b981;"></span> 들어오는 백링크</span>
        <span><span class="status-dot" style="background:#0f7b6c;"></span> 태그</span>
        <span><span class="status-dot" style="background:#9b9a97;"></span> 미생성</span>
      </div>
    `;

    const canvasHost = this.graphViewWrapEl.querySelector('#global-graph-canvas-wrap') as HTMLElement;
    const detailCardEl = this.graphViewWrapEl.querySelector('#graph-detail-card') as HTMLElement;

    const renderDetailCard = (
      node: import('./types').GraphNodeDto | null,
      connections: {
        inbound: import('./types').GraphNodeDto[];
        outbound: import('./types').GraphNodeDto[];
      }
    ) => {
      if (!node) {
        detailCardEl.style.display = 'none';
        detailCardEl.innerHTML = '';
        return;
      }
      detailCardEl.style.display = 'flex';
      const badge =
        node.kind === 'note'
          ? `📁 ${escapeHtml(node.folder || '루트 노트')}`
          : node.kind === 'tag'
            ? '🏷️ 태그'
            : '👻 미생성 링크';
      const actionLabel =
        node.kind === 'note'
          ? '📄 이 노트 열기'
          : node.kind === 'ghost'
            ? '✨ 새 노트로 생성'
            : '🔍 이 태그 검색';

      const makeChips = (list: import('./types').GraphNodeDto[], arrow: string) =>
        list.length === 0
          ? '<span style="color:var(--text-muted);font-size:11.5px;">없음</span>'
          : list
              .slice(0, 12)
              .map(
                (c) =>
                  `<button type="button" class="graph-conn-chip" data-focus-node="${escapeAttr(c.id)}">${arrow} ${escapeHtml(c.label)}</button>`
              )
              .join('');

      detailCardEl.innerHTML = `
        <div style="display:flex;align-items:flex-start;justify-content:space-between;gap:8px;">
          <div>
            <div style="font-size:11px;color:var(--text-muted);margin-bottom:2px;">${badge}</div>
            <div style="font-weight:700;font-size:14.5px;line-height:1.3;">${
              node.icon ? `${escapeHtml(node.icon)} ` : ''
            }${escapeHtml(node.label)}</div>
          </div>
          <button type="button" class="mini-icon-btn" data-detail-close title="닫기">✕</button>
        </div>
        ${
          node.excerpt
            ? `<div style="font-size:12px;color:var(--text-secondary);line-height:1.45;background:var(--bg-inset);padding:7px 9px;border-radius:7px;">${escapeHtml(node.excerpt)}</div>`
            : ''
        }
        ${
          node.tags && node.tags.length > 0
            ? `<div style="display:flex;flex-wrap:wrap;gap:4px;">${node.tags
                .map((t) => `<span class="pill-badge">#${escapeHtml(t)}</span>`)
                .join('')}</div>`
            : ''
        }
        <div style="font-size:11.5px;color:var(--text-muted);">
          들어오는 백링크 <strong>${connections.inbound.length}</strong> · 나가는 링크 <strong>${connections.outbound.length}</strong> (더블클릭으로 열기)
        </div>
        <div>
          <div style="font-size:11px;font-weight:700;color:#3b82f6;margin-bottom:4px;">나가는 연결 (→)</div>
          <div style="display:flex;flex-wrap:wrap;gap:4px;">${makeChips(connections.outbound, '→')}</div>
        </div>
        <div>
          <div style="font-size:11px;font-weight:700;color:#10b981;margin-bottom:4px;">들어오는 백링크 (←)</div>
          <div style="display:flex;flex-wrap:wrap;gap:4px;">${makeChips(connections.inbound, '←')}</div>
        </div>
        <button type="button" class="btn btn-primary btn-sm" data-open-selected style="margin-top:4px;">${actionLabel}</button>
      `;

      detailCardEl.querySelector('[data-detail-close]')?.addEventListener('click', () => {
        this.globalGraphCtrl?.selectNodeById(null);
      });
      detailCardEl.querySelectorAll('[data-focus-node]').forEach((el) => {
        el.addEventListener('click', () => {
          const id = el.getAttribute('data-focus-node');
          if (id) this.globalGraphCtrl?.selectNodeById(id);
        });
      });
      detailCardEl.querySelector('[data-open-selected]')?.addEventListener('click', () => {
        if (node.kind === 'note' && node.path) {
          void this.openNote(node.path);
        } else if (node.kind === 'tag') {
          this.openCommandPalette(`tag:${node.label}`);
        } else if (node.kind === 'ghost') {
          void this.openWikiLinkByTitle(node.label);
        }
      });
    };

    const openGraphNode = (node: import('./types').GraphNodeDto) => {
      if (node.kind === 'note' && node.path) {
        void this.openNote(node.path);
      } else if (node.kind === 'tag') {
        this.openCommandPalette(`tag:${node.label}`);
      } else if (node.kind === 'ghost') {
        void this.openWikiLinkByTitle(node.label);
      }
    };

    const loadGraph = async () => {
      if (!this.workspace.active_vault) return;
      const g = await api.vaultGraph({
        vault: this.workspace.active_vault,
        center_path: this.globalGraphLocalOnly ? this.currentDoc?.path || null : null,
        depth: this.globalGraphLocalOnly ? 2 : undefined,
        include_tags: this.globalGraphIncludeTags,
      });
      if (this.globalGraphCtrl) {
        this.globalGraphCtrl.setData(g, this.currentDoc?.path);
      } else {
        this.globalGraphCtrl = new ForceGraphCanvas(canvasHost, g, {
          centerPath: this.currentDoc?.path,
          theme: this.workspace.theme,
          searchQuery: this.globalGraphSearch,
          layoutMode: this.globalGraphLayoutMode,
          hideOrphans: this.globalGraphHideOrphans,
          folderFilter: this.globalGraphFolderFilter,
          onNodeSelect: renderDetailCard,
          onNodeOpen: openGraphNode,
        });
      }
    };

    await loadGraph();

    this.graphViewWrapEl.querySelectorAll('[data-graph-mode]').forEach((btn) => {
      btn.addEventListener('click', () => {
        const m = btn.getAttribute('data-graph-mode') as 'cluster' | 'radial' | 'hierarchy';
        if (!m) return;
        this.globalGraphLayoutMode = m;
        this.graphViewWrapEl
          .querySelectorAll('[data-graph-mode]')
          .forEach((b) => b.classList.toggle('active', b === btn));
        this.globalGraphCtrl?.setLayoutMode(m);
      });
    });

    const folderSel = this.graphViewWrapEl.querySelector('[data-graph-folder]') as HTMLSelectElement;
    folderSel?.addEventListener('change', () => {
      this.globalGraphFolderFilter = folderSel.value;
      this.globalGraphCtrl?.setFolderFilter(this.globalGraphFolderFilter);
    });

    const orphanChk = this.graphViewWrapEl.querySelector('[data-graph-orphans]') as HTMLInputElement;
    orphanChk?.addEventListener('change', () => {
      this.globalGraphHideOrphans = orphanChk.checked;
      this.globalGraphCtrl?.setHideOrphans(this.globalGraphHideOrphans);
    });

    const searchIn = this.graphViewWrapEl.querySelector('[data-graph-search]') as HTMLInputElement;
    searchIn.addEventListener('input', () => {
      this.globalGraphSearch = searchIn.value;
      this.globalGraphCtrl?.setSearchQuery(this.globalGraphSearch);
    });

    const tagsChk = this.graphViewWrapEl.querySelector('[data-graph-tags]') as HTMLInputElement;
    tagsChk.addEventListener('change', () => {
      this.globalGraphIncludeTags = tagsChk.checked;
      void loadGraph();
    });

    const localChk = this.graphViewWrapEl.querySelector('[data-graph-local]') as HTMLInputElement;
    localChk.addEventListener('change', () => {
      this.globalGraphLocalOnly = localChk.checked;
      void loadGraph();
    });

    this.graphViewWrapEl.querySelector('[data-graph-reset]')?.addEventListener('click', () => {
      this.globalGraphCtrl?.resetCamera();
    });
    this.graphViewWrapEl.querySelector('[data-back-editor]')?.addEventListener('click', () => {
      void this.switchMainView('editor');
    });
  }

  // ───────────────────────── Sync Center View ─────────────────────────
  private mountSyncCenterView(): void {
    if (!this.syncCtrl) {
      this.syncCtrl = new SyncCenterController(this.syncViewWrapEl, this.syncSnap, {
        onSnapshotUpdated: (snap) => {
          this.syncSnap = snap;
          this.updateSidebarSyncIndicators();
        },
        onOpenVaultInEditor: async (vaultPath) => {
          await this.selectVaultPath(vaultPath);
        },
        onGetActiveVault: () => this.workspace?.active_vault || null,
        onToast: (msg, err) => this.showToast(msg, err),
      });
    } else {
      this.syncCtrl.receiveSnapshot(this.syncSnap);
    }
    this.syncCtrl.renderFull();
  }

  // ───────────────────────── Command Palette ─────────────────────────
  private openCommandPalette(initialQuery = ''): void {
    const commands: QuickCommandItem[] = [
      {
        id: 'new_note',
        icon: '📝',
        label: '새 노트 만들기',
        shortcut: '새 페이지',
        run: () => this.promptCreateNote(this.currentDoc ? parentFolderOf(this.currentDoc.path) : ''),
      },
      {
        id: 'daily_note',
        icon: '📅',
        label: '오늘의 일일 노트 열기',
        shortcut: 'Ctrl+D',
        run: () => this.openTodayDailyNote(),
      },
      {
        id: 'daily_folder',
        icon: '⚙️',
        label: '일일 문서 저장 폴더 설정',
        run: () => this.openDailyNoteFolderDialog(),
      },
      {
        id: 'find_replace',
        icon: '🔍',
        label: '현재 문서 내 찾기 / 바꾸기',
        shortcut: 'Ctrl+F',
        run: () => this.editorCtrl?.openFindBar(true),
      },
      {
        id: 'page_setup',
        icon: '📐',
        label: '페이지 용지(A4)·여백·인쇄 설정',
        run: () => this.openPageSetupDialog(),
      },
      {
        id: 'print_note',
        icon: '🖨️',
        label: '현재 문서 인쇄 (1:1 A4 여백)',
        shortcut: 'Ctrl+P',
        run: () => this.handlePrintNote(),
      },
      {
        id: 'export_pdf',
        icon: '📄',
        label: '현재 문서 PDF로 내보내기',
        run: () => this.handleExportPdf(),
      },
      {
        id: 'toggle_tags',
        icon: '🏷️',
        label: this.workspace.show_tags ? '태그 숨기기' : '태그 보기',
        run: () => this.toggleShowTags(),
      },
      {
        id: 'note_crypto',
        icon: '🔐',
        label: '현재 노트 암호화 / 잠금 관리',
        run: () => this.openNoteCryptoDialog(),
      },
      {
        id: 'vault_crypto',
        icon: '🛡️',
        label: '전체 볼트 일괄 암호화 설정',
        run: () => this.openVaultCryptoDialog(),
      },
      {
        id: 'toggle_mode',
        icon: '🔀',
        label: '위지윅 / 마크다운 분할 모드 전환',
        run: () =>
          this.changeEditorMode(
            this.workspace.editor_mode === 'wysiwyg' ? 'markdown' : 'wysiwyg'
          ),
      },
      {
        id: 'toggle_theme',
        icon: this.workspace.theme === 'dark' ? '☀️' : '🌙',
        label: '다크 / 라이트 테마 전환',
        run: async () => {
          const next = this.workspace.theme === 'dark' ? 'light' : 'dark';
          this.applyTheme(next);
          await this.persistWorkspace({ theme: next });
          this.renderTopbar();
        },
      },
      {
        id: 'open_graph',
        icon: '🕸️',
        label: '그래프 뷰 열기',
        shortcut: 'Ctrl+G',
        run: () => this.switchMainView('graph'),
      },
      {
        id: 'open_sync',
        icon: '🔄',
        label: '동기화 센터 열기',
        run: () => this.switchMainView('sync'),
      },
      {
        id: 'sync_now',
        icon: '⚡',
        label: '지금 동기화 실행',
        run: async () => {
          try {
            const snap = await api.syncNow();
            this.syncSnap = snap;
            this.updateSidebarSyncIndicators();
            this.showToast('동기화를 실행했습니다.');
          } catch (err: any) {
            this.showToast(err?.message || '동기화 실행 실패', true);
          }
        },
      },
      {
        id: 'open_vault_dialog',
        icon: '📂',
        label: '다른 볼트 폴더 열기…',
        run: () => this.handleOpenVaultDialog(),
      },
    ];

    if (isAndroidApp()) {
      commands.push(
        {
          id: 'android_update',
          icon: '📲',
          label: 'Android 앱 업데이트 확인',
          run: async () => {
            try {
              const res = await api.androidUpdateNow();
              this.showToast(res.message);
            } catch (err: any) {
              this.showToast(err?.message || '업데이트 확인 실패', true);
            }
          },
        },
        {
          id: 'android_files_perm',
          icon: '🔓',
          label: 'Android 모든 파일 접근 권한 설정',
          run: () => {
            void api.androidGrantFilesAccess();
          },
        }
      );
    }

    openCommandPaletteModal({
      vault: this.workspace.active_vault,
      notes: (this.scan?.notes || []) as NoteMetaDto[],
      commands,
      initialQuery,
      onSelectNote: (path) => void this.openNote(path),
    });
  }
}

const appRoot = document.getElementById('app');
if (appRoot) {
  const studio = new ObsidianSyncStudioApp(appRoot);
  void studio.init();
}
