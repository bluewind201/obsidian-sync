import { api } from './api';
import type {
  NoteMetaDto,
  NotePropertyDto,
  NoteTemplateDto,
  SearchHitDto,
  TrashItemDto,
  TreeNodeDto,
} from './types';

export interface QuickCommandItem {
  id: string;
  icon: string;
  label: string;
  shortcut?: string;
  run: () => void | Promise<void>;
}

export interface ContextMenuEntry {
  icon?: string;
  label?: string;
  danger?: boolean;
  separator?: boolean;
  onClick?: () => void | Promise<void>;
}

const EMOJI_PRESETS = [
  '📝', '📄', '📚', '📌', '💡', '🚀', '🎯', '🔥',
  '✅', '📅', '🗂️', '🧠', '💻', '⚙️', '🔬', '📊',
  '🎨', '🌱', '⭐', '💎', '🧭', '🏆', '💬', '🤝',
  '📖', '🔖', '🛠️', '🧪', '🌐', '🔒', '🎵', '☕',
  '🏠', '✈️', '📷', '🎁', '⚡', '🌈', '🍀', '📎',
];

export const COVER_PRESETS: { name: string; value: string }[] = [
  { name: '노션 선셋', value: 'linear-gradient(135deg, #f6d365 0%, #fda085 100%)' },
  { name: '오션 블루', value: 'linear-gradient(135deg, #2383e2 0%, #6dd5ed 100%)' },
  { name: '오로라 퍼플', value: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)' },
  { name: '포레스트 민트', value: 'linear-gradient(135deg, #11998e 0%, #38ef7d 100%)' },
  { name: '로즈 쿼츠', value: 'linear-gradient(135deg, #ff9a9e 0%, #fecfef 100%)' },
  { name: '미드나잇 슬레이트', value: 'linear-gradient(135deg, #232526 0%, #414345 100%)' },
  { name: '노션 웜그레이', value: 'linear-gradient(135deg, #e6e4df 0%, #c8c4bc 100%)' },
  { name: '딥 인디고', value: 'linear-gradient(135deg, #1e3c72 0%, #2a5298 100%)' },
  { name: '피치 코랄', value: 'linear-gradient(135deg, #ffecd2 0%, #fcb69f 100%)' },
  { name: '사이버 네온', value: 'linear-gradient(135deg, #00c6ff 0%, #0072ff 100%)' },
  { name: '어텀 앰버', value: 'linear-gradient(135deg, #d9730d 0%, #f59e0b 100%)' },
  { name: '노르딕 세이지', value: 'linear-gradient(135deg, #448361 0%, #84cc16 100%)' },
];

const BUILTIN_TEMPLATES: NoteTemplateDto[] = [
  {
    id: '__builtin_meeting',
    name: '회의록 (Meeting Notes)',
    icon: '🤝',
    description: '참석자, 안건, 논의 내용, 액션 아이템 구조',
    properties: [
      { key: 'date', value: new Date().toISOString().slice(0, 10), prop_type: 'date' },
      { key: 'tags', value: '회의록, 업무', prop_type: 'tags' },
    ],
    body: `> [!note] 회의 개요\n> 목적과 핵심 결정 사항을 한 줄로 요약하세요.\n\n## 📋 주요 안건\n- 안건 1\n- 안건 2\n\n## 💬 논의 내용\n- \n\n## ✅ 실행 과제 (Action Items)\n- [ ] 담당자: 할 일 내용\n`,
  },
  {
    id: '__builtin_project',
    name: '프로젝트 기획서 (Project Spec)',
    icon: '🚀',
    description: '목표, 일정, 핵심 지표 및 마일스톤 정리',
    properties: [
      { key: 'tags', value: '프로젝트, 기획', prop_type: 'tags' },
      { key: 'status', value: '진행 중', prop_type: 'text' },
    ],
    body: `> [!important] 핵심 목표\n> 이 프로젝트로 해결하려는 핵심 문제를 적으세요.\n\n## 🎯 배경 및 목표\n- \n\n## 🗓️ 마일스톤\n| 단계 | 목표 일정 | 상태 |\n| --- | --- | --- |\n| 1단계 설계 | ${new Date().toISOString().slice(0, 10)} | 진행 중 |\n| 2단계 구현 |  | 대기 |\n\n## 🔗 관련 노트\n- \n`,
  },
  {
    id: '__builtin_journal',
    name: '일간 / 주간 회고 (Reflection)',
    icon: '🌱',
    description: '배운 점, 잘한 점, 개선할 점 (KPT 회고)',
    properties: [
      { key: 'date', value: new Date().toISOString().slice(0, 10), prop_type: 'date' },
      { key: 'tags', value: '회고', prop_type: 'tags' },
    ],
    body: `## ✨ Keep (잘한 점 / 유지할 점)\n- \n\n## 🔍 Problem (아쉬웠던 점)\n- \n\n## 💡 Try (다음에 시도할 점)\n- [ ] \n`,
  },
];

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

let activeContextMenuEl: HTMLElement | null = null;

export function closeContextMenu(): void {
  if (activeContextMenuEl) {
    activeContextMenuEl.remove();
    activeContextMenuEl = null;
  }
}

export function showContextMenu(x: number, y: number, entries: ContextMenuEntry[]): void {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'context-menu';

  for (const item of entries) {
    if (item.separator) {
      const sep = document.createElement('div');
      sep.className = 'context-menu-sep';
      menu.appendChild(sep);
      continue;
    }
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `context-menu-item${item.danger ? ' danger' : ''}`;
    btn.innerHTML = `<span style="width:18px;text-align:center;">${item.icon || ''}</span><span>${escapeHtml(item.label || '')}</span>`;
    btn.addEventListener('click', (ev) => {
      ev.stopPropagation();
      closeContextMenu();
      void item.onClick?.();
    });
    menu.appendChild(btn);
  }

  document.body.appendChild(menu);
  const rect = menu.getBoundingClientRect();
  const clampedX = Math.min(x, window.innerWidth - rect.width - 8);
  const clampedY = Math.min(y, window.innerHeight - rect.height - 8);
  menu.style.left = `${Math.max(8, clampedX)}px`;
  menu.style.top = `${Math.max(8, clampedY)}px`;
  activeContextMenuEl = menu;

  const dismiss = (e: MouseEvent) => {
    if (activeContextMenuEl && !activeContextMenuEl.contains(e.target as Node)) {
      closeContextMenu();
      window.removeEventListener('mousedown', dismiss);
    }
  };
  setTimeout(() => window.addEventListener('mousedown', dismiss), 0);
}

function createModalBase(widthPx = 560): {
  backdrop: HTMLElement;
  card: HTMLElement;
  close: () => void;
} {
  const backdrop = document.createElement('div');
  backdrop.className = 'modal-backdrop';
  const card = document.createElement('div');
  card.className = 'modal-card';
  card.style.width = `${widthPx}px`;
  backdrop.appendChild(card);

  const onKey = (ev: KeyboardEvent) => {
    if (ev.key === 'Escape') {
      close();
    }
  };

  const close = () => {
    window.removeEventListener('keydown', onKey);
    backdrop.remove();
  };

  backdrop.addEventListener('mousedown', (ev) => {
    if (ev.target === backdrop) close();
  });
  window.addEventListener('keydown', onKey);
  document.body.appendChild(backdrop);

  return { backdrop, card, close };
}

export function showTextPromptModal(opts: {
  title: string;
  label: string;
  placeholder?: string;
  initialValue?: string;
  confirmText?: string;
}): Promise<string | null> {
  return new Promise((resolve) => {
    const { card, close } = createModalBase(440);
    let settled = false;
    const finish = (val: string | null) => {
      if (settled) return;
      settled = true;
      close();
      resolve(val);
    };

    card.innerHTML = `
      <div class="modal-header">
        <span>${escapeHtml(opts.title)}</span>
        <button type="button" class="icon-btn" data-close>✕</button>
      </div>
      <div class="modal-body">
        <label style="display:block;font-size:12.5px;color:var(--text-secondary);margin-bottom:6px;">
          ${escapeHtml(opts.label)}
        </label>
        <input type="text" class="text-input" data-input placeholder="${escapeHtml(opts.placeholder || '')}" />
      </div>
      <div class="modal-footer">
        <button type="button" class="btn" data-cancel>취소</button>
        <button type="button" class="btn btn-primary" data-ok>${escapeHtml(opts.confirmText || '확인')}</button>
      </div>
    `;

    const input = card.querySelector('[data-input]') as HTMLInputElement;
    input.value = opts.initialValue || '';
    setTimeout(() => {
      input.focus();
      input.select();
    }, 10);

    const submit = () => {
      const v = input.value.trim();
      if (!v) return;
      finish(v);
    };

    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        submit();
      }
    });
    card.querySelector('[data-ok]')?.addEventListener('click', submit);
    card.querySelector('[data-cancel]')?.addEventListener('click', () => finish(null));
    card.querySelector('[data-close]')?.addEventListener('click', () => finish(null));
  });
}

export function collectFoldersFromTree(nodes: TreeNodeDto[]): string[] {
  const out: string[] = [''];
  const walk = (list: TreeNodeDto[]) => {
    for (const n of list) {
      if (n.kind === 'folder') {
        out.push(n.path);
        walk(n.children);
      }
    }
  };
  walk(nodes);
  return out;
}

export function showFolderMoveModal(opts: {
  itemName: string;
  folders: string[];
  currentFolder: string;
}): Promise<string | null> {
  return new Promise((resolve) => {
    const { card, close } = createModalBase(460);
    let settled = false;
    const finish = (val: string | null) => {
      if (settled) return;
      settled = true;
      close();
      resolve(val);
    };

    card.innerHTML = `
      <div class="modal-header">
        <span>📁 폴더 이동 — ${escapeHtml(opts.itemName)}</span>
        <button type="button" class="icon-btn" data-close>✕</button>
      </div>
      <div class="modal-body">
        <label style="display:block;font-size:12.5px;color:var(--text-secondary);margin-bottom:8px;">
          이동할 대상 폴더를 선택하세요:
        </label>
        <select class="select-input" data-select></select>
      </div>
      <div class="modal-footer">
        <button type="button" class="btn" data-cancel>취소</button>
        <button type="button" class="btn btn-primary" data-ok>이동하기</button>
      </div>
    `;

    const sel = card.querySelector('[data-select]') as HTMLSelectElement;
    for (const f of opts.folders) {
      const op = document.createElement('option');
      op.value = f;
      op.textContent = f ? `📁 ${f}` : '🏠 볼트 최상위 (루트)';
      if (f === opts.currentFolder) op.selected = true;
      sel.appendChild(op);
    }

    card.querySelector('[data-ok]')?.addEventListener('click', () => finish(sel.value));
    card.querySelector('[data-cancel]')?.addEventListener('click', () => finish(null));
    card.querySelector('[data-close]')?.addEventListener('click', () => finish(null));
  });
}

export function openEmojiPickerModal(opts: {
  currentIcon: string | null;
  onSelect: (emoji: string | null) => void;
}): void {
  const { card, close } = createModalBase(380);
  card.innerHTML = `
    <div class="modal-header">
      <span>😀 페이지 아이콘 선택</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body">
      <div class="emoji-grid" data-grid></div>
      <div style="margin-top:12px;display:flex;gap:6px;">
        <input type="text" class="text-input" data-custom placeholder="직접 이모지/문자 입력 (예: 🦊)" maxlength="4" />
        <button type="button" class="btn btn-primary" data-apply-custom>적용</button>
      </div>
    </div>
    <div class="modal-footer">
      <button type="button" class="btn btn-danger" data-remove>아이콘 제거</button>
      <button type="button" class="btn" data-close2>닫기</button>
    </div>
  `;

  const grid = card.querySelector('[data-grid]')!;
  for (const em of EMOJI_PRESETS) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'emoji-btn';
    b.textContent = em;
    b.addEventListener('click', () => {
      opts.onSelect(em);
      close();
    });
    grid.appendChild(b);
  }

  const customInput = card.querySelector('[data-custom]') as HTMLInputElement;
  if (opts.currentIcon) customInput.value = opts.currentIcon;

  card.querySelector('[data-apply-custom]')?.addEventListener('click', () => {
    const v = customInput.value.trim();
    if (v) {
      opts.onSelect(v);
      close();
    }
  });
  card.querySelector('[data-remove]')?.addEventListener('click', () => {
    opts.onSelect(null);
    close();
  });
  card.querySelector('[data-close]')?.addEventListener('click', close);
  card.querySelector('[data-close2]')?.addEventListener('click', close);
}

export function openCoverPickerModal(opts: {
  vault: string | null;
  currentCover: string | null;
  onSelect: (cover: string | null) => void;
  onToast: (msg: string, isErr?: boolean) => void;
}): void {
  const { card, close } = createModalBase(500);
  card.innerHTML = `
    <div class="modal-header">
      <span>🖼️ 노션 페이지 커버 배너</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body">
      <div style="font-size:12px;font-weight:600;color:var(--text-secondary);margin-bottom:8px;">
        추천 그라데이션 &amp; 컬러
      </div>
      <div class="cover-preset-grid" data-presets></div>
      <div style="margin-top:14px;font-size:12px;font-weight:600;color:var(--text-secondary);margin-bottom:6px;">
        이미지 URL 또는 내 PC 이미지
      </div>
      <div style="display:flex;gap:6px;">
        <input type="text" class="text-input" data-url placeholder="https://... 이미지 주소 입력" />
        <button type="button" class="btn btn-primary" data-apply-url>적용</button>
      </div>
      <div style="margin-top:8px;">
        <button type="button" class="btn btn-sm" data-pick-pc>🖼️ 내 PC에서 커버 이미지 고르기</button>
      </div>
    </div>
    <div class="modal-footer">
      <button type="button" class="btn btn-danger" data-remove>커버 제거</button>
      <button type="button" class="btn" data-close2>닫기</button>
    </div>
  `;

  const grid = card.querySelector('[data-presets]')!;
  for (const p of COVER_PRESETS) {
    const sw = document.createElement('button');
    sw.type = 'button';
    sw.className = 'cover-preset-swatch';
    sw.style.background = p.value;
    sw.title = p.name;
    sw.addEventListener('click', () => {
      opts.onSelect(p.value);
      close();
    });
    grid.appendChild(sw);
  }

  const urlInput = card.querySelector('[data-url]') as HTMLInputElement;
  if (opts.currentCover && !opts.currentCover.startsWith('linear-gradient')) {
    urlInput.value = opts.currentCover;
  }

  card.querySelector('[data-apply-url]')?.addEventListener('click', () => {
    const v = urlInput.value.trim();
    if (v) {
      opts.onSelect(v);
      close();
    }
  });

  card.querySelector('[data-pick-pc]')?.addEventListener('click', async () => {
    if (!opts.vault) return;
    try {
      const res = await api.pickAttachment(opts.vault);
      if (res.picked && res.media_url) {
        opts.onSelect(res.media_url);
        close();
      }
    } catch (err: any) {
      opts.onToast(err?.message || '커버 이미지를 불러오지 못했습니다', true);
    }
  });

  card.querySelector('[data-remove]')?.addEventListener('click', () => {
    opts.onSelect(null);
    close();
  });
  card.querySelector('[data-close]')?.addEventListener('click', close);
  card.querySelector('[data-close2]')?.addEventListener('click', close);
}

export async function openTemplatesModal(opts: {
  vault: string;
  templatesFolder: string;
  onApplyTemplate: (tpl: { icon: string; properties: NotePropertyDto[]; body: string }) => void;
  onToast: (msg: string, err?: boolean) => void;
}): Promise<void> {
  const { card, close } = createModalBase(520);
  card.innerHTML = `
    <div class="modal-header">
      <span>✨ 노트 템플릿 적용</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body" data-body>
      <div style="display:flex;align-items:center;gap:8px;color:var(--text-secondary);">
        <span class="spinner"></span> 템플릿을 불러오는 중…
      </div>
    </div>
  `;
  card.querySelector('[data-close]')?.addEventListener('click', close);

  let vaultTemplates: NoteTemplateDto[] = [];
  try {
    const res = await api.vaultTemplates(opts.vault, opts.templatesFolder);
    vaultTemplates = res.templates || [];
  } catch (err: any) {
    opts.onToast(err?.message || '볼트 템플릿 조회 실패', true);
  }

  const all = [...vaultTemplates, ...BUILTIN_TEMPLATES];
  const bodyEl = card.querySelector('[data-body]')!;
  bodyEl.innerHTML = `
    <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:10px;">
      선택한 템플릿의 본문과 속성을 현재 노트에 추가합니다. (볼트 내 <code>${escapeHtml(opts.templatesFolder || 'Templates')}</code> 폴더의 마크다운 파일도 자동 인식됩니다.)
    </div>
    <div style="display:flex;flex-direction:column;gap:6px;" data-list></div>
  `;
  const listEl = bodyEl.querySelector('[data-list]')!;
  for (const tpl of all) {
    const row = document.createElement('div');
    row.className = 'palette-item';
    row.style.border = '1px solid var(--border-subtle)';
    row.innerHTML = `
      <span style="font-size:22px;">${escapeHtml(tpl.icon || '📄')}</span>
      <div style="flex:1;min-width:0;">
        <div style="font-weight:600;font-size:13.5px;">${escapeHtml(tpl.name)}</div>
        <div style="font-size:12px;color:var(--text-secondary);">${escapeHtml(tpl.description || '노트 템플릿')}</div>
      </div>
      <span class="btn btn-sm btn-primary">적용</span>
    `;
    row.addEventListener('click', () => {
      opts.onApplyTemplate({
        icon: tpl.icon,
        properties: tpl.properties || [],
        body: tpl.body,
      });
      close();
    });
    listEl.appendChild(row);
  }
}

export async function openTrashModal(opts: {
  vault: string;
  onRestored: (restoredPath: string) => void;
  onEmptied: () => void;
  onToast: (msg: string, err?: boolean) => void;
}): Promise<void> {
  const { card, close } = createModalBase(560);
  let confirmEmpty = false;

  const renderList = async () => {
    card.innerHTML = `
      <div class="modal-header">
        <span>🗑️ 볼트 휴지통 (.trash)</span>
        <button type="button" class="icon-btn" data-close>✕</button>
      </div>
      <div class="modal-body" data-body>
        <span class="spinner"></span> 불러오는 중…
      </div>
      <div class="modal-footer" data-footer>
        <button type="button" class="btn btn-danger" data-empty>휴지통 비우기</button>
        <button type="button" class="btn" data-close2>닫기</button>
      </div>
    `;
    card.querySelector('[data-close]')?.addEventListener('click', close);
    card.querySelector('[data-close2]')?.addEventListener('click', close);

    let items: TrashItemDto[] = [];
    try {
      const res = await api.trashList(opts.vault);
      items = res.items || [];
    } catch (err: any) {
      opts.onToast(err?.message || '휴지통 조회 실패', true);
    }

    const body = card.querySelector('[data-body]')!;
    if (items.length === 0) {
      body.innerHTML = `<div style="padding:28px;text-align:center;color:var(--text-muted);">휴지통이 비어 있습니다.</div>`;
    } else {
      body.innerHTML = '';
      for (const item of items) {
        const row = document.createElement('div');
        row.style.cssText =
          'display:flex;align-items:center;justify-content:space-between;gap:10px;padding:8px 10px;border-bottom:1px solid var(--border-subtle);';
        row.innerHTML = `
          <div style="min-width:0;flex:1;">
            <div style="font-weight:600;font-size:13.5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">
              ${item.is_dir ? '📁' : '📄'} ${escapeHtml(item.display_name)}
            </div>
            <div style="font-size:11.5px;color:var(--text-muted);">${escapeHtml(item.modified_at)}</div>
          </div>
          <button type="button" class="btn btn-sm btn-primary" data-restore>복원</button>
        `;
        row.querySelector('[data-restore]')?.addEventListener('click', async () => {
          try {
            const r = await api.trashRestore(opts.vault, item.trash_name);
            opts.onToast(`복원 완료: ${r.restored_path}`);
            opts.onRestored(r.restored_path);
            await renderList();
          } catch (err: any) {
            opts.onToast(err?.message || '복원에 실패했습니다', true);
          }
        });
        body.appendChild(row);
      }
    }

    const emptyBtn = card.querySelector('[data-empty]') as HTMLButtonElement;
    if (items.length === 0) {
      emptyBtn.disabled = true;
    }
    emptyBtn.addEventListener('click', async () => {
      if (!confirmEmpty) {
        confirmEmpty = true;
        emptyBtn.textContent = '정말 영구 삭제할까요? (한 번 더 클릭)';
        return;
      }
      try {
        await api.trashEmpty(opts.vault);
        confirmEmpty = false;
        opts.onToast('휴지통을 비웠습니다.');
        opts.onEmptied();
        close();
      } catch (err: any) {
        opts.onToast(err?.message || '휴지통 비우기 실패', true);
      }
    });
  };

  await renderList();
}

export function openWikiLinkPickerModal(opts: {
  notes: NoteMetaDto[];
  onPick: (noteTitle: string) => void;
}): void {
  const { card, close } = createModalBase(480);
  card.innerHTML = `
    <input type="text" class="palette-input" data-input placeholder="연결할 노트 제목 검색… (없으면 새 노트 링크로 삽입)" />
    <div class="palette-list" data-list></div>
  `;

  const input = card.querySelector('[data-input]') as HTMLInputElement;
  const listEl = card.querySelector('[data-list]') as HTMLElement;
  let selectedIdx = 0;
  let filtered: { title: string; subtitle: string; icon: string }[] = [];

  const update = () => {
    const q = input.value.trim().toLowerCase();
    filtered = opts.notes
      .filter((n) => !q || n.title.toLowerCase().includes(q) || n.path.toLowerCase().includes(q))
      .slice(0, 40)
      .map((n) => ({
        title: n.title,
        subtitle: n.path,
        icon: n.icon || '📄',
      }));

    if (input.value.trim() && !filtered.some((x) => x.title.toLowerCase() === q)) {
      filtered.push({
        title: input.value.trim(),
        subtitle: '새 위키링크로 삽입',
        icon: '✨',
      });
    }

    selectedIdx = Math.min(selectedIdx, Math.max(0, filtered.length - 1));
    listEl.innerHTML = '';
    if (filtered.length === 0) {
      listEl.innerHTML = `<div style="padding:18px;text-align:center;color:var(--text-muted);">일치하는 노트가 없습니다.</div>`;
      return;
    }

    filtered.forEach((item, idx) => {
      const row = document.createElement('div');
      row.className = `palette-item${idx === selectedIdx ? ' selected' : ''}`;
      row.innerHTML = `
        <span style="font-size:17px;">${escapeHtml(item.icon)}</span>
        <div style="flex:1;min-width:0;">
          <div style="font-weight:600;font-size:13.5px;">${escapeHtml(item.title)}</div>
          <div style="font-size:11.5px;color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(item.subtitle)}</div>
        </div>
      `;
      row.addEventListener('click', () => {
        close();
        opts.onPick(item.title);
      });
      listEl.appendChild(row);
    });
  };

  input.addEventListener('input', () => {
    selectedIdx = 0;
    update();
  });

  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      selectedIdx = Math.min(filtered.length - 1, selectedIdx + 1);
      update();
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      selectedIdx = Math.max(0, selectedIdx - 1);
      update();
    } else if (ev.key === 'Enter') {
      ev.preventDefault();
      const chosen = filtered[selectedIdx];
      if (chosen) {
        close();
        opts.onPick(chosen.title);
      }
    }
  });

  update();
  setTimeout(() => input.focus(), 10);
}

export function openCommandPaletteModal(opts: {
  vault: string | null;
  notes: NoteMetaDto[];
  commands: QuickCommandItem[];
  initialQuery?: string;
  onSelectNote: (path: string) => void;
}): void {
  const { card, close } = createModalBase(620);
  card.innerHTML = `
    <input type="text" class="palette-input" data-input placeholder="🔍 노트 제목·본문 통합 검색 또는 빠른 명령어 실행…" />
    <div class="palette-list" data-list></div>
  `;

  const input = card.querySelector('[data-input]') as HTMLInputElement;
  const listEl = card.querySelector('[data-list]') as HTMLElement;
  input.value = opts.initialQuery || '';

  interface PaletteRow {
    icon: string;
    title: string;
    subtitle?: string;
    badge?: string;
    action: () => void;
  }

  let rows: PaletteRow[] = [];
  let selectedIdx = 0;
  let searchTimer: number | null = null;
  let searchHits: SearchHitDto[] = [];

  const renderRows = () => {
    const q = input.value.trim().toLowerCase();
    rows = [];

    // 1. Matching quick commands
    for (const cmd of opts.commands) {
      if (!q || cmd.label.toLowerCase().includes(q)) {
        rows.push({
          icon: cmd.icon,
          title: cmd.label,
          badge: cmd.shortcut || '명령어',
          action: () => {
            close();
            void cmd.run();
          },
        });
      }
    }

    // 2. Full-text search hits if query non-empty, else recent/all notes
    if (q && searchHits.length > 0) {
      for (const hit of searchHits) {
        const snippet = hit.matches[0]?.snippet || hit.path;
        rows.push({
          icon: hit.icon || '📄',
          title: hit.title,
          subtitle: snippet,
          badge: hit.path,
          action: () => {
            close();
            opts.onSelectNote(hit.path);
          },
        });
      }
    } else {
      const noteMatches = opts.notes
        .filter((n) => !q || n.title.toLowerCase().includes(q) || n.path.toLowerCase().includes(q))
        .slice(0, 25);
      for (const n of noteMatches) {
        rows.push({
          icon: n.icon || '📄',
          title: n.title,
          subtitle: n.path,
          badge: n.updated_at,
          action: () => {
            close();
            opts.onSelectNote(n.path);
          },
        });
      }
    }

    selectedIdx = Math.min(selectedIdx, Math.max(0, rows.length - 1));
    listEl.innerHTML = '';

    if (rows.length === 0) {
      listEl.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);">검색 결과가 없습니다.</div>`;
      return;
    }

    rows.forEach((r, idx) => {
      const el = document.createElement('div');
      el.className = `palette-item${idx === selectedIdx ? ' selected' : ''}`;
      el.innerHTML = `
        <span style="font-size:17px;width:22px;text-align:center;">${escapeHtml(r.icon)}</span>
        <div style="flex:1;min-width:0;">
          <div style="font-weight:600;font-size:13.5px;">${escapeHtml(r.title)}</div>
          ${
            r.subtitle
              ? `<div style="font-size:12px;color:var(--text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">${escapeHtml(r.subtitle)}</div>`
              : ''
          }
        </div>
        ${r.badge ? `<span class="kbd-hint">${escapeHtml(r.badge)}</span>` : ''}
      `;
      el.addEventListener('click', r.action);
      listEl.appendChild(el);
    });
  };

  const triggerSearch = () => {
    const q = input.value.trim();
    if (!q || !opts.vault) {
      searchHits = [];
      renderRows();
      return;
    }
    if (searchTimer !== null) window.clearTimeout(searchTimer);
    searchTimer = window.setTimeout(async () => {
      if (!opts.vault) return;
      try {
        const res = await api.searchVault(opts.vault, q);
        searchHits = res.hits || [];
        renderRows();
      } catch {
        // ignore transient search error
      }
    }, 130);
  };

  input.addEventListener('input', () => {
    selectedIdx = 0;
    renderRows();
    triggerSearch();
  });

  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      selectedIdx = Math.min(rows.length - 1, selectedIdx + 1);
      renderRows();
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      selectedIdx = Math.max(0, selectedIdx - 1);
      renderRows();
    } else if (ev.key === 'Enter') {
      ev.preventDefault();
      rows[selectedIdx]?.action();
    }
  });

  renderRows();
  if (opts.initialQuery) {
    triggerSearch();
  }
  setTimeout(() => {
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  }, 10);
}

// ───────────────────────── v0.6.0 Modals ─────────────────────────

export interface PageSetupValues {
  page_layout: 'a4' | 'full';
  page_orientation: 'portrait' | 'landscape';
  margin_top_cm: number;
  margin_bottom_cm: number;
  margin_left_cm: number;
  margin_right_cm: number;
  auto_fit_a4: boolean;
  show_page_guides: boolean;
  editor_zoom: number;
  print_include_title: boolean;
  print_include_properties: boolean;
}

export function openPageSetupModal(opts: {
  initial: PageSetupValues;
  onApply: (patch: PageSetupValues) => Promise<void> | void;
  onPrintNow: (patch: PageSetupValues) => Promise<void> | void;
  onExportPdfNow: (patch: PageSetupValues) => Promise<void> | void;
}): void {
  const { card, close } = createModalBase(650);
  const state: PageSetupValues = { ...opts.initial };

  const clampMargin = (v: number): number => {
    if (!Number.isFinite(v)) return 0;
    return Math.round(Math.min(6, Math.max(0, v)) * 10) / 10;
  };

  card.innerHTML = `
    <div class="modal-header">
      <span>📐 페이지 용지(A4) · 여백 · 인쇄/PDF 설정</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body">
      <div class="page-setup-grid">
        <div style="display:flex;flex-direction:column;gap:12px;">
          <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px;">
            <div>
              <label style="display:block;font-size:12px;color:var(--text-secondary);margin-bottom:4px;">편집기 레이아웃</label>
              <select class="text-input" data-ps-layout>
                <option value="a4">📄 A4 용지 기준</option>
                <option value="full">🖥️ 전체 화면 폭</option>
              </select>
            </div>
            <div>
              <label style="display:block;font-size:12px;color:var(--text-secondary);margin-bottom:4px;">용지 방향</label>
              <select class="text-input" data-ps-orient>
                <option value="portrait">세로 (210 × 297 mm)</option>
                <option value="landscape">가로 (297 × 210 mm)</option>
              </select>
            </div>
          </div>

          <div>
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:6px;">
              <span style="font-size:12px;font-weight:700;color:var(--text-secondary);">여백 설정 (0.0 ~ 6.0 cm)</span>
              <div style="display:flex;gap:4px;flex-wrap:wrap;">
                <button type="button" class="btn btn-sm" data-preset="default">기본 (상2/하1/좌우3)</button>
                <button type="button" class="btn btn-sm" data-preset="standard">표준 (2.5)</button>
                <button type="button" class="btn btn-sm" data-preset="narrow">좁게 (1.5)</button>
              </div>
            </div>
            <div style="display:grid;grid-template-columns:repeat(4,1fr);gap:8px;">
              <div>
                <label style="display:block;font-size:11.5px;color:var(--text-muted);margin-bottom:3px;">상단 (cm)</label>
                <input type="number" step="0.1" min="0" max="6" class="text-input" data-m-top />
              </div>
              <div>
                <label style="display:block;font-size:11.5px;color:var(--text-muted);margin-bottom:3px;">하단 (cm)</label>
                <input type="number" step="0.1" min="0" max="6" class="text-input" data-m-bottom />
              </div>
              <div>
                <label style="display:block;font-size:11.5px;color:var(--text-muted);margin-bottom:3px;">좌측 (cm)</label>
                <input type="number" step="0.1" min="0" max="6" class="text-input" data-m-left />
              </div>
              <div>
                <label style="display:block;font-size:11.5px;color:var(--text-muted);margin-bottom:3px;">우측 (cm)</label>
                <input type="number" step="0.1" min="0" max="6" class="text-input" data-m-right />
              </div>
            </div>
          </div>

          <div style="display:flex;align-items:center;gap:10px;">
            <span style="font-size:12px;color:var(--text-secondary);min-width:92px;">화면 배율: <strong data-zoom-val>100%</strong></span>
            <input type="range" min="50" max="150" step="5" data-ps-zoom style="flex:1;" />
          </div>

          <div style="display:flex;flex-direction:column;gap:6px;font-size:12.5px;">
            <label style="display:flex;align-items:center;gap:7px;cursor:pointer;">
              <input type="checkbox" data-ps-autofit /> 모니터 창 폭에 맞춰 A4 자동 축소 (Auto-Fit)
            </label>
            <label style="display:flex;align-items:center;gap:7px;cursor:pointer;">
              <input type="checkbox" data-ps-guides /> A4 페이지 구분선 (297mm 단위) 표시
            </label>
            <label style="display:flex;align-items:center;gap:7px;cursor:pointer;">
              <input type="checkbox" data-ps-title /> 인쇄·PDF 상단에 문서 제목 포함
            </label>
            <label style="display:flex;align-items:center;gap:7px;cursor:pointer;">
              <input type="checkbox" data-ps-props /> 인쇄·PDF 상단에 문서 속성 표 포함
            </label>
          </div>
        </div>

        <div class="page-setup-preview-wrap">
          <div style="font-size:11.5px;font-weight:700;color:var(--text-secondary);">용지 미리보기</div>
          <div class="page-setup-preview-sheet" data-preview-sheet>
            <div class="page-setup-preview-content"></div>
          </div>
          <div style="font-size:11px;color:var(--text-muted);text-align:center;" data-preview-info></div>
        </div>
      </div>
    </div>
    <div class="modal-footer" style="justify-content:space-between;">
      <div style="display:flex;gap:6px;">
        <button type="button" class="btn" data-ps-print>🖨️ 인쇄</button>
        <button type="button" class="btn" data-ps-pdf>📄 PDF 출력</button>
      </div>
      <div style="display:flex;gap:6px;">
        <button type="button" class="btn" data-cancel>취소</button>
        <button type="button" class="btn btn-primary" data-apply>설정 적용</button>
      </div>
    </div>
  `;

  const layoutSel = card.querySelector('[data-ps-layout]') as HTMLSelectElement;
  const orientSel = card.querySelector('[data-ps-orient]') as HTMLSelectElement;
  const topIn = card.querySelector('[data-m-top]') as HTMLInputElement;
  const bottomIn = card.querySelector('[data-m-bottom]') as HTMLInputElement;
  const leftIn = card.querySelector('[data-m-left]') as HTMLInputElement;
  const rightIn = card.querySelector('[data-m-right]') as HTMLInputElement;
  const zoomIn = card.querySelector('[data-ps-zoom]') as HTMLInputElement;
  const zoomVal = card.querySelector('[data-zoom-val]') as HTMLElement;
  const autoFitChk = card.querySelector('[data-ps-autofit]') as HTMLInputElement;
  const guidesChk = card.querySelector('[data-ps-guides]') as HTMLInputElement;
  const titleChk = card.querySelector('[data-ps-title]') as HTMLInputElement;
  const propsChk = card.querySelector('[data-ps-props]') as HTMLInputElement;
  const previewSheet = card.querySelector('[data-preview-sheet]') as HTMLElement;
  const previewInfo = card.querySelector('[data-preview-info]') as HTMLElement;

  const syncDomFromState = () => {
    layoutSel.value = state.page_layout;
    orientSel.value = state.page_orientation;
    topIn.value = String(state.margin_top_cm);
    bottomIn.value = String(state.margin_bottom_cm);
    leftIn.value = String(state.margin_left_cm);
    rightIn.value = String(state.margin_right_cm);
    zoomIn.value = String(state.editor_zoom);
    zoomVal.textContent = `${state.editor_zoom}%`;
    autoFitChk.checked = state.auto_fit_a4;
    guidesChk.checked = state.show_page_guides;
    titleChk.checked = state.print_include_title;
    propsChk.checked = state.print_include_properties;
  };

  const updatePreview = () => {
    state.page_layout = layoutSel.value === 'full' ? 'full' : 'a4';
    state.page_orientation = orientSel.value === 'landscape' ? 'landscape' : 'portrait';
    state.margin_top_cm = clampMargin(Number(topIn.value));
    state.margin_bottom_cm = clampMargin(Number(bottomIn.value));
    state.margin_left_cm = clampMargin(Number(leftIn.value));
    state.margin_right_cm = clampMargin(Number(rightIn.value));
    state.editor_zoom = Math.min(150, Math.max(50, Math.round(Number(zoomIn.value) || 100)));
    state.auto_fit_a4 = autoFitChk.checked;
    state.show_page_guides = guidesChk.checked;
    state.print_include_title = titleChk.checked;
    state.print_include_properties = propsChk.checked;

    zoomVal.textContent = `${state.editor_zoom}%`;

    const isLand = state.page_orientation === 'landscape';
    previewSheet.classList.toggle('landscape', isLand);
    const pageW = isLand ? 29.7 : 21.0;
    const pageH = isLand ? 21.0 : 29.7;
    const sheetPxW = isLand ? 168 : 126;
    const sheetPxH = isLand ? 119 : 178;

    previewSheet.style.paddingTop = `${Math.round((state.margin_top_cm / pageH) * sheetPxH)}px`;
    previewSheet.style.paddingBottom = `${Math.round((state.margin_bottom_cm / pageH) * sheetPxH)}px`;
    previewSheet.style.paddingLeft = `${Math.round((state.margin_left_cm / pageW) * sheetPxW)}px`;
    previewSheet.style.paddingRight = `${Math.round((state.margin_right_cm / pageW) * sheetPxW)}px`;

    const contentW = Math.max(1, pageW - state.margin_left_cm - state.margin_right_cm);
    const contentH = Math.max(1, pageH - state.margin_top_cm - state.margin_bottom_cm);
    previewInfo.textContent = `유효 본문: ${contentW.toFixed(1)} × ${contentH.toFixed(1)} cm`;
  };

  syncDomFromState();
  updatePreview();

  for (const el of [
    layoutSel,
    orientSel,
    topIn,
    bottomIn,
    leftIn,
    rightIn,
    zoomIn,
    autoFitChk,
    guidesChk,
    titleChk,
    propsChk,
  ]) {
    el.addEventListener('input', updatePreview);
    el.addEventListener('change', updatePreview);
  }

  card.querySelectorAll('[data-preset]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const kind = btn.getAttribute('data-preset');
      if (kind === 'default') {
        state.margin_top_cm = 2.0;
        state.margin_bottom_cm = 1.0;
        state.margin_left_cm = 3.0;
        state.margin_right_cm = 3.0;
      } else if (kind === 'standard') {
        state.margin_top_cm = 2.5;
        state.margin_bottom_cm = 2.5;
        state.margin_left_cm = 2.5;
        state.margin_right_cm = 2.5;
      } else {
        state.margin_top_cm = 1.5;
        state.margin_bottom_cm = 1.5;
        state.margin_left_cm = 1.5;
        state.margin_right_cm = 1.5;
      }
      syncDomFromState();
      updatePreview();
    });
  });

  card.querySelector('[data-close]')?.addEventListener('click', close);
  card.querySelector('[data-cancel]')?.addEventListener('click', close);
  card.querySelector('[data-apply]')?.addEventListener('click', () => {
    updatePreview();
    close();
    void opts.onApply({ ...state });
  });
  card.querySelector('[data-ps-print]')?.addEventListener('click', () => {
    updatePreview();
    close();
    void opts.onPrintNow({ ...state });
  });
  card.querySelector('[data-ps-pdf]')?.addEventListener('click', () => {
    updatePreview();
    close();
    void opts.onExportPdfNow({ ...state });
  });
}

export function openDailyNoteFolderModal(opts: {
  vault: string;
  folders: string[];
  currentFolder: string;
  onSelect: (folder: string) => Promise<void> | void;
  onFolderCreated: () => Promise<void>;
  onToast: (msg: string, isError?: boolean) => void;
}): void {
  const { card, close } = createModalBase(500);
  let selected = opts.currentFolder;

  const uniqueFolders = Array.from(
    new Set(
      [...opts.folders, opts.currentFolder]
        .map((f) => f.trim().replace(/^\/+|\/+$/g, ''))
        .filter((f) => f.length > 0)
    )
  ).sort((a, b) => a.localeCompare(b, 'ko'));

  card.innerHTML = `
    <div class="modal-header">
      <span>📅 일일 문서 저장 폴더 설정</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body">
      <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:10px;line-height:1.5;">
        현재 볼트에서 <strong>오늘의 일일 노트(Ctrl+D)</strong>를 생성할 대상 폴더를 선택하세요.
      </div>
      <div class="palette-list" data-folder-list style="max-height:240px;border:1px solid var(--border-subtle);border-radius:8px;padding:6px;margin-bottom:12px;"></div>
      <div style="padding-top:10px;border-top:1px solid var(--border-subtle);">
        <label style="display:block;font-size:12px;font-weight:700;color:var(--text-secondary);margin-bottom:6px;">
          ✨ 새 일일 폴더 바로 만들기
        </label>
        <div style="display:flex;gap:8px;">
          <input type="text" class="text-input" data-new-folder placeholder="예: 일일업무 또는 Journal/2026" style="flex:1;" />
          <button type="button" class="btn" data-create-folder>📁 생성 후 지정</button>
        </div>
      </div>
    </div>
    <div class="modal-footer">
      <button type="button" class="btn" data-cancel>취소</button>
      <button type="button" class="btn btn-primary" data-save>이 폴더로 저장</button>
    </div>
  `;

  const listEl = card.querySelector('[data-folder-list]') as HTMLElement;
  const renderList = () => {
    listEl.innerHTML = '';
    const choices: Array<{ value: string; label: string }> = [
      { value: '', label: '📂 (볼트 최상위 루트 폴더)' },
      ...uniqueFolders.map((f) => ({ value: f, label: `📁 ${f}` })),
    ];
    for (const c of choices) {
      const row = document.createElement('div');
      row.className = `palette-item${selected === c.value ? ' selected' : ''}`;
      row.innerHTML = `
        <span style="flex:1;font-weight:${selected === c.value ? '700' : '500'};">${escapeHtml(c.label)}</span>
        <span style="font-size:12px;color:var(--accent);">${selected === c.value ? '✓ 선택됨' : ''}</span>
      `;
      row.addEventListener('click', () => {
        selected = c.value;
        renderList();
      });
      listEl.appendChild(row);
    }
  };
  renderList();

  const newFolderInput = card.querySelector('[data-new-folder]') as HTMLInputElement;
  const handleCreate = async () => {
    const raw = newFolderInput.value.trim().replace(/^\/+|\/+$/g, '');
    if (!raw) {
      opts.onToast('폴더 이름을 입력하세요.', true);
      return;
    }
    try {
      const res = await api.createFolder(opts.vault, '', raw);
      await opts.onFolderCreated();
      close();
      await opts.onSelect(res.path);
    } catch (err: any) {
      opts.onToast(err?.message || '폴더 생성에 실패했습니다.', true);
    }
  };

  card.querySelector('[data-create-folder]')?.addEventListener('click', () => void handleCreate());
  newFolderInput.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') {
      ev.preventDefault();
      void handleCreate();
    }
  });

  card.querySelector('[data-close]')?.addEventListener('click', close);
  card.querySelector('[data-cancel]')?.addEventListener('click', close);
  card.querySelector('[data-save]')?.addEventListener('click', () => {
    close();
    void opts.onSelect(selected);
  });
}

export function openNoteCryptoModal(opts: {
  vault: string;
  notePath: string;
  noteTitle: string;
  encrypted: boolean;
  locked: boolean;
  hint?: string | null;
  onCompleted: () => Promise<void>;
  onToast: (msg: string, isError?: boolean) => void;
}): void {
  const { card, close } = createModalBase(480);

  card.innerHTML = `
    <div class="modal-header">
      <span>🔐 노트 암호화 / 잠금 관리</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body" data-crypto-body></div>
  `;

  const body = card.querySelector('[data-crypto-body]') as HTMLElement;
  card.querySelector('[data-close]')?.addEventListener('click', close);

  if (!opts.encrypted) {
    body.innerHTML = `
      <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:12px;line-height:1.5;">
        <strong>${escapeHtml(opts.noteTitle)}</strong> 노트를 <strong>AES-256-GCM</strong>으로 암호화합니다.
        디스크와 모든 클라우드·NAS 동기화 저장소에 암호문만 기록됩니다.
      </div>
      <div style="display:flex;flex-direction:column;gap:10px;">
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">암호 (4자 이상)</label>
          <input type="password" class="text-input" data-pw placeholder="암호 입력" />
        </div>
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">암호 확인</label>
          <input type="password" class="text-input" data-pw2 placeholder="암호 다시 입력" />
        </div>
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">암호 힌트 (선택)</label>
          <input type="text" class="text-input" data-hint placeholder="예: 자주 쓰는 조합" />
        </div>
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:6px;">
          <button type="button" class="btn" data-cancel>취소</button>
          <button type="button" class="btn btn-primary" data-encrypt-btn>🔒 이 노트 암호화</button>
        </div>
      </div>
    `;
    const pw1 = body.querySelector('[data-pw]') as HTMLInputElement;
    const pw2 = body.querySelector('[data-pw2]') as HTMLInputElement;
    const hintIn = body.querySelector('[data-hint]') as HTMLInputElement;
    body.querySelector('[data-cancel]')?.addEventListener('click', close);
    body.querySelector('[data-encrypt-btn]')?.addEventListener('click', async () => {
      const p1 = pw1.value;
      if (p1.length < 4) {
        opts.onToast('암호는 최소 4자 이상 입력해 주세요.', true);
        return;
      }
      if (p1 !== pw2.value) {
        opts.onToast('두 암호가 일치하지 않습니다.', true);
        return;
      }
      try {
        await api.cryptoEncryptNote(opts.vault, opts.notePath, p1, hintIn.value.trim() || null);
        close();
        await opts.onCompleted();
        opts.onToast('노트를 AES-256-GCM으로 암호화했습니다.');
      } catch (err: any) {
        opts.onToast(err?.message || '노트 암호화 실패', true);
      }
    });
    setTimeout(() => pw1.focus(), 10);
    return;
  }

  body.innerHTML = `
    <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:12px;line-height:1.5;">
      <strong>${escapeHtml(opts.noteTitle)}</strong> 노트는 현재
      <strong>${opts.locked ? '🔒 잠금 상태' : '🔓 세션 잠금 해제됨'}</strong>입니다.
      ${opts.hint ? `<div class="locked-note-hint" style="margin-top:8px;">힌트: ${escapeHtml(opts.hint)}</div>` : ''}
    </div>
    <div style="display:flex;flex-direction:column;gap:10px;">
      ${
        !opts.locked
          ? `<button type="button" class="btn" data-lock-now>🔒 지금 즉시 이 노트 잠그기</button>`
          : ''
      }
      <div style="padding-top:8px;border-top:1px solid var(--border-subtle);">
        <label style="display:block;font-size:12px;margin-bottom:4px;">암호 입력 (잠금 해제 또는 영구 복호화)</label>
        <input type="password" class="text-input" data-pw placeholder="암호 입력" />
      </div>
      <div style="display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap;">
        <button type="button" class="btn" data-cancel>닫기</button>
        ${
          opts.locked
            ? `<button type="button" class="btn btn-primary" data-unlock-btn>🔓 잠금 해제</button>`
            : ''
        }
        <button type="button" class="btn btn-danger" data-decrypt-btn>⚠️ 암호화 영구 해제 (평문)</button>
      </div>
    </div>
  `;

  const pwIn = body.querySelector('[data-pw]') as HTMLInputElement;
  body.querySelector('[data-cancel]')?.addEventListener('click', close);
  body.querySelector('[data-lock-now]')?.addEventListener('click', async () => {
    try {
      await api.cryptoLockNote(opts.vault, opts.notePath);
      close();
      await opts.onCompleted();
      opts.onToast('노트를 잠갔습니다.');
    } catch (err: any) {
      opts.onToast(err?.message || '노트 잠금 실패', true);
    }
  });
  body.querySelector('[data-unlock-btn]')?.addEventListener('click', async () => {
    if (!pwIn.value) {
      opts.onToast('암호를 입력해 주세요.', true);
      return;
    }
    try {
      await api.cryptoUnlockNote(opts.vault, opts.notePath, pwIn.value);
      close();
      await opts.onCompleted();
      opts.onToast('노트 잠금이 해제되었습니다.');
    } catch (err: any) {
      opts.onToast(err?.message || '암호가 올바르지 않습니다.', true);
    }
  });
  body.querySelector('[data-decrypt-btn]')?.addEventListener('click', async () => {
    if (!pwIn.value) {
      opts.onToast('영구 복호화하려면 암호를 입력해 주세요.', true);
      return;
    }
    try {
      await api.cryptoDecryptNote(opts.vault, opts.notePath, pwIn.value);
      close();
      await opts.onCompleted();
      opts.onToast('노트 암호화를 영구 해제(평문 저장)했습니다.');
    } catch (err: any) {
      opts.onToast(err?.message || '복호화에 실패했습니다.', true);
    }
  });
  setTimeout(() => pwIn.focus(), 10);
}

export function openVaultCryptoModal(opts: {
  vault: string;
  status: {
    vault_encrypted: boolean;
    vault_unlocked: boolean;
    total_notes: number;
    encrypted_notes: number;
    unlocked_notes: number;
    auto_encrypt_new_notes: boolean;
    hint: string | null;
  };
  onCompleted: () => Promise<void>;
  onToast: (msg: string, isError?: boolean) => void;
}): void {
  const { card, close } = createModalBase(540);
  const st = opts.status;

  card.innerHTML = `
    <div class="modal-header">
      <span>🔐 전체 볼트 암호화 설정 (AES-256-GCM)</span>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="modal-body" data-vault-crypto-body></div>
  `;

  card.querySelector('[data-close]')?.addEventListener('click', close);
  const body = card.querySelector('[data-vault-crypto-body]') as HTMLElement;

  const statsBanner = `
    <div style="display:flex;gap:12px;flex-wrap:wrap;padding:10px 12px;border-radius:8px;background:var(--bg-inset);border:1px solid var(--border-subtle);margin-bottom:12px;font-size:12px;">
      <span>📄 전체 노트: <strong>${st.total_notes}</strong></span>
      <span>🔒 암호화됨: <strong>${st.encrypted_notes}</strong></span>
      <span>🔓 세션 해제: <strong>${st.unlocked_notes}</strong></span>
      <span>상태: <strong>${st.vault_encrypted ? (st.vault_unlocked ? '볼트 해제됨' : '볼트 잠금됨') : '평문 볼트'}</strong></span>
    </div>
  `;

  if (!st.vault_encrypted) {
    body.innerHTML = `
      ${statsBanner}
      <div style="font-size:12.5px;color:var(--text-secondary);margin-bottom:12px;line-height:1.5;">
        마스터 암호를 설정해 볼트 내 모든 마크다운 노트를 일괄 암호화합니다.
        파일 확장자(.md)와 폴더 구조가 유지되므로 6종 클라우드·NAS 동기화가 안전하게 계속 동작합니다.
      </div>
      <div style="display:flex;flex-direction:column;gap:10px;">
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">볼트 마스터 암호 (4자 이상)</label>
          <input type="password" class="text-input" data-v-pw placeholder="마스터 암호 입력" />
        </div>
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">암호 확인</label>
          <input type="password" class="text-input" data-v-pw2 placeholder="마스터 암호 다시 입력" />
        </div>
        <div>
          <label style="display:block;font-size:12px;margin-bottom:4px;">암호 힌트 (선택)</label>
          <input type="text" class="text-input" data-v-hint placeholder="힌트 입력" />
        </div>
        <label style="display:flex;align-items:center;gap:7px;font-size:12.5px;cursor:pointer;">
          <input type="checkbox" data-v-auto checked /> 새로 만드는 노트·일일 노트도 자동 암호화
        </label>
        <div style="display:flex;justify-content:flex-end;gap:8px;margin-top:6px;">
          <button type="button" class="btn" data-cancel>취소</button>
          <button type="button" class="btn btn-primary" data-v-encrypt>🔐 볼트 전체 일괄 암호화</button>
        </div>
      </div>
    `;

    const pw1 = body.querySelector('[data-v-pw]') as HTMLInputElement;
    const pw2 = body.querySelector('[data-v-pw2]') as HTMLInputElement;
    const hintIn = body.querySelector('[data-v-hint]') as HTMLInputElement;
    const autoChk = body.querySelector('[data-v-auto]') as HTMLInputElement;

    body.querySelector('[data-cancel]')?.addEventListener('click', close);
    body.querySelector('[data-v-encrypt]')?.addEventListener('click', async () => {
      if (pw1.value.length < 4) {
        opts.onToast('암호는 최소 4자 이상 입력해 주세요.', true);
        return;
      }
      if (pw1.value !== pw2.value) {
        opts.onToast('두 암호가 일치하지 않습니다.', true);
        return;
      }
      try {
        const res = await api.cryptoEncryptVault(
          opts.vault,
          pw1.value,
          hintIn.value.trim() || null,
          autoChk.checked
        );
        close();
        await opts.onCompleted();
        opts.onToast(`전체 볼트 암호화 완료 (${res.result.encrypted_count}개 암호화됨)`);
      } catch (err: any) {
        opts.onToast(err?.message || '볼트 암호화 실패', true);
      }
    });
    setTimeout(() => pw1.focus(), 10);
    return;
  }

  body.innerHTML = `
    ${statsBanner}
    ${st.hint ? `<div class="locked-note-hint" style="margin-bottom:10px;">힌트: ${escapeHtml(st.hint)}</div>` : ''}
    <div style="display:flex;flex-direction:column;gap:12px;">
      <label style="display:flex;align-items:center;gap:7px;font-size:12.5px;cursor:pointer;">
        <input type="checkbox" data-v-auto ${st.auto_encrypt_new_notes ? 'checked' : ''} />
        새 노트 및 일일 노트 자동 암호화
      </label>

      ${
        st.vault_unlocked
          ? `<button type="button" class="btn" data-v-lock>🔒 볼트 전체 즉시 잠금 (세션 키 삭제)</button>`
          : ''
      }

      <div style="padding-top:10px;border-top:1px solid var(--border-subtle);">
        <label style="display:block;font-size:12px;margin-bottom:4px;">볼트 마스터 암호</label>
        <input type="password" class="text-input" data-v-pw placeholder="볼트 마스터 암호 입력" />
      </div>

      <div style="display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap;">
        <button type="button" class="btn" data-cancel>닫기</button>
        ${
          !st.vault_unlocked
            ? `<button type="button" class="btn btn-primary" data-v-unlock>🔓 볼트 잠금 해제</button>`
            : `<button type="button" class="btn" data-v-reencrypt>🔐 미암호화 노트까지 일괄 암호화</button>`
        }
        <button type="button" class="btn btn-danger" data-v-decrypt>⚠️ 전체 암호화 영구 해제</button>
      </div>
    </div>
  `;

  const pwIn = body.querySelector('[data-v-pw]') as HTMLInputElement;
  const autoChk = body.querySelector('[data-v-auto]') as HTMLInputElement;

  autoChk.addEventListener('change', async () => {
    try {
      await api.cryptoSetAutoEncrypt(opts.vault, autoChk.checked);
      await opts.onCompleted();
      opts.onToast(
        autoChk.checked
          ? '새 노트 자동 암호화를 켰습니다.'
          : '새 노트 자동 암호화를 껐습니다.'
      );
    } catch (err: any) {
      opts.onToast(err?.message || '설정 변경 실패', true);
    }
  });

  body.querySelector('[data-cancel]')?.addEventListener('click', close);
  body.querySelector('[data-v-lock]')?.addEventListener('click', async () => {
    try {
      await api.cryptoLockVault(opts.vault);
      close();
      await opts.onCompleted();
      opts.onToast('볼트 전체를 잠갔습니다.');
    } catch (err: any) {
      opts.onToast(err?.message || '볼트 잠금 실패', true);
    }
  });

  body.querySelector('[data-v-unlock]')?.addEventListener('click', async () => {
    if (!pwIn.value) {
      opts.onToast('암호를 입력해 주세요.', true);
      return;
    }
    try {
      const res = await api.cryptoUnlockVault(opts.vault, pwIn.value);
      close();
      await opts.onCompleted();
      opts.onToast(`볼트 잠금 해제 완료 (${res.result.unlocked_count}개 노트 해제)`);
    } catch (err: any) {
      opts.onToast(err?.message || '마스터 암호가 올바르지 않습니다.', true);
    }
  });

  body.querySelector('[data-v-reencrypt]')?.addEventListener('click', async () => {
    if (!pwIn.value) {
      opts.onToast('마스터 암호를 입력해 주세요.', true);
      return;
    }
    try {
      const res = await api.cryptoEncryptVault(
        opts.vault,
        pwIn.value,
        st.hint,
        autoChk.checked
      );
      close();
      await opts.onCompleted();
      opts.onToast(`볼트 일괄 암호화 완료 (${res.result.encrypted_count}개 암호화됨)`);
    } catch (err: any) {
      opts.onToast(err?.message || '일괄 암호화 실패', true);
    }
  });

  body.querySelector('[data-v-decrypt]')?.addEventListener('click', async () => {
    if (!pwIn.value) {
      opts.onToast('영구 복호화하려면 마스터 암호를 입력해 주세요.', true);
      return;
    }
    try {
      const res = await api.cryptoDecryptVault(opts.vault, pwIn.value);
      close();
      await opts.onCompleted();
      opts.onToast(`전체 볼트 암호화를 영구 해제했습니다 (${res.result.decrypted_count}개 복호화).`);
    } catch (err: any) {
      opts.onToast(err?.message || '전체 복호화에 실패했습니다.', true);
    }
  });

  setTimeout(() => pwIn.focus(), 10);
}

