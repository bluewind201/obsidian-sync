import '@toast-ui/editor/dist/toastui-editor.css';
import '@toast-ui/editor/dist/theme/toastui-editor-dark.css';
import 'prismjs/themes/prism.css';
import '@toast-ui/editor-plugin-code-syntax-highlight/dist/toastui-editor-plugin-code-syntax-highlight.css';
import 'tui-color-picker/dist/tui-color-picker.css';
import '@toast-ui/editor-plugin-color-syntax/dist/toastui-editor-plugin-color-syntax.css';
import '@toast-ui/editor-plugin-table-merged-cell/dist/toastui-editor-plugin-table-merged-cell.css';

import Editor from '@toast-ui/editor';
import type { EditorType, PluginContext, PluginInfo } from '@toast-ui/editor';
import '@toast-ui/editor/dist/i18n/ko-kr';
import colorSyntax from '@toast-ui/editor-plugin-color-syntax';
import codeSyntaxHighlight from '@toast-ui/editor-plugin-code-syntax-highlight';
import tableMergedCell from '@toast-ui/editor-plugin-table-merged-cell';

import Prism from 'prismjs';
import 'prismjs/components/prism-clike';
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-rust';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-sql';
import 'prismjs/components/prism-css';
import 'prismjs/components/prism-markdown';

import {
  api,
  blobToBase64,
  dehydrateMarkdownMediaUrls,
  hydrateMarkdownMediaUrls,
} from './api';

export interface CalloutPreset {
  type: string;
  icon: string;
  label: string;
  defaultTitle: string;
}

export const CALLOUT_PRESETS: CalloutPreset[] = [
  { type: 'note', icon: '📝', label: '[!note] 메모', defaultTitle: '메모' },
  { type: 'tip', icon: '💡', label: '[!tip] 팁', defaultTitle: '팁' },
  { type: 'important', icon: '🔥', label: '[!important] 중요', defaultTitle: '중요' },
  { type: 'warning', icon: '⚠️', label: '[!warning] 경고', defaultTitle: '주의' },
  { type: 'question', icon: '❓', label: '[!question] 질문', defaultTitle: '질문' },
  { type: 'success', icon: '✅', label: '[!success] 완료', defaultTitle: '완료' },
  { type: 'example', icon: '📌', label: '[!example] 예시', defaultTitle: '예시' },
  { type: 'quote', icon: '💬', label: '[!quote] 인용', defaultTitle: '인용' },
];

const NOTION_COLOR_PRESET = [
  '#37352F',
  '#787774',
  '#9F6B53',
  '#D9730D',
  '#CB912F',
  '#448361',
  '#337EA9',
  '#9065B0',
  '#C14C8A',
  '#D44C47',
  '#E03E3E',
  '#2383E2',
  '#0F7B6C',
  '#6940A5',
  '#AD1A72',
  '#64748B',
];

const HIGHLIGHT_SWATCHES = [
  { label: '노란 형광펜', color: 'rgba(250, 204, 21, 0.45)', chipBg: '#FEF08A' },
  { label: '초록 형광펜', color: 'rgba(74, 222, 128, 0.35)', chipBg: '#BBF7D0' },
  { label: '파란 형광펜', color: 'rgba(96, 165, 250, 0.35)', chipBg: '#BFDBFE' },
  { label: '분홍 형광펜', color: 'rgba(244, 114, 182, 0.32)', chipBg: '#FBCFE8' },
  { label: '보라 형광펜', color: 'rgba(192, 132, 252, 0.35)', chipBg: '#E9D5FF' },
  { label: '주황 형광펜', color: 'rgba(251, 146, 60, 0.35)', chipBg: '#FED7AA' },
];

const CALLOUT_ICONS: Record<string, string> = {
  note: '📝',
  tip: '💡',
  important: '🔥',
  warning: '⚠️',
  caution: '⚠️',
  question: '❓',
  success: '✅',
  check: '✅',
  example: '📌',
  quote: '💬',
  info: 'ℹ️',
};

/**
 * Enhances rendered Preview / Split-View HTML with:
 * - `==텍스트==` -> `<mark>텍스트</mark>`
 * - `[[Target|Alias]]` / `[[Target]]` -> clickable `.wikilink-inline`
 * - `> [!note] 제목` -> styled `.callout-badge`
 */
export function enhanceRenderedHtml(html: string): string {
  if (!html) return html;
  // Avoid transforming inside <code> or <pre> blocks by splitting on code/pre tags
  const parts = html.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);
  for (let i = 0; i < parts.length; i += 2) {
    let seg = parts[i];

    // Callout header `[!type] Title` at start of blockquote paragraph
    seg = seg.replace(
      /(<blockquote[^>]*>\s*<p[^>]*>)\s*\[!([a-zA-Z]+)\]\s*([^\n<]*)/gi,
      (_m, prefix: string, rawType: string, restTitle: string) => {
        const key = rawType.toLowerCase();
        const icon = CALLOUT_ICONS[key] || '💡';
        const label = restTitle.trim() || rawType.toUpperCase();
        return `${prefix}<span class="callout-badge">${icon} ${escapeHtml(label)}</span><br/>`;
      }
    );

    // Highlight syntax ==text==
    seg = seg.replace(/==([^=\n]+)==/g, '<mark>$1</mark>');

    // WikiLinks [[Note|Alias]] or [[Note]] (not preceded by !)
    seg = seg.replace(/(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, rawTarget: string, rawAlias?: string) => {
      const target = rawTarget.trim();
      const display = (rawAlias || rawTarget).trim();
      return `<a href="#" class="wikilink-inline" data-wikilink="${escapeAttr(target)}">📄 ${escapeHtml(display)}</a>`;
    });

    parts[i] = seg;
  }
  return parts.join('');
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

/**
 * Custom TOAST UI Editor plugin registering:
 * - `underline` command (Markdown `<u>...</u>` & WYSIWYG span underline toggle)
 * - `highlightColor` command (Markdown `<mark>` / `==...==` & WYSIWYG background span)
 * - `clearInlineStyle` command
 * - `insertCallout` command (Obsidian/Notion `[!type]` blockquote)
 * - Inline HTML renderers for `<u>` and `<mark>` so both render cleanly in WYSIWYG & Preview
 */
function notionRichPlugin(context: PluginContext): PluginInfo {
  const { pmState } = context;

  const wrapSelectionMd = (
    state: any,
    dispatch: any,
    openTag: string,
    closeTag: string,
    placeholder: string
  ): boolean => {
    const { tr, selection, schema } = state;
    const slice = selection.content();
    const selectedText = slice.content.textBetween(0, slice.content.size, '\n');
    const inner = selectedText || placeholder;
    const replacement = `${openTag}${inner}${closeTag}`;
    const from = selection.from;
    tr.replaceSelectionWith(schema.text(replacement));
    const selStart = from + openTag.length;
    const selEnd = selStart + inner.length;
    tr.setSelection(pmState.TextSelection.create(tr.doc, selStart, selEnd));
    dispatch(tr);
    return true;
  };

  return {
    markdownCommands: {
      underline: (_payload, state, dispatch) => {
        return wrapSelectionMd(state, dispatch, '<u>', '</u>', '밑줄 텍스트');
      },
      markdownHighlight: (_payload, state, dispatch) => {
        return wrapSelectionMd(state, dispatch, '==', '==', '형광펜 텍스트');
      },
      highlightColor: (payload, state, dispatch) => {
        const bg = payload?.color as string | undefined;
        if (!bg) {
          return wrapSelectionMd(state, dispatch, '==', '==', '형광펜 텍스트');
        }
        return wrapSelectionMd(
          state,
          dispatch,
          `<mark style="background-color: ${bg}">`,
          '</mark>',
          '형광펜 텍스트'
        );
      },
      clearHighlight: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const slice = selection.content();
        const selectedText = slice.content.textBetween(0, slice.content.size, '\n');
        if (!selectedText) return false;
        const cleaned = selectedText
          .replace(/^==([\s\S]*)==$/, '$1')
          .replace(/<\/?(?:mark|u|span)[^>]*>/gi, '');
        tr.replaceSelectionWith(schema.text(cleaned));
        dispatch(tr);
        return true;
      },
      insertCallout: (payload, state, dispatch) => {
        const cType = (payload?.calloutType as string) || 'note';
        const cTitle = (payload?.calloutTitle as string) || '메모';
        const { tr, selection, schema } = state;
        const slice = selection.content();
        const selected = slice.content.textBetween(0, slice.content.size, '\n').trim();
        const contentLine = selected
          ? selected
              .split('\n')
              .map((l: string) => `> ${l}`)
              .join('\n')
          : '> 여기에 내용을 입력하세요.';
        const block = `\n> [!${cType}] ${cTitle}\n${contentLine}\n\n`;
        tr.replaceSelectionWith(schema.text(block));
        dispatch(tr);
        return true;
      },
    },
    wysiwygCommands: {
      underline: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const spanMarkType = schema.marks.span;
        if (!spanMarkType) return false;
        const { from, to, empty } = selection;
        const attrs = { htmlAttrs: { style: 'text-decoration: underline' } };
        if (empty) {
          const textNode = schema.text('밑줄 텍스트', [spanMarkType.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 6));
          dispatch(tr);
          return true;
        }
        // Toggle off if already has span underline
        let hasUnderline = false;
        state.doc.nodesBetween(from, to, (node: any) => {
          if (node.marks) {
            for (const m of node.marks) {
              if (
                m.type === spanMarkType &&
                String(m.attrs?.htmlAttrs?.style || '').includes('underline')
              ) {
                hasUnderline = true;
              }
            }
          }
        });
        if (hasUnderline) {
          tr.removeMark(from, to, spanMarkType);
        } else {
          tr.addMark(from, to, spanMarkType.create(attrs));
        }
        dispatch(tr);
        return true;
      },
      markdownHighlight: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const spanMarkType = schema.marks.span;
        if (!spanMarkType) return false;
        const { from, to, empty } = selection;
        const attrs = {
          htmlAttrs: {
            style: 'background-color: rgba(250, 204, 21, 0.45); border-radius: 3px; padding: 0 2px',
          },
        };
        if (empty) {
          const textNode = schema.text('형광펜', [spanMarkType.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 3));
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, spanMarkType.create(attrs));
        dispatch(tr);
        return true;
      },
      highlightColor: (payload, state, dispatch) => {
        const bg = (payload?.color as string) || 'rgba(250, 204, 21, 0.45)';
        const { tr, selection, schema } = state;
        const spanMarkType = schema.marks.span;
        if (!spanMarkType) return false;
        const { from, to, empty } = selection;
        const attrs = {
          htmlAttrs: {
            style: `background-color: ${bg}; border-radius: 3px; padding: 0 2px`,
          },
        };
        if (empty) {
          const textNode = schema.text('형광펜', [spanMarkType.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 3));
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, spanMarkType.create(attrs));
        dispatch(tr);
        return true;
      },
      clearHighlight: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const spanMarkType = schema.marks.span;
        if (!spanMarkType) return false;
        tr.removeMark(selection.from, selection.to, spanMarkType);
        dispatch(tr);
        return true;
      },
      insertCallout: (payload, state, dispatch) => {
        const cType = (payload?.calloutType as string) || 'note';
        const cTitle = (payload?.calloutTitle as string) || '메모';
        const { tr, schema } = state;
        const bqType = schema.nodes.blockQuote;
        const paraType = schema.nodes.paragraph;
        if (!bqType || !paraType) return false;
        const headerPara = paraType.create(null, schema.text(`[!${cType}] ${cTitle}`));
        const bodyPara = paraType.create(null, schema.text('여기에 내용을 입력하세요.'));
        const bqNode = bqType.create(null, [headerPara, bodyPara]);
        tr.replaceSelectionWith(bqNode);
        dispatch(tr);
        return true;
      },
    },
    toHTMLRenderers: {
      htmlInline: {
        u(node: any, { entering }: any) {
          return entering
            ? { type: 'openTag', tagName: 'u', attributes: node.attrs || {} }
            : { type: 'closeTag', tagName: 'u' };
        },
        mark(node: any, { entering }: any) {
          return entering
            ? { type: 'openTag', tagName: 'mark', attributes: node.attrs || {} }
            : { type: 'closeTag', tagName: 'mark' };
        },
      },
    },
  };
}

export interface StudioEditorCallbacks {
  getActiveVault: () => string | null;
  onMarkdownChange: (cleanMarkdown: string) => void;
  onRequestWikiLinkPicker: (fromBracketTrigger: boolean) => void;
  onOpenWikiLink: (targetTitle: string) => void;
  onSaveShortcut: () => void;
  onToast: (message: string, isError?: boolean) => void;
}

export class StudioEditorController {
  private editor: Editor | null = null;
  private hostEl: HTMLElement;
  private callbacks: StudioEditorCallbacks;
  private suppressChange = false;
  private bracketTriggeredFromTyping = false;

  constructor(hostEl: HTMLElement, callbacks: StudioEditorCallbacks) {
    this.hostEl = hostEl;
    this.callbacks = callbacks;
  }

  public mount(params: {
    initialMarkdown: string;
    mode: EditorType;
    theme: 'light' | 'dark';
  }): void {
    this.destroy();
    this.hostEl.innerHTML = '';

    const vault = this.callbacks.getActiveVault();
    const hydrated = hydrateMarkdownMediaUrls(params.initialMarkdown, vault);

    const underlineBtn = document.createElement('button');
    underlineBtn.type = 'button';
    underlineBtn.className = 'tui-custom-btn underline-btn';
    underlineBtn.textContent = 'U';
    underlineBtn.addEventListener('click', () => {
      this.editor?.exec('underline');
      this.editor?.focus();
    });

    // Highlight popup menu (==형광펜== + 6 Notion background swatches + clear)
    const highlightPopupBody = document.createElement('div');
    highlightPopupBody.innerHTML = `
      <div style="margin-bottom:8px;font-weight:700;font-size:12px;">배경 형광펜 스타일</div>
      <div class="highlight-swatch-grid"></div>
      <div style="margin-top:8px;display:flex;gap:6px;">
        <button type="button" data-action="md-mark" class="btn btn-sm" style="flex:1;">==형광펜==</button>
        <button type="button" data-action="clear-mark" class="btn btn-sm">지우기</button>
      </div>
    `;
    const swatchGrid = highlightPopupBody.querySelector('.highlight-swatch-grid')!;
    for (const sw of HIGHLIGHT_SWATCHES) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'highlight-swatch-btn';
      b.style.background = sw.chipBg;
      b.style.color = '#1f2937';
      b.textContent = sw.label.replace(' 형광펜', '');
      b.title = sw.label;
      b.addEventListener('click', () => {
        this.editor?.exec('highlightColor', { color: sw.color });
        this.closeToolbarPopup();
        this.editor?.focus();
      });
      swatchGrid.appendChild(b);
    }
    highlightPopupBody
      .querySelector('[data-action="md-mark"]')
      ?.addEventListener('click', () => {
        this.editor?.exec('markdownHighlight');
        this.closeToolbarPopup();
        this.editor?.focus();
      });
    highlightPopupBody
      .querySelector('[data-action="clear-mark"]')
      ?.addEventListener('click', () => {
        this.editor?.exec('clearHighlight');
        this.closeToolbarPopup();
        this.editor?.focus();
      });

    const highlightBtn = document.createElement('button');
    highlightBtn.type = 'button';
    highlightBtn.className = 'tui-custom-btn highlight-btn';
    highlightBtn.textContent = '형광펜';

    // Callout dropdown popup
    const calloutPopupBody = document.createElement('div');
    calloutPopupBody.className = 'tui-custom-popup-menu';
    for (const preset of CALLOUT_PRESETS) {
      const item = document.createElement('button');
      item.type = 'button';
      item.className = 'tui-custom-popup-item';
      item.innerHTML = `<span>${preset.icon}</span><span>${escapeHtml(preset.label)}</span>`;
      item.addEventListener('click', () => {
        this.editor?.exec('insertCallout', {
          calloutType: preset.type,
          calloutTitle: preset.defaultTitle,
        });
        this.closeToolbarPopup();
        this.editor?.focus();
      });
      calloutPopupBody.appendChild(item);
    }

    const calloutBtn = document.createElement('button');
    calloutBtn.type = 'button';
    calloutBtn.className = 'tui-custom-btn';
    calloutBtn.textContent = '💡 콜아웃';

    // [[노트 연결]] button
    const wikiLinkBtn = document.createElement('button');
    wikiLinkBtn.type = 'button';
    wikiLinkBtn.className = 'tui-custom-btn';
    wikiLinkBtn.textContent = '[[노트 연결]]';
    wikiLinkBtn.addEventListener('click', () => {
      this.bracketTriggeredFromTyping = false;
      this.callbacks.onRequestWikiLinkPicker(false);
    });

    // 🖼️ PC 이미지 native picker button
    const nativeImgBtn = document.createElement('button');
    nativeImgBtn.type = 'button';
    nativeImgBtn.className = 'tui-custom-btn';
    nativeImgBtn.textContent = '🖼️ PC 이미지';
    nativeImgBtn.addEventListener('click', async () => {
      const activeVault = this.callbacks.getActiveVault();
      if (!activeVault) {
        this.callbacks.onToast('먼저 볼트를 열어 주세요.', true);
        return;
      }
      try {
        const picked = await api.pickAttachment(activeVault);
        if (picked.picked && picked.media_url) {
          const alt = picked.filename || '이미지';
          this.insertImageMarkdown(picked.media_url, alt);
          this.callbacks.onToast(`이미지를 첨부했습니다 (${picked.rel_path || alt})`);
        }
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '이미지를 불러오지 못했습니다.', true);
      }
    });

    this.editor = new Editor({
      el: this.hostEl,
      height: 'auto',
      minHeight: '460px',
      initialEditType: params.mode,
      previewStyle: 'vertical',
      initialValue: hydrated,
      language: 'ko-KR',
      hideModeSwitch: true,
      usageStatistics: false,
      theme: params.theme === 'dark' ? 'dark' : 'default',
      placeholder: '내용을 입력하세요…  ([[ 로 다른 노트 연결, 이미지 붙여넣기/드래그 앤 드롭 지원)',
      plugins: [
        [colorSyntax, { preset: NOTION_COLOR_PRESET }] as any,
        [codeSyntaxHighlight, { highlighter: Prism }] as any,
        tableMergedCell as any,
        notionRichPlugin as any,
      ],
      toolbarItems: [
        [
          'heading',
          'bold',
          'italic',
          {
            name: 'underline',
            tooltip: '밑줄 (Ctrl+U)',
            el: underlineBtn,
          },
          'strike',
          {
            name: 'notionHighlight',
            tooltip: '배경 형광펜 팔레트 (==형광펜==)',
            el: highlightBtn,
            popup: {
              body: highlightPopupBody,
              style: { width: 'auto' },
            },
          },
        ],
        ['ul', 'ol', 'task', 'indent', 'outdent'],
        [
          'table',
          {
            name: 'notionCallout',
            tooltip: '콜아웃 박스 삽입 (메모, 팁, 중요, 경고 등)',
            el: calloutBtn,
            popup: {
              body: calloutPopupBody,
              style: { width: '230px' },
            },
          },
          'quote',
          'hr',
        ],
        ['code', 'codeblock'],
        [
          'link',
          {
            name: 'wikiLinkInsert',
            tooltip: '볼트 내 다른 노트 연결 ([[위키링크]])',
            el: wikiLinkBtn,
          },
          'image',
          {
            name: 'pickNativeAttachment',
            tooltip: 'PC에서 이미지 선택 (볼트 attachments 폴더에 자동 저장)',
            el: nativeImgBtn,
          },
        ],
      ],
      hooks: {
        addImageBlobHook: async (blob: Blob | File, callback: (url: string, text?: string) => void) => {
          const activeVault = this.callbacks.getActiveVault();
          if (!activeVault) {
            this.callbacks.onToast('열려 있는 볼트가 없어 이미지를 저장할 수 없습니다.', true);
            return;
          }
          try {
            const rawName =
              blob instanceof File && blob.name
                ? blob.name
                : `pasted-${Date.now()}.${(blob.type.split('/')[1] || 'png').replace(/[^a-z0-9]/gi, '')}`;
            const dataBase64 = await blobToBase64(blob);
            const res = await api.uploadAttachment(activeVault, rawName, dataBase64);
            callback(res.media_url, rawName);
            this.callbacks.onToast(`첨부파일 저장됨: ${res.rel_path}`);
          } catch (err: any) {
            this.callbacks.onToast(err?.message || '이미지 업로드 실패', true);
          }
        },
      },
      events: {
        change: () => {
          if (this.suppressChange || !this.editor) return;
          const cleanMd = this.getCleanMarkdown();
          this.callbacks.onMarkdownChange(cleanMd);
        },
        beforePreviewRender: (html: string) => {
          return enhanceRenderedHtml(html);
        },
        keydown: (_editorType: EditorType, ev: KeyboardEvent) => {
          if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 's') {
            ev.preventDefault();
            this.callbacks.onSaveShortcut();
            return;
          }
          if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 'u') {
            ev.preventDefault();
            this.editor?.exec('underline');
          }
        },
        keyup: (_editorType: EditorType, ev: KeyboardEvent) => {
          if (ev.key === '[' && this.didJustTypeDoubleBracket()) {
            this.bracketTriggeredFromTyping = true;
            this.callbacks.onRequestWikiLinkPicker(true);
          }
        },
      },
    });

    // Delegate clicks on `.wikilink-inline` inside Markdown preview AND Ctrl+Click on `[[Note]]` inside editor
    this.hostEl.addEventListener('click', this.handleHostClick);
  }

  private handleHostClick = (ev: MouseEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (!target) return;

    const wikiAnchor = target.closest('.wikilink-inline') as HTMLElement | null;
    if (wikiAnchor) {
      ev.preventDefault();
      ev.stopPropagation();
      const noteTitle = wikiAnchor.getAttribute('data-wikilink');
      if (noteTitle) {
        this.callbacks.onOpenWikiLink(noteTitle);
      }
      return;
    }

    // Support Ctrl/Meta + Click directly on `[[Note Title]]` text inside WYSIWYG or Markdown editor
    if (ev.ctrlKey || ev.metaKey) {
      const text = target.textContent || '';
      const match = /\[\[([^\]|]+)(?:\|[^\]]+)?\]\]/.exec(text);
      if (match && match[1]) {
        ev.preventDefault();
        this.callbacks.onOpenWikiLink(match[1].trim());
      }
    }
  };

  private didJustTypeDoubleBracket(): boolean {
    const sel = window.getSelection();
    if (!sel || !sel.anchorNode) return false;
    const text = sel.anchorNode.textContent || '';
    const offset = sel.anchorOffset;
    if (offset >= 2 && text.slice(offset - 2, offset) === '[[') {
      return true;
    }
    return false;
  }

  private closeToolbarPopup(): void {
    if (!this.editor) return;
    try {
      (this.editor as any).eventEmitter?.emit('closePopup');
    } catch {
      // ignore
    }
  }

  public insertImageMarkdown(mediaUrl: string, alt: string): void {
    if (!this.editor) return;
    if (this.editor.isWysiwygMode()) {
      this.editor.exec('addImage', { imageUrl: mediaUrl, altText: alt });
    } else {
      this.editor.insertText(`![${alt}](${mediaUrl})`);
    }
    this.editor.focus();
  }

  public insertWikiLink(noteTitle: string): void {
    if (!this.editor) return;
    const token = `[[${noteTitle}]]`;

    if (this.bracketTriggeredFromTyping) {
      this.bracketTriggeredFromTyping = false;
      // Remove the trailing `[[` before inserting `[[noteTitle]]` if possible
      const activeEditor = this.getActiveProseMirrorView();
      if (activeEditor) {
        const { state, dispatch } = activeEditor;
        const pos = state.selection.from;
        if (pos >= 2) {
          const prevTwo = state.doc.textBetween(pos - 2, pos, '');
          if (prevTwo === '[[') {
            const tr = state.tr.insertText(token, pos - 2, pos);
            dispatch(tr);
            this.editor.focus();
            return;
          }
        }
      }
    }

    this.editor.insertText(token);
    this.editor.focus();
  }

  private getActiveProseMirrorView(): any | null {
    if (!this.editor) return null;
    try {
      const anyEd = this.editor as any;
      return this.editor.isWysiwygMode() ? anyEd.wwEditor?.view : anyEd.mdEditor?.view;
    } catch {
      return null;
    }
  }

  public getCleanMarkdown(): string {
    if (!this.editor) return '';
    const raw = this.editor.getMarkdown();
    return dehydrateMarkdownMediaUrls(raw);
  }

  public setMarkdownContent(markdown: string): void {
    if (!this.editor) return;
    this.suppressChange = true;
    try {
      const hydrated = hydrateMarkdownMediaUrls(markdown, this.callbacks.getActiveVault());
      this.editor.setMarkdown(hydrated, false);
    } finally {
      this.suppressChange = false;
    }
  }

  public setMode(mode: EditorType): void {
    if (!this.editor) return;
    const current: EditorType = this.editor.isWysiwygMode() ? 'wysiwyg' : 'markdown';
    if (current !== mode) {
      this.suppressChange = true;
      try {
        this.editor.changeMode(mode, true);
      } finally {
        this.suppressChange = false;
      }
    }
  }

  public setTheme(theme: 'light' | 'dark'): void {
    const uiEl = this.hostEl.querySelector('.toastui-editor-defaultUI');
    if (!uiEl) return;
    if (theme === 'dark') {
      uiEl.classList.add('toastui-editor-dark');
    } else {
      uiEl.classList.remove('toastui-editor-dark');
    }
  }

  public scrollToHeading(headingText: string): void {
    const container = this.editor?.isWysiwygMode()
      ? this.hostEl.querySelector('.toastui-editor-ww-container')
      : this.hostEl.querySelector('.toastui-editor-md-preview');
    if (!container) return;
    const headings = container.querySelectorAll('h1, h2, h3, h4, h5, h6');
    for (const h of Array.from(headings)) {
      if ((h.textContent || '').trim() === headingText.trim()) {
        h.scrollIntoView({ behavior: 'smooth', block: 'center' });
        return;
      }
    }
  }

  public destroy(): void {
    this.hostEl.removeEventListener('click', this.handleHostClick);
    if (this.editor) {
      try {
        this.editor.destroy();
      } catch {
        // ignore
      }
      this.editor = null;
    }
  }
}

/**
 * Mounts a read-only TOAST UI Viewer for the right Split View pane (`◫ 우측 분할 보기`)
 */
export function renderSplitViewer(params: {
  container: HTMLElement;
  markdown: string;
  vault: string | null;
  theme: 'light' | 'dark';
  onOpenWikiLink: (title: string) => void;
}): { destroy: () => void } {
  params.container.innerHTML = '';
  const hydrated = hydrateMarkdownMediaUrls(params.markdown, params.vault);

  const viewer = Editor.factory({
    el: params.container,
    viewer: true,
    initialValue: hydrated,
    usageStatistics: false,
    theme: params.theme === 'dark' ? 'dark' : 'default',
    plugins: [
      [codeSyntaxHighlight, { highlighter: Prism }] as any,
      tableMergedCell as any,
      notionRichPlugin as any,
    ],
  });

  // Apply WikiLink & Callout enhancements to rendered viewer contents
  const contentsEl = params.container.querySelector('.toastui-editor-contents');
  if (contentsEl) {
    contentsEl.innerHTML = enhanceRenderedHtml(contentsEl.innerHTML);
  }

  const onClick = (ev: MouseEvent) => {
    const target = ev.target as HTMLElement | null;
    const link = target?.closest('.wikilink-inline') as HTMLElement | null;
    if (link) {
      ev.preventDefault();
      const note = link.getAttribute('data-wikilink');
      if (note) params.onOpenWikiLink(note);
    }
  };
  params.container.addEventListener('click', onClick);

  return {
    destroy: () => {
      params.container.removeEventListener('click', onClick);
      try {
        viewer.destroy();
      } catch {
        // ignore
      }
    },
  };
}
