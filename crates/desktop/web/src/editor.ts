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
import { redo, undo } from 'prosemirror-history';

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
  copyDiagramAsPngOrSvg,
  cycleFlowchartTheme,
  decodeFlowCode,
  FLOWCHART_TEMPLATES,
  isDiagramCodeBlock,
  looksLikeBareFlowchartText,
  openFlowchartStudioModal,
  renderDiagramCardHtml,
  toggleFlowchartCurve,
  toggleFlowchartDirection,
} from './flowchart';

// Ensure @toast-ui/editor-plugin-code-syntax-highlight escapes & tokenizes diagram fences safely
if (!(Prism.languages as any).mermaid) {
  (Prism.languages as any).mermaid = {
    comment: /%%[^\n]*/,
    keyword:
      /\b(?:flowchart|graph|subgraph|end|direction|sequenceDiagram|stateDiagram(?:-v2)?|mindmap|gantt|classDef|class|style|linkStyle)\b/i,
    operator: /-->|--->|---|-\.->|-\.-|==>|===|<-->|<-.->|<==>|o--o|x--x/,
    string: /"(?:\\.|[^"\\])*"/,
  };
}
(Prism.languages as any).flowchart = (Prism.languages as any).mermaid;
(Prism.languages as any).flow = (Prism.languages as any).mermaid;
(Prism.languages as any).diagram = (Prism.languages as any).mermaid;

import {
  api,
  blobToBase64,
  dehydrateMarkdownMediaUrls,
  hydrateMarkdownMediaUrls,
} from './api';
import {
  classifyClipboardData,
  CleanPasteSegment,
  dataUrlToImageBlob,
  extractPlainTextAndImagesFromHtml,
  normalizePastedPlainText,
  ONYX_INTERNAL_CLIPBOARD_MIME,
} from './pasteClean';

let toastPmViewMod: any = null;
let toastPmModelMod: any = null;

export interface CalloutPreset {
  type: string;
  icon: string;
  label: string;
  defaultTitle: string;
}

export const CALLOUT_PRESETS: CalloutPreset[] = [
  { type: 'note', icon: '📝', label: '[!note] 메모', defaultTitle: '메모' },
  { type: 'info', icon: 'ℹ️', label: '[!info] 안내', defaultTitle: '안내' },
  { type: 'tip', icon: '💡', label: '[!tip] 팁', defaultTitle: '팁' },
  { type: 'important', icon: '🔥', label: '[!important] 중요', defaultTitle: '중요' },
  { type: 'warning', icon: '⚠️', label: '[!warning] 경고', defaultTitle: '주의' },
  { type: 'danger', icon: '🚨', label: '[!danger] 위험', defaultTitle: '위험' },
  { type: 'success', icon: '✅', label: '[!success] 완료', defaultTitle: '완료' },
  { type: 'question', icon: '❓', label: '[!question] 질문', defaultTitle: '질문' },
  { type: 'example', icon: '📌', label: '[!example] 예시', defaultTitle: '예시' },
  { type: 'quote', icon: '💬', label: '[!quote] 인용', defaultTitle: '인용' },
  { type: 'bug', icon: '🐞', label: '[!bug] 버그', defaultTitle: '버그' },
  { type: 'abstract', icon: '📋', label: '[!abstract] 요약', defaultTitle: '요약' },
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

const FONT_SIZE_PRESETS = [
  { label: '아주 작게 (8pt)', size: '8pt' },
  { label: '작게 (9pt)', size: '9pt' },
  { label: '보통 · 기본 (10pt)', size: '10pt' },
  { label: '조금 크게 (11pt)', size: '11pt' },
  { label: '중간 (12pt)', size: '12pt' },
  { label: '크게 (14pt)', size: '14pt' },
  { label: '제목급 (18pt)', size: '18pt' },
  { label: '대제목급 (24pt)', size: '24pt' },
];

const STATUS_BADGES = [
  { label: '⬜ 진행 대기', cls: 'todo', text: '진행 대기' },
  { label: '🔵 진행 중', cls: 'doing', text: '진행 중' },
  { label: '✅ 완료', cls: 'done', text: '완료' },
  { label: '🟡 보류', cls: 'hold', text: '보류' },
  { label: '🚨 긴급', cls: 'urgent', text: '긴급' },
];

const KOREAN_OFFICE_SYMBOLS = [
  '※',
  '★',
  '☆',
  '✓',
  '✗',
  '■',
  '□',
  '▶',
  '▷',
  '●',
  '○',
  '◆',
  '◇',
  '①',
  '②',
  '③',
  '④',
  '⑤',
  '⑥',
  '⑦',
  '⑧',
  '⑨',
  '⑩',
  '㉠',
  '㉡',
  '㉢',
  '㉣',
  '㉤',
  '「」',
  '『』',
  '【】',
  '→',
  '←',
  '↔',
  '⇒',
  '℃',
  '㎡',
  '₩',
];

const MATH_SYMBOLS = [
  '±',
  '×',
  '÷',
  '≠',
  '≈',
  '≤',
  '≥',
  '∞',
  '∑',
  '∏',
  '√',
  '∫',
  '∂',
  '∆',
  'π',
  'θ',
  'λ',
  'μ',
  'σ',
  'Ω',
  '∈',
  '∉',
  '⊂',
  '∪',
  '∩',
  '∀',
  '∃',
  '∴',
];

const CALLOUT_ICONS: Record<string, string> = {
  note: '📝',
  info: 'ℹ️',
  tip: '💡',
  important: '🔥',
  warning: '⚠️',
  caution: '⚠️',
  danger: '🚨',
  error: '🚨',
  question: '❓',
  success: '✅',
  check: '✅',
  example: '📌',
  quote: '💬',
  bug: '🐞',
  abstract: '📋',
  summary: '📋',
};

const ALLOWED_HTML_TAGS = new Set([
  'a',
  'b',
  'blockquote',
  'br',
  'code',
  'col',
  'colgroup',
  'dd',
  'del',
  'details',
  'div',
  'dl',
  'dt',
  'em',
  'figcaption',
  'figure',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'hr',
  'i',
  'img',
  'input',
  'ins',
  'kbd',
  'li',
  'mark',
  'ol',
  'p',
  'pre',
  's',
  'section',
  'small',
  'span',
  'strong',
  'sub',
  'summary',
  'sup',
  'table',
  'tbody',
  'td',
  'th',
  'thead',
  'tr',
  'u',
  'ul',
]);

const FORBIDDEN_HTML_TAGS = new Set([
  'script',
  'iframe',
  'object',
  'embed',
  'form',
  'textarea',
  'select',
  'button',
  'style',
  'link',
  'meta',
  'base',
]);

/**
 * Custom sanitizer preserving safe document structure elements (<details>, <summary>,
 * <div class="page-break">, <div class="doc-cols">, align styles, <u>, <mark>, <sup>, <sub>, <kbd>)
 * while stripping dangerous scripts and inline event handlers.
 */
export function safeHtmlSanitizer(html: string): string {
  if (!html) return '';
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, 'text/html');

  const cleanNode = (node: Node): void => {
    const children = Array.from(node.childNodes);
    for (const child of children) {
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const el = child as HTMLElement;
      const tag = el.tagName.toLowerCase();

      if (FORBIDDEN_HTML_TAGS.has(tag)) {
        el.remove();
        continue;
      }

      if (!ALLOWED_HTML_TAGS.has(tag)) {
        // Unwrap disallowed non-dangerous wrapper tags while cleaning descendants
        cleanNode(el);
        const frag = doc.createDocumentFragment();
        while (el.firstChild) {
          frag.appendChild(el.firstChild);
        }
        el.replaceWith(frag);
        continue;
      }

      // Clean attributes
      for (const attr of Array.from(el.attributes)) {
        const name = attr.name.toLowerCase();
        const val = attr.value;
        if (name.startsWith('on')) {
          el.removeAttribute(attr.name);
          continue;
        }
        if ((name === 'href' || name === 'src') && /^\s*(?:javascript|vbscript):/i.test(val)) {
          el.removeAttribute(attr.name);
          continue;
        }
        if (name === 'style' && /(?:expression\s*\(|javascript:|vbscript:|url\s*\()/i.test(val)) {
          el.removeAttribute(attr.name);
          continue;
        }
        if (tag === 'input' && name === 'type' && val.toLowerCase() !== 'checkbox') {
          el.remove();
          break;
        }
      }

      if (el.isConnected) {
        cleanNode(el);
      }
    }
  };

  cleanNode(doc.body);
  return doc.body.innerHTML;
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

function normalizeCalloutClass(rawType: string): string {
  const k = rawType.toLowerCase();
  if (k === 'caution') return 'warning';
  if (k === 'check') return 'success';
  if (k === 'summary') return 'abstract';
  if (k === 'error') return 'danger';
  return CALLOUT_ICONS[k] ? k : 'note';
}

function decodeHtmlEntitiesToText(rawHtml: string): string {
  const withNl = rawHtml.replace(/<br\s*\/?>/gi, '\n').replace(/<\/p>\s*<p[^>]*>/gi, '\n');
  const noTags = withNl.replace(/<[^>]+>/g, '');
  return noTags
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&#x27;/g, "'")
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&');
}

function hashShort(str: string): string {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(36);
}

function getOnyxDiagramPluginKey(view: any): any | null {
  const plugins = view?.state?.plugins;
  if (!Array.isArray(plugins)) return null;
  for (const p of plugins) {
    if ((p as any).__onyxDiagramKey) return (p as any).__onyxDiagramKey;
  }
  return null;
}

interface MarkdownDiagramFence {
  lang: string;
  body: string;
  bodyStart: number;
  bodyEnd: number;
}

function findDiagramFencesInMarkdown(md: string): MarkdownDiagramFence[] {
  const out: MarkdownDiagramFence[] = [];
  const norm = md.replace(/\r\n?/g, '\n');
  const lines = norm.split('\n');
  let offset = 0;
  let inFence = false;
  let fenceChar = '`';
  let fenceLen = 3;
  let lang = '';
  let bodyStart = 0;
  let bodyLines: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const nextOffset = offset + line.length + 1;
    const trimmed = line.trim();
    if (!inFence) {
      const openM = /^(`{3,}|~{3,})\s*([a-zA-Z0-9_-]*)/.exec(trimmed);
      if (openM) {
        inFence = true;
        fenceChar = openM[1][0];
        fenceLen = openM[1].length;
        lang = openM[2] || '';
        bodyStart = nextOffset;
        bodyLines = [];
      }
    } else {
      const closeRe = new RegExp(`^\\${fenceChar}{${fenceLen},}\\s*$`);
      if (closeRe.test(trimmed)) {
        const body = bodyLines.join('\n');
        if (isDiagramCodeBlock(lang, body)) {
          out.push({
            lang,
            body,
            bodyStart,
            bodyEnd: Math.max(bodyStart, offset - 1),
          });
        }
        inFence = false;
      } else {
        bodyLines.push(line);
      }
    }
    offset = nextOffset;
  }
  return out;
}

/**
 * Enhances rendered Preview / Split-View / Print HTML with:
 * - Mermaid / Flowchart (`flowchart TB`, `LR`, `subgraph`, `sequenceDiagram`, `mindmap`, `gantt`) vector SVG cards
 * - Bare pasted `flowchart TB` paragraphs auto-rendered as Flowchart cards
 * - `==텍스트==` -> `<mark>텍스트</mark>`
 * - `[[Target|Alias]]` / `[[Target]]` -> clickable `.wikilink-inline`
 * - `> [!note] 제목` -> styled `blockquote.callout.callout-note` with `.callout-title`
 * - `$$math$$` -> `.math-block`
 * - `[^ref]` -> styled footnote marker
 */
export function enhanceRenderedHtml(html: string, opts?: { forPrint?: boolean }): string {
  if (!html) return html;
  const slots: string[] = [];
  let diagramIdx = 0;

  // Step 1: Extract diagram <pre><code> blocks into opaque slots (preserving data-nodeid for TOAST UI preview diffing)
  let work = html.replace(
    /<pre([^>]*)>\s*<code([^>]*)>([\s\S]*?)<\/code>\s*<\/pre>/gi,
    (full, preAttrs: string, codeAttrs: string, innerHtml: string) => {
      const langFromCode = /data-language=["']([^"']+)["']/i.exec(codeAttrs)?.[1];
      const langFromPre = /class=["'][^"']*\blang-([a-zA-Z0-9_-]+)/i.exec(preAttrs)?.[1];
      const lang = langFromCode || langFromPre || '';
      const decoded = decodeHtmlEntitiesToText(innerHtml);
      if (!isDiagramCodeBlock(lang, decoded)) {
        return full;
      }
      const nodeId =
        /data-nodeid=["']([^"']+)["']/i.exec(preAttrs)?.[1] ||
        /data-nodeid=["']([^"']+)["']/i.exec(codeAttrs)?.[1];
      const slotId = slots.length;
      slots.push(
        renderDiagramCardHtml(decoded, {
          forPrint: opts?.forPrint,
          editorActions: !opts?.forPrint,
          diagramIndex: diagramIdx++,
          nodeIdAttr: nodeId,
          collapsedCode: true,
        })
      );
      return `\n__ONYX_FLOW_SLOT_${slotId}__\n`;
    }
  );

  // Step 2: Detect bare <p>flowchart TB ...</p> blocks outside <pre>/<code>
  work = work.replace(/<p([^>]*)>([\s\S]*?)<\/p>/gi, (full, pAttrs: string, inner: string) => {
    const plain = decodeHtmlEntitiesToText(inner);
    if (!looksLikeBareFlowchartText(plain)) return full;
    const nodeId = /data-nodeid=["']([^"']+)["']/i.exec(pAttrs)?.[1];
    const slotId = slots.length;
    slots.push(
      renderDiagramCardHtml(plain, {
        forPrint: opts?.forPrint,
        editorActions: !opts?.forPrint,
        diagramIndex: diagramIdx++,
        nodeIdAttr: nodeId,
        collapsedCode: true,
      })
    );
    return `\n__ONYX_FLOW_SLOT_${slotId}__\n`;
  });

  // Step 3: Run inline Obsidian/Notion markdown enhancements outside remaining <pre>/<code>
  const parts = work.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);
  for (let i = 0; i < parts.length; i += 2) {
    let seg = parts[i];

    // Callout header `[!type] Title` at start of blockquote paragraph
    seg = seg.replace(
      /(<blockquote[^>]*>)\s*<p([^>]*)>\s*\[!([a-zA-Z]+)\]\s*([^\n<]*)(?:\s*<\/p>)?/gi,
      (full, _bq: string, pAttrs: string, rawType: string, restTitle: string) => {
        const key = rawType.toLowerCase();
        const cls = normalizeCalloutClass(key);
        const icon = CALLOUT_ICONS[key] || '💡';
        const label = restTitle.trim() || rawType.toUpperCase();
        const closedP = full.trim().endsWith('</p>') ? '' : `<p${pAttrs}>`;
        return `<blockquote class="callout callout-${cls}"><div class="callout-title">${icon} ${escapeHtml(label)}</div>${closedP}`;
      }
    );

    // Block math $$...$$
    seg = seg.replace(/\$\$\s*([\s\S]+?)\s*\$\$/g, (_m, expr: string) => {
      return `<div class="math-block">${expr}</div>`;
    });

    // Highlight syntax ==text==
    seg = seg.replace(/==([^=\n]+)==/g, '<mark>$1</mark>');

    // WikiLinks [[Note|Alias]] or [[Note]] (not preceded by !)
    seg = seg.replace(
      /(?<!!)\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g,
      (_m, rawTarget: string, rawAlias?: string) => {
        const target = rawTarget.trim();
        const display = (rawAlias || rawTarget).trim();
        return `<a href="#" class="wikilink-inline" data-wikilink="${escapeAttr(target)}">📄 ${escapeHtml(display)}</a>`;
      }
    );

    // Inline footnote refs [^1]
    seg = seg.replace(/\[\^([a-zA-Z0-9_-]+)\](?!:)/g, (_m, refId: string) => {
      return `<sup class="footnote-ref">[${escapeHtml(refId)}]</sup>`;
    });

    parts[i] = seg;
  }

  let joined = parts.join('');
  if (slots.length > 0) {
    joined = joined.replace(/__ONYX_FLOW_SLOT_(\d+)__/g, (_m, idxStr: string) => {
      return slots[Number(idxStr)] || '';
    });
  }
  return joined;
}

function makeInlineMarkToggle(tagName: string, placeholder: string) {
  return (_payload: any, state: any, dispatch: any): boolean => {
    const { tr, selection, schema } = state;
    const markType = schema.marks[tagName];
    if (!markType) return false;
    const { from, to, empty } = selection;
    if (empty) {
      const node = schema.text(placeholder, [markType.create()]);
      tr.replaceSelectionWith(node, false);
      dispatch(tr);
      return true;
    }
    const active = state.doc.rangeHasMark(from, to, markType);
    if (active) {
      tr.removeMark(from, to, markType);
    } else {
      tr.addMark(from, to, markType.create());
    }
    dispatch(tr);
    return true;
  };
}

function makeHtmlTagRenderer(tagName: string) {
  return (node: any, { entering }: any) => {
    return entering
      ? { type: 'openTag', tagName, attributes: node.attrs || {} }
      : { type: 'closeTag', tagName };
  };
}

/**
 * Custom TOAST UI Editor plugin registering 20+ Markdown & WYSIWYG commands +
 * safe HTML renderers for rich typography, callouts, toggles, columns, and page breaks.
 */
function notionRichPlugin(context: PluginContext): PluginInfo {
  const { pmState, pmView, pmModel } = context;
  if (pmView) toastPmViewMod = pmView;
  if (pmModel) toastPmModelMod = pmModel;

  const diagramPluginKey = new (pmState.PluginKey as any)('onyxDiagramLive');
  const buildDiagramWysiwygPlugin = () => {
    const plugin = new (pmState.Plugin as any)({
      key: diagramPluginKey,
      state: {
        init: () => ({ expanded: [] }),
        apply: (tr: any, prev: { expanded: number[] }) => {
          let next = prev.expanded;
          if (tr.docChanged && next.length > 0) {
            const mapped: number[] = [];
            for (const oldPos of next) {
              const res = tr.mapping.mapResult(oldPos);
              if (!res.deleted) mapped.push(res.pos);
            }
            next = mapped;
          }
          const meta = tr.getMeta(diagramPluginKey);
          if (meta && typeof meta.togglePos === 'number') {
            const p = meta.togglePos as number;
            next = next.includes(p) ? next.filter((x) => x !== p) : [...next, p];
          }
          if (meta && typeof meta.openPos === 'number') {
            const p = meta.openPos as number;
            if (!next.includes(p)) next = [...next, p];
          }
          if (meta && typeof meta.closePos === 'number') {
            const p = meta.closePos as number;
            next = next.filter((x) => x !== p);
          }
          return { expanded: next };
        },
      },
      props: {
        decorations(state: any) {
          const decos: any[] = [];
          const pState = diagramPluginKey.getState(state) || { expanded: [] };
          const selFrom = state.selection.from;
          const selTo = state.selection.to;
          let diagIndex = 0;

          // 1) Render live Flowchart/Diagram cards directly above diagram codeBlocks
          state.doc.descendants((node: any, pos: number) => {
            if (node.type?.name !== 'codeBlock') return;
            const lang = String(node.attrs?.language || '').trim().toLowerCase();
            const code = node.textContent || '';
            if (!isDiagramCodeBlock(lang, code)) return;

            const curIdx = diagIndex++;
            const cursorInside =
              selFrom > pos &&
              selFrom < pos + node.nodeSize &&
              selTo >= pos &&
              selTo <= pos + node.nodeSize;
            const isOpen = cursorInside || pState.expanded.includes(pos);

            decos.push(
              pmView.Decoration.node(pos, pos + node.nodeSize, {
                class: isOpen ? 'onyx-ww-diagram-source is-open' : 'onyx-ww-diagram-source is-collapsed',
              })
            );

            const wKey = `onyx-ww-flow-${pos}-${isOpen ? 'o' : 'c'}-${hashShort(code)}`;
            decos.push(
              pmView.Decoration.widget(
                pos,
                () => {
                  const wrap = document.createElement('div');
                  wrap.className = 'onyx-ww-diagram-widget';
                  wrap.setAttribute('contenteditable', 'false');
                  wrap.setAttribute('data-pm-pos', String(pos));
                  wrap.innerHTML = renderDiagramCardHtml(code, {
                    editorActions: true,
                    diagramIndex: curIdx,
                    collapsedCode: !isOpen,
                  });
                  return wrap;
                },
                {
                  side: -1,
                  key: wKey,
                  stopEvent: () => true,
                  ignoreMutation: () => true,
                } as any
              )
            );
          });

          // 2) Detect bare `flowchart TB ...` paragraph runs in WYSIWYG and show live card + 1-click convert chip
          let childOffset = 0;
          for (let i = 0; i < state.doc.childCount; i++) {
            const child = state.doc.child(i);
            const startPos = childOffset;
            const firstTxt = (child.textContent || '').trim();
            if (
              child.type?.name === 'paragraph' &&
              /^(?:flowchart|graph)\s+(?:TB|TD|BT|RL|LR)\b/i.test(firstTxt)
            ) {
              const collectedLines: string[] = [firstTxt];
              let endPos = startPos + child.nodeSize;
              let j = i + 1;
              while (j < state.doc.childCount && collectedLines.length < 45) {
                const nextNode = state.doc.child(j);
                const nextTxt = (nextNode.textContent || '').trim();
                if (nextNode.type?.name !== 'paragraph' || !nextTxt) break;
                collectedLines.push(nextTxt);
                endPos += nextNode.nodeSize;
                j++;
              }
              const joined = collectedLines.join('\n');
              if (looksLikeBareFlowchartText(joined)) {
                const bKey = `onyx-ww-bare-${startPos}-${endPos}-${hashShort(joined)}`;
                decos.push(
                  pmView.Decoration.widget(
                    startPos,
                    () => {
                      const box = document.createElement('div');
                      box.className = 'onyx-ww-bare-flow-wrap';
                      box.setAttribute('contenteditable', 'false');
                      box.innerHTML = `<div class="onyx-flow-convert-bar">
                        <span>🔀 플로우차트 텍스트 감지됨 — 블록으로 변환하면 방향/테마/빌더를 바로 편집할 수 있습니다.</span>
                        <button type="button" class="btn btn-sm btn-primary" data-flow-convert-start="${startPos}" data-flow-convert-end="${endPos}">✨ 플로우차트 블록으로 변환</button>
                      </div>${renderDiagramCardHtml(joined, { editorActions: false })}`;
                      return box;
                    },
                    {
                      side: -1,
                      key: bKey,
                      stopEvent: () => true,
                      ignoreMutation: () => true,
                    } as any
                  )
                );
                childOffset = endPos;
                i = j - 1;
                continue;
              }
            }
            childOffset += child.nodeSize;
          }

          return pmView.DecorationSet.create(state.doc, decos);
        },
        handlePaste(view: any, ev: ClipboardEvent) {
          const text = ev.clipboardData?.getData('text/plain') || '';
          if (!text.trim()) return false;
          const parentType = view.state.selection.$from.parent?.type?.name;
          if (parentType === 'codeBlock') return false;

          const trimmed = text.replace(/\r\n?/g, '\n').trim();
          let lang: string | null = null;
          let body = '';

          const fenced =
            /^```(mermaid|flowchart|flow|diagram|graph)\s*\n([\s\S]*?)\n```\s*$/i.exec(trimmed);
          if (fenced) {
            const rawLang = fenced[1].toLowerCase();
            lang = rawLang === 'diagram' ? 'mermaid' : rawLang;
            body = fenced[2];
          } else if (looksLikeBareFlowchartText(trimmed)) {
            lang = 'mermaid';
            body = trimmed;
          }

          if (!lang) return false;
          const codeBlockType = view.state.schema.nodes.codeBlock;
          if (!codeBlockType) return false;
          ev.preventDefault();
          const cbNode = codeBlockType.create(
            { language: lang },
            body ? view.state.schema.text(body) : undefined
          );
          view.dispatch(view.state.tr.replaceSelectionWith(cbNode).scrollIntoView());
          return true;
        },
      },
    });
    (plugin as any).__onyxDiagramKey = diagramPluginKey;
    return plugin;
  };

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
      undo: (_payload, state, dispatch, view) => undo(state, dispatch, view),
      redo: (_payload, state, dispatch, view) => redo(state, dispatch, view),
      underline: (_payload, state, dispatch) =>
        wrapSelectionMd(state, dispatch, '<u>', '</u>', '밑줄 텍스트'),
      superscript: (_payload, state, dispatch) =>
        wrapSelectionMd(state, dispatch, '<sup>', '</sup>', '2'),
      subscript: (_payload, state, dispatch) =>
        wrapSelectionMd(state, dispatch, '<sub>', '</sub>', '2'),
      kbdKey: (_payload, state, dispatch) =>
        wrapSelectionMd(state, dispatch, '<kbd>', '</kbd>', 'Ctrl+S'),
      fontSize: (payload, state, dispatch) => {
        const size = (payload?.size as string) || '18px';
        return wrapSelectionMd(
          state,
          dispatch,
          `<span style="font-size: ${size}">`,
          '</span>',
          '글자'
        );
      },
      markdownHighlight: (_payload, state, dispatch) =>
        wrapSelectionMd(state, dispatch, '==', '==', '형광펜 텍스트'),
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
      clearInlineStyle: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const slice = selection.content();
        const selectedText = slice.content.textBetween(0, slice.content.size, '\n');
        if (!selectedText) return false;
        const cleaned = selectedText
          .replace(
            /<\/?(?:mark|u|span|sup|sub|kbd|b|strong|i|em|s|del|font|div|p|a|h[1-6]|code)[^>]*>/gi,
            ''
          )
          .replace(/(?<!!)\[([^\]]+)\]\([^)]+\)/g, '$1')
          .replace(/\*\*([\s\S]*?)\*\*/g, '$1')
          .replace(/__([\s\S]*?)__/g, '$1')
          .replace(/~~([\s\S]*?)~~/g, '$1')
          .replace(/==([\s\S]*?)==/g, '$1')
          .replace(/`([^`\n]+)`/g, '$1')
          .replace(/(^|[^*])\*([^*\n]+)\*(?!\*)/g, '$1$2');
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
      undo: (_payload, state, dispatch, view) => undo(state, dispatch, view),
      redo: (_payload, state, dispatch, view) => redo(state, dispatch, view),
      underline: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const uMark = schema.marks.u;
        const spanMark = schema.marks.span;
        if (uMark) {
          const { from, to, empty } = selection;
          if (empty) {
            const node = schema.text('밑줄 텍스트', [uMark.create()]);
            tr.replaceSelectionWith(node, false);
            tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 6));
            dispatch(tr);
            return true;
          }
          if (state.doc.rangeHasMark(from, to, uMark)) {
            tr.removeMark(from, to, uMark);
          } else {
            tr.addMark(from, to, uMark.create());
          }
          dispatch(tr);
          return true;
        }
        if (!spanMark) return false;
        const { from, to, empty } = selection;
        const attrs = { htmlAttrs: { style: 'text-decoration: underline' } };
        if (empty) {
          const textNode = schema.text('밑줄 텍스트', [spanMark.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 6));
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, spanMark.create(attrs));
        dispatch(tr);
        return true;
      },
      superscript: makeInlineMarkToggle('sup', '2'),
      subscript: makeInlineMarkToggle('sub', '2'),
      kbdKey: makeInlineMarkToggle('kbd', 'Ctrl+S'),
      fontSize: (payload, state, dispatch) => {
        const size = (payload?.size as string) || '18px';
        const { tr, selection, schema } = state;
        const spanMark = schema.marks.span;
        if (!spanMark) return false;
        const { from, to, empty } = selection;
        const attrs = { htmlAttrs: { style: `font-size: ${size}` } };
        if (empty) {
          const textNode = schema.text('글자', [spanMark.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, spanMark.create(attrs));
        dispatch(tr);
        return true;
      },
      markdownHighlight: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const markType = schema.marks.mark || schema.marks.span;
        if (!markType) return false;
        const { from, to, empty } = selection;
        const attrs =
          markType === schema.marks.mark
            ? {}
            : {
                htmlAttrs: {
                  style:
                    'background-color: rgba(250, 204, 21, 0.45); border-radius: 3px; padding: 0 2px',
                },
              };
        if (empty) {
          const textNode = schema.text('형광펜', [markType.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 3));
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, markType.create(attrs));
        dispatch(tr);
        return true;
      },
      highlightColor: (payload, state, dispatch) => {
        const bg = (payload?.color as string) || 'rgba(250, 204, 21, 0.45)';
        const { tr, selection, schema } = state;
        const spanMark = schema.marks.span || schema.marks.mark;
        if (!spanMark) return false;
        const { from, to, empty } = selection;
        const attrs = {
          htmlAttrs: {
            style: `background-color: ${bg}; border-radius: 3px; padding: 0 2px`,
          },
        };
        if (empty) {
          const textNode = schema.text('형광펜', [spanMark.create(attrs)]);
          tr.replaceSelectionWith(textNode, false);
          tr.setSelection(pmState.TextSelection.create(tr.doc, from, from + 3));
          dispatch(tr);
          return true;
        }
        tr.addMark(from, to, spanMark.create(attrs));
        dispatch(tr);
        return true;
      },
      clearHighlight: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        if (schema.marks.mark) tr.removeMark(selection.from, selection.to, schema.marks.mark);
        if (schema.marks.span) tr.removeMark(selection.from, selection.to, schema.marks.span);
        dispatch(tr);
        return true;
      },
      clearInlineStyle: (_payload, state, dispatch) => {
        const { tr, selection, schema } = state;
        const markNames = [
          'strong',
          'emph',
          'strike',
          'code',
          'link',
          'span',
          'mark',
          'u',
          'sup',
          'sub',
          'kbd',
        ];
        for (const name of markNames) {
          const m = schema.marks[name];
          if (m) tr.removeMark(selection.from, selection.to, m);
        }
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
    wysiwygPlugins: [buildDiagramWysiwygPlugin],
    toHTMLRenderers: {
      htmlInline: {
        u: makeHtmlTagRenderer('u'),
        mark: makeHtmlTagRenderer('mark'),
        sup: makeHtmlTagRenderer('sup'),
        sub: makeHtmlTagRenderer('sub'),
        kbd: makeHtmlTagRenderer('kbd'),
        span: makeHtmlTagRenderer('span'),
      },
      htmlBlock: {
        div: makeHtmlTagRenderer('div'),
        details: makeHtmlTagRenderer('details'),
        summary: makeHtmlTagRenderer('summary'),
      },
    },
  } as PluginInfo;
}

export interface StudioEditorCallbacks {
  getActiveVault: () => string | null;
  onMarkdownChange: (cleanMarkdown: string) => void;
  onRequestWikiLinkPicker: (fromBracketTrigger: boolean) => void;
  onOpenWikiLink: (targetTitle: string) => void;
  onSaveShortcut: () => void;
  onToast: (message: string, isError?: boolean) => void;
  onPickFileAttachment?: () => void | Promise<void>;
  onZoomDelta?: (deltaPct: number) => void;
}

interface TextMatchRange {
  from: number;
  to: number;
}

export class StudioEditorController {
  private editor: Editor | null = null;
  private hostEl: HTMLElement;
  private findBarEl: HTMLElement | null = null;
  private editorMountEl: HTMLElement | null = null;
  private toolbarDockEl: HTMLElement | null = null;
  private toolbarEl: HTMLElement | null = null;
  private popupClampObserver: MutationObserver | null = null;
  private docToolbarResizeBound: (() => void) | null = null;
  private clampingOverlays = false;
  private callbacks: StudioEditorCallbacks;
  private suppressChange = false;
  private bracketTriggeredFromTyping = false;
  private footnoteSeq = 1;

  // Find/replace bar state
  private findInputEl: HTMLInputElement | null = null;
  private replaceInputEl: HTMLInputElement | null = null;
  private findCountEl: HTMLElement | null = null;
  private findCaseSensitive = false;
  private lastMatchIdx = -1;

  // Right-click context menu state
  private contextMenuEl: HTMLElement | null = null;
  private ctxDocClickBound: ((ev: Event) => void) | null = null;
  private ctxDocKeyBound: ((ev: KeyboardEvent) => void) | null = null;
  private shiftHeldForPaste = false;
  private imagePasteSeq = 0;

  constructor(hostEl: HTMLElement, callbacks: StudioEditorCallbacks) {
    this.hostEl = hostEl;
    this.callbacks = callbacks;
  }

  private makeBtn(label: string, title: string, extraClass = ''): HTMLButtonElement {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `tui-custom-btn${extraClass ? ` ${extraClass}` : ''}`;
    btn.textContent = label;
    btn.title = title;
    return btn;
  }

  private getScrollPane(): HTMLElement | null {
    return this.hostEl.closest('#app-editor-pane') as HTMLElement | null;
  }

  private preservePaneScroll(fn: () => void): void {
    const pane = this.getScrollPane();
    const savedTop = pane ? pane.scrollTop : 0;
    const savedLeft = pane ? pane.scrollLeft : 0;
    try {
      fn();
    } finally {
      if (pane) {
        pane.scrollTop = savedTop;
        pane.scrollLeft = savedLeft;
        requestAnimationFrame(() => {
          if (pane.isConnected) {
            pane.scrollTop = savedTop;
            pane.scrollLeft = savedLeft;
          }
        });
        window.setTimeout(() => {
          if (pane.isConnected) {
            pane.scrollTop = savedTop;
            pane.scrollLeft = savedLeft;
          }
        }, 0);
      }
    }
  }

  private focusEditorWithoutScroll(): void {
    this.preservePaneScroll(() => {
      const anyEd = this.editor as any;
      const mode: EditorType = this.editor?.isWysiwygMode() ? 'wysiwyg' : 'markdown';
      const targetDom: HTMLElement | null =
        (mode === 'wysiwyg' ? anyEd?.wwEditor?.view?.dom : anyEd?.mdEditor?.view?.dom) ??
        this.hostEl.querySelector<HTMLElement>('.ProseMirror');
      if (targetDom) {
        try {
          targetDom.focus({ preventScroll: true });
        } catch {
          targetDom.focus();
        }
      }
    });
  }

  private handleHostMouseDownCapture = (ev: MouseEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (!target) return;
    if (
      target.closest(
        'button, input, textarea, select, [data-flow-action], [data-flow-convert-start]'
      )
    ) {
      return;
    }
    const pmEl = target.closest('.ProseMirror') as HTMLElement | null;
    if (!pmEl) return;
    const pane = this.getScrollPane();
    if (!pane) return;
    const savedTop = pane.scrollTop;
    const savedLeft = pane.scrollLeft;
    if (document.activeElement !== pmEl) {
      try {
        pmEl.focus({ preventScroll: true });
      } catch {
        // ignore
      }
      pane.scrollTop = savedTop;
      pane.scrollLeft = savedLeft;
      requestAnimationFrame(() => {
        if (pane.isConnected && Math.abs(pane.scrollTop - savedTop) > 150) {
          const sel = window.getSelection();
          if (!sel || sel.isCollapsed) {
            pane.scrollTop = savedTop;
            pane.scrollLeft = savedLeft;
          }
        }
      });
    }
  };

  private createPopupItem(label: string, onClick: () => void): HTMLButtonElement {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = 'tui-custom-popup-item';
    item.textContent = label;
    item.addEventListener('click', () => {
      onClick();
      this.closeToolbarPopup();
      if (this.toolbarDockEl?.classList.contains('mobile-all-tools-open')) {
        this.toolbarDockEl.classList.remove('mobile-all-tools-open');
        const moreBtn = this.toolbarDockEl.querySelector('[data-mobile-more-tools]');
        if (moreBtn) {
          moreBtn.classList.remove('active');
          moreBtn.textContent = '🛠️ 도구 ▾';
        }
      }
      this.focusEditorWithoutScroll();
    });
    return item;
  }

  public mount(params: {
    initialMarkdown: string;
    mode: EditorType;
    theme: 'light' | 'dark';
    toolbarDockEl?: HTMLElement | null;
  }): void {
    this.destroy();
    this.hostEl.innerHTML = '';
    this.toolbarDockEl = params.toolbarDockEl ?? null;

    this.findBarEl = document.createElement('div');
    this.findBarEl.className = 'editor-find-bar';
    this.findBarEl.style.display = 'none';
    this.buildFindBarDom(this.findBarEl);

    this.editorMountEl = document.createElement('div');
    if (this.toolbarDockEl) {
      this.toolbarDockEl.innerHTML = '';
      this.toolbarDockEl.classList.toggle('toastui-editor-dark', params.theme === 'dark');
      this.toolbarDockEl.style.display = '';
      this.hostEl.append(this.editorMountEl);
    } else {
      this.hostEl.append(this.findBarEl, this.editorMountEl);
    }
    this.attachEditorContextMenu();

    const vault = this.callbacks.getActiveVault();
    const hydrated = hydrateMarkdownMediaUrls(params.initialMarkdown, vault);

    // ─── Group 1: Undo / Redo / Find ───
    const undoBtn = this.makeBtn('↩', '실행 취소 (Ctrl+Z)');
    undoBtn.addEventListener('click', () => {
      this.editor?.exec('undo');
      this.focusEditorWithoutScroll();
    });
    const redoBtn = this.makeBtn('↪', '다시 실행 (Ctrl+Y)');
    redoBtn.addEventListener('click', () => {
      this.editor?.exec('redo');
      this.focusEditorWithoutScroll();
    });
    const findBtn = this.makeBtn('🔍찾기', '찾기 및 바꾸기 (Ctrl+F / Ctrl+H)');
    findBtn.addEventListener('click', () => this.openFindBar(true));

    // ─── Group 2: Quick Headings H1~H6 & Font Size ───
    const quickHeadingBody = document.createElement('div');
    quickHeadingBody.className = 'tui-custom-popup-menu';
    for (let lvl = 1; lvl <= 6; lvl++) {
      quickHeadingBody.appendChild(
        this.createPopupItem(`H${lvl}  제목 ${lvl}`, () => {
          this.editor?.exec('heading', { level: lvl });
        })
      );
    }
    quickHeadingBody.appendChild(
      this.createPopupItem('¶  일반 본문', () => {
        this.editor?.exec('heading', { level: 0 });
      })
    );
    const quickHeadingBtn = this.makeBtn('H▾', 'H1~H6 빠른 제목 적용');

    const fontSizeBody = document.createElement('div');
    fontSizeBody.className = 'tui-custom-popup-menu';
    for (const fs of FONT_SIZE_PRESETS) {
      const it = this.createPopupItem(fs.label, () => {
        this.editor?.exec('fontSize', { size: fs.size });
      });
      fontSizeBody.appendChild(it);
    }
    const fontSizeBtn = this.makeBtn('크기▾', '글자 크기 변경');

    // ─── Group 3: Underline / Sup / Sub / Kbd / Highlight / Clear Format ───
    const underlineBtn = this.makeBtn('U', '밑줄 (Ctrl+U)', 'underline-btn');
    underlineBtn.addEventListener('click', () => {
      this.editor?.exec('underline');
      this.focusEditorWithoutScroll();
    });

    const supBtn = this.makeBtn('X²', '위 첨자 (<sup>)');
    supBtn.addEventListener('click', () => {
      this.editor?.exec('superscript');
      this.focusEditorWithoutScroll();
    });

    const subBtn = this.makeBtn('X₂', '아래 첨자 (<sub>)');
    subBtn.addEventListener('click', () => {
      this.editor?.exec('subscript');
      this.focusEditorWithoutScroll();
    });

    const kbdBtn = this.makeBtn('⌨', '단축키 배지 (<kbd>)');
    kbdBtn.addEventListener('click', () => {
      this.editor?.exec('kbdKey');
      this.focusEditorWithoutScroll();
    });

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
        this.focusEditorWithoutScroll();
      });
      swatchGrid.appendChild(b);
    }
    highlightPopupBody.querySelector('[data-action="md-mark"]')?.addEventListener('click', () => {
      this.editor?.exec('markdownHighlight');
      this.closeToolbarPopup();
      this.focusEditorWithoutScroll();
    });
    highlightPopupBody.querySelector('[data-action="clear-mark"]')?.addEventListener('click', () => {
      this.editor?.exec('clearHighlight');
      this.closeToolbarPopup();
      this.focusEditorWithoutScroll();
    });
    const highlightBtn = this.makeBtn('형광펜', '배경 형광펜 팔레트', 'highlight-btn');

    const clearFmtBtn = this.makeBtn('Tx', '선택 서식 지우기');
    clearFmtBtn.addEventListener('click', () => {
      this.editor?.exec('clearInlineStyle');
      this.focusEditorWithoutScroll();
    });

    // ─── Group 4: Alignment ───
    const alignBody = document.createElement('div');
    alignBody.className = 'tui-custom-popup-menu';
    for (const al of [
      { label: '⫤ 왼쪽 정렬', dir: 'left' },
      { label: '≡ 가운데 정렬', dir: 'center' },
      { label: '⫥ 오른쪽 정렬', dir: 'right' },
      { label: '☰ 양쪽 맞춤', dir: 'justify' },
    ]) {
      alignBody.appendChild(
        this.createPopupItem(al.label, () => {
          this.wrapSelectionWithBlockAlign(al.dir);
        })
      );
    }
    const alignBtn = this.makeBtn('정렬▾', '문단 정렬 (좌/중/우/양쪽)');

    // ─── Group 5: Callouts / Toggle / Columns / PageBreak / TOC ───
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
        this.focusEditorWithoutScroll();
      });
      calloutPopupBody.appendChild(item);
    }
    const calloutBtn = this.makeBtn('💡콜아웃', '12종 콜아웃 박스 삽입');

    const toggleBtn = this.makeBtn('▸토글', '접기/펼치기 토글 (<details>)');
    toggleBtn.addEventListener('click', () => {
      this.insertBlockSnippet(
        `<details class="notion-toggle" open>\n<summary>토글 제목을 입력하세요</summary>\n\n여기에 접을 수 있는 본문 내용을 작성하세요.\n\n</details>`
      );
    });

    const colsBody = document.createElement('div');
    colsBody.className = 'tui-custom-popup-menu';
    colsBody.appendChild(
      this.createPopupItem('⫼ 2단 레이아웃 삽입', () => {
        this.insertBlockSnippet(
          `<div class="doc-cols doc-cols-2">\n<div class="doc-col">\n\n**왼쪽 열**\n\n내용 1\n\n</div>\n<div class="doc-col">\n\n**오른쪽 열**\n\n내용 2\n\n</div>\n</div>`
        );
      })
    );
    colsBody.appendChild(
      this.createPopupItem('⫼ 3단 레이아웃 삽입', () => {
        this.insertBlockSnippet(
          `<div class="doc-cols doc-cols-3">\n<div class="doc-col">\n\n**1열**\n\n내용\n\n</div>\n<div class="doc-col">\n\n**2열**\n\n내용\n\n</div>\n<div class="doc-col">\n\n**3열**\n\n내용\n\n</div>\n</div>`
        );
      })
    );
    const colsBtn = this.makeBtn('⫼다단▾', '2단 / 3단 다단 블록 삽입');

    const pageBreakBtn = this.makeBtn('✂쪽나눔', '인쇄·PDF 페이지 나누기');
    pageBreakBtn.addEventListener('click', () => {
      this.insertBlockSnippet('<div class="page-break"></div>');
      this.callbacks.onToast('페이지 나누기를 삽입했습니다.');
    });

    const tocBtn = this.makeBtn('📑목차', '문서 제목(#~###)에서 목차 자동 생성');
    tocBtn.addEventListener('click', () => {
      this.insertAutoToc();
    });

    // ─── Group 6: WikiLink / Footnote / Images / Any Attachment ───
    const wikiLinkBtn = this.makeBtn('[[연결]]', '볼트 내 다른 노트 연결');
    wikiLinkBtn.addEventListener('click', () => {
      this.bracketTriggeredFromTyping = false;
      this.callbacks.onRequestWikiLinkPicker(false);
    });

    const footnoteBtn = this.makeBtn('[^각주]', '각주 참조 및 하단 주석 삽입');
    footnoteBtn.addEventListener('click', () => {
      this.insertFootnote();
    });

    const nativeImgBtn = this.makeBtn('🖼이미지', 'PC에서 이미지 파일 첨부');
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

    const anyFileBtn = this.makeBtn('📎첨부', 'PDF/문서/일반 파일 첨부');
    anyFileBtn.addEventListener('click', async () => {
      if (this.callbacks.onPickFileAttachment) {
        await this.callbacks.onPickFileAttachment();
        return;
      }
      const activeVault = this.callbacks.getActiveVault();
      if (!activeVault) {
        this.callbacks.onToast('먼저 볼트를 열어 주세요.', true);
        return;
      }
      try {
        const picked = await api.pickFileAttachment(activeVault);
        if (picked.picked && picked.media_url) {
          this.insertAttachmentLink(picked.file_name || '첨부파일', picked.media_url, picked.is_image);
          this.callbacks.onToast(`파일을 첨부했습니다 (${picked.rel_path || picked.file_name})`);
        }
      } catch (err: any) {
        this.callbacks.onToast(err?.message || '파일을 첨부하지 못했습니다.', true);
      }
    });

    // ─── Group 7: Math & Diagrams ───
    const mathBody = document.createElement('div');
    mathBody.style.minWidth = '230px';
    mathBody.innerHTML = `
      <div class="tui-custom-popup-menu" data-math-actions></div>
      <div style="margin-top:8px;padding-top:6px;border-top:1px solid var(--border-subtle);display:flex;flex-wrap:wrap;gap:3px;" data-math-syms></div>
    `;
    const mathActions = mathBody.querySelector('[data-math-actions]')!;
    mathActions.appendChild(
      this.createPopupItem('∑ 블록 수식 ($$ ... $$)', () => {
        this.insertBlockSnippet('$$\nE = mc^2\n$$');
      })
    );
    mathActions.appendChild(
      this.createPopupItem('𝑓 인라인 수식 ($ ... $)', () => {
        this.insertInlineText('$x^2 + y^2 = r^2$');
      })
    );
    const mathSymsWrap = mathBody.querySelector('[data-math-syms]')!;
    for (const sym of MATH_SYMBOLS) {
      const sb = document.createElement('button');
      sb.type = 'button';
      sb.className = 'btn btn-sm';
      sb.style.padding = '2px 6px';
      sb.style.fontSize = '12px';
      sb.textContent = sym;
      sb.addEventListener('click', () => {
        this.insertInlineText(sym);
        this.closeToolbarPopup();
        this.focusEditorWithoutScroll();
      });
      mathSymsWrap.appendChild(sb);
    }
    const mathBtn = this.makeBtn('∑수식▾', '수식 블록 & 수학 기호');

    const diagramBody = document.createElement('div');
    diagramBody.className = 'tui-custom-popup-menu';
    diagramBody.appendChild(
      this.createPopupItem('✨ 시각적 플로우차트 빌더 열기…', () => {
        this.openFlowchartBuilderModal();
      })
    );
    for (const tpl of FLOWCHART_TEMPLATES) {
      diagramBody.appendChild(
        this.createPopupItem(tpl.label, () => {
          this.insertBlockSnippet(`\`\`\`mermaid\n${tpl.code}\n\`\`\``);
        })
      );
    }
    diagramBody.appendChild(
      this.createPopupItem('📅 간트 일정표 (Gantt)', () => {
        this.insertBlockSnippet(
          '```mermaid\ngantt\n  title 프로젝트 일정\n  dateFormat YYYY-MM-DD\n  section 기획\n  설계 확정 :done, 2026-11-01, 5d\n  section 구현\n  핵심 개발 :active, 2026-11-06, 10d\n```'
        );
      })
    );
    diagramBody.appendChild(
      this.createPopupItem('⚖️ 장단점 비교표 (2열 표)', () => {
        this.insertBlockSnippet(
          '| 항목 | 장점 | 단점 / 리스크 | 비고 |\n| --- | --- | --- | --- |\n| 대안 A | 빠른 적용 | 확장성 제한 | 우선 검토 |\n| 대안 B | 높은 안정성 | 초기 비용 | 장기 추천 |'
        );
      })
    );
    const diagramBtn = this.makeBtn('🔀플로우▾', '고급 플로우차트(TB/LR/Subgraph) · 빌더 · 다이어그램');

    // ─── Group 8: Date/Time / Status Badges / Symbols / Text Ops / Snippets / Zoom ───
    const dateBody = document.createElement('div');
    dateBody.className = 'tui-custom-popup-menu';
    for (const d of [
      { label: '📅 오늘 날짜 (YYYY-MM-DD)', fn: () => this.formatTodayDate('isoDate') },
      { label: '🗓️ 한국어 날짜+요일', fn: () => this.formatTodayDate('koreanFull') },
      { label: '⏰ 현재 시각 (HH:mm)', fn: () => this.formatTodayDate('time') },
      { label: '🕒 전체 타임스탬프', fn: () => this.formatTodayDate('isoFull') },
      { label: '🎯 D-Day 표기', fn: () => '[D-7]' },
    ]) {
      dateBody.appendChild(
        this.createPopupItem(d.label, () => {
          this.insertInlineText(d.fn());
        })
      );
    }
    const dateBtn = this.makeBtn('📅날짜▾', '날짜/시각/D-Day 삽입');

    const badgeBody = document.createElement('div');
    badgeBody.className = 'tui-custom-popup-menu';
    for (const st of STATUS_BADGES) {
      badgeBody.appendChild(
        this.createPopupItem(st.label, () => {
          this.insertInlineText(`<span class="status-pill ${st.cls}">${st.text}</span> `);
        })
      );
    }
    for (const pct of [25, 50, 75, 100]) {
      badgeBody.appendChild(
        this.createPopupItem(`▬ 진행률 바 ${pct}%`, () => {
          this.insertInlineText(
            `<span class="progress-inline"><span class="progress-inline-track"><span class="progress-inline-fill" style="width:${pct}%"></span></span><strong>${pct}%</strong></span> `
          );
        })
      );
    }
    const badgeBtn = this.makeBtn('🏷배지▾', '상태 배지 및 진행률 바 삽입');

    const symBody = document.createElement('div');
    symBody.style.width = '240px';
    symBody.innerHTML = `<div style="font-size:11.5px;font-weight:700;margin-bottom:6px;">특수 기호 삽입</div><div style="display:flex;flex-wrap:wrap;gap:4px;" data-sym-grid></div>`;
    const symGrid = symBody.querySelector('[data-sym-grid]')!;
    for (const ch of KOREAN_OFFICE_SYMBOLS) {
      const sb = document.createElement('button');
      sb.type = 'button';
      sb.className = 'btn btn-sm';
      sb.style.padding = '3px 7px';
      sb.textContent = ch;
      sb.addEventListener('click', () => {
        this.insertInlineText(ch);
        this.closeToolbarPopup();
        this.focusEditorWithoutScroll();
      });
      symGrid.appendChild(sb);
    }
    const symBtn = this.makeBtn('※기호▾', '한국어 공문서·업무 특수기호');

    const textOpsBody = document.createElement('div');
    textOpsBody.className = 'tui-custom-popup-menu';
    for (const op of [
      { label: 'UPPER 대문자 변환', mode: 'upper' },
      { label: 'lower 소문자 변환', mode: 'lower' },
      { label: 'Title Case 첫 글자 대문자', mode: 'title' },
      { label: '↑ 줄 오름차순 정렬', mode: 'sortAsc' },
      { label: '↓ 줄 내림차순 정렬', mode: 'sortDesc' },
      { label: '🧹 중복 줄 제거', mode: 'dedupe' },
      { label: '✂ 앞뒤 공백·빈줄 정리', mode: 'trimLines' },
      { label: '🔢 줄 번호 매기기 (1. 2. 3.)', mode: 'numberLines' },
    ] as const) {
      textOpsBody.appendChild(
        this.createPopupItem(op.label, () => {
          this.transformSelectionText(op.mode);
        })
      );
    }
    const textOpsBtn = this.makeBtn('Aa변환▾', '선택 텍스트 대소문자/정렬/중복제거/줄번호');

    const snippetBody = document.createElement('div');
    snippetBody.className = 'tui-custom-popup-menu';
    for (const sn of [
      {
        label: '📝 회의록 양식',
        md: `## 회의록\n- **일시**: ${this.formatTodayDate('koreanFull')}\n- **참석자**: \n- **안건**: \n\n### 논의 내용\n1. \n\n### 결정 및 실행 과제 (Action Items)\n- [ ] 담당자 / 기한`,
      },
      {
        label: '💼 일일 업무 보고',
        md: `## 일일 업무 보고 (${this.formatTodayDate('isoDate')})\n### 1. 오늘 완료 업무\n- \n\n### 2. 진행 중 / 이슈 사항\n- \n\n### 3. 내일 계획\n- [ ] `,
      },
      {
        label: '📅 주간 업무 계획',
        md: `## 주간 업무 계획\n| 요일 | 핵심 목표 | 세부 작업 | 상태 |\n| --- | --- | --- | --- |\n| 월 | | | |\n| 화 | | | |\n| 수 | | | |\n| 목 | | | |\n| 금 | | | |`,
      },
      {
        label: '📄 기안·품의 결재 헤더',
        md: `| 문서번호 | 작성일자 | 작성자 | 부서 |\n| --- | --- | --- | --- |\n| DOC-${this.formatTodayDate('isoDate')} | ${this.formatTodayDate('isoDate')} | | |\n\n### 1. 검토 배경\n\n### 2. 세부 내용\n`,
      },
      {
        label: '🎓 코넬 노트 (Cornell Notes)',
        md: `<div class="doc-cols doc-cols-2">\n<div class="doc-col">\n\n### 키워드 · 질문\n- \n\n</div>\n<div class="doc-col">\n\n### 핵심 필기 내용\n- \n\n</div>\n</div>\n\n> [!abstract] 핵심 요약\n> 한 줄 요약 내용을 정리하세요.`,
      },
      {
        label: '☑️ 업무 체크리스트 표',
        md: `| 번호 | 작업 항목 | 담당 | 마감일 | 진행률 |\n| --- | --- | --- | --- | --- |\n| 1 | | | | 0% |\n| 2 | | | | 0% |`,
      },
    ]) {
      snippetBody.appendChild(
        this.createPopupItem(sn.label, () => {
          this.insertBlockSnippet(sn.md);
        })
      );
    }
    const snippetBtn = this.makeBtn('📋양식▾', '회의록·업무보고·기안·체크리스트 양식 삽입');

    const copyMdBtn = this.makeBtn('📋복사', '현재 문서 마크다운 전체 복사');
    copyMdBtn.addEventListener('click', () => {
      void this.copyCleanMarkdown();
    });

    const zoomOutBtn = this.makeBtn('−', '화면 배율 축소');
    zoomOutBtn.addEventListener('click', () => this.callbacks.onZoomDelta?.(-10));
    const zoomInBtn = this.makeBtn('+', '화면 배율 확대');
    zoomInBtn.addEventListener('click', () => this.callbacks.onZoomDelta?.(10));

    this.editor = new Editor({
      el: this.editorMountEl,
      height: 'auto',
      minHeight: '460px',
      autofocus: false,
      initialEditType: params.mode,
      previewStyle: 'vertical',
      initialValue: hydrated,
      language: 'ko-KR',
      hideModeSwitch: true,
      usageStatistics: false,
      theme: params.theme === 'dark' ? 'dark' : 'default',
      placeholder:
        '내용을 입력하세요… ([[ 로 다른 노트 연결, Ctrl+F 찾기/바꾸기, 이미지/파일 드래그 앤 드롭)',
      ...({ customHTMLSanitizer: safeHtmlSanitizer } as any),
      plugins: [
        [colorSyntax, { preset: NOTION_COLOR_PRESET }] as any,
        [codeSyntaxHighlight, { highlighter: Prism }] as any,
        tableMergedCell as any,
        notionRichPlugin as any,
      ],
      toolbarItems: [
        [
          { name: 'undoHistory', tooltip: '실행 취소 (Ctrl+Z)', el: undoBtn },
          { name: 'redoHistory', tooltip: '다시 실행 (Ctrl+Y)', el: redoBtn },
          { name: 'findReplace', tooltip: '찾기 / 바꾸기 (Ctrl+F)', el: findBtn },
        ],
        [
          'heading',
          {
            name: 'quickHeadings',
            tooltip: 'H1~H6 제목 빠른 선택',
            el: quickHeadingBtn,
            popup: { body: quickHeadingBody, style: { width: '170px' } },
          },
          {
            name: 'fontSizePick',
            tooltip: '글자 크기 (11px ~ 32px)',
            el: fontSizeBtn,
            popup: { body: fontSizeBody, style: { width: '185px' } },
          },
        ],
        [
          'bold',
          'italic',
          { name: 'underline', tooltip: '밑줄 (Ctrl+U)', el: underlineBtn },
          'strike',
          { name: 'superscript', tooltip: '위 첨자', el: supBtn },
          { name: 'subscript', tooltip: '아래 첨자', el: subBtn },
          { name: 'kbdKey', tooltip: '단축키 배지 (<kbd>)', el: kbdBtn },
          {
            name: 'notionHighlight',
            tooltip: '배경 형광펜 팔레트 (==형광펜==)',
            el: highlightBtn,
            popup: { body: highlightPopupBody, style: { width: 'auto' } },
          },
          { name: 'clearInlineStyle', tooltip: '서식 지우기', el: clearFmtBtn },
        ],
        [
          {
            name: 'textAlign',
            tooltip: '문단 정렬 (좌/중/우/양쪽)',
            el: alignBtn,
            popup: { body: alignBody, style: { width: '165px' } },
          },
          'ul',
          'ol',
          'task',
          'indent',
          'outdent',
        ],
        [
          {
            name: 'notionCallout',
            tooltip: '12종 콜아웃 박스',
            el: calloutBtn,
            popup: { body: calloutPopupBody, style: { width: '230px' } },
          },
          { name: 'toggleBlock', tooltip: '접기/펼치기 토글 블록', el: toggleBtn },
          {
            name: 'columnsLayout',
            tooltip: '2단 / 3단 다단 레이아웃',
            el: colsBtn,
            popup: { body: colsBody, style: { width: '190px' } },
          },
          'quote',
          'hr',
          { name: 'pageBreak', tooltip: '인쇄/PDF 페이지 나누기', el: pageBreakBtn },
          { name: 'autoToc', tooltip: '제목 기반 목차 자동 생성', el: tocBtn },
        ],
        [
          'table',
          'link',
          { name: 'wikiLinkInsert', tooltip: '노트 위키링크 연결 [[...]]', el: wikiLinkBtn },
          { name: 'footnoteInsert', tooltip: '각주 [^1] 삽입', el: footnoteBtn },
          'image',
          { name: 'pickNativeAttachment', tooltip: 'PC 이미지 선택', el: nativeImgBtn },
          { name: 'pickAnyAttachment', tooltip: 'PC 일반 파일 첨부 (PDF 등)', el: anyFileBtn },
        ],
        [
          'code',
          'codeblock',
          {
            name: 'mathTools',
            tooltip: '수식 및 수학 기호',
            el: mathBtn,
            popup: { body: mathBody, style: { width: '250px' } },
          },
          {
            name: 'diagramTemplates',
            tooltip: '고급 플로우차트 빌더 & 다이어그램',
            el: diagramBtn,
            popup: { body: diagramBody, style: { width: '255px' } },
          },
        ],
        [
          {
            name: 'dateTimeStamps',
            tooltip: '오늘 날짜 / 시각 / D-Day',
            el: dateBtn,
            popup: { body: dateBody, style: { width: '205px' } },
          },
          {
            name: 'statusBadges',
            tooltip: '상태 배지 / 진행률 바',
            el: badgeBtn,
            popup: { body: badgeBody, style: { width: '195px' } },
          },
          {
            name: 'specialSymbols',
            tooltip: '한국어 공문서 특수문자',
            el: symBtn,
            popup: { body: symBody, style: { width: '260px' } },
          },
          {
            name: 'textTransforms',
            tooltip: '선택 텍스트 변환/정렬/줄번호',
            el: textOpsBtn,
            popup: { body: textOpsBody, style: { width: '215px' } },
          },
          {
            name: 'docTemplates',
            tooltip: '문서 양식 (회의록/보고/기안/코넬)',
            el: snippetBtn,
            popup: { body: snippetBody, style: { width: '220px' } },
          },
          { name: 'copyMarkdown', tooltip: '마크다운 전체 복사', el: copyMdBtn },
          { name: 'zoomOutBtn', tooltip: '편집기 축소', el: zoomOutBtn },
          { name: 'zoomInBtn', tooltip: '편집기 확대', el: zoomInBtn },
          'scrollSync',
        ],
      ],
      hooks: {
        addImageBlobHook: async (
          blob: Blob | File,
          callback: (url: string, text?: string) => void
        ) => {
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
          if (this.findBarEl && this.findBarEl.style.display !== 'none') {
            this.refreshFindMatchCount();
          }
        },
        beforePreviewRender: (html: string) => {
          return enhanceRenderedHtml(html);
        },
        keydown: (_editorType: EditorType, ev: KeyboardEvent) => {
          const mod = ev.ctrlKey || ev.metaKey;
          const k = ev.key.toLowerCase();
          if (mod && k === 's') {
            ev.preventDefault();
            this.callbacks.onSaveShortcut();
            return;
          }
          if (mod && k === 'u') {
            ev.preventDefault();
            this.editor?.exec('underline');
            return;
          }
          if (mod && k === 'f') {
            ev.preventDefault();
            this.openFindBar(false);
            return;
          }
          if (mod && k === 'h') {
            ev.preventDefault();
            this.openFindBar(true);
            return;
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

    // Prevent TOAST UI's internal EditorBase.prototype.focus() (`setTimeout` + `tr.scrollIntoView()`)
    // from yanking #app-editor-pane back to the cursor after toolbar clicks or popups.
    const anyEd = this.editor as any;
    const safeFocus = () => this.focusEditorWithoutScroll();
    anyEd.focus = safeFocus;
    if (anyEd.wwEditor) anyEd.wwEditor.focus = safeFocus;
    if (anyEd.mdEditor) anyEd.mdEditor.focus = safeFocus;

    this.attachToolbarToDock(params.theme);
    this.attachCleanClipboardHandlers();
    this.hostEl.addEventListener('mousedown', this.handleHostMouseDownCapture, true);
    this.hostEl.addEventListener('click', this.handleHostClick);
  }

  private handleMountKeyTrack = (ev: KeyboardEvent): void => {
    this.shiftHeldForPaste = Boolean(ev.shiftKey);
    if (ev.type !== 'keydown') return;
    const mod = ev.ctrlKey || ev.metaKey;
    if (!mod || ev.altKey) return;
    const k = ev.key.toLowerCase();
    // Intercept Ctrl+S before TOAST UI's built-in `Mod-s` Strike (`~~~~`) keymap runs!
    if (k === 's' && !ev.shiftKey) {
      ev.preventDefault();
      ev.stopPropagation();
      this.callbacks.onSaveShortcut();
      return;
    }
    if (k === 'u' && !ev.shiftKey) {
      ev.preventDefault();
      ev.stopPropagation();
      this.editor?.exec('underline');
      return;
    }
    if (k === 'f' && !ev.shiftKey) {
      ev.preventDefault();
      ev.stopPropagation();
      this.openFindBar(false);
      return;
    }
    if (k === 'h' && !ev.shiftKey) {
      ev.preventDefault();
      ev.stopPropagation();
      this.openFindBar(true);
    }
  };

  private attachCleanClipboardHandlers(): void {
    if (!this.editorMountEl) return;
    this.editorMountEl.addEventListener('keydown', this.handleMountKeyTrack, true);
    this.editorMountEl.addEventListener('keyup', this.handleMountKeyTrack, true);
    this.editorMountEl.addEventListener('copy', this.handleEditorCopyBubble, false);
    this.editorMountEl.addEventListener('cut', this.handleEditorCopyBubble, false);
    this.editorMountEl.addEventListener('paste', this.handleEditorPasteCapture, true);
    this.editorMountEl.addEventListener('drop', this.handleEditorDropCapture, true);
  }

  private detachCleanClipboardHandlers(): void {
    if (!this.editorMountEl) return;
    this.editorMountEl.removeEventListener('keydown', this.handleMountKeyTrack, true);
    this.editorMountEl.removeEventListener('keyup', this.handleMountKeyTrack, true);
    this.editorMountEl.removeEventListener('copy', this.handleEditorCopyBubble, false);
    this.editorMountEl.removeEventListener('cut', this.handleEditorCopyBubble, false);
    this.editorMountEl.removeEventListener('paste', this.handleEditorPasteCapture, true);
    this.editorMountEl.removeEventListener('drop', this.handleEditorDropCapture, true);
  }

  private handleEditorCopyBubble = (ev: ClipboardEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (!target || !target.closest('.ProseMirror')) return;
    const dt = ev.clipboardData;
    if (!dt) return;
    const html = dt.getData('text/html') || '';
    if (html && !/\b(?:font-family|mso-[a-z-]+)\s*:/i.test(html)) {
      try {
        dt.setData(ONYX_INTERNAL_CLIPBOARD_MIME, '1');
      } catch {
        // ignore restricted clipboard implementations
      }
    }
  };

  private isPasteTargetEditorSurface(target: HTMLElement | null): boolean {
    if (!target) return false;
    if (target.classList.contains('toastui-editor-pseudo-clipboard')) return true;
    const tag = target.tagName.toLowerCase();
    if (tag === 'input' || tag === 'textarea' || tag === 'select') return false;
    return Boolean(target.closest('.ProseMirror'));
  }

  private flushActiveImeComposition(view: any): void {
    if (!view || !view.composing) return;
    try {
      if (typeof toastPmViewMod?.__endComposition === 'function') {
        toastPmViewMod.__endComposition(view);
      } else {
        view.domObserver?.forceFlush?.();
      }
    } catch {
      // ignore
    }
  }

  private clearPseudoClipboardTextarea(): void {
    if (!this.editorMountEl) return;
    const pseudo = this.editorMountEl.querySelector<HTMLTextAreaElement>(
      'textarea.toastui-editor-pseudo-clipboard'
    );
    if (pseudo) {
      pseudo.value = '';
    }
  }

  private hasAncestorOfType($pos: any, names: string[]): boolean {
    if (!$pos || typeof $pos.depth !== 'number') return false;
    for (let d = $pos.depth; d >= 0; d--) {
      const n = $pos.node(d)?.type?.name;
      if (n && names.includes(n)) return true;
    }
    return false;
  }

  private tryInsertPastedFlowchartBlock(plainText: string): boolean {
    if (!this.editor || !this.editor.isWysiwygMode()) return false;
    const view = this.getActiveProseMirrorView();
    if (!view) return false;
    const parentType = view.state.selection.$from.parent?.type?.name;
    if (parentType === 'codeBlock') return false;

    const trimmed = plainText.replace(/\r\n?/g, '\n').trim();
    if (!trimmed) return false;

    let lang: string | null = null;
    let body = '';
    const fenced =
      /^```(mermaid|flowchart|flow|diagram|graph)\s*\n([\s\S]*?)\n```\s*$/i.exec(trimmed);
    if (fenced) {
      const rawLang = fenced[1].toLowerCase();
      lang = rawLang === 'diagram' ? 'mermaid' : rawLang;
      body = fenced[2];
    } else if (looksLikeBareFlowchartText(trimmed)) {
      lang = 'mermaid';
      body = trimmed;
    }
    if (!lang) return false;

    const codeBlockType = view.state.schema.nodes.codeBlock;
    if (!codeBlockType) return false;
    this.flushActiveImeComposition(view);
    const cbNode = codeBlockType.create(
      { language: lang },
      body ? view.state.schema.text(body) : undefined
    );
    view.dispatch(
      view.state.tr
        .setStoredMarks([])
        .replaceSelectionWith(cbNode)
        .scrollIntoView()
        .setMeta('paste', true)
        .setMeta('uiEvent', 'paste')
    );
    return true;
  }

  /**
   * Inserts pure plain text into either WYSIWYG or Markdown mode with zero HTML tags,
   * zero inline style attributes, and zero inherited ProseMirror marks.
   */
  public insertCleanPlainText(rawText: string): void {
    if (!this.editor) return;
    this.clearPseudoClipboardTextarea();

    if (this.tryInsertPastedFlowchartBlock(rawText)) {
      this.focusEditorWithoutScroll();
      return;
    }

    const anyEd = this.editor as any;
    if (!this.editor.isWysiwygMode()) {
      const cleanMdText = normalizePastedPlainText(rawText);
      if (!cleanMdText) return;
      const mdView = anyEd.mdEditor?.view;
      this.flushActiveImeComposition(mdView);
      this.preservePaneScroll(() => {
        if (anyEd.mdEditor?.replaceSelection) {
          anyEd.mdEditor.replaceSelection(cleanMdText);
        } else {
          this.editor?.insertText(cleanMdText);
        }
      });
      this.focusEditorWithoutScroll();
      return;
    }

    const wwView = anyEd.wwEditor?.view;
    if (!wwView) return;
    this.flushActiveImeComposition(wwView);

    const { state } = wwView;
    const { schema, selection } = state;
    const $from = selection.$from;

    // 1) Inside code block: preserve verbatim newlines & indentation
    if ($from.parent?.type?.name === 'codeBlock' || $from.parent?.type?.spec?.code) {
      const codeTxt = normalizePastedPlainText(rawText);
      if (!codeTxt) return;
      const tr = state.tr
        .insertText(codeTxt, selection.from, selection.to)
        .scrollIntoView()
        .setMeta('paste', true)
        .setMeta('uiEvent', 'paste');
      wwView.dispatch(tr);
      this.focusEditorWithoutScroll();
      return;
    }

    // 2) Inside table cell: GFM table cells hold a single paragraph
    if (this.hasAncestorOfType($from, ['tableHeadCell', 'tableBodyCell'])) {
      const cellTxt = normalizePastedPlainText(rawText).replace(/\n+/g, ' ').trim();
      if (!cellTxt) return;
      const tr = state.tr
        .setStoredMarks([])
        .replaceSelectionWith(schema.text(cellTxt, []), false)
        .scrollIntoView()
        .setMeta('paste', true)
        .setMeta('uiEvent', 'paste');
      wwView.dispatch(tr);
      this.focusEditorWithoutScroll();
      return;
    }

    // 3) Normal document blocks (or list items): mark-free plain text paragraphs
    const inList = this.hasAncestorOfType($from, ['listItem']);
    let clean = normalizePastedPlainText(rawText, { collapseExcessBlankLines: true });
    if (inList) {
      clean = clean.replace(/\n{2,}/g, '\n');
    }
    if (!clean) return;

    if (!clean.includes('\n')) {
      const tr = state.tr
        .setStoredMarks([])
        .replaceSelectionWith(schema.text(clean, []), false)
        .scrollIntoView()
        .setMeta('paste', true)
        .setMeta('uiEvent', 'paste');
      wwView.dispatch(tr);
      this.focusEditorWithoutScroll();
      return;
    }

    const paraType = schema.nodes.paragraph;
    const lines = clean.split('\n');
    const paras = lines.map((line) =>
      paraType.create(null, line.length > 0 ? [schema.text(line, [])] : [])
    );
    const SliceCls = toastPmModelMod?.Slice ?? (selection.content().constructor as any);
    const FragCls = toastPmModelMod?.Fragment ?? (state.doc.content.constructor as any);
    const slice = new SliceCls(FragCls.from(paras), 1, 1);
    const tr = state.tr
      .setStoredMarks([])
      .replaceSelection(slice)
      .scrollIntoView()
      .setMeta('paste', true)
      .setMeta('uiEvent', 'paste');
    wwView.dispatch(tr);
    this.focusEditorWithoutScroll();
  }

  private async uploadPastedImageBlob(
    blob: Blob | File,
    suggestedExt?: string
  ): Promise<{ url: string; alt: string; relPath: string } | null> {
    const activeVault = this.callbacks.getActiveVault();
    if (!activeVault) {
      this.callbacks.onToast('열려 있는 볼트가 없어 이미지를 저장할 수 없습니다.', true);
      return null;
    }
    try {
      const ext =
        (suggestedExt || blob.type.split('/')[1] || 'png').replace(/[^a-z0-9]/gi, '') || 'png';
      this.imagePasteSeq = (this.imagePasteSeq + 1) % 10000;
      const rawName =
        blob instanceof File && blob.name && !/^image\.[a-z0-9]+$/i.test(blob.name)
          ? blob.name
          : `pasted-${Date.now()}-${this.imagePasteSeq}.${ext}`;
      const dataBase64 = await blobToBase64(blob);
      const res = await api.uploadAttachment(activeVault, rawName, dataBase64);
      return { url: res.media_url, alt: rawName, relPath: res.rel_path };
    } catch (err: any) {
      this.callbacks.onToast(err?.message || '이미지 업로드 실패', true);
      return null;
    }
  }

  private async insertPastedImageBlobs(blobs: Array<File | Blob>): Promise<void> {
    this.clearPseudoClipboardTextarea();
    const view = this.getActiveProseMirrorView();
    this.flushActiveImeComposition(view);

    let savedCount = 0;
    let lastRelPath = '';
    for (const blob of blobs) {
      const uploaded = await this.uploadPastedImageBlob(blob);
      if (!uploaded) continue;
      this.insertImageMarkdown(uploaded.url, uploaded.alt);
      savedCount++;
      lastRelPath = uploaded.relPath;
    }
    if (savedCount === 1) {
      this.callbacks.onToast(`첨부파일 저장됨: ${lastRelPath}`);
    } else if (savedCount > 1) {
      this.callbacks.onToast(`이미지 ${savedCount}개 첨부됨`);
    }
  }

  private async insertCleanSegments(segments: CleanPasteSegment[]): Promise<void> {
    if (!this.editor || segments.length === 0) return;
    this.clearPseudoClipboardTextarea();

    // Resolve any `data:image/...` URIs into clean vault attachment files first
    const resolved: Array<
      { kind: 'text'; text: string } | { kind: 'image'; url: string; alt: string }
    > = [];
    for (const seg of segments) {
      if (seg.kind === 'text') {
        const clean = normalizePastedPlainText(seg.text, { collapseExcessBlankLines: true });
        if (clean) resolved.push({ kind: 'text', text: clean });
      } else {
        if (seg.src.trim().toLowerCase().startsWith('data:image/')) {
          const decoded = dataUrlToImageBlob(seg.src);
          if (decoded) {
            const up = await this.uploadPastedImageBlob(decoded.blob, decoded.ext);
            if (up) {
              resolved.push({
                kind: 'image',
                url: up.url,
                alt: seg.alt && seg.alt !== 'image' ? seg.alt : up.alt,
              });
            }
          }
        } else {
          resolved.push({ kind: 'image', url: seg.src, alt: seg.alt || 'image' });
        }
      }
    }

    if (resolved.length === 0 || !this.editor) return;

    const anyEd = this.editor as any;
    if (!this.editor.isWysiwygMode()) {
      const mdBlocks = resolved.map((r) =>
        r.kind === 'text' ? r.text : `![${r.alt}](${r.url})`
      );
      const joined = mdBlocks.join('\n\n');
      const mdView = anyEd.mdEditor?.view;
      this.flushActiveImeComposition(mdView);
      this.preservePaneScroll(() => {
        if (anyEd.mdEditor?.replaceSelection) {
          anyEd.mdEditor.replaceSelection(joined);
        } else {
          this.editor?.insertText(joined);
        }
      });
      this.focusEditorWithoutScroll();
      return;
    }

    const wwView = anyEd.wwEditor?.view;
    if (!wwView) return;
    this.flushActiveImeComposition(wwView);

    const { state } = wwView;
    const { schema, selection } = state;
    const paraType = schema.nodes.paragraph;
    const imageType = schema.nodes.image;
    const paras: any[] = [];

    for (const item of resolved) {
      if (item.kind === 'text') {
        for (const line of item.text.split('\n')) {
          paras.push(paraType.create(null, line.length > 0 ? [schema.text(line, [])] : []));
        }
      } else if (imageType) {
        const imgNode = imageType.createAndFill({
          imageUrl: item.url,
          altText: item.alt,
        });
        if (imgNode) {
          paras.push(paraType.create(null, [imgNode]));
        }
      }
    }

    if (paras.length === 0) return;
    const SliceCls = toastPmModelMod?.Slice ?? (selection.content().constructor as any);
    const FragCls = toastPmModelMod?.Fragment ?? (state.doc.content.constructor as any);
    const slice = new SliceCls(FragCls.from(paras), 1, 1);
    const tr = state.tr
      .setStoredMarks([])
      .replaceSelection(slice)
      .scrollIntoView()
      .setMeta('paste', true)
      .setMeta('uiEvent', 'paste');
    wwView.dispatch(tr);
    this.focusEditorWithoutScroll();
  }

  private handleEditorPasteCapture = (ev: ClipboardEvent): void => {
    if (!this.editor) return;
    const target = ev.target as HTMLElement | null;
    if (!this.isPasteTargetEditorSurface(target)) return;

    const view = this.getActiveProseMirrorView();
    const forcePlain = Boolean(this.shiftHeldForPaste || view?.input?.shiftKey);
    const action = classifyClipboardData(ev.clipboardData, { forcePlainText: forcePlain });

    if (action.kind === 'internal' || action.kind === 'noop') {
      return;
    }

    ev.preventDefault();
    ev.stopPropagation();

    if (action.kind === 'image-blobs') {
      void this.insertPastedImageBlobs(action.blobs);
      return;
    }
    if (action.kind === 'segments') {
      void this.insertCleanSegments(action.segments);
      return;
    }
    if (action.kind === 'plain-text') {
      this.insertCleanPlainText(action.text);
    }
  };

  private handleEditorDropCapture = (ev: DragEvent): void => {
    if (!this.editor) return;
    const view = this.getActiveProseMirrorView();
    // Preserve internal editor selection dragging & native image/file drops
    if (view?.dragging) return;
    const dt = ev.dataTransfer;
    if (!dt) return;
    if (dt.files && dt.files.length > 0) return;

    const action = classifyClipboardData(dt, { forcePlainText: Boolean(ev.shiftKey) });
    if (action.kind === 'internal' || action.kind === 'noop') return;

    ev.preventDefault();
    ev.stopPropagation();

    if (view && typeof view.posAtCoords === 'function') {
      const coords = view.posAtCoords({ left: ev.clientX, top: ev.clientY });
      if (coords && typeof coords.pos === 'number') {
        const SelCls = view.state.selection.constructor as any;
        if (typeof SelCls?.near === 'function') {
          view.dispatch(view.state.tr.setSelection(SelCls.near(view.state.doc.resolve(coords.pos))));
        }
      }
    }

    if (action.kind === 'image-blobs') {
      void this.insertPastedImageBlobs(action.blobs);
    } else if (action.kind === 'segments') {
      void this.insertCleanSegments(action.segments);
    } else if (action.kind === 'plain-text') {
      this.insertCleanPlainText(action.text);
    }
  };

  private async pasteFromSystemClipboard(): Promise<void> {
    try {
      if (navigator.clipboard && typeof navigator.clipboard.read === 'function') {
        const items = await navigator.clipboard.read();
        for (const item of items) {
          const imgType = item.types.find((t) => t.toLowerCase().startsWith('image/'));
          const hasText = item.types.includes('text/plain');
          if (imgType && !hasText) {
            const blob = await item.getType(imgType);
            await this.insertPastedImageBlobs([blob]);
            return;
          }
          if (item.types.includes('text/html')) {
            const htmlBlob = await item.getType('text/html');
            const htmlText = await htmlBlob.text();
            if (/<img\b/i.test(htmlText)) {
              const segs = extractPlainTextAndImagesFromHtml(htmlText);
              if (segs.some((s) => s.kind === 'image')) {
                await this.insertCleanSegments(segs);
                return;
              }
            }
          }
          if (hasText) {
            const txtBlob = await item.getType('text/plain');
            const txt = await txtBlob.text();
            if (txt) {
              this.insertCleanPlainText(txt);
              return;
            }
          }
        }
      }
    } catch {
      // Fallback to readText below
    }

    try {
      const plain = await navigator.clipboard.readText();
      if (plain) {
        this.insertCleanPlainText(plain);
      }
    } catch {
      this.callbacks.onToast('클립보드 접근 권한이 없거나 비어 있습니다. Ctrl+V를 사용해 주세요.', true);
    }
  }

  /**
   * Moves TOAST UI's `.toastui-editor-toolbar` (plus its nested dropdown/popups)
   * and the inline find bar out of `#a4-paper-sheet` (`transform: scale(...)`)
   * and `#app-editor-pane` (`overflow-y: auto`) into `#app-editor-toolbar-dock`.
   * Also attaches a minimal 1-line `.mobile-quick-toolbar` above the full drawer.
   */
  private attachToolbarToDock(theme: 'light' | 'dark'): void {
    if (!this.toolbarDockEl) return;
    const uiEl = this.hostEl.querySelector('.toastui-editor-defaultUI');
    const toolbarEl = uiEl?.querySelector('.toastui-editor-toolbar') as HTMLElement | null;
    if (!toolbarEl) return;

    this.toolbarDockEl.innerHTML = '';
    this.toolbarDockEl.classList.remove('mobile-all-tools-open');
    this.toolbarDockEl.classList.toggle('toastui-editor-dark', theme === 'dark');
    this.toolbarDockEl.style.display = '';

    const quickBar = this.buildMobileQuickToolbar();
    this.toolbarDockEl.append(quickBar, toolbarEl);
    if (this.findBarEl) {
      this.toolbarDockEl.append(this.findBarEl);
    }
    this.toolbarEl = toolbarEl;

    this.popupClampObserver = new MutationObserver(() => {
      this.clampVisibleToolbarOverlays();
    });
    this.popupClampObserver.observe(toolbarEl, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['style', 'class'],
    });
    this.docToolbarResizeBound = () => this.clampVisibleToolbarOverlays();
    window.addEventListener('resize', this.docToolbarResizeBound);
  }

  private buildMobileQuickToolbar(): HTMLElement {
    const bar = document.createElement('div');
    bar.className = 'mobile-quick-toolbar';

    let headingStep = 0;
    const addQ = (
      label: string,
      title: string,
      onClick: (btn: HTMLButtonElement) => void,
      extraCls = '',
      attr?: [string, string]
    ) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `mobile-qbtn${extraCls ? ` ${extraCls}` : ''}`;
      btn.textContent = label;
      btn.title = title;
      if (attr) btn.setAttribute(attr[0], attr[1]);
      btn.addEventListener('click', () => onClick(btn));
      bar.appendChild(btn);
    };

    addQ('↩', '실행 취소', () => this.execHistoryStep('undo'));
    addQ('↪', '다시 실행', () => this.execHistoryStep('redo'));
    addQ('H', '제목 크기 순환 (H1/H2/H3/본문)', () => {
      const levels = [1, 2, 3, 0];
      const level = levels[headingStep % levels.length];
      headingStep++;
      this.execEditorCommand('heading', { level });
    });
    addQ('B', '굵게', () => this.execEditorCommand('bold'));
    addQ('I', '기울임', () => this.execEditorCommand('italic'));
    addQ('☑️', '체크리스트', () => this.execEditorCommand('task'));
    addQ('•', '글머리 목록', () => this.execEditorCommand('bulletList'));
    addQ('[[ ]]', '노트 링크 연결', () => {
      this.bracketTriggeredFromTyping = false;
      this.callbacks.onRequestWikiLinkPicker(false);
    });
    addQ('🔀', '플로우차트 빌더', () => this.openFlowchartBuilderModal());
    addQ('🔍', '본문 찾기 / 바꾸기', () => {
      if (this.findBarEl && this.findBarEl.style.display !== 'none') {
        this.closeFindBar();
      } else {
        this.openFindBar(false);
      }
    });
    addQ(
      '🛠️ 도구 ▾',
      '전체 서식 도구 펼치기 / 접기',
      (btn) => {
        if (!this.toolbarDockEl) return;
        const opened = this.toolbarDockEl.classList.toggle('mobile-all-tools-open');
        btn.classList.toggle('active', opened);
        btn.textContent = opened ? '▲ 도구접기' : '🛠️ 도구 ▾';
        if (opened) {
          requestAnimationFrame(() => window.dispatchEvent(new Event('resize')));
        }
      },
      'mobile-qbtn-tools',
      ['data-mobile-more-tools', '1']
    );

    return bar;
  }

  /** Keeps visible `.toastui-editor-popup` dialogs within viewport and dock bounds. */
  private clampVisibleToolbarOverlays(): void {
    if (this.clampingOverlays || !this.toolbarDockEl || !this.toolbarEl) return;
    this.clampingOverlays = true;
    try {
      const dockRect = this.toolbarDockEl.getBoundingClientRect();
      const pad = 8;
      const minLeft = Math.max(pad, dockRect.left + 4);
      const maxRight = Math.min(window.innerWidth - pad, dockRect.right - 4);
      this.toolbarEl
        .querySelectorAll<HTMLElement>('.toastui-editor-popup')
        .forEach((popup) => {
          if (popup.style.display === 'none' || popup.offsetParent === null) return;
          const rect = popup.getBoundingClientRect();
          if (rect.width <= 0) return;
          let dx = 0;
          if (rect.right > maxRight) {
            dx -= rect.right - maxRight;
          }
          if (rect.left + dx < minLeft) {
            dx += minLeft - (rect.left + dx);
          }
          if (Math.abs(dx) >= 1) {
            const curLeft = parseFloat(popup.style.left || '0') || 0;
            popup.style.left = `${Math.round(curLeft + dx)}px`;
          }
        });
    } finally {
      this.clampingOverlays = false;
    }
  }

  private buildFindBarDom(bar: HTMLElement): void {
    bar.innerHTML = `
      <input type="text" class="text-input input-sm" data-find-q placeholder="찾을 텍스트…" />
      <button type="button" class="btn btn-sm" data-find-case title="대소문자 구분">Aa</button>
      <span class="editor-find-count" data-find-count>0 / 0</span>
      <button type="button" class="btn btn-sm" data-find-prev title="이전 찾기">▲</button>
      <button type="button" class="btn btn-sm" data-find-next title="다음 찾기 (Enter)">▼</button>
      <input type="text" class="text-input input-sm" data-replace-q placeholder="바꿀 텍스트…" />
      <button type="button" class="btn btn-sm" data-replace-one>바꾸기</button>
      <button type="button" class="btn btn-sm" data-replace-all>모두 바꾸기</button>
      <button type="button" class="icon-btn" data-find-close title="닫기 (Esc)">✕</button>
    `;

    this.findInputEl = bar.querySelector('[data-find-q]') as HTMLInputElement;
    this.replaceInputEl = bar.querySelector('[data-replace-q]') as HTMLInputElement;
    this.findCountEl = bar.querySelector('[data-find-count]') as HTMLElement;
    const caseBtn = bar.querySelector('[data-find-case]') as HTMLButtonElement;

    this.findInputEl.addEventListener('input', () => {
      this.lastMatchIdx = -1;
      this.refreshFindMatchCount();
    });
    this.findInputEl.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter') {
        ev.preventDefault();
        this.jumpFindMatch(ev.shiftKey ? -1 : 1);
      } else if (ev.key === 'Escape') {
        this.closeFindBar();
      }
    });
    this.replaceInputEl.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape') {
        this.closeFindBar();
      }
    });
    caseBtn.addEventListener('click', () => {
      this.findCaseSensitive = !this.findCaseSensitive;
      caseBtn.classList.toggle('btn-primary', this.findCaseSensitive);
      this.lastMatchIdx = -1;
      this.refreshFindMatchCount();
    });

    bar.querySelector('[data-find-prev]')?.addEventListener('click', () => this.jumpFindMatch(-1));
    bar.querySelector('[data-find-next]')?.addEventListener('click', () => this.jumpFindMatch(1));
    bar.querySelector('[data-replace-one]')?.addEventListener('click', () => this.replaceCurrentMatch());
    bar.querySelector('[data-replace-all]')?.addEventListener('click', () => this.replaceAllMatches());
    bar.querySelector('[data-find-close]')?.addEventListener('click', () => this.closeFindBar());
  }

  public openFindBar(focusReplace = false): void {
    if (!this.findBarEl) return;
    this.findBarEl.style.display = 'flex';
    const view = this.getActiveProseMirrorView();
    if (view && this.findInputEl) {
      const { from, to } = view.state.selection;
      if (to > from && to - from < 120) {
        const selected = view.state.doc.textBetween(from, to, ' ');
        if (selected.trim()) {
          this.findInputEl.value = selected;
        }
      }
    }
    this.refreshFindMatchCount();
    setTimeout(() => {
      if (focusReplace && this.findInputEl?.value && this.replaceInputEl) {
        this.replaceInputEl.focus();
      } else {
        this.findInputEl?.focus();
        this.findInputEl?.select();
      }
    }, 10);
  }

  public closeFindBar(): void {
    if (this.findBarEl) {
      this.findBarEl.style.display = 'none';
    }
    this.focusEditorWithoutScroll();
  }

  private collectProseMirrorMatches(query: string): TextMatchRange[] {
    const view = this.getActiveProseMirrorView();
    if (!view || !query) return [];
    const needle = this.findCaseSensitive ? query : query.toLowerCase();
    const matches: TextMatchRange[] = [];

    view.state.doc.descendants((node: any, pos: number) => {
      if (!node.isText || !node.text) return;
      const hay = this.findCaseSensitive ? node.text : node.text.toLowerCase();
      let idx = 0;
      while ((idx = hay.indexOf(needle, idx)) !== -1) {
        matches.push({ from: pos + idx, to: pos + idx + query.length });
        idx += Math.max(1, query.length);
      }
    });
    return matches;
  }

  private refreshFindMatchCount(): void {
    const q = this.findInputEl?.value || '';
    const matches = this.collectProseMirrorMatches(q);
    if (this.findCountEl) {
      if (matches.length === 0) {
        this.findCountEl.textContent = q ? '0 / 0' : '0건';
      } else {
        const activeIdx =
          this.lastMatchIdx >= 0 && this.lastMatchIdx < matches.length ? this.lastMatchIdx + 1 : 0;
        this.findCountEl.textContent = `${activeIdx || 1} / ${matches.length}`;
      }
    }
  }

  private jumpFindMatch(delta: number): void {
    const view = this.getActiveProseMirrorView();
    const q = this.findInputEl?.value || '';
    const matches = this.collectProseMirrorMatches(q);
    if (!view || matches.length === 0) {
      this.refreshFindMatchCount();
      return;
    }

    if (this.lastMatchIdx < 0 || this.lastMatchIdx >= matches.length) {
      this.lastMatchIdx = delta >= 0 ? 0 : matches.length - 1;
    } else {
      this.lastMatchIdx = (this.lastMatchIdx + delta + matches.length) % matches.length;
    }

    const m = matches[this.lastMatchIdx];
    const SelClass = view.state.selection.constructor as any;
    const tr = view.state.tr.setSelection(SelClass.create(view.state.doc, m.from, m.to));
    view.dispatch(tr.scrollIntoView());
    this.refreshFindMatchCount();
  }

  private replaceCurrentMatch(): void {
    const view = this.getActiveProseMirrorView();
    const q = this.findInputEl?.value || '';
    if (!view || !q) return;
    const rep = this.replaceInputEl?.value ?? '';
    const { from, to } = view.state.selection;
    const curText = view.state.doc.textBetween(from, to, '\n');
    const isSame = this.findCaseSensitive
      ? curText === q
      : curText.toLowerCase() === q.toLowerCase();

    if (isSame) {
      const tr = view.state.tr.insertText(rep, from, to);
      const SelClass = view.state.selection.constructor as any;
      tr.setSelection(SelClass.create(tr.doc, from + rep.length, from + rep.length));
      view.dispatch(tr.scrollIntoView());
      this.lastMatchIdx = Math.max(-1, this.lastMatchIdx - 1);
      this.jumpFindMatch(1);
    } else {
      this.jumpFindMatch(1);
    }
  }

  private replaceAllMatches(): void {
    const view = this.getActiveProseMirrorView();
    const q = this.findInputEl?.value || '';
    if (!view || !q) return;
    const rep = this.replaceInputEl?.value ?? '';
    const matches = this.collectProseMirrorMatches(q);
    if (matches.length === 0) {
      this.callbacks.onToast('일치하는 항목이 없습니다.');
      return;
    }

    let tr = view.state.tr;
    for (let i = matches.length - 1; i >= 0; i--) {
      tr = tr.insertText(rep, matches[i].from, matches[i].to);
    }
    view.dispatch(tr);
    this.lastMatchIdx = -1;
    this.refreshFindMatchCount();
    this.callbacks.onToast(`${matches.length}건 모두 바꾸기 완료`);
  }

  private formatTodayDate(kind: 'isoDate' | 'koreanFull' | 'time' | 'isoFull'): string {
    const now = new Date();
    const y = now.getFullYear();
    const m = String(now.getMonth() + 1).padStart(2, '0');
    const d = String(now.getDate()).padStart(2, '0');
    const hh = String(now.getHours()).padStart(2, '0');
    const mm = String(now.getMinutes()).padStart(2, '0');
    const days = ['일', '월', '화', '수', '목', '금', '토'];
    const dow = days[now.getDay()];

    if (kind === 'isoDate') return `${y}-${m}-${d}`;
    if (kind === 'time') return `${hh}:${mm}`;
    if (kind === 'koreanFull') return `${y}년 ${m}월 ${d}일 (${dow})`;
    return `${y}-${m}-${d} ${hh}:${mm}`;
  }

  private insertInlineText(text: string): void {
    if (!this.editor) return;
    const trimmed = text.trim();
    if (
      this.editor.isWysiwygMode() &&
      (trimmed.startsWith('<span') || trimmed.startsWith('<mark'))
    ) {
      this.insertInlineHtmlViaMarker(trimmed);
      return;
    }
    this.preservePaneScroll(() => {
      this.editor?.insertText(text);
    });
    this.focusEditorWithoutScroll();
  }

  private insertInlineHtmlViaMarker(htmlToken: string): void {
    if (!this.editor) return;
    this.preservePaneScroll(() => {
      if (!this.editor) return;
      const marker = `__OBS_INLINE_${Date.now()}_${Math.floor(Math.random() * 10000)}__`;
      this.editor.insertText(marker);
      const md = this.editor.getMarkdown();
      if (md.includes(marker)) {
        const next = md.replace(marker, htmlToken);
        this.editor.setMarkdown(next, false);
        this.callbacks.onMarkdownChange(dehydrateMarkdownMediaUrls(next));
      }
    });
    this.focusEditorWithoutScroll();
  }

  public insertBlockSnippet(snippet: string): void {
    if (!this.editor) return;
    const clean = snippet.trim();
    if (!this.editor.isWysiwygMode()) {
      this.preservePaneScroll(() => {
        this.editor?.insertText(`\n\n${clean}\n\n`);
      });
      this.focusEditorWithoutScroll();
      return;
    }

    this.preservePaneScroll(() => {
      if (!this.editor) return;
      const marker = `__OBS_BLOCK_${Date.now()}_${Math.floor(Math.random() * 10000)}__`;
      this.editor.insertText(marker);
      const md = this.editor.getMarkdown();
      if (md.includes(marker)) {
        const next = md.replace(marker, `\n\n${clean}\n\n`);
        this.editor.setMarkdown(next, false);
        this.callbacks.onMarkdownChange(dehydrateMarkdownMediaUrls(next));
      } else {
        const appended = `${md.trimEnd()}\n\n${clean}\n\n`;
        this.editor.setMarkdown(appended, false);
        this.callbacks.onMarkdownChange(dehydrateMarkdownMediaUrls(appended));
      }
    });
    this.focusEditorWithoutScroll();
  }

  private wrapSelectionWithBlockAlign(align: string): void {
    const view = this.getActiveProseMirrorView();
    let selected = '정렬할 문단 내용을 입력하세요.';
    if (view) {
      const { from, to } = view.state.selection;
      const picked = view.state.doc.textBetween(from, to, '\n').trim();
      if (picked) selected = picked;
    }
    this.insertBlockSnippet(`<div style="text-align: ${align};">\n\n${selected}\n\n</div>`);
  }

  private insertAutoToc(): void {
    if (!this.editor) return;
    const md = this.editor.getMarkdown();
    const lines = md.split('\n');
    const items: string[] = [];
    let inFence = false;
    for (const line of lines) {
      if (line.trim().startsWith('```')) {
        inFence = !inFence;
        continue;
      }
      if (inFence) continue;
      const m = /^(#{1,4})\s+(.+)$/.exec(line.trim());
      if (m) {
        const lvl = m[1].length;
        const txt = m[2].trim();
        items.push(`${'  '.repeat(lvl - 1)}- ${txt}`);
      }
    }
    if (items.length === 0) {
      this.callbacks.onToast('제목(#, ##, ###)이 없어 목차를 생성할 수 없습니다.', true);
      return;
    }
    const tocBlock = `<div class="doc-toc-box">\n\n**📑 목차**\n\n${items.join('\n')}\n\n</div>`;
    this.insertBlockSnippet(tocBlock);
    this.callbacks.onToast(`제목 ${items.length}개 기반 목차를 삽입했습니다.`);
  }

  private insertFootnote(): void {
    if (!this.editor) return;
    this.preservePaneScroll(() => {
      if (!this.editor) return;
      const md = this.editor.getMarkdown();
      while (md.includes(`[^${this.footnoteSeq}]`)) {
        this.footnoteSeq++;
      }
      const n = this.footnoteSeq++;
      this.editor.insertText(`[^${n}]`);
      const nextMd = `${this.editor.getMarkdown().trimEnd()}\n\n[^${n}]: 각주 설명을 입력하세요.\n`;
      this.editor.setMarkdown(nextMd, false);
      this.callbacks.onMarkdownChange(dehydrateMarkdownMediaUrls(nextMd));
    });
    this.focusEditorWithoutScroll();
  }

  private transformSelectionText(
    mode:
      | 'upper'
      | 'lower'
      | 'title'
      | 'sortAsc'
      | 'sortDesc'
      | 'dedupe'
      | 'trimLines'
      | 'numberLines'
  ): void {
    const view = this.getActiveProseMirrorView();
    if (!view) return;
    const { from, to } = view.state.selection;
    if (to <= from) {
      this.callbacks.onToast('먼저 변환할 텍스트를 드래그해 선택하세요.', true);
      return;
    }
    const raw: string = view.state.doc.textBetween(from, to, '\n');
    let out = raw;
    if (mode === 'upper') {
      out = raw.toUpperCase();
    } else if (mode === 'lower') {
      out = raw.toLowerCase();
    } else if (mode === 'title') {
      out = raw.replace(/\b([a-z])/gi, (c: string) => c.toUpperCase());
    } else if (mode === 'sortAsc') {
      out = raw
        .split('\n')
        .sort((a: string, b: string) => a.localeCompare(b, 'ko'))
        .join('\n');
    } else if (mode === 'sortDesc') {
      out = raw
        .split('\n')
        .sort((a: string, b: string) => b.localeCompare(a, 'ko'))
        .join('\n');
    } else if (mode === 'dedupe') {
      out = Array.from(new Set(raw.split('\n'))).join('\n');
    } else if (mode === 'trimLines') {
      out = raw
        .split('\n')
        .map((l: string) => l.trim())
        .filter((l: string, i: number, arr: string[]) => l.length > 0 || (i > 0 && arr[i - 1].length > 0))
        .join('\n');
    } else if (mode === 'numberLines') {
      out = raw
        .split('\n')
        .map((l: string, idx: number) => `${idx + 1}. ${l.replace(/^\d+\.\s+/, '')}`)
        .join('\n');
    }

    view.dispatch(view.state.tr.insertText(out, from, to));
    this.focusEditorWithoutScroll();
  }

  private async copyCleanMarkdown(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.getCleanMarkdown());
      this.callbacks.onToast('마크다운 본문을 클립보드에 복사했습니다.');
    } catch {
      this.callbacks.onToast('클립보드 복사에 실패했습니다.', true);
    }
  }

  public openFlowchartBuilderModal(
    existingCode?: string,
    applyExisting?: (nextCode: string) => void
  ): void {
    openFlowchartStudioModal({
      initialCode: existingCode || FLOWCHART_TEMPLATES[0].code,
      onToast: (m, err) => this.callbacks.onToast(m, err),
      onApply: (nextCode) => {
        if (applyExisting) {
          applyExisting(nextCode);
        } else {
          this.insertBlockSnippet(`\`\`\`mermaid\n${nextCode}\n\`\`\``);
        }
      },
    });
  }

  private replaceDiagramFenceInMarkdown(
    prevCode: string,
    diagramIdx: number,
    nextCode: string
  ): void {
    if (!this.editor) return;
    const rawMd = this.editor.getMarkdown();
    const fences = findDiagramFencesInMarkdown(rawMd);
    if (fences.length === 0) return;

    const trimmedPrev = prevCode.trim();
    const matched =
      fences.find((f) => f.body.trim() === trimmedPrev) ||
      fences[diagramIdx] ||
      fences[0];
    if (!matched) return;

    const normMd = rawMd.replace(/\r\n?/g, '\n');
    const updatedMd = `${normMd.slice(0, matched.bodyStart)}${nextCode}${normMd.slice(matched.bodyEnd)}`;
    this.suppressChange = true;
    try {
      this.preservePaneScroll(() => {
        this.editor?.setMarkdown(updatedMd, false);
      });
    } finally {
      this.suppressChange = false;
    }
    this.callbacks.onMarkdownChange(dehydrateMarkdownMediaUrls(updatedMd));
  }

  private applyCodeUpdateToDiagramCard(
    cardEl: HTMLElement,
    nextCode: string
  ): void {
    if (!this.editor) return;
    const wwWidget = cardEl.closest('.onyx-ww-diagram-widget') as HTMLElement | null;
    if (this.editor.isWysiwygMode() && wwWidget) {
      const pmPos = Number(wwWidget.getAttribute('data-pm-pos'));
      const view = this.getActiveProseMirrorView();
      if (view && !Number.isNaN(pmPos)) {
        const node = view.state.doc.nodeAt(pmPos);
        if (node && node.type?.name === 'codeBlock') {
          const textNode = nextCode ? view.state.schema.text(nextCode) : null;
          let tr = view.state.tr.replaceWith(
            pmPos + 1,
            pmPos + node.nodeSize - 1,
            textNode ? [textNode] : []
          );
          if (!node.attrs?.language) {
            tr = tr.setNodeMarkup(pmPos, undefined, { ...node.attrs, language: 'mermaid' });
          }
          view.dispatch(tr);
          return;
        }
      }
    }

    const prevCode = decodeFlowCode(cardEl.getAttribute('data-flow-code') || '');
    const diagIdx = Number(cardEl.getAttribute('data-diagram-index') || '0') || 0;
    this.replaceDiagramFenceInMarkdown(prevCode, diagIdx, nextCode);
  }

  private handleHostClick = (ev: MouseEvent): void => {
    const target = ev.target as HTMLElement | null;
    if (!target) return;

    // 1) One-click convert bare `flowchart TB ...` paragraphs in WYSIWYG into a mermaid codeBlock
    const convertBtn = target.closest('[data-flow-convert-start]') as HTMLElement | null;
    if (convertBtn && this.editor?.isWysiwygMode()) {
      ev.preventDefault();
      ev.stopPropagation();
      const startPos = Number(convertBtn.getAttribute('data-flow-convert-start'));
      const endPos = Number(convertBtn.getAttribute('data-flow-convert-end'));
      const view = this.getActiveProseMirrorView();
      if (view && !Number.isNaN(startPos) && !Number.isNaN(endPos) && endPos > startPos) {
        const lines: string[] = [];
        view.state.doc.nodesBetween(startPos, endPos, (node: any) => {
          if (node.type?.name === 'paragraph') {
            lines.push((node.textContent || '').trim());
          }
        });
        const joined = lines.filter(Boolean).join('\n');
        const cbType = view.state.schema.nodes.codeBlock;
        if (joined && cbType) {
          const cbNode = cbType.create({ language: 'mermaid' }, view.state.schema.text(joined));
          view.dispatch(view.state.tr.replaceWith(startPos, endPos, cbNode));
          this.callbacks.onToast('플로우차트 코드 블록으로 변환했습니다.');
        }
      }
      return;
    }

    // 2) Flowchart / Diagram interactive card toolbar actions
    const flowBtn = target.closest('[data-flow-action]') as HTMLElement | null;
    if (flowBtn) {
      ev.preventDefault();
      ev.stopPropagation();
      const cardEl = flowBtn.closest('.onyx-flow-card') as HTMLElement | null;
      if (!cardEl) return;
      const action = flowBtn.getAttribute('data-flow-action');
      const curCode = decodeFlowCode(cardEl.getAttribute('data-flow-code') || '');

      if (action === 'zoom-in' || action === 'zoom-out' || action === 'zoom-reset') {
        const curZoom = Number(cardEl.getAttribute('data-flow-zoom') || '100') || 100;
        const nextZoom =
          action === 'zoom-reset'
            ? 100
            : Math.max(50, Math.min(200, curZoom + (action === 'zoom-in' ? 15 : -15)));
        cardEl.setAttribute('data-flow-zoom', String(nextZoom));
        const lbl = cardEl.querySelector('.onyx-flow-zoom-label');
        if (lbl) lbl.textContent = `${nextZoom}%`;
        const stage = cardEl.querySelector('.onyx-flow-stage') as HTMLElement | null;
        if (stage) {
          (stage.style as any).zoom = nextZoom === 100 ? '' : (nextZoom / 100).toFixed(2);
        }
        return;
      }

      if (action === 'copy-img') {
        void copyDiagramAsPngOrSvg(cardEl, this.callbacks.onToast);
        return;
      }

      if (action === 'dir') {
        const nextCode = toggleFlowchartDirection(curCode);
        if (nextCode !== curCode) {
          this.applyCodeUpdateToDiagramCard(cardEl, nextCode);
        }
        return;
      }

      if (action === 'curve') {
        const { nextCode } = toggleFlowchartCurve(curCode);
        this.applyCodeUpdateToDiagramCard(cardEl, nextCode);
        return;
      }

      if (action === 'theme') {
        const { nextCode } = cycleFlowchartTheme(curCode);
        this.applyCodeUpdateToDiagramCard(cardEl, nextCode);
        return;
      }

      if (action === 'toggle-code') {
        const wwWidget = cardEl.closest('.onyx-ww-diagram-widget') as HTMLElement | null;
        const view = this.getActiveProseMirrorView();
        if (this.editor?.isWysiwygMode() && wwWidget && view) {
          const pmPos = Number(wwWidget.getAttribute('data-pm-pos'));
          const pKey = getOnyxDiagramPluginKey(view);
          if (pKey && !Number.isNaN(pmPos)) {
            const node = view.state.doc.nodeAt(pmPos);
            const pState = pKey.getState(view.state) || { expanded: [] };
            const selFrom = view.state.selection.from;
            const selTo = view.state.selection.to;
            const cursorInside =
              node &&
              selFrom > pmPos &&
              selFrom < pmPos + node.nodeSize &&
              selTo >= pmPos &&
              selTo <= pmPos + node.nodeSize;
            const wasOpen = Boolean(cursorInside || pState.expanded.includes(pmPos));

            let tr = view.state.tr;
            if (wasOpen) {
              tr = tr.setMeta(pKey, { closePos: pmPos });
              if (cursorInside && node) {
                const afterPos = Math.min(tr.doc.content.size, pmPos + node.nodeSize);
                const SelCls = view.state.selection.constructor as any;
                if (typeof SelCls?.near === 'function') {
                  tr = tr.setSelection(SelCls.near(tr.doc.resolve(afterPos)));
                }
              }
              view.dispatch(tr);
            } else {
              tr = tr.setMeta(pKey, { openPos: pmPos });
              if (node && node.nodeSize >= 2) {
                const SelCls = view.state.selection.constructor as any;
                if (typeof SelCls?.near === 'function') {
                  tr = tr.setSelection(SelCls.near(tr.doc.resolve(pmPos + 1)));
                }
              }
              view.dispatch(tr);
              this.focusEditorWithoutScroll();
            }
            return;
          }
        }
        this.focusEditorWithoutScroll();
        return;
      }

      if (action === 'builder') {
        this.openFlowchartBuilderModal(curCode, (updated) => {
          this.applyCodeUpdateToDiagramCard(cardEl, updated);
          this.callbacks.onToast('플로우차트 변경사항을 문서에 반영했습니다.');
        });
        return;
      }
    }

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
    return offset >= 2 && text.slice(offset - 2, offset) === '[[';
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
      const wwView = (this.editor as any).wwEditor?.view;
      const imageType = wwView?.state?.schema?.nodes?.image;
      if (wwView && imageType) {
        this.flushActiveImeComposition(wwView);
        const imgNode = imageType.createAndFill({
          imageUrl: mediaUrl,
          altText: alt || 'image',
        });
        if (imgNode) {
          const tr = wwView.state.tr
            .setStoredMarks([])
            .replaceSelectionWith(imgNode, false)
            .scrollIntoView();
          wwView.dispatch(tr);
          this.focusEditorWithoutScroll();
          return;
        }
      }
      this.editor.exec('addImage', { imageUrl: mediaUrl, altText: alt });
    } else {
      this.editor.insertText(`![${alt}](${mediaUrl})`);
    }
    this.focusEditorWithoutScroll();
  }

  public insertAttachmentLink(label: string, mediaUrl: string, isImage = false): void {
    if (!this.editor) return;
    if (isImage) {
      this.insertImageMarkdown(mediaUrl, label);
      return;
    }
    const display = `📎 ${label}`;
    if (this.editor.isWysiwygMode()) {
      this.editor.exec('addLink', { linkUrl: mediaUrl, linkText: display });
    } else {
      this.editor.insertText(`[${display}](${mediaUrl})`);
    }
    this.focusEditorWithoutScroll();
  }

  public insertWikiLink(noteTitle: string): void {
    if (!this.editor) return;
    const token = `[[${noteTitle}]]`;

    if (this.bracketTriggeredFromTyping) {
      this.bracketTriggeredFromTyping = false;
      const activeEditor = this.getActiveProseMirrorView();
      if (activeEditor) {
        const { state, dispatch } = activeEditor;
        const pos = state.selection.from;
        if (pos >= 2) {
          const prevTwo = state.doc.textBetween(pos - 2, pos, '');
          if (prevTwo === '[[') {
            const tr = state.tr.insertText(token, pos - 2, pos);
            dispatch(tr);
            this.focusEditorWithoutScroll();
            return;
          }
        }
      }
    }

    this.editor.insertText(token);
    this.focusEditorWithoutScroll();
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

  private execHistoryStep(kind: 'undo' | 'redo'): void {
    const view = this.getActiveProseMirrorView();
    if (!view) return;
    if (kind === 'undo') {
      undo(view.state, view.dispatch);
    } else {
      redo(view.state, view.dispatch);
    }
    this.focusEditorWithoutScroll();
  }

  private execEditorCommand(cmd: string, payload?: Record<string, any>): void {
    if (!this.editor) return;
    this.preservePaneScroll(() => {
      try {
        this.editor?.exec(cmd, payload);
      } catch {
        // fallback ignore for mode-specific commands
      }
    });
    this.focusEditorWithoutScroll();
  }

  private insertTextOrSnippet(text: string): void {
    if (!this.editor) return;
    this.preservePaneScroll(() => {
      this.editor?.insertText(text);
    });
    this.focusEditorWithoutScroll();
  }

  private attachEditorContextMenu(): void {
    if (!this.editorMountEl) return;
    this.editorMountEl.addEventListener('contextmenu', (ev: MouseEvent) => {
      // Ctrl/Shift+우클릭 시에는 브라우저 기본 맞춤법 메뉴 허용
      if (ev.ctrlKey || ev.shiftKey) return;
      if (!this.editor) return;
      ev.preventDefault();
      ev.stopPropagation();
      this.openEditorContextMenu(ev);
    });
  }

  private closeEditorContextMenu(): void {
    if (this.contextMenuEl) {
      this.contextMenuEl.remove();
      this.contextMenuEl = null;
    }
    if (this.ctxDocClickBound) {
      document.removeEventListener('mousedown', this.ctxDocClickBound, true);
      window.removeEventListener('resize', this.ctxDocClickBound);
      this.ctxDocClickBound = null;
    }
    if (this.ctxDocKeyBound) {
      document.removeEventListener('keydown', this.ctxDocKeyBound, true);
      this.ctxDocKeyBound = null;
    }
  }

  private openEditorContextMenu(ev: MouseEvent): void {
    this.closeEditorContextMenu();
    if (!this.editor) return;

    const target = ev.target as HTMLElement | null;
    const isWysiwyg = this.editor.isWysiwygMode();
    const inTableCell = Boolean(target?.closest('td, th'));
    const hasSelection = (window.getSelection()?.toString() || '').length > 0;

    const menu = document.createElement('div');
    menu.className = 'editor-ctx-menu';
    menu.setAttribute('role', 'menu');

    // Top Quick Formatting Bar
    const quickBar = document.createElement('div');
    quickBar.className = 'editor-ctx-quickbar';
    const quickActions: { label: string; title: string; run: () => void }[] = [
      { label: '↩', title: '실행 취소 (Ctrl+Z)', run: () => this.execHistoryStep('undo') },
      { label: '↪', title: '다시 실행 (Ctrl+Y)', run: () => this.execHistoryStep('redo') },
      { label: 'B', title: '굵게 (Ctrl+B)', run: () => this.execEditorCommand('bold') },
      { label: 'I', title: '기울임 (Ctrl+I)', run: () => this.execEditorCommand('italic') },
      { label: 'U', title: '밑줄', run: () => this.execEditorCommand('toggleUnderline') },
      { label: 'S', title: '취소선', run: () => this.execEditorCommand('strike') },
      {
        label: '==',
        title: '노란 형광펜',
        run: () => this.execEditorCommand('setHighlight', { color: 'rgba(250, 204, 21, 0.45)' }),
      },
      { label: '<>', title: '인라인 코드', run: () => this.execEditorCommand('code') },
      { label: 'Tx', title: '서식 지우기', run: () => this.execEditorCommand('clearInlineStyle') },
    ];
    for (const qa of quickActions) {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = 'editor-ctx-qbtn';
      b.textContent = qa.label;
      b.title = qa.title;
      b.addEventListener('click', () => {
        qa.run();
        this.closeEditorContextMenu();
      });
      quickBar.appendChild(b);
    }
    menu.appendChild(quickBar);

    const addSection = (title: string) => {
      const s = document.createElement('div');
      s.className = 'editor-ctx-section';
      s.textContent = title;
      menu.appendChild(s);
    };
    const addSep = () => {
      const d = document.createElement('div');
      d.className = 'editor-ctx-sep';
      menu.appendChild(d);
    };
    const addItem = (icon: string, label: string, onClick: () => void, danger = false) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.className = `editor-ctx-item${danger ? ' danger' : ''}`;
      b.innerHTML = `<span style="width:18px;text-align:center;">${icon}</span><span>${label}</span>`;
      b.addEventListener('click', () => {
        onClick();
        this.closeEditorContextMenu();
      });
      menu.appendChild(b);
    };

    if (isWysiwyg && inTableCell) {
      addSection('표 편집 도구');
      addItem('⬆️', '위에 행 추가', () => this.execEditorCommand('addRowToUp'));
      addItem('⬇️', '아래에 행 추가', () => this.execEditorCommand('addRowToDown'));
      addItem('⬅️', '왼쪽에 열 추가', () => this.execEditorCommand('addColumnToLeft'));
      addItem('➡️', '오른쪽에 열 추가', () => this.execEditorCommand('addColumnToRight'));
      addItem('➖', '현재 행 삭제', () => this.execEditorCommand('removeRow'), true);
      addItem('⏐', '현재 열 삭제', () => this.execEditorCommand('removeColumn'), true);
      addItem('🗑️', '표 전체 삭제', () => this.execEditorCommand('removeTable'), true);
      addSep();
    }

    addSection('제목 · 문단 정렬');
    addItem('H1', '제목 1', () => this.execEditorCommand('heading', { level: 1 }));
    addItem('H2', '제목 2', () => this.execEditorCommand('heading', { level: 2 }));
    addItem('H3', '제목 3', () => this.execEditorCommand('heading', { level: 3 }));
    addItem('¶', '기본 본문 (10pt)', () => this.execEditorCommand('heading', { level: 0 }));
    addItem('⬅', '왼쪽 정렬', () => this.execEditorCommand('setTextAlign', { align: 'left' }));
    addItem('⬌', '가운데 정렬', () => this.execEditorCommand('setTextAlign', { align: 'center' }));
    addItem('➡', '오른쪽 정렬', () => this.execEditorCommand('setTextAlign', { align: 'right' }));

    addSep();
    addSection('블록 · 요소 삽입');
    addItem('📊', '3×3 표 삽입', () =>
      this.execEditorCommand('addTable', { columnCount: 3, rowCount: 3 })
    );
    addItem('☑️', '체크리스트', () => this.execEditorCommand('task'));
    addItem('•', '글머리 목록', () => this.execEditorCommand('bulletList'));
    addItem('💡', '콜아웃 메모 블록', () =>
      this.execEditorCommand('insertCallout', { calloutType: 'note', calloutTitle: '메모' })
    );
    addItem('▸', '접기 / 펼치기 토글', () => {
      if (isWysiwyg) {
        this.preservePaneScroll(() => {
          this.editor?.setHTML(
            `${this.editor!.getHTML()}<details open><summary>토글 제목</summary><p>접기/펼치기 내용을 입력하세요.</p></details>`,
            false
          );
        });
      } else {
        this.insertTextOrSnippet(
          '\n<details open>\n<summary>토글 제목</summary>\n\n내용을 입력하세요.\n\n</details>\n'
        );
      }
    });
    addItem('💻', '코드 블록', () => this.execEditorCommand('codeBlock'));
    addItem('🔀', '시각적 플로우차트 빌더 / 삽입…', () => {
      const selTxt = window.getSelection()?.toString() || '';
      const init = looksLikeBareFlowchartText(selTxt) ? selTxt.trim() : undefined;
      this.openFlowchartBuilderModal(init);
    });
    if (hasSelection && looksLikeBareFlowchartText(window.getSelection()?.toString() || '')) {
      addItem('✨', '선택 텍스트 → 플로우차트 블록 변환', () => {
        const view = this.getActiveProseMirrorView();
        const selTxt = (window.getSelection()?.toString() || '').trim();
        if (!view || !selTxt) return;
        const { from, to } = view.state.selection;
        const cbType = view.state.schema.nodes.codeBlock;
        if (this.editor?.isWysiwygMode() && cbType) {
          view.dispatch(
            view.state.tr.replaceRangeWith(
              from,
              to,
              cbType.create({ language: 'mermaid' }, view.state.schema.text(selTxt))
            )
          );
        } else {
          this.insertBlockSnippet(`\`\`\`mermaid\n${selTxt}\n\`\`\``);
        }
      });
    }
    addItem('🔗', '[[위키링크]] 연결…', () => {
      this.bracketTriggeredFromTyping = false;
      this.callbacks.onRequestWikiLinkPicker(false);
    });
    if (this.callbacks.onPickFileAttachment) {
      addItem('📎', '파일 / 이미지 첨부…', () => {
        void this.callbacks.onPickFileAttachment?.();
      });
    }
    addItem('✂️', '인쇄 페이지 나누기', () =>
      this.insertTextOrSnippet('\n<div class="page-break"></div>\n')
    );
    addItem('📅', '오늘 날짜 삽입', () => {
      const now = new Date();
      const pad = (n: number) => String(n).padStart(2, '0');
      this.insertTextOrSnippet(`${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`);
    });

    addSep();
    addItem('🔍', '본문 찾기 / 바꾸기 (Ctrl+F)', () => this.openFindBar(true));
    addItem('📥', '붙여넣기 — 일반 텍스트 · 이미지 (Ctrl+V)', () => {
      void this.pasteFromSystemClipboard();
    });
    if (hasSelection) {
      addItem('📋', '선택 텍스트 복사', () => {
        const sel = window.getSelection()?.toString() || '';
        if (sel) {
          void navigator.clipboard.writeText(sel);
          this.callbacks.onToast('선택 영역을 복사했습니다.');
        }
      });
    }
    addItem('📄', '문서 마크다운 전체 복사', () => {
      const md = this.getCleanMarkdown();
      void navigator.clipboard.writeText(md);
      this.callbacks.onToast('마크다운 전체를 클립보드에 복사했습니다.');
    });

    document.body.appendChild(menu);
    this.contextMenuEl = menu;

    const r = menu.getBoundingClientRect();
    const maxX = Math.max(8, window.innerWidth - r.width - 8);
    const maxY = Math.max(8, window.innerHeight - r.height - 8);
    menu.style.left = `${Math.min(ev.clientX, maxX)}px`;
    menu.style.top = `${Math.min(ev.clientY, maxY)}px`;

    this.ctxDocClickBound = (e: Event) => {
      if (e.type === 'mousedown' && menu.contains(e.target as Node)) return;
      this.closeEditorContextMenu();
    };
    this.ctxDocKeyBound = (ke: KeyboardEvent) => {
      if (ke.key === 'Escape') {
        this.closeEditorContextMenu();
      }
    };
    setTimeout(() => {
      if (this.ctxDocClickBound) {
        document.addEventListener('mousedown', this.ctxDocClickBound, true);
        window.addEventListener('resize', this.ctxDocClickBound);
      }
      if (this.ctxDocKeyBound) {
        document.addEventListener('keydown', this.ctxDocKeyBound, true);
      }
    }, 0);
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
      this.preservePaneScroll(() => {
        this.editor?.setMarkdown(hydrated, false);
      });
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
    if (uiEl) {
      uiEl.classList.toggle('toastui-editor-dark', theme === 'dark');
    }
    if (this.toolbarDockEl) {
      this.toolbarDockEl.classList.toggle('toastui-editor-dark', theme === 'dark');
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
    this.closeEditorContextMenu();
    if (this.popupClampObserver) {
      this.popupClampObserver.disconnect();
      this.popupClampObserver = null;
    }
    if (this.docToolbarResizeBound) {
      window.removeEventListener('resize', this.docToolbarResizeBound);
      this.docToolbarResizeBound = null;
    }
    this.detachCleanClipboardHandlers();
    this.hostEl.removeEventListener('mousedown', this.handleHostMouseDownCapture, true);
    this.hostEl.removeEventListener('click', this.handleHostClick);
    this.findBarEl = null;
    this.editorMountEl = null;
    this.findInputEl = null;
    this.replaceInputEl = null;
    this.findCountEl = null;
    if (this.editor) {
      try {
        this.editor.destroy();
      } catch {
        // ignore
      }
      this.editor = null;
    }
    this.toolbarEl = null;
    if (this.toolbarDockEl) {
      this.toolbarDockEl.innerHTML = '';
      this.toolbarDockEl.classList.remove('toastui-editor-dark', 'mobile-all-tools-open');
      this.toolbarDockEl = null;
    }
  }
}

function toggleNthTaskInMarkdown(md: string, targetIdx: number): string | null {
  const norm = md.replace(/\r\n?/g, '\n');
  const lines = norm.split('\n');
  let inFence = false;
  let fenceChar = '`';
  let fenceLen = 3;
  let seen = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const t = line.trim();
    if (!inFence) {
      const openM = /^(`{3,}|~{3,})/.exec(t);
      if (openM) {
        inFence = true;
        fenceChar = openM[1][0];
        fenceLen = openM[1].length;
        continue;
      }
      const taskM = /^(\s*(?:[-*+]|\d+[.)])\s+\[)([ xX])(\].*)$/.exec(line);
      if (taskM) {
        if (seen === targetIdx) {
          const nextMark = taskM[2].trim().toLowerCase() === 'x' ? ' ' : 'x';
          lines[i] = `${taskM[1]}${nextMark}${taskM[3]}`;
          return lines.join('\n');
        }
        seen++;
      }
    } else {
      const closeRe = new RegExp(`^\\${fenceChar}{${fenceLen},}\\s*$`);
      if (closeRe.test(t)) {
        inFence = false;
      }
    }
  }
  return null;
}

function handleViewerFlowAndWikiClick(
  ev: MouseEvent,
  onOpenWikiLink: (title: string) => void,
  onToast?: (msg: string, isErr?: boolean) => void
): boolean {
  const target = ev.target as HTMLElement | null;
  if (!target) return false;

  const flowBtn = target.closest('[data-flow-action]') as HTMLElement | null;
  if (flowBtn) {
    ev.preventDefault();
    ev.stopPropagation();
    const cardEl = flowBtn.closest('.onyx-flow-card') as HTMLElement | null;
    if (!cardEl) return true;
    const action = flowBtn.getAttribute('data-flow-action');
    if (action === 'zoom-in' || action === 'zoom-out' || action === 'zoom-reset') {
      const curZoom = Number(cardEl.getAttribute('data-flow-zoom') || '100') || 100;
      const nextZoom =
        action === 'zoom-reset'
          ? 100
          : Math.max(50, Math.min(200, curZoom + (action === 'zoom-in' ? 15 : -15)));
      cardEl.setAttribute('data-flow-zoom', String(nextZoom));
      const lbl = cardEl.querySelector('.onyx-flow-zoom-label');
      if (lbl) lbl.textContent = `${nextZoom}%`;
      const stage = cardEl.querySelector('.onyx-flow-stage') as HTMLElement | null;
      if (stage) {
        (stage.style as any).zoom = nextZoom === 100 ? '' : (nextZoom / 100).toFixed(2);
      }
    } else if (action === 'copy-img') {
      void copyDiagramAsPngOrSvg(cardEl, onToast || (() => {}));
    }
    return true;
  }

  const link = target.closest('.wikilink-inline') as HTMLElement | null;
  if (link) {
    ev.preventDefault();
    const note = link.getAttribute('data-wikilink');
    if (note) onOpenWikiLink(note);
    return true;
  }

  return false;
}

/**
 * Mounts the mobile-first / distraction-free Markdown Reader view with:
 * - Interactive task checkboxes (`- [ ]`) syncing directly back to markdown
 * - Flowchart / Diagram SVG zoom & PNG/SVG export
 * - 1-tap code block copy buttons
 * - Collapsible Heading extraction for quick Reader TOC
 */
export function renderNoteReaderView(params: {
  container: HTMLElement;
  markdown: string;
  vault: string | null;
  theme: 'light' | 'dark';
  onOpenWikiLink: (title: string) => void;
  onToggleTaskMarkdown?: (updatedMarkdown: string) => void;
  onToast?: (msg: string, isErr?: boolean) => void;
}): {
  destroy: () => void;
  headings: Array<{ level: number; text: string; scrollTo: () => void }>;
} {
  params.container.innerHTML = '';
  const hydrated = hydrateMarkdownMediaUrls(params.markdown, params.vault);

  const viewer = Editor.factory({
    el: params.container,
    viewer: true,
    initialValue: hydrated,
    usageStatistics: false,
    theme: params.theme === 'dark' ? 'dark' : 'default',
    ...({ customHTMLSanitizer: safeHtmlSanitizer } as any),
    plugins: [
      [codeSyntaxHighlight, { highlighter: Prism }] as any,
      tableMergedCell as any,
      notionRichPlugin as any,
    ],
  });

  const contentsEl = params.container.querySelector('.toastui-editor-contents') as HTMLElement | null;
  let liveMarkdown = params.markdown;
  const headings: Array<{ level: number; text: string; scrollTo: () => void }> = [];

  if (contentsEl) {
    contentsEl.innerHTML = enhanceRenderedHtml(contentsEl.innerHTML);

    // Stamp sequential task indices so tapping checkboxes toggles the exact markdown line
    const taskItems = Array.from(contentsEl.querySelectorAll('li.task-list-item'));
    taskItems.forEach((li, idx) => {
      li.setAttribute('data-task-idx', String(idx));
    });

    // Collect headings for quick mobile TOC jump pills
    contentsEl.querySelectorAll<HTMLElement>('h1, h2, h3').forEach((h) => {
      const txt = (h.textContent || '').trim();
      if (!txt) return;
      const lvl = parseInt(h.tagName.slice(1), 10) || 1;
      headings.push({
        level: lvl,
        text: txt,
        scrollTo: () => h.scrollIntoView({ behavior: 'smooth', block: 'start' }),
      });
    });

    // Add 1-tap copy button on standard code blocks
    contentsEl.querySelectorAll<HTMLElement>('pre').forEach((pre) => {
      if (pre.closest('.onyx-flow-card')) return;
      pre.style.position = 'relative';
      const copyBtn = document.createElement('button');
      copyBtn.type = 'button';
      copyBtn.className = 'reader-code-copy-btn';
      copyBtn.textContent = '📋 복사';
      copyBtn.addEventListener('click', (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        const codeTxt = (pre.querySelector('code')?.textContent ?? pre.textContent ?? '').trim();
        void navigator.clipboard
          ?.writeText(codeTxt)
          .then(() => params.onToast?.('코드를 복사했습니다.'))
          .catch(() => params.onToast?.('복사에 실패했습니다.', true));
      });
      pre.appendChild(copyBtn);
    });
  }

  const onClick = (ev: MouseEvent) => {
    if (handleViewerFlowAndWikiClick(ev, params.onOpenWikiLink, params.onToast)) {
      return;
    }

    const target = ev.target as HTMLElement | null;
    const taskLi = target?.closest('li.task-list-item') as HTMLElement | null;
    if (taskLi && !target?.closest('a, button, code')) {
      const rect = taskLi.getBoundingClientRect();
      if (ev.clientX - rect.left <= 38) {
        const idx = Number(taskLi.getAttribute('data-task-idx'));
        if (!Number.isNaN(idx)) {
          const nextMd = toggleNthTaskInMarkdown(liveMarkdown, idx);
          if (nextMd !== null) {
            liveMarkdown = nextMd;
            taskLi.classList.toggle('checked');
            params.onToggleTaskMarkdown?.(nextMd);
          }
        }
      }
    }
  };
  params.container.addEventListener('click', onClick);

  return {
    headings,
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
    ...({ customHTMLSanitizer: safeHtmlSanitizer } as any),
    plugins: [
      [codeSyntaxHighlight, { highlighter: Prism }] as any,
      tableMergedCell as any,
      notionRichPlugin as any,
    ],
  });

  const contentsEl = params.container.querySelector('.toastui-editor-contents');
  if (contentsEl) {
    contentsEl.innerHTML = enhanceRenderedHtml(contentsEl.innerHTML);
  }

  const onClick = (ev: MouseEvent) => {
    void handleViewerFlowAndWikiClick(ev, params.onOpenWikiLink);
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

/**
 * Renders clean, print/PDF-ready Light-theme HTML using an offscreen TOAST UI Viewer.
 */
export function renderPrintableHtml(markdown: string, vault: string | null): string {
  const host = document.createElement('div');
  host.style.position = 'fixed';
  host.style.left = '-99999px';
  host.style.top = '0';
  host.style.width = '680px';
  host.style.visibility = 'hidden';
  document.body.appendChild(host);

  let inner = '';
  let viewer: any = null;
  try {
    const hydrated = hydrateMarkdownMediaUrls(markdown, vault);
    viewer = Editor.factory({
      el: host,
      viewer: true,
      initialValue: hydrated,
      usageStatistics: false,
      theme: 'default',
      ...({ customHTMLSanitizer: safeHtmlSanitizer } as any),
      plugins: [
        [codeSyntaxHighlight, { highlighter: Prism }] as any,
        tableMergedCell as any,
        notionRichPlugin as any,
      ],
    });
    const contentsEl = host.querySelector('.toastui-editor-contents');
    inner = enhanceRenderedHtml(contentsEl?.innerHTML || '', { forPrint: true });

    // Ensure <details> toggles are expanded and tables have explicit print-safe DOM attributes
    const cleanBox = document.createElement('div');
    cleanBox.innerHTML = inner;
    cleanBox.querySelectorAll('details').forEach((d) => {
      d.setAttribute('open', '');
    });
    cleanBox.querySelectorAll('.toastui-editor-dark').forEach((el) => {
      el.classList.remove('toastui-editor-dark');
    });
    inner = cleanBox.innerHTML;
  } finally {
    try {
      viewer?.destroy();
    } catch {
      // ignore
    }
    host.remove();
  }
  return `<div class="toastui-editor-contents">${inner}</div>`;
}
