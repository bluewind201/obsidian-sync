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
