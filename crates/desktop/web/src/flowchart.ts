import { createModalBase } from './modals';

export type FlowDirection = 'TB' | 'TD' | 'BT' | 'LR' | 'RL';
export type FlowThemeName = 'modern' | 'ocean' | 'emerald' | 'sunset' | 'mono';
export type FlowCurveMode = 'smooth' | 'ortho';
export type DiagramKind = 'flowchart' | 'sequence' | 'mindmap' | 'gantt' | 'empty';

export type FlowShape =
  | 'rect'
  | 'round'
  | 'stadium'
  | 'subroutine'
  | 'cylinder'
  | 'circle'
  | 'double-circle'
  | 'diamond'
  | 'hexagon'
  | 'lean-right'
  | 'lean-left'
  | 'trapezoid'
  | 'inv-trapezoid'
  | 'asymmetric';

export interface FlowStyleProps {
  fill?: string;
  stroke?: string;
  color?: string;
  strokeWidth?: string;
  strokeDasharray?: string;
}

export interface FlowNode {
  id: string;
  label: string;
  shape: FlowShape;
  subgraphId?: string;
  classNames: string[];
  inlineStyle?: FlowStyleProps;
  explicitDecl: boolean;
  isSubgraphRef?: boolean;
  // Computed layout metrics
  rank: number;
  order: number;
  x: number;
  y: number;
  w: number;
  h: number;
  lines: string[];
}

export interface FlowEdge {
  index: number;
  from: string;
  to: string;
  label: string;
  lineStyle: 'solid' | 'dotted' | 'thick';
  arrowStart: 'none' | 'arrow' | 'circle' | 'cross';
  arrowEnd: 'none' | 'arrow' | 'circle' | 'cross';
  minLen: number;
  isBackEdge?: boolean;
  inlineStyle?: FlowStyleProps;
}

export interface FlowSubgraph {
  id: string;
  title: string;
  parentId?: string;
  nodeIds: string[];
  childSubgraphIds: string[];
  direction?: FlowDirection;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ParsedFlowchart {
  kind: 'flowchart';
  direction: 'TB' | 'BT' | 'LR' | 'RL';
  theme: FlowThemeName;
  curve: FlowCurveMode;
  nodes: FlowNode[];
  edges: FlowEdge[];
  subgraphs: FlowSubgraph[];
  classDefs: Record<string, FlowStyleProps>;
  warnings: string[];
}

export interface DiagramRenderResult {
  svg: string;
  kind: DiagramKind;
  direction: string;
  theme: FlowThemeName;
  curve: FlowCurveMode;
  nodeCount: number;
  edgeCount: number;
  width: number;
  height: number;
  warnings: string[];
  layout?: {
    nodes: Array<{ id: string; x: number; y: number; w: number; h: number; rank: number }>;
    subgraphs: Array<{ id: string; x: number; y: number; w: number; h: number }>;
  };
}

const FLOW_THEMES: FlowThemeName[] = ['modern', 'ocean', 'emerald', 'sunset', 'mono'];
const FLOW_THEME_LABELS: Record<FlowThemeName, string> = {
  modern: '모던 슬레이트',
  ocean: '오션 블루',
  emerald: '에메랄드 민트',
  sunset: '선셋 앰버',
  mono: '모노 잉크',
};

const DIAGRAM_LANGS = new Set(['mermaid', 'flowchart', 'flow', 'diagram', 'graph']);

let svgInstanceSeq = 1;

export function escapeXml(s: string): string {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function unescapeInlineLineBreaks(s: string): string {
  return s.replace(/<br\s*\/?>/gi, '\n').replace(/\\n/g, '\n');
}

function stripOuterQuotes(s: string): string {
  const t = s.trim();
  if (
    (t.startsWith('"') && t.endsWith('"') && t.length >= 2) ||
    (t.startsWith("'") && t.endsWith("'") && t.length >= 2)
  ) {
    return t.slice(1, -1);
  }
  return t;
}

export function encodeFlowCode(code: string): string {
  try {
    const bytes = new TextEncoder().encode(code);
    let bin = '';
    for (let i = 0; i < bytes.length; i++) {
      bin += String.fromCharCode(bytes[i]);
    }
    if (typeof btoa === 'function') {
      return btoa(bin);
    }
  } catch {
    // fallback below
  }
  return encodeURIComponent(code);
}

export function decodeFlowCode(encoded: string): string {
  if (!encoded) return '';
  try {
    if (typeof atob === 'function' && !encoded.includes('%')) {
      const bin = atob(encoded);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) {
        bytes[i] = bin.charCodeAt(i);
      }
      return new TextDecoder().decode(bytes);
    }
  } catch {
    // fallback below
  }
  try {
    return decodeURIComponent(encoded);
  } catch {
    return encoded;
  }
}

export function parseDiagramMeta(code: string): {
  theme: FlowThemeName;
  curve: FlowCurveMode;
  cleanedLines: string[];
} {
  let theme: FlowThemeName = 'modern';
  let curve: FlowCurveMode = 'smooth';
  const cleanedLines: string[] = [];

  for (const rawLine of code.replace(/\r\n?/g, '\n').split('\n')) {
    const trimmed = rawLine.trim();
    const themeMatch = /^%%\s*onyx-theme:\s*([a-zA-Z0-9_-]+)/i.exec(trimmed);
    if (themeMatch) {
      const cand = themeMatch[1].toLowerCase() as FlowThemeName;
      if (FLOW_THEMES.includes(cand)) theme = cand;
      continue;
    }
    const curveMatch = /^%%\s*onyx-curve:\s*(smooth|ortho)/i.exec(trimmed);
    if (curveMatch) {
      curve = curveMatch[1].toLowerCase() as FlowCurveMode;
      continue;
    }
    cleanedLines.push(rawLine);
  }
  return { theme, curve, cleanedLines };
}

function setDiagramMetaComment(
  code: string,
  key: 'onyx-theme' | 'onyx-curve',
  val: string
): string {
  const lines = code.replace(/\r\n?/g, '\n').split('\n');
  const re = new RegExp(`^%%\\s*${key}:`, 'i');
  const nextLine = `%% ${key}: ${val}`;
  const idx = lines.findIndex((l) => re.test(l.trim()));
  if (idx >= 0) {
    lines[idx] = nextLine;
    return lines.join('\n');
  }
  return `${nextLine}\n${lines.join('\n')}`;
}

export function firstSignificantLine(code: string): string {
  for (const line of code.replace(/\r\n?/g, '\n').split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('%%')) continue;
    return t;
  }
  return '';
}

export function isDiagramCodeBlock(language: string | null | undefined, code: string): boolean {
  const lang = (language || '').trim().toLowerCase();
  if (DIAGRAM_LANGS.has(lang)) return true;
  if (lang && lang !== 'text' && lang !== 'plain' && lang !== 'txt') return false;
  const first = firstSignificantLine(code);
  return (
    /^(?:flowchart|graph)(?:\s+(?:TB|TD|BT|RL|LR))?\b/i.test(first) ||
    /^(?:sequenceDiagram|stateDiagram(?:-v2)?|mindmap|gantt)\b/i.test(first)
  );
}

export function looksLikeBareFlowchartText(text: string): boolean {
  const norm = text.replace(/\r\n?/g, '\n').trim();
  if (!norm) return false;
  const first = firstSignificantLine(norm);
  if (!/^(?:flowchart|graph)\s+(?:TB|TD|BT|RL|LR)\b/i.test(first)) {
    return false;
  }
  return /(?:-->|--->|---|-\.->|-\.-|==>|===|<-->|<-.->|<==>|o--o|x--x|--o|--x|\bsubgraph\b)/i.test(
    norm
  );
}

export function toggleFlowchartDirection(code: string): string {
  const order: Array<'TB' | 'LR' | 'BT' | 'RL'> = ['TB', 'LR', 'BT', 'RL'];
  const lines = code.replace(/\r\n?/g, '\n').split('\n');
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (!t || t.startsWith('%%')) continue;
    const m = /^(\s*(?:flowchart|graph))(?:\s+(TB|TD|BT|LR|RL))?(.*)$/i.exec(lines[i]);
    if (m) {
      const curRaw = (m[2] || 'TB').toUpperCase();
      const cur = (curRaw === 'TD' ? 'TB' : curRaw) as 'TB' | 'LR' | 'BT' | 'RL';
      const next = order[(order.indexOf(cur) + 1) % order.length];
      lines[i] = `${m[1]} ${next}${m[3] || ''}`;
      return lines.join('\n');
    }
    break;
  }
  return code;
}

export function setFlowchartDirection(code: string, dir: 'TB' | 'LR' | 'BT' | 'RL'): string {
  const lines = code.replace(/\r\n?/g, '\n').split('\n');
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (!t || t.startsWith('%%')) continue;
    const m = /^(\s*(?:flowchart|graph))(?:\s+(TB|TD|BT|LR|RL))?(.*)$/i.exec(lines[i]);
    if (m) {
      lines[i] = `${m[1]} ${dir}${m[3] || ''}`;
      return lines.join('\n');
    }
    break;
  }
  return code;
}

export function cycleFlowchartTheme(code: string): { nextCode: string; theme: FlowThemeName } {
  const { theme } = parseDiagramMeta(code);
  const next = FLOW_THEMES[(FLOW_THEMES.indexOf(theme) + 1) % FLOW_THEMES.length];
  return { nextCode: setDiagramMetaComment(code, 'onyx-theme', next), theme: next };
}

export function toggleFlowchartCurve(code: string): { nextCode: string; curve: FlowCurveMode } {
  const { curve } = parseDiagramMeta(code);
  const next: FlowCurveMode = curve === 'smooth' ? 'ortho' : 'smooth';
  return { nextCode: setDiagramMetaComment(code, 'onyx-curve', next), curve: next };
}

function measureVisualCharWidth(ch: string): number {
  const code = ch.codePointAt(0) || 0;
  if (ch === ' ' || ch === '\t') return 4.2;
  if (code >= 0x1100 && code <= 0x11ff) return 13.6; // Hangul Jamo
  if (code >= 0x3130 && code <= 0x318f) return 13.6; // Hangul Compat
  if (code >= 0xac00 && code <= 0xd7af) return 13.8; // Hangul Syllables
  if (code >= 0x4e00 && code <= 0x9fff) return 13.8; // CJK Unified
  if (code >= 0x3000 && code <= 0x30ff) return 13.4; // CJK punct / Kana
  if (code >= 0x1f300) return 16.5; // Emoji
  if ('WM@%#'.includes(ch)) return 10.6;
  if ('iltIjf.,;:\'"|!()[]{}'.includes(ch)) return 4.6;
  if (ch >= 'A' && ch <= 'Z') return 8.6;
  return 7.6;
}

function measureLinePx(line: string, fontScale = 1): number {
  const clean = line.replace(/\*\*([^*]+)\*\*/g, '$1');
  let sum = 0;
  for (const ch of clean) {
    sum += measureVisualCharWidth(ch);
  }
  return Math.max(8, sum * fontScale);
}

function wrapLabelLines(rawLabel: string, maxPx = 230, fontScale = 1): {
  lines: string[];
  widthPx: number;
  heightPx: number;
} {
  const normalized = unescapeInlineLineBreaks(rawLabel).trim() || ' ';
  const paragraphs = normalized.split('\n');
  const lines: string[] = [];

  for (const para of paragraphs) {
    const p = para.trim();
    if (!p) {
      if (lines.length > 0) lines.push('');
      continue;
    }
    if (measureLinePx(p, fontScale) <= maxPx) {
      lines.push(p);
      continue;
    }
    // Wrap on spaces if available, else break cleanly by visual width
    const words = p.split(/\s+/);
    if (words.length > 1) {
      let cur = '';
      for (const w of words) {
        const cand = cur ? `${cur} ${w}` : w;
        if (measureLinePx(cand, fontScale) <= maxPx || !cur) {
          cur = cand;
        } else {
          lines.push(cur);
          cur = w;
        }
      }
      if (cur) lines.push(cur);
    } else {
      let cur = '';
      for (const ch of p) {
        if (measureLinePx(cur + ch, fontScale) > maxPx && cur) {
          lines.push(cur);
          cur = ch;
        } else {
          cur += ch;
        }
      }
      if (cur) lines.push(cur);
    }
  }

  if (lines.length === 0) lines.push(' ');
  const widthPx = Math.max(...lines.map((l) => measureLinePx(l, fontScale)));
  const lineH = Math.round(18.5 * fontScale);
  const heightPx = Math.max(lineH, lines.length * lineH);
  return { lines, widthPx, heightPx };
}

function renderSvgTextLines(
  lines: string[],
  cx: number,
  cy: number,
  options: {
    fontSize: number;
    fontWeight?: number | string;
    fill: string;
    lineHeight?: number;
  }
): string {
  const lh = options.lineHeight ?? options.fontSize * 1.38;
  const totalH = (lines.length - 1) * lh;
  const firstY = cy - totalH / 2 + options.fontSize * 0.35;
  const fw = options.fontWeight ?? 600;
  const tspans = lines
    .map((line, idx) => {
      const y = (firstY + idx * lh).toFixed(1);
      // Support **bold** spans safely after escaping
      const parts = line.split(/(\*\*[^*]+\*\*)/g);
      const inner = parts
        .filter(Boolean)
        .map((part) => {
          if (part.startsWith('**') && part.endsWith('**') && part.length > 4) {
            return `<tspan font-weight="800">${escapeXml(part.slice(2, -2))}</tspan>`;
          }
          return escapeXml(part);
        })
        .join('');
      return `<tspan x="${cx.toFixed(1)}" y="${y}">${inner || ' '}</tspan>`;
    })
    .join('');

  return `<text text-anchor="middle" font-size="${options.fontSize}" font-weight="${fw}" fill="${options.fill}" font-family="'Pretendard', 'Noto Sans KR', 'Malgun Gothic', -apple-system, BlinkMacSystemFont, sans-serif">${tspans}</text>`;
}

function splitStatementsRespectingQuotes(line: string): string[] {
  const out: string[] = [];
  let buf = '';
  let quote: string | null = null;
  let brDepth = 0;
  let parDepth = 0;
  let curDepth = 0;

  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (quote) {
      buf += ch;
      if (ch === quote && line[i - 1] !== '\\') quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      buf += ch;
      continue;
    }
    if (ch === '[') brDepth++;
    else if (ch === ']' && brDepth > 0) brDepth--;
    else if (ch === '(') parDepth++;
    else if (ch === ')' && parDepth > 0) parDepth--;
    else if (ch === '{') curDepth++;
    else if (ch === '}' && curDepth > 0) curDepth--;

    if (ch === ';' && brDepth === 0 && parDepth === 0 && curDepth === 0) {
      if (buf.trim()) out.push(buf.trim());
      buf = '';
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) out.push(buf.trim());
  return out;
}

function parseStyleProps(raw: string): FlowStyleProps {
  const out: FlowStyleProps = {};
  for (const part of raw.split(',')) {
    const colon = part.indexOf(':');
    if (colon <= 0) continue;
    const k = part.slice(0, colon).trim().toLowerCase();
    const v = part.slice(colon + 1).trim();
    if (!v) continue;
    // Allow only safe CSS values (colors, lengths, dash arrays)
    if (/[<>;"'()]/.test(v) && !/^rgba?\([0-9.,\s]+\)$/i.test(v)) continue;
    if (k === 'fill' || k === 'background') out.fill = v;
    else if (k === 'stroke') out.stroke = v;
    else if (k === 'color') out.color = v;
    else if (k === 'stroke-width') out.strokeWidth = v;
    else if (k === 'stroke-dasharray') out.strokeDasharray = v;
  }
  return out;
}

interface ParsedNodeToken {
  id: string;
  label?: string;
  shape?: FlowShape;
  className?: string;
}

function parseNodeToken(rawToken: string): ParsedNodeToken | null {
  let token = rawToken.trim();
  if (!token) return null;

  let className: string | undefined;
  const clsIdx = token.lastIndexOf(':::');
  if (clsIdx > 0) {
    className = token.slice(clsIdx + 3).trim();
    token = token.slice(0, clsIdx).trim();
  }

  const shapePatterns: Array<{ re: RegExp; shape: FlowShape }> = [
    { re: /^([^\s\[\(\{<>]+)\(\(\(([\s\S]+)\)\)\)$/, shape: 'double-circle' },
    { re: /^([^\s\[\(\{<>]+)\(\(([\s\S]+)\)\)$/, shape: 'circle' },
    { re: /^([^\s\[\(\{<>]+)\(\[([\s\S]+)\]\)$/, shape: 'stadium' },
    { re: /^([^\s\[\(\{<>]+)\[\[([\s\S]+)\]\]$/, shape: 'subroutine' },
    { re: /^([^\s\[\(\{<>]+)\[\(([\s\S]+)\)\]$/, shape: 'cylinder' },
    { re: /^([^\s\[\(\{<>]+)\{\{([\s\S]+)\}\}$/, shape: 'hexagon' },
    { re: /^([^\s\[\(\{<>]+)\{([\s\S]+)\}$/, shape: 'diamond' },
    { re: /^([^\s\[\(\{<>]+)\[\/([\s\S]+)\/\]$/, shape: 'lean-right' },
    { re: /^([^\s\[\(\{<>]+)\[\\([\s\S]+)\\\]$/, shape: 'lean-left' },
    { re: /^([^\s\[\(\{<>]+)\[\/([\s\S]+)\\\]$/, shape: 'trapezoid' },
    { re: /^([^\s\[\(\{<>]+)\[\\([\s\S]+)\/\]$/, shape: 'inv-trapezoid' },
    { re: /^([^\s\[\(\{<>]+)>([\s\S]+)\]$/, shape: 'asymmetric' },
    { re: /^([^\s\[\(\{<>]+)\(([\s\S]+)\)$/, shape: 'round' },
    { re: /^([^\s\[\(\{<>]+)\[([\s\S]+)\]$/, shape: 'rect' },
  ];

  for (const { re, shape } of shapePatterns) {
    const m = re.exec(token);
    if (m) {
      return {
        id: m[1].trim(),
        label: stripOuterQuotes(m[2]),
        shape,
        className,
      };
    }
  }

  const idOnly = /^([^\s\[\(\{<>]+)$/.exec(token);
  if (idOnly) {
    return { id: idOnly[1].trim(), className };
  }
  return null;
}

interface EdgeOpInfo {
  lineStyle: 'solid' | 'dotted' | 'thick';
  arrowStart: 'none' | 'arrow' | 'circle' | 'cross';
  arrowEnd: 'none' | 'arrow' | 'circle' | 'cross';
  label: string;
  minLen: number;
  length: number;
}

function matchEdgeOperatorAt(str: string, pos: number): EdgeOpInfo | null {
  const rest = str.slice(pos);

  // 1) Inline-labeled operators: `-- text -->`, `-. text .->`, `== text ==>`
  const inlineSpecs: Array<{
    re: RegExp;
    lineStyle: 'solid' | 'dotted' | 'thick';
    arrowEnd: 'arrow' | 'none';
  }> = [
    { re: /^--\s+(.+?)\s+(?:--->|-->)/, lineStyle: 'solid', arrowEnd: 'arrow' },
    { re: /^--\s+(.+?)\s+---/, lineStyle: 'solid', arrowEnd: 'none' },
    { re: /^-\.\s*(.+?)\s*\.->/, lineStyle: 'dotted', arrowEnd: 'arrow' },
    { re: /^-\.\s*(.+?)\s*\.-/, lineStyle: 'dotted', arrowEnd: 'none' },
    { re: /^==\s+(.+?)\s+(?:===>|==>)/, lineStyle: 'thick', arrowEnd: 'arrow' },
    { re: /^==\s+(.+?)\s+===/, lineStyle: 'thick', arrowEnd: 'none' },
  ];

  for (const spec of inlineSpecs) {
    const m = spec.re.exec(rest);
    if (m) {
      return {
        lineStyle: spec.lineStyle,
        arrowStart: 'none',
        arrowEnd: spec.arrowEnd,
        label: stripOuterQuotes(m[1]),
        minLen: 1,
        length: m[0].length,
      };
    }
  }

  // 2) Standard direct operators (longest first)
  const directSpecs: Array<{
    re: RegExp;
    lineStyle: 'solid' | 'dotted' | 'thick';
    arrowStart: EdgeOpInfo['arrowStart'];
    arrowEnd: EdgeOpInfo['arrowEnd'];
  }> = [
    { re: /^<--+>/, lineStyle: 'solid', arrowStart: 'arrow', arrowEnd: 'arrow' },
    { re: /^<-\.+->/, lineStyle: 'dotted', arrowStart: 'arrow', arrowEnd: 'arrow' },
    { re: /^<=+>/, lineStyle: 'thick', arrowStart: 'arrow', arrowEnd: 'arrow' },
    { re: /^o--+o/, lineStyle: 'solid', arrowStart: 'circle', arrowEnd: 'circle' },
    { re: /^x--+x/, lineStyle: 'solid', arrowStart: 'cross', arrowEnd: 'cross' },
    { re: /^-\.+->/, lineStyle: 'dotted', arrowStart: 'none', arrowEnd: 'arrow' },
    { re: /^-\.+-/, lineStyle: 'dotted', arrowStart: 'none', arrowEnd: 'none' },
    { re: /^==+>/, lineStyle: 'thick', arrowStart: 'none', arrowEnd: 'arrow' },
    { re: /^===/, lineStyle: 'thick', arrowStart: 'none', arrowEnd: 'none' },
    { re: /^--+>/, lineStyle: 'solid', arrowStart: 'none', arrowEnd: 'arrow' },
    { re: /^--+o/, lineStyle: 'solid', arrowStart: 'none', arrowEnd: 'circle' },
    { re: /^--+x/, lineStyle: 'solid', arrowStart: 'none', arrowEnd: 'cross' },
    { re: /^---+/, lineStyle: 'solid', arrowStart: 'none', arrowEnd: 'none' },
  ];

  for (const spec of directSpecs) {
    const m = spec.re.exec(rest);
    if (m) {
      let totalLen = m[0].length;
      let label = '';
      // Optional |label| right after operator
      const after = rest.slice(totalLen);
      const pipeMatch = /^(\s*\|([^|]*)\|)/.exec(after);
      if (pipeMatch) {
        label = stripOuterQuotes(pipeMatch[2]);
        totalLen += pipeMatch[1].length;
      }
      const dashCount = (m[0].match(/-/g) || []).length;
      return {
        lineStyle: spec.lineStyle,
        arrowStart: spec.arrowStart,
        arrowEnd: spec.arrowEnd,
        label,
        minLen: Math.max(1, Math.min(3, dashCount - 1)),
        length: totalLen,
      };
    }
  }

  return null;
}

interface ScannedChain {
  groups: string[];
  ops: EdgeOpInfo[];
}

function scanStatementChain(stmt: string): ScannedChain {
  const groups: string[] = [];
  const ops: EdgeOpInfo[] = [];

  let i = 0;
  let start = 0;
  let quote: string | null = null;
  let brDepth = 0;
  let parDepth = 0;
  let curDepth = 0;

  while (i < stmt.length) {
    const ch = stmt[i];
    if (quote) {
      if (ch === quote && stmt[i - 1] !== '\\') quote = null;
      i++;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      i++;
      continue;
    }
    if (ch === '[') {
      brDepth++;
      i++;
      continue;
    }
    if (ch === ']' && brDepth > 0) {
      brDepth--;
      i++;
      continue;
    }
    if (ch === '(') {
      parDepth++;
      i++;
      continue;
    }
    if (ch === ')' && parDepth > 0) {
      parDepth--;
      i++;
      continue;
    }
    if (ch === '{') {
      curDepth++;
      i++;
      continue;
    }
    if (ch === '}' && curDepth > 0) {
      curDepth--;
      i++;
      continue;
    }

    if (brDepth === 0 && parDepth === 0 && curDepth === 0) {
      // Inline text edge "-- text -->" only starts on whitespace boundary or directly after node id
      const op = matchEdgeOperatorAt(stmt, i);
      const prevCh = i > 0 ? stmt[i - 1] : '';
      const validBoundary =
        i > start &&
        (/[\s\]\)\}a-zA-Z0-9_\u3130-\u318f\uac00-\ud7af-]/.test(prevCh) ||
          prevCh === '');
      if (op && validBoundary) {
        groups.push(stmt.slice(start, i).trim());
        ops.push(op);
        i += op.length;
        start = i;
        continue;
      }
    }
    i++;
  }

  groups.push(stmt.slice(start).trim());
  return { groups, ops };
}

function splitAmpersandRespectingBrackets(group: string): string[] {
  const parts: string[] = [];
  let buf = '';
  let quote: string | null = null;
  let brDepth = 0;
  let parDepth = 0;
  let curDepth = 0;

  for (let i = 0; i < group.length; i++) {
    const ch = group[i];
    if (quote) {
      buf += ch;
      if (ch === quote && group[i - 1] !== '\\') quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      buf += ch;
      continue;
    }
    if (ch === '[') brDepth++;
    else if (ch === ']' && brDepth > 0) brDepth--;
    else if (ch === '(') parDepth++;
    else if (ch === ')' && parDepth > 0) parDepth--;
    else if (ch === '{') curDepth++;
    else if (ch === '}' && curDepth > 0) curDepth--;

    if (ch === '&' && brDepth === 0 && parDepth === 0 && curDepth === 0) {
      if (buf.trim()) parts.push(buf.trim());
      buf = '';
      continue;
    }
    buf += ch;
  }
  if (buf.trim()) parts.push(buf.trim());
  return parts;
}

export function parseFlowchart(code: string): ParsedFlowchart {
  const { theme, curve, cleanedLines } = parseDiagramMeta(code);

  let direction: 'TB' | 'BT' | 'LR' | 'RL' = 'TB';
  let isStateDiagram = false;

  const nodeMap = new Map<string, FlowNode>();
  const nodeOrder: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const subgraphMap = new Map<string, FlowSubgraph>();
  const subgraphs: FlowSubgraph[] = [];
  const subgraphStack: FlowSubgraph[] = [];
  const classDefs: Record<string, FlowStyleProps> = {};
  const warnings: string[] = [];

  const ensureNode = (tok: ParsedNodeToken): FlowNode => {
    const curSub = subgraphStack[subgraphStack.length - 1];
    const existing = nodeMap.get(tok.id);
    if (existing) {
      if (tok.label !== undefined) {
        existing.label = tok.label;
        existing.explicitDecl = true;
      }
      if (tok.shape) existing.shape = tok.shape;
      if (tok.className && !existing.classNames.includes(tok.className)) {
        existing.classNames.push(tok.className);
      }
      if (!existing.subgraphId && curSub && !subgraphMap.has(tok.id)) {
        existing.subgraphId = curSub.id;
        if (!curSub.nodeIds.includes(tok.id)) curSub.nodeIds.push(tok.id);
      }
      return existing;
    }

    const subRef = subgraphMap.get(tok.id);
    const node: FlowNode = {
      id: tok.id,
      label: tok.label !== undefined ? tok.label : tok.id,
      shape: tok.shape || 'round',
      subgraphId: subRef ? subRef.parentId : curSub?.id,
      classNames: tok.className ? [tok.className] : [],
      explicitDecl: tok.label !== undefined || Boolean(tok.shape),
      isSubgraphRef: Boolean(subRef && tok.label === undefined),
      rank: 0,
      order: 0,
      x: 0,
      y: 0,
      w: 120,
      h: 46,
      lines: [],
    };
    nodeMap.set(tok.id, node);
    nodeOrder.push(node);
    if (!node.isSubgraphRef && curSub && !curSub.nodeIds.includes(tok.id)) {
      curSub.nodeIds.push(tok.id);
    }
    return node;
  };

  let started = false;
  let anonSubIdx = 1;
  let stateAnchorSeq = 1;

  for (const rawLine of cleanedLines) {
    const trimmedLine = rawLine.trim();
    if (!trimmedLine || trimmedLine.startsWith('%%')) continue;

    // Header check on first significant line
    if (!started) {
      const flowHdr = /^(?:flowchart|graph)(?:\s+(TB|TD|BT|LR|RL))?\s*$/i.exec(trimmedLine);
      if (flowHdr) {
        const d = (flowHdr[1] || 'TB').toUpperCase();
        direction = (d === 'TD' ? 'TB' : d) as 'TB' | 'BT' | 'LR' | 'RL';
        started = true;
        continue;
      }
      if (/^stateDiagram(?:-v2)?\b/i.test(trimmedLine)) {
        isStateDiagram = true;
        direction = 'TB';
        started = true;
        continue;
      }
      started = true;
    }

    const statements = splitStatementsRespectingQuotes(trimmedLine);
    for (const stmt of statements) {
      if (!stmt || stmt.startsWith('%%')) continue;

      if (isStateDiagram) {
        const dirM = /^direction\s+(TB|TD|BT|LR|RL)$/i.exec(stmt);
        if (dirM) {
          const d = dirM[1].toUpperCase();
          direction = (d === 'TD' ? 'TB' : d) as 'TB' | 'BT' | 'LR' | 'RL';
          continue;
        }
        const aliasM = /^state\s+"([^"]+)"\s+as\s+([^\s{]+)$/i.exec(stmt);
        if (aliasM) {
          ensureNode({ id: aliasM[2], label: aliasM[1], shape: 'round' });
          continue;
        }
        const stateEdge =
          /^(\[\*\]|[^\s:]+)\s*(-->)\s*(\[\*\]|[^\s:]+)(?:\s*:\s*(.+))?$/.exec(stmt);
        if (stateEdge) {
          const mkEndpoint = (raw: string, isSource: boolean): string => {
            if (raw === '[*]') {
              const sid = isSource
                ? `__start_${stateAnchorSeq++}`
                : `__end_${stateAnchorSeq++}`;
              ensureNode({
                id: sid,
                label: isSource ? '시작' : '종료',
                shape: isSource ? 'circle' : 'double-circle',
              });
              return sid;
            }
            ensureNode({ id: raw, shape: 'round' });
            return raw;
          };
          const fromId = mkEndpoint(stateEdge[1], true);
          const toId = mkEndpoint(stateEdge[3], false);
          edges.push({
            index: edges.length,
            from: fromId,
            to: toId,
            label: stateEdge[4] ? stateEdge[4].trim() : '',
            lineStyle: 'solid',
            arrowStart: 'none',
            arrowEnd: 'arrow',
            minLen: 1,
          });
          continue;
        }
      }

      // subgraph end
      if (/^end$/i.test(stmt)) {
        if (subgraphStack.length > 0) subgraphStack.pop();
        continue;
      }

      // subgraph header: `subgraph ID ["Title"]` / `subgraph ID [Title]` / `subgraph "Title"` / `subgraph ID`
      const subM = /^subgraph\s+([\s\S]+)$/i.exec(stmt);
      if (subM) {
        const arg = subM[1].trim();
        let id = '';
        let title = '';
        const bracketForm = /^([^\s\["']+)\s*\[([\s\S]+)\]$/.exec(arg);
        const quotedForm = /^"([^"]+)"$/.exec(arg);
        if (bracketForm) {
          id = bracketForm[1].trim();
          title = stripOuterQuotes(bracketForm[2]);
        } else if (quotedForm) {
          id = `__sub_${anonSubIdx++}`;
          title = quotedForm[1].trim();
        } else {
          const parts = arg.split(/\s+/);
          id = parts[0].trim();
          title = parts.length > 1 ? stripOuterQuotes(arg.slice(id.length).trim()) : id;
        }
        const parent = subgraphStack[subgraphStack.length - 1];
        const sub: FlowSubgraph = {
          id,
          title: unescapeInlineLineBreaks(title),
          parentId: parent?.id,
          nodeIds: [],
          childSubgraphIds: [],
          x: 0,
          y: 0,
          w: 0,
          h: 0,
        };
        if (parent) parent.childSubgraphIds.push(id);
        subgraphMap.set(id, sub);
        subgraphs.push(sub);
        subgraphStack.push(sub);
        continue;
      }

      // direction inside subgraph (or root)
      const dirCmd = /^direction\s+(TB|TD|BT|LR|RL)$/i.exec(stmt);
      if (dirCmd) {
        const d = (dirCmd[1].toUpperCase() === 'TD' ? 'TB' : dirCmd[1].toUpperCase()) as
          | 'TB'
          | 'BT'
          | 'LR'
          | 'RL';
        if (subgraphStack.length > 0) {
          subgraphStack[subgraphStack.length - 1].direction = d;
        } else {
          direction = d;
        }
        continue;
      }

      // classDef name prop:val,...
      const cdM = /^classDef\s+([A-Za-z0-9_-]+)\s+([\s\S]+)$/i.exec(stmt);
      if (cdM) {
        classDefs[cdM[1]] = parseStyleProps(cdM[2]);
        continue;
      }

      // class nodeA,nodeB clsName
      const clsAssign = /^class\s+([^\s]+)\s+([A-Za-z0-9_-]+)$/i.exec(stmt);
      if (clsAssign) {
        for (const id of clsAssign[1].split(',')) {
          const n = ensureNode({ id: id.trim() });
          if (!n.classNames.includes(clsAssign[2])) n.classNames.push(clsAssign[2]);
        }
        continue;
      }

      // style nodeId prop:val,...
      const stM = /^style\s+([^\s]+)\s+([\s\S]+)$/i.exec(stmt);
      if (stM) {
        const n = ensureNode({ id: stM[1].trim() });
        n.inlineStyle = { ...n.inlineStyle, ...parseStyleProps(stM[2]) };
        continue;
      }

      // linkStyle 0,1 prop:val,...
      const lsM = /^linkStyle\s+([0-9,]+|default)\s+([\s\S]+)$/i.exec(stmt);
      if (lsM) {
        const props = parseStyleProps(lsM[2]);
        if (lsM[1].toLowerCase() === 'default') {
          for (const e of edges) e.inlineStyle = { ...props, ...e.inlineStyle };
        } else {
          for (const idxStr of lsM[1].split(',')) {
            const idx = parseInt(idxStr.trim(), 10);
            if (!Number.isNaN(idx) && edges[idx]) {
              edges[idx].inlineStyle = { ...edges[idx].inlineStyle, ...props };
            }
          }
        }
        continue;
      }

      // click / link / accTitle / title ignore safely
      if (/^(?:click|accTitle|accDescr)\b/i.test(stmt)) continue;

      // Statement chain with edges (or standalone node declaration)
      const chain = scanStatementChain(stmt);
      if (chain.ops.length > 0) {
        const groupNodes: FlowNode[][] = [];
        for (const g of chain.groups) {
          const toks = splitAmpersandRespectingBrackets(g)
            .map(parseNodeToken)
            .filter((x): x is ParsedNodeToken => Boolean(x));
          const created = toks.map(ensureNode);
          groupNodes.push(created);
        }
        for (let idx = 0; idx < chain.ops.length; idx++) {
          const left = groupNodes[idx] || [];
          const right = groupNodes[idx + 1] || [];
          const op = chain.ops[idx];
          for (const u of left) {
            for (const v of right) {
              edges.push({
                index: edges.length,
                from: u.id,
                to: v.id,
                label: op.label,
                lineStyle: op.lineStyle,
                arrowStart: op.arrowStart,
                arrowEnd: op.arrowEnd,
                minLen: op.minLen,
              });
            }
          }
        }
      } else {
        const toks = splitAmpersandRespectingBrackets(stmt)
          .map(parseNodeToken)
          .filter((x): x is ParsedNodeToken => Boolean(x));
        for (const tok of toks) {
          ensureNode(tok);
        }
      }
    }
  }

  // Filter out pure subgraph reference placeholders from real leaf node list when possible
  const leafNodes = nodeOrder.filter(
    (n) => !(n.isSubgraphRef && subgraphMap.has(n.id) && (subgraphMap.get(n.id)!.nodeIds.length > 0))
  );

  return {
    kind: 'flowchart',
    direction,
    theme,
    curve,
    nodes: leafNodes,
    edges,
    subgraphs,
    classDefs,
    warnings,
  };
}

function measureShapeBox(node: FlowNode): {
  w: number;
  h: number;
  lines: string[];
} {
  const { lines, widthPx, heightPx } = wrapLabelLines(node.label, 220, 1);
  let padX = 34;
  let padY = 22;
  let w = Math.max(88, Math.ceil(widthPx + padX));
  let h = Math.max(44, Math.ceil(heightPx + padY));

  switch (node.shape) {
    case 'diamond':
      // Diamond inscribed diamond needs extra diagonals so corners don't clip Korean labels
      w = Math.max(114, Math.ceil(widthPx * 1.62 + 48));
      h = Math.max(76, Math.ceil(heightPx * 1.7 + 38));
      break;
    case 'circle':
    case 'double-circle': {
      const diam = Math.max(58, Math.ceil(Math.hypot(widthPx, heightPx) + 30));
      w = diam;
      h = diam;
      break;
    }
    case 'cylinder':
      w = Math.max(100, Math.ceil(widthPx + 40));
      h = Math.max(58, Math.ceil(heightPx + 34));
      break;
    case 'hexagon':
      w = Math.max(112, Math.ceil(widthPx + 54));
      h = Math.max(48, Math.ceil(heightPx + 24));
      break;
    case 'subroutine':
      w = Math.max(110, Math.ceil(widthPx + 52));
      break;
    case 'stadium':
      w = Math.max(96, Math.ceil(widthPx + 46));
      break;
    case 'lean-right':
    case 'lean-left':
    case 'trapezoid':
    case 'inv-trapezoid':
    case 'asymmetric':
      w = Math.max(110, Math.ceil(widthPx + 50));
      break;
    default:
      break;
  }

  return { w, h, lines };
}

interface LayoutNode {
  id: string;
  isDummy: boolean;
  ref?: FlowNode;
  w: number;
  h: number;
  rank: number;
  order: number;
  primaryCoord: number; // along rank axis
  crossCoord: number; // across rank axis
  x: number;
  y: number;
  subgraphChain: string[];
}

interface LayoutEdge {
  edge: FlowEdge;
  u: string;
  v: string;
  reversed: boolean;
  points: Array<{ x: number; y: number }>;
  isBackEdge: boolean;
  isSelfLoop: boolean;
}

function collectSubgraphLeafMembers(
  subId: string,
  subMap: Map<string, FlowSubgraph>,
  seen = new Set<string>()
): string[] {
  if (seen.has(subId)) return [];
  seen.add(subId);
  const sub = subMap.get(subId);
  if (!sub) return [];
  const res = [...sub.nodeIds];
  for (const childId of sub.childSubgraphIds) {
    res.push(...collectSubgraphLeafMembers(childId, subMap, seen));
  }
  return res;
}

function getSubgraphChain(node: FlowNode, subMap: Map<string, FlowSubgraph>): string[] {
  const chain: string[] = [];
  let cur = node.subgraphId;
  const guard = new Set<string>();
  while (cur && !guard.has(cur)) {
    guard.add(cur);
    chain.unshift(cur);
    cur = subMap.get(cur)?.parentId;
  }
  return chain;
}

function renderFlowchartSvg(
  parsed: ParsedFlowchart,
  options: { forPrint?: boolean; svgIdPrefix?: string }
): DiagramRenderResult {
  const prefix = options.svgIdPrefix || `onyxflow-${svgInstanceSeq++}`;
  const horizontal = parsed.direction === 'LR' || parsed.direction === 'RL';
  const reversedAxis = parsed.direction === 'BT' || parsed.direction === 'RL';

  if (parsed.nodes.length === 0) {
    return {
      svg: renderEmptyHintSvg('흐름도 노드나 화살표(`A[시작] --> B[완료]`)를 입력해 주세요.', 380, 120),
      kind: 'flowchart',
      direction: parsed.direction,
      theme: parsed.theme,
      curve: parsed.curve,
      nodeCount: 0,
      edgeCount: 0,
      width: 380,
      height: 120,
      warnings: parsed.warnings,
    };
  }

  const subMap = new Map<string, FlowSubgraph>();
  for (const s of parsed.subgraphs) subMap.set(s.id, s);

  const leafMap = new Map<string, FlowNode>();
  for (const n of parsed.nodes) {
    const metrics = measureShapeBox(n);
    n.w = metrics.w;
    n.h = metrics.h;
    n.lines = metrics.lines;
    leafMap.set(n.id, n);
  }

  // Expand subgraph-endpoint edges into rank constraints between member leaves,
  // while tracking direct visual edges for rendering!
  const rankConstraints: Array<{ from: string; to: string; minLen: number }> = [];
  const nodeEdges: FlowEdge[] = [];

  for (const e of parsed.edges) {
    const fromIsSub = !leafMap.has(e.from) && subMap.has(e.from);
    const toIsSub = !leafMap.has(e.to) && subMap.has(e.to);

    if (fromIsSub || toIsSub) {
      const fromLeaves = fromIsSub ? collectSubgraphLeafMembers(e.from, subMap) : [e.from];
      const toLeaves = toIsSub ? collectSubgraphLeafMembers(e.to, subMap) : [e.to];
      for (const fl of fromLeaves) {
        for (const tl of toLeaves) {
          if (leafMap.has(fl) && leafMap.has(tl) && fl !== tl) {
            rankConstraints.push({ from: fl, to: tl, minLen: e.minLen });
          }
        }
      }
      nodeEdges.push(e);
    } else if (leafMap.has(e.from) && leafMap.has(e.to)) {
      rankConstraints.push({ from: e.from, to: e.to, minLen: e.minLen });
      nodeEdges.push(e);
    }
  }

  // 1) Cycle breaking via DFS on leaf dependency graph
  const adj = new Map<string, Array<{ to: string; minLen: number }>>();
  for (const n of parsed.nodes) adj.set(n.id, []);
  for (const c of rankConstraints) {
    if (c.from !== c.to) adj.get(c.from)!.push({ to: c.to, minLen: c.minLen });
  }

  const state = new Map<string, 0 | 1 | 2>(); // 0=unvisited, 1=visiting, 2=done
  const dagEdges: Array<{ from: string; to: string; minLen: number }> = [];
  const backPairs = new Set<string>();

  const dfs = (u: string) => {
    state.set(u, 1);
    for (const next of adj.get(u) || []) {
      const st = state.get(next.to) || 0;
      if (st === 1) {
        backPairs.add(`${u}=>${next.to}`);
      } else {
        dagEdges.push({ from: u, to: next.to, minLen: next.minLen });
        if (st === 0) dfs(next.to);
      }
    }
    state.set(u, 2);
  };
  for (const n of parsed.nodes) {
    if ((state.get(n.id) || 0) === 0) dfs(n.id);
  }

  // Mark user edges that are back-edges (including self-loops)
  for (const e of nodeEdges) {
    if (e.from === e.to) {
      e.isBackEdge = true;
    } else if (backPairs.has(`${e.from}=>${e.to}`)) {
      e.isBackEdge = true;
    }
  }

  // 2) Longest-path rank assignment (top-down topological order)
  const indeg = new Map<string, number>();
  const outAdj = new Map<string, Array<{ to: string; minLen: number }>>();
  const inAdj = new Map<string, Array<{ from: string; minLen: number }>>();
  for (const n of parsed.nodes) {
    indeg.set(n.id, 0);
    outAdj.set(n.id, []);
    inAdj.set(n.id, []);
  }
  for (const e of dagEdges) {
    outAdj.get(e.from)!.push({ to: e.to, minLen: e.minLen });
    inAdj.get(e.to)!.push({ from: e.from, minLen: e.minLen });
    indeg.set(e.to, (indeg.get(e.to) || 0) + 1);
  }

  const queue: string[] = [];
  for (const n of parsed.nodes) {
    if ((indeg.get(n.id) || 0) === 0) queue.push(n.id);
  }

  const topo: string[] = [];
  const rank = new Map<string, number>();
  for (const n of parsed.nodes) rank.set(n.id, 0);

  while (queue.length > 0) {
    const u = queue.shift()!;
    topo.push(u);
    const r = rank.get(u) || 0;
    for (const { to, minLen } of outAdj.get(u) || []) {
      const cand = r + minLen;
      if (cand > (rank.get(to) || 0)) rank.set(to, cand);
      const nextIn = (indeg.get(to) || 0) - 1;
      indeg.set(to, nextIn);
      if (nextIn === 0) queue.push(to);
    }
  }

  // Tightening pass: pull source nodes down close to their earliest consumer so roots don't float needlessly
  for (let i = topo.length - 1; i >= 0; i--) {
    const u = topo[i];
    const outs = outAdj.get(u) || [];
    const ins = inAdj.get(u) || [];
    if (outs.length > 0 && ins.length === 0) {
      let minTargetRank = Infinity;
      for (const o of outs) {
        minTargetRank = Math.min(minTargetRank, (rank.get(o.to) || 0) - o.minLen);
      }
      if (Number.isFinite(minTargetRank) && minTargetRank > (rank.get(u) || 0)) {
        rank.set(u, minTargetRank);
      }
    }
  }

  const minR = Math.min(...Array.from(rank.values()));
  if (minR !== 0) {
    for (const [k, v] of rank.entries()) rank.set(k, v - minR);
  }
  for (const n of parsed.nodes) {
    n.rank = rank.get(n.id) || 0;
  }

  // 3) Insert dummy virtual nodes for forward leaf-to-leaf edges spanning > 1 rank
  const layoutNodes = new Map<string, LayoutNode>();
  for (let idx = 0; idx < parsed.nodes.length; idx++) {
    const n = parsed.nodes[idx];
    layoutNodes.set(n.id, {
      id: n.id,
      isDummy: false,
      ref: n,
      w: n.w,
      h: n.h,
      rank: n.rank,
      order: idx,
      primaryCoord: 0,
      crossCoord: 0,
      x: 0,
      y: 0,
      subgraphChain: getSubgraphChain(n, subMap),
    });
  }

  const routedChains = new Map<number, string[]>();
  const layoutEdgesForOrder: Array<{ u: string; v: string }> = [];
  let dummySeq = 1;

  for (const e of nodeEdges) {
    if (e.isBackEdge || e.from === e.to) continue;
    const uNode = layoutNodes.get(e.from);
    const vNode = layoutNodes.get(e.to);
    if (!uNode || !vNode) continue;
    const dr = vNode.rank - uNode.rank;
    if (dr <= 1) {
      routedChains.set(e.index, [uNode.id, vNode.id]);
      if (dr === 1) layoutEdgesForOrder.push({ u: uNode.id, v: vNode.id });
      continue;
    }
    const chain = [uNode.id];
    let prevId = uNode.id;
    const sharedSub =
      uNode.subgraphChain[0] && uNode.subgraphChain[0] === vNode.subgraphChain[0]
        ? [uNode.subgraphChain[0]]
        : [];
    for (let r = uNode.rank + 1; r < vNode.rank; r++) {
      const did = `__dummy_${e.index}_${dummySeq++}`;
      const dummyW = e.label ? 26 : 10;
      const dummyH = e.label ? 22 : 10;
      layoutNodes.set(did, {
        id: did,
        isDummy: true,
        w: horizontal ? dummyW : dummyW,
        h: horizontal ? dummyH : dummyH,
        rank: r,
        order: 9999 + dummySeq,
        primaryCoord: 0,
        crossCoord: 0,
        x: 0,
        y: 0,
        subgraphChain: sharedSub,
      });
      layoutEdgesForOrder.push({ u: prevId, v: did });
      chain.push(did);
      prevId = did;
    }
    layoutEdgesForOrder.push({ u: prevId, v: vNode.id });
    chain.push(vNode.id);
    routedChains.set(e.index, chain);
  }

  // Group into ranks
  const maxRank = Math.max(0, ...Array.from(layoutNodes.values()).map((n) => n.rank));
  const ranks: LayoutNode[][] = Array.from({ length: maxRank + 1 }, () => []);
  for (const ln of layoutNodes.values()) {
    ranks[ln.rank].push(ln);
  }

  // Build adjacency for barycenter ordering
  const upNeighbors = new Map<string, string[]>();
  const downNeighbors = new Map<string, string[]>();
  for (const id of layoutNodes.keys()) {
    upNeighbors.set(id, []);
    downNeighbors.set(id, []);
  }
  for (const le of layoutEdgesForOrder) {
    downNeighbors.get(le.u)?.push(le.v);
    upNeighbors.get(le.v)?.push(le.u);
  }

  const updateRankIndices = () => {
    for (const layer of ranks) {
      layer.forEach((ln, idx) => {
        ln.order = idx;
      });
    }
  };
  updateRankIndices();

  // Keep nodes sharing the same top-level subgraph contiguous so sibling subgraph frames never overlap!
  const sortLayerWithSubgraphContiguity = (
    layer: LayoutNode[],
    baryMap: Map<string, number>
  ): LayoutNode[] => {
    const groupScore = new Map<string, number[]>();
    for (const n of layer) {
      const key = n.subgraphChain[0] || `__leaf_${n.id}`;
      const arr = groupScore.get(key) || [];
      arr.push(baryMap.get(n.id) ?? n.order);
      groupScore.set(key, arr);
    }
    const groupMean = new Map<string, number>();
    for (const [k, arr] of groupScore.entries()) {
      groupMean.set(k, arr.reduce((a, b) => a + b, 0) / arr.length);
    }

    return [...layer].sort((a, b) => {
      const ga = a.subgraphChain[0] || `__leaf_${a.id}`;
      const gb = b.subgraphChain[0] || `__leaf_${b.id}`;
      if (ga !== gb) {
        const diff = (groupMean.get(ga) ?? 0) - (groupMean.get(gb) ?? 0);
        if (Math.abs(diff) > 0.01) return diff;
        return ga.localeCompare(gb);
      }
      // Sub-level grouping if nested
      const ga2 = a.subgraphChain[1] || a.id;
      const gb2 = b.subgraphChain[1] || b.id;
      if (a.subgraphChain[1] && b.subgraphChain[1] && ga2 !== gb2) {
        return ga2.localeCompare(gb2);
      }
      const ba = baryMap.get(a.id) ?? a.order;
      const bb = baryMap.get(b.id) ?? b.order;
      if (Math.abs(ba - bb) > 0.01) return ba - bb;
      return a.order - b.order;
    });
  };

  for (let sweep = 0; sweep < 8; sweep++) {
    // Top -> Bottom
    for (let r = 1; r <= maxRank; r++) {
      const bary = new Map<string, number>();
      for (const ln of ranks[r]) {
        const ups = upNeighbors.get(ln.id) || [];
        if (ups.length === 0) {
          bary.set(ln.id, ln.order);
        } else {
          const sum = ups.reduce((acc, uid) => acc + (layoutNodes.get(uid)?.order ?? 0), 0);
          bary.set(ln.id, sum / ups.length);
        }
      }
      ranks[r] = sortLayerWithSubgraphContiguity(ranks[r], bary);
      updateRankIndices();
    }
    // Bottom -> Top
    for (let r = maxRank - 1; r >= 0; r--) {
      const bary = new Map<string, number>();
      for (const ln of ranks[r]) {
        const downs = downNeighbors.get(ln.id) || [];
        if (downs.length === 0) {
          bary.set(ln.id, ln.order);
        } else {
          const sum = downs.reduce((acc, did) => acc + (layoutNodes.get(did)?.order ?? 0), 0);
          bary.set(ln.id, sum / downs.length);
        }
      }
      ranks[r] = sortLayerWithSubgraphContiguity(ranks[r], bary);
      updateRankIndices();
    }
  }

  // 4) Orientation-aware coordinate assignment!
  // Primary axis = direction of flow (Y for TB/BT, X for LR/RL)
  // Cross axis = perpendicular to flow (X for TB/BT, Y for LR/RL)
  const primarySizeOf = (ln: LayoutNode): number => (horizontal ? ln.w : ln.h);
  const crossSizeOf = (ln: LayoutNode): number => (horizontal ? ln.h : ln.w);

  const hasSubgraphs = parsed.subgraphs.length > 0;
  const RANK_GAP = horizontal ? (hasSubgraphs ? 84 : 72) : hasSubgraphs ? 72 : 60;
  const CROSS_GAP = horizontal ? 34 : 36;
  const SUBGRAPH_EXTRA_CROSS_GAP = 24;

  // Assign primary coordinates per rank using max primarySize in that rank
  const rankPrimaryCenter: number[] = [];
  let primaryCursor = 0;
  for (let r = 0; r <= maxRank; r++) {
    const layer = ranks[r];
    const maxP = layer.length
      ? Math.max(...layer.map((ln) => (ln.isDummy ? 12 : primarySizeOf(ln))))
      : 44;
    const center = primaryCursor + maxP / 2;
    rankPrimaryCenter.push(center);
    for (const ln of layer) {
      ln.primaryCoord = center;
    }
    primaryCursor += maxP + RANK_GAP;
  }

  // Assign cross coordinates per rank, adding subtle extra breathing room between distinct subgraphs
  for (let r = 0; r <= maxRank; r++) {
    const layer = ranks[r];
    let cursor = 0;
    for (let i = 0; i < layer.length; i++) {
      const ln = layer[i];
      if (i > 0) {
        const prev = layer[i - 1];
        const prevSub = prev.subgraphChain[0] || '';
        const curSub = ln.subgraphChain[0] || '';
        const subGap = prevSub !== curSub && (prevSub || curSub) ? SUBGRAPH_EXTRA_CROSS_GAP : 0;
        cursor += CROSS_GAP + subGap;
      }
      const cSz = crossSizeOf(ln);
      ln.crossCoord = cursor + cSz / 2;
      cursor += cSz;
    }
    // Center this rank around 0 initially
    const totalW = cursor;
    for (const ln of layer) {
      ln.crossCoord -= totalW / 2;
    }
  }

  // Iterative symmetry & parent/child alignment without creating overlap inside a rank
  const resolveRankOverlaps = (layer: LayoutNode[]) => {
    for (let i = 1; i < layer.length; i++) {
      const prev = layer[i - 1];
      const cur = layer[i];
      const prevSub = prev.subgraphChain[0] || '';
      const curSub = cur.subgraphChain[0] || '';
      const gap = CROSS_GAP + (prevSub !== curSub && (prevSub || curSub) ? SUBGRAPH_EXTRA_CROSS_GAP : 0);
      const minCenter = prev.crossCoord + crossSizeOf(prev) / 2 + gap + crossSizeOf(cur) / 2;
      if (cur.crossCoord < minCenter) {
        cur.crossCoord = minCenter;
      }
    }
    for (let i = layer.length - 2; i >= 0; i--) {
      const next = layer[i + 1];
      const cur = layer[i];
      const nextSub = next.subgraphChain[0] || '';
      const curSub = cur.subgraphChain[0] || '';
      const gap = CROSS_GAP + (nextSub !== curSub && (nextSub || curSub) ? SUBGRAPH_EXTRA_CROSS_GAP : 0);
      const maxCenter = next.crossCoord - crossSizeOf(next) / 2 - gap - crossSizeOf(cur) / 2;
      if (cur.crossCoord > maxCenter) {
        cur.crossCoord = maxCenter;
      }
    }
  };

  for (let pass = 0; pass < 4; pass++) {
    // Downward pull toward parents
    for (let r = 1; r <= maxRank; r++) {
      for (const ln of ranks[r]) {
        const ups = (upNeighbors.get(ln.id) || [])
          .map((id) => layoutNodes.get(id))
          .filter((n): n is LayoutNode => Boolean(n));
        if (ups.length > 0) {
          const avg = ups.reduce((s, p) => s + p.crossCoord, 0) / ups.length;
          ln.crossCoord = ln.crossCoord * 0.35 + avg * 0.65;
        }
      }
      resolveRankOverlaps(ranks[r]);
    }
    // Upward pull toward children (centers diamond/hub parents nicely over multiple child branches!)
    for (let r = maxRank - 1; r >= 0; r--) {
      for (const ln of ranks[r]) {
        const downs = (downNeighbors.get(ln.id) || [])
          .map((id) => layoutNodes.get(id))
          .filter((n): n is LayoutNode => Boolean(n));
        if (downs.length > 0) {
          const avg = downs.reduce((s, c) => s + c.crossCoord, 0) / downs.length;
          ln.crossCoord = ln.crossCoord * 0.25 + avg * 0.75;
        }
      }
      resolveRankOverlaps(ranks[r]);
    }
  }

  // Flip primary axis if BT / RL
  const maxPrimary =
    rankPrimaryCenter.length > 0
      ? Math.max(...ranks.flat().map((ln) => ln.primaryCoord + primarySizeOf(ln) / 2))
      : 0;
  for (const ln of layoutNodes.values()) {
    const p = reversedAxis ? maxPrimary - ln.primaryCoord : ln.primaryCoord;
    if (horizontal) {
      ln.x = p;
      ln.y = ln.crossCoord;
    } else {
      ln.x = ln.crossCoord;
      ln.y = p;
    }
  }
  for (const n of parsed.nodes) {
    const ln = layoutNodes.get(n.id);
    if (ln) {
      n.x = ln.x;
      n.y = ln.y;
    }
  }

  // 5) Compute bottom-up Subgraph bounding rectangles BEFORE outer margins
  const computeSubgraphBox = (sub: FlowSubgraph, depth = 0): { minX: number; minY: number; maxX: number; maxY: number } | null => {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;

    for (const nid of sub.nodeIds) {
      const n = leafMap.get(nid);
      if (!n) continue;
      minX = Math.min(minX, n.x - n.w / 2);
      minY = Math.min(minY, n.y - n.h / 2);
      maxX = Math.max(maxX, n.x + n.w / 2);
      maxY = Math.max(maxY, n.y + n.h / 2);
    }
    for (const childId of sub.childSubgraphIds) {
      const child = subMap.get(childId);
      if (!child) continue;
      const box = computeSubgraphBox(child, depth + 1);
      if (box) {
        minX = Math.min(minX, box.minX);
        minY = Math.min(minY, box.minY);
        maxX = Math.max(maxX, box.maxX);
        maxY = Math.max(maxY, box.maxY);
      }
    }
    if (!Number.isFinite(minX)) return null;
    const padX = 22;
    const padTop = 38;
    const padBot = 18;
    sub.x = minX - padX;
    sub.y = minY - padTop;
    sub.w = maxX - minX + padX * 2;
    sub.h = maxY - minY + padTop + padBot;
    return {
      minX: sub.x,
      minY: sub.y,
      maxX: sub.x + sub.w,
      maxY: sub.y + sub.h,
    };
  };

  for (const s of parsed.subgraphs) {
    if (!s.parentId) computeSubgraphBox(s);
  }

  // 6) Shift everything into positive canvas space with crisp padding & room for back-edge outer channel
  let globalMinX = Infinity;
  let globalMinY = Infinity;
  let globalMaxX = -Infinity;
  let globalMaxY = -Infinity;
  for (const n of parsed.nodes) {
    globalMinX = Math.min(globalMinX, n.x - n.w / 2);
    globalMinY = Math.min(globalMinY, n.y - n.h / 2);
    globalMaxX = Math.max(globalMaxX, n.x + n.w / 2);
    globalMaxY = Math.max(globalMaxY, n.y + n.h / 2);
  }
  for (const s of parsed.subgraphs) {
    if (s.w > 0 && s.h > 0) {
      globalMinX = Math.min(globalMinX, s.x);
      globalMinY = Math.min(globalMinY, s.y);
      globalMaxX = Math.max(globalMaxX, s.x + s.w);
      globalMaxY = Math.max(globalMaxY, s.y + s.h);
    }
  }

  const backEdgeCount = nodeEdges.filter((e) => e.isBackEdge).length;
  const padLeft = 36;
  const padTop = 30;
  const padRight = 36 + (backEdgeCount > 0 ? (horizontal ? 24 : 52 + backEdgeCount * 16) : 0);
  const padBottom = 30 + (backEdgeCount > 0 && horizontal ? 52 + backEdgeCount * 16 : 0);

  const shiftX = padLeft - globalMinX;
  const shiftY = padTop - globalMinY;

  for (const n of parsed.nodes) {
    n.x += shiftX;
    n.y += shiftY;
  }
  for (const ln of layoutNodes.values()) {
    ln.x += shiftX;
    ln.y += shiftY;
  }
  for (const s of parsed.subgraphs) {
    if (s.w > 0) {
      s.x += shiftX;
      s.y += shiftY;
    }
  }

  const canvasW = Math.max(240, Math.ceil(globalMaxX - globalMinX + padLeft + padRight));
  const canvasH = Math.max(140, Math.ceil(globalMaxY - globalMinY + padTop + padBottom));

  // Helpers for anchor points on node/subgraph rectangles & diamonds
  const getBoxOfEndpoint = (
    id: string
  ): { cx: number; cy: number; w: number; h: number; shape: FlowShape; isSub: boolean } | null => {
    const n = leafMap.get(id);
    if (n) return { cx: n.x, cy: n.y, w: n.w, h: n.h, shape: n.shape, isSub: false };
    const s = subMap.get(id);
    if (s && s.w > 0) {
      return { cx: s.x + s.w / 2, cy: s.y + s.h / 2, w: s.w, h: s.h, shape: 'rect', isSub: true };
    }
    return null;
  };

  // Compute port offsets when multiple edges leave/enter the same node
  const outEdgesByNode = new Map<string, FlowEdge[]>();
  const inEdgesByNode = new Map<string, FlowEdge[]>();
  for (const e of nodeEdges) {
    if (e.isBackEdge) continue;
    const oa = outEdgesByNode.get(e.from) || [];
    oa.push(e);
    outEdgesByNode.set(e.from, oa);
    const ia = inEdgesByNode.get(e.to) || [];
    ia.push(e);
    inEdgesByNode.set(e.to, ia);
  }
  // Sort sibling outgoing edges by target cross-coordinate for clean fan-out
  for (const arr of outEdgesByNode.values()) {
    arr.sort((a, b) => {
      const ta = getBoxOfEndpoint(a.to);
      const tb = getBoxOfEndpoint(b.to);
      if (!ta || !tb) return 0;
      return horizontal ? ta.cy - tb.cy : ta.cx - tb.cx;
    });
  }
  for (const arr of inEdgesByNode.values()) {
    arr.sort((a, b) => {
      const sa = getBoxOfEndpoint(a.from);
      const sb = getBoxOfEndpoint(b.from);
      if (!sa || !sb) return 0;
      return horizontal ? sa.cy - sb.cy : sa.cx - sb.cx;
    });
  }

  const getPortAnchor = (
    box: { cx: number; cy: number; w: number; h: number; shape: FlowShape },
    role: 'out' | 'in',
    edge: FlowEdge,
    otherBox: { cx: number; cy: number; w: number; h: number }
  ): { x: number; y: number } => {
    const list =
      (role === 'out' ? outEdgesByNode.get(edge.from) : inEdgesByNode.get(edge.to)) || [edge];
    const idx = Math.max(0, list.indexOf(edge));
    const count = list.length;
    const spread = count > 1 ? (idx - (count - 1) / 2) * 14 : 0;

    // Decision diamond smart side exit when sibling branches clearly sit left/right of the diamond!
    if (role === 'out' && box.shape === 'diamond' && !horizontal && count >= 2) {
      const dx = otherBox.cx - box.cx;
      if (dx < -box.w * 0.28) {
        return { x: box.cx - box.w / 2, y: box.cy };
      }
      if (dx > box.w * 0.28) {
        return { x: box.cx + box.w / 2, y: box.cy };
      }
    }
    if (role === 'out' && box.shape === 'diamond' && horizontal && count >= 2) {
      const dy = otherBox.cy - box.cy;
      if (dy < -box.h * 0.28) {
        return { x: box.cx, y: box.cy - box.h / 2 };
      }
      if (dy > box.h * 0.28) {
        return { x: box.cx, y: box.cy + box.h / 2 };
      }
    }

    if (horizontal) {
      const forwardSign = parsed.direction === 'LR' ? 1 : -1;
      const sideSign = role === 'out' ? forwardSign : -forwardSign;
      const maxOffset = Math.max(0, box.h * 0.28);
      const oy = Math.max(-maxOffset, Math.min(maxOffset, spread));
      return { x: box.cx + sideSign * (box.w / 2), y: box.cy + oy };
    } else {
      const forwardSign = parsed.direction === 'TB' ? 1 : -1;
      const sideSign = role === 'out' ? forwardSign : -forwardSign;
      const maxOffset = Math.max(0, box.w * 0.28);
      const ox = Math.max(-maxOffset, Math.min(maxOffset, spread));
      return { x: box.cx + ox, y: box.cy + sideSign * (box.h / 2) };
    }
  };

  // Build SVG path & label position for every edge
  const edgeDrawItems: Array<{
    edge: FlowEdge;
    pathD: string;
    labelPos: { x: number; y: number; w: number; h: number; lines: string[] } | null;
    tone: 'default' | 'ok' | 'warn';
  }> = [];

  let backLane = 0;
  for (const e of nodeEdges) {
    const srcBox = getBoxOfEndpoint(e.from);
    const dstBox = getBoxOfEndpoint(e.to);
    if (!srcBox || !dstBox) continue;

    // Determine semantic edge tint from label
    const lblLower = e.label.trim().toLowerCase();
    let tone: 'default' | 'ok' | 'warn' = 'default';
    if (
      /^(?:예|승인|성공|통과|완료|허용|정상|yes|ok|pass|true|approve|success)/i.test(lblLower)
    ) {
      tone = 'ok';
    } else if (
      /^(?:아니오|아니요|거절|반려|보류|실패|오류|예외|재시도|수정|no|fail|error|false|reject|retry)/i.test(
        lblLower
      )
    ) {
      tone = 'warn';
    }

    let pts: Array<{ x: number; y: number }> = [];
    let isSelf = false;

    if (e.from === e.to) {
      isSelf = true;
      const rX = srcBox.cx + srcBox.w / 2;
      const yTop = srcBox.cy - Math.min(14, srcBox.h * 0.24);
      const yBot = srcBox.cy + Math.min(14, srcBox.h * 0.24);
      const bulge = rX + 42;
      const d = `M ${rX.toFixed(1)} ${yTop.toFixed(1)} C ${bulge.toFixed(1)} ${(yTop - 14).toFixed(1)}, ${bulge.toFixed(1)} ${(yBot + 14).toFixed(1)}, ${rX.toFixed(1)} ${yBot.toFixed(1)}`;
      pts = [
        { x: rX, y: yTop },
        { x: bulge - 4, y: srcBox.cy },
        { x: rX, y: yBot },
      ];
      edgeDrawItems.push({
        edge: e,
        pathD: d,
        labelPos: buildEdgeLabelBox(e.label, bulge - 2, srcBox.cy),
        tone,
      });
      continue;
    }

    if (e.isBackEdge) {
      const laneOffset = 34 + backLane * 18;
      backLane++;
      if (!horizontal) {
        const laneX =
          Math.max(
            srcBox.cx + srcBox.w / 2,
            dstBox.cx + dstBox.w / 2,
            globalMaxX + shiftX - 4
          ) + laneOffset - (backEdgeCount > 1 ? 18 : 0);
        const startPt = { x: srcBox.cx + srcBox.w / 2, y: srcBox.cy };
        const endPt = { x: dstBox.cx + dstBox.w / 2, y: dstBox.cy };
        const clampedLaneX = Math.min(canvasW - 14, Math.max(startPt.x + 28, endPt.x + 28, laneX));
        const d =
          parsed.curve === 'ortho'
            ? buildOrthoPath([
                startPt,
                { x: clampedLaneX, y: startPt.y },
                { x: clampedLaneX, y: endPt.y },
                endPt,
              ])
            : `M ${startPt.x.toFixed(1)} ${startPt.y.toFixed(1)} C ${clampedLaneX.toFixed(1)} ${startPt.y.toFixed(1)}, ${clampedLaneX.toFixed(1)} ${endPt.y.toFixed(1)}, ${endPt.x.toFixed(1)} ${endPt.y.toFixed(1)}`;
        const midY = (startPt.y + endPt.y) / 2;
        edgeDrawItems.push({
          edge: e,
          pathD: d,
          labelPos: buildEdgeLabelBox(e.label, clampedLaneX - 8, midY),
          tone,
        });
      } else {
        const laneY =
          Math.max(
            srcBox.cy + srcBox.h / 2,
            dstBox.cy + dstBox.h / 2,
            globalMaxY + shiftY - 4
          ) + laneOffset - (backEdgeCount > 1 ? 18 : 0);
        const startPt = { x: srcBox.cx, y: srcBox.cy + srcBox.h / 2 };
        const endPt = { x: dstBox.cx, y: dstBox.cy + dstBox.h / 2 };
        const clampedLaneY = Math.min(canvasH - 14, Math.max(startPt.y + 28, endPt.y + 28, laneY));
        const d =
          parsed.curve === 'ortho'
            ? buildOrthoPath([
                startPt,
                { x: startPt.x, y: clampedLaneY },
                { x: endPt.x, y: clampedLaneY },
                endPt,
              ])
            : `M ${startPt.x.toFixed(1)} ${startPt.y.toFixed(1)} C ${startPt.x.toFixed(1)} ${clampedLaneY.toFixed(1)}, ${endPt.x.toFixed(1)} ${clampedLaneY.toFixed(1)}, ${endPt.x.toFixed(1)} ${endPt.y.toFixed(1)}`;
        const midX = (startPt.x + endPt.x) / 2;
        edgeDrawItems.push({
          edge: e,
          pathD: d,
          labelPos: buildEdgeLabelBox(e.label, midX, clampedLaneY - 6),
          tone,
        });
      }
      void isSelf;
      continue;
    }

    const startPt = getPortAnchor(srcBox, 'out', e, dstBox);
    const endPt = getPortAnchor(dstBox, 'in', e, srcBox);
    pts.push(startPt);

    const chain = routedChains.get(e.index);
    if (chain && chain.length > 2) {
      for (let k = 1; k < chain.length - 1; k++) {
        const dNode = layoutNodes.get(chain[k]);
        if (dNode) {
          pts.push({ x: dNode.x, y: dNode.y });
        }
      }
    }
    pts.push(endPt);

    const d =
      parsed.curve === 'ortho'
        ? buildFlowOrthoRoute(pts, horizontal)
        : buildFlowSmoothRoute(pts, horizontal);

    const labelAnchor = samplePolylineMidpoint(pts);
    edgeDrawItems.push({
      edge: e,
      pathD: d,
      labelPos: buildEdgeLabelBox(e.label, labelAnchor.x, labelAnchor.y),
      tone,
    });
  }

  // Nudge colliding edge label pills lightly so parallel edge badges stay legible
  for (let i = 0; i < edgeDrawItems.length; i++) {
    const a = edgeDrawItems[i].labelPos;
    if (!a) continue;
    for (let j = 0; j < i; j++) {
      const b = edgeDrawItems[j].labelPos;
      if (!b) continue;
      if (Math.abs(a.x - b.x) < (a.w + b.w) / 2 + 6 && Math.abs(a.y - b.y) < (a.h + b.h) / 2 + 4) {
        a.y += (a.y >= b.y ? 1 : -1) * (b.h / 2 + a.h / 2 + 5);
      }
    }
  }

  // 7) Emit clean high-DPI Vector SVG!
  const markerDefs = buildFlowDefs(prefix, options.forPrint);
  const subSvg = parsed.subgraphs
    .filter((s) => s.w > 0 && s.h > 0)
    .map((s) => renderSubgraphBoxSvg(s, prefix))
    .join('\n');

  const edgesSvg = edgeDrawItems
    .map((item) => renderEdgeSvg(item, prefix))
    .join('\n');

  const labelsSvg = edgeDrawItems
    .filter((item) => item.labelPos)
    .map((item) => renderEdgeLabelPillSvg(item.labelPos!, item.tone))
    .join('\n');

  const nodesSvg = parsed.nodes
    .map((n) => renderFlowNodeSvg(n, parsed.classDefs, prefix, options.forPrint))
    .join('\n');

  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" class="onyx-flow-svg" viewBox="0 0 ${canvasW} ${canvasH}" width="${canvasW}" height="${canvasH}" role="img" aria-label="Flowchart Diagram">`,
    markerDefs,
    `<rect x="0" y="0" width="${canvasW}" height="${canvasH}" rx="12" fill="var(--flow-canvas-bg, transparent)" />`,
    `<g class="onyx-flow-subgraphs">${subSvg}</g>`,
    `<g class="onyx-flow-edges">${edgesSvg}</g>`,
    `<g class="onyx-flow-nodes">${nodesSvg}</g>`,
    `<g class="onyx-flow-edge-labels">${labelsSvg}</g>`,
    `</svg>`,
  ].join('\n');

  return {
    svg,
    kind: 'flowchart',
    direction: parsed.direction,
    theme: parsed.theme,
    curve: parsed.curve,
    nodeCount: parsed.nodes.length,
    edgeCount: nodeEdges.length,
    width: canvasW,
    height: canvasH,
    warnings: parsed.warnings,
    layout: {
      nodes: parsed.nodes.map((n) => ({
        id: n.id,
        x: n.x,
        y: n.y,
        w: n.w,
        h: n.h,
        rank: n.rank,
      })),
      subgraphs: parsed.subgraphs
        .filter((s) => s.w > 0 && s.h > 0)
        .map((s) => ({ id: s.id, x: s.x, y: s.y, w: s.w, h: s.h })),
    },
  };
}

function buildEdgeLabelBox(
  label: string,
  cx: number,
  cy: number
): { x: number; y: number; w: number; h: number; lines: string[] } | null {
  if (!label || !label.trim()) return null;
  const m = wrapLabelLines(label, 140, 0.85);
  return {
    x: cx,
    y: cy,
    w: Math.max(28, Math.ceil(m.widthPx + 18)),
    h: Math.max(20, Math.ceil(m.heightPx + 6)),
    lines: m.lines,
  };
}

function samplePolylineMidpoint(pts: Array<{ x: number; y: number }>): { x: number; y: number } {
  if (pts.length === 0) return { x: 0, y: 0 };
  if (pts.length === 1) return pts[0];
  let totalLen = 0;
  const segLens: number[] = [];
  for (let i = 1; i < pts.length; i++) {
    const d = Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y);
    segLens.push(d);
    totalLen += d;
  }
  if (totalLen < 1) return pts[0];
  let target = totalLen * 0.5;
  for (let i = 0; i < segLens.length; i++) {
    if (target <= segLens[i] || i === segLens.length - 1) {
      const t = segLens[i] > 0 ? target / segLens[i] : 0.5;
      return {
        x: pts[i].x + (pts[i + 1].x - pts[i].x) * t,
        y: pts[i].y + (pts[i + 1].y - pts[i].y) * t,
      };
    }
    target -= segLens[i];
  }
  return pts[0];
}

function buildFlowSmoothRoute(
  pts: Array<{ x: number; y: number }>,
  horizontal: boolean
): string {
  if (pts.length < 2) return '';
  if (pts.length === 2) {
    const [a, b] = pts;
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    if ((!horizontal && Math.abs(dx) < 2) || (horizontal && Math.abs(dy) < 2)) {
      return `M ${a.x.toFixed(1)} ${a.y.toFixed(1)} L ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
    }
    if (!horizontal) {
      const c1y = a.y + dy * 0.52;
      const c2y = b.y - dy * 0.52;
      return `M ${a.x.toFixed(1)} ${a.y.toFixed(1)} C ${a.x.toFixed(1)} ${c1y.toFixed(1)}, ${b.x.toFixed(1)} ${c2y.toFixed(1)}, ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
    } else {
      const c1x = a.x + dx * 0.52;
      const c2x = b.x - dx * 0.52;
      return `M ${a.x.toFixed(1)} ${a.y.toFixed(1)} C ${c1x.toFixed(1)} ${a.y.toFixed(1)}, ${c2x.toFixed(1)} ${b.y.toFixed(1)}, ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
    }
  }
  // Multi-point smooth spline through dummy waypoints
  let d = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
  for (let i = 1; i < pts.length; i++) {
    const p0 = pts[i - 1];
    const p1 = pts[i];
    if (!horizontal) {
      const my = (p0.y + p1.y) / 2;
      d += ` C ${p0.x.toFixed(1)} ${my.toFixed(1)}, ${p1.x.toFixed(1)} ${my.toFixed(1)}, ${p1.x.toFixed(1)} ${p1.y.toFixed(1)}`;
    } else {
      const mx = (p0.x + p1.x) / 2;
      d += ` C ${mx.toFixed(1)} ${p0.y.toFixed(1)}, ${mx.toFixed(1)} ${p1.y.toFixed(1)}, ${p1.x.toFixed(1)} ${p1.y.toFixed(1)}`;
    }
  }
  return d;
}

function buildFlowOrthoRoute(
  pts: Array<{ x: number; y: number }>,
  horizontal: boolean
): string {
  const expanded: Array<{ x: number; y: number }> = [pts[0]];
  for (let i = 1; i < pts.length; i++) {
    const a = expanded[expanded.length - 1];
    const b = pts[i];
    if (!horizontal) {
      if (Math.abs(b.x - a.x) > 2) {
        const midY = (a.y + b.y) / 2;
        expanded.push({ x: a.x, y: midY }, { x: b.x, y: midY });
      }
    } else {
      if (Math.abs(b.y - a.y) > 2) {
        const midX = (a.x + b.x) / 2;
        expanded.push({ x: midX, y: a.y }, { x: midX, y: b.y });
      }
    }
    expanded.push(b);
  }
  return buildOrthoPath(expanded);
}

function buildOrthoPath(points: Array<{ x: number; y: number }>): string {
  if (points.length < 2) return '';
  // Deduplicate adjacent identical points
  const pts: Array<{ x: number; y: number }> = [points[0]];
  for (let i = 1; i < points.length; i++) {
    const prev = pts[pts.length - 1];
    if (Math.hypot(points[i].x - prev.x, points[i].y - prev.y) > 0.8) {
      pts.push(points[i]);
    }
  }
  if (pts.length < 2) return '';
  let d = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
  const radius = 10;
  for (let i = 1; i < pts.length - 1; i++) {
    const prev = pts[i - 1];
    const cur = pts[i];
    const next = pts[i + 1];
    const d1 = Math.hypot(cur.x - prev.x, cur.y - prev.y);
    const d2 = Math.hypot(next.x - cur.x, next.y - cur.y);
    const r = Math.min(radius, d1 * 0.45, d2 * 0.45);
    if (r < 1.5) {
      d += ` L ${cur.x.toFixed(1)} ${cur.y.toFixed(1)}`;
      continue;
    }
    const ux1 = (cur.x - prev.x) / d1;
    const uy1 = (cur.y - prev.y) / d1;
    const ux2 = (next.x - cur.x) / d2;
    const uy2 = (next.y - cur.y) / d2;
    const inX = cur.x - ux1 * r;
    const inY = cur.y - uy1 * r;
    const outX = cur.x + ux2 * r;
    const outY = cur.y + uy2 * r;
    d += ` L ${inX.toFixed(1)} ${inY.toFixed(1)} Q ${cur.x.toFixed(1)} ${cur.y.toFixed(1)}, ${outX.toFixed(1)} ${outY.toFixed(1)}`;
  }
  const last = pts[pts.length - 1];
  d += ` L ${last.x.toFixed(1)} ${last.y.toFixed(1)}`;
  return d;
}

function buildFlowDefs(prefix: string, forPrint?: boolean): string {
  const mkArrow = (id: string, colorVar: string) => `
    <marker id="${prefix}-${id}" viewBox="0 0 12 12" refX="10" refY="6" markerWidth="7.5" markerHeight="7.5" orient="auto-start-reverse" markerUnits="strokeWidth">
      <path d="M 1.5 2 L 10.5 6 L 1.5 10 z" fill="${colorVar}" />
    </marker>`;
  const mkCircle = (id: string, colorVar: string) => `
    <marker id="${prefix}-${id}" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" orient="auto">
      <circle cx="5" cy="5" r="3.4" fill="var(--flow-surface, #ffffff)" stroke="${colorVar}" stroke-width="1.8" />
    </marker>`;
  const mkCross = (id: string, colorVar: string) => `
    <marker id="${prefix}-${id}" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto">
      <path d="M 2 2 L 8 8 M 8 2 L 2 8" stroke="${colorVar}" stroke-width="1.9" stroke-linecap="round" />
    </marker>`;

  const shadowDef = forPrint
    ? ''
    : `<filter id="${prefix}-shadow" x="-12%" y="-12%" width="124%" height="132%">
        <feDropShadow dx="0" dy="2" stdDeviation="2.6" flood-color="#0f172a" flood-opacity="0.08" />
      </filter>`;

  return `<defs>
    ${shadowDef}
    ${mkArrow('arr-default', 'var(--flow-edge, #64748b)')}
    ${mkArrow('arr-ok', 'var(--flow-ok-stroke, #0f7b6c)')}
    ${mkArrow('arr-warn', 'var(--flow-warn-stroke, #d97706)')}
    ${mkCircle('circ-default', 'var(--flow-edge, #64748b)')}
    ${mkCross('cross-default', 'var(--flow-edge, #64748b)')}
  </defs>`;
}

function renderSubgraphBoxSvg(sub: FlowSubgraph, _prefix: string): string {
  const pillText = sub.title || sub.id;
  const pillW = Math.min(sub.w - 20, Math.max(66, Math.ceil(measureLinePx(pillText, 0.88) + 22)));
  return `<g class="onyx-flow-subgraph">
    <rect x="${sub.x.toFixed(1)}" y="${sub.y.toFixed(1)}" width="${sub.w.toFixed(1)}" height="${sub.h.toFixed(1)}" rx="14" ry="14" fill="var(--flow-subg-bg, rgba(241,245,249,0.72))" stroke="var(--flow-subg-border, #cbd5e1)" stroke-width="1.4" stroke-dasharray="6 4" />
    <rect x="${(sub.x + 12).toFixed(1)}" y="${(sub.y + 8).toFixed(1)}" width="${pillW}" height="22" rx="7" fill="var(--flow-subg-pill, #e2e8f0)" stroke="var(--flow-subg-border, #cbd5e1)" stroke-width="1" />
    ${renderSvgTextLines([pillText], sub.x + 12 + pillW / 2, sub.y + 19, {
      fontSize: 11.5,
      fontWeight: 700,
      fill: 'var(--flow-subg-text, #334155)',
    })}
  </g>`;
}

function renderEdgeSvg(
  item: { edge: FlowEdge; pathD: string; tone: 'default' | 'ok' | 'warn' },
  prefix: string
): string {
  const { edge, pathD, tone } = item;
  let stroke =
    tone === 'ok'
      ? 'var(--flow-ok-stroke, #0f7b6c)'
      : tone === 'warn'
        ? 'var(--flow-warn-stroke, #d97706)'
        : 'var(--flow-edge, #64748b)';
  if (edge.inlineStyle?.stroke) stroke = edge.inlineStyle.stroke;

  const sw =
    edge.inlineStyle?.strokeWidth ||
    (edge.lineStyle === 'thick' ? '2.8' : edge.isBackEdge ? '1.6' : '1.9');
  const dash =
    edge.inlineStyle?.strokeDasharray ||
    (edge.lineStyle === 'dotted' ? '5 4' : edge.isBackEdge ? '6 4' : '');

  const arrSuffix = tone === 'ok' ? 'ok' : tone === 'warn' ? 'warn' : 'default';
  const mEnd =
    edge.arrowEnd === 'arrow'
      ? ` marker-end="url(#${prefix}-arr-${arrSuffix})"`
      : edge.arrowEnd === 'circle'
        ? ` marker-end="url(#${prefix}-circ-default)"`
        : edge.arrowEnd === 'cross'
          ? ` marker-end="url(#${prefix}-cross-default)"`
          : '';
  const mStart =
    edge.arrowStart === 'arrow'
      ? ` marker-start="url(#${prefix}-arr-${arrSuffix})"`
      : edge.arrowStart === 'circle'
        ? ` marker-start="url(#${prefix}-circ-default)"`
        : edge.arrowStart === 'cross'
          ? ` marker-start="url(#${prefix}-cross-default)"`
          : '';

  return `<path d="${pathD}" fill="none" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(sw)}"${dash ? ` stroke-dasharray="${escapeXml(dash)}"` : ''} stroke-linecap="round" stroke-linejoin="round"${mStart}${mEnd} />`;
}

function renderEdgeLabelPillSvg(
  pos: { x: number; y: number; w: number; h: number; lines: string[] },
  tone: 'default' | 'ok' | 'warn'
): string {
  const bg =
    tone === 'ok'
      ? 'var(--flow-ok-pill, #dcfce7)'
      : tone === 'warn'
        ? 'var(--flow-warn-pill, #fef3c7)'
        : 'var(--flow-pill-bg, #ffffff)';
  const stroke =
    tone === 'ok'
      ? 'var(--flow-ok-stroke, #0f7b6c)'
      : tone === 'warn'
        ? 'var(--flow-warn-stroke, #d97706)'
        : 'var(--flow-border, #cbd5e1)';
  const txt =
    tone === 'ok'
      ? 'var(--flow-ok-text, #065f46)'
      : tone === 'warn'
        ? 'var(--flow-warn-text, #92400e)'
        : 'var(--flow-text, #334155)';

  return `<g class="onyx-flow-edge-label">
    <rect x="${(pos.x - pos.w / 2).toFixed(1)}" y="${(pos.y - pos.h / 2).toFixed(1)}" width="${pos.w}" height="${pos.h}" rx="10" ry="10" fill="${bg}" stroke="${stroke}" stroke-width="1.1" />
    ${renderSvgTextLines(pos.lines, pos.x, pos.y, {
      fontSize: 11,
      fontWeight: 700,
      fill: txt,
      lineHeight: 14,
    })}
  </g>`;
}

function resolveNodePalette(
  node: FlowNode,
  classDefs: Record<string, FlowStyleProps>
): { fill: string; stroke: string; text: string; strokeWidth: string } {
  let fill = 'var(--flow-surface, #ffffff)';
  let stroke = 'var(--flow-accent, #2383e2)';
  let text = 'var(--flow-text, #1e293b)';
  let strokeWidth = '1.7';

  if (node.shape === 'stadium' || node.shape === 'circle' || node.shape === 'double-circle') {
    fill = 'var(--flow-ok-fill, #ecfdf5)';
    stroke = 'var(--flow-ok-stroke, #0f7b6c)';
  } else if (node.shape === 'diamond' || node.shape === 'hexagon') {
    fill = 'var(--flow-warn-fill, #fffbeb)';
    stroke = 'var(--flow-warn-stroke, #d97706)';
  } else if (node.shape === 'cylinder') {
    fill = 'var(--flow-db-fill, #ecfeff)';
    stroke = 'var(--flow-db-stroke, #0891b2)';
  } else if (node.shape === 'subroutine') {
    fill = 'var(--flow-sub-fill, #f5f3ff)';
    stroke = 'var(--flow-sub-stroke, #7c3aed)';
  } else {
    fill = 'var(--flow-accent-fill, #ffffff)';
    stroke = 'var(--flow-accent, #3b82f6)';
  }

  for (const cname of node.classNames) {
    const def = classDefs[cname];
    if (!def) continue;
    if (def.fill) fill = def.fill;
    if (def.stroke) stroke = def.stroke;
    if (def.color) text = def.color;
    if (def.strokeWidth) strokeWidth = def.strokeWidth;
  }
  if (node.inlineStyle) {
    if (node.inlineStyle.fill) fill = node.inlineStyle.fill;
    if (node.inlineStyle.stroke) stroke = node.inlineStyle.stroke;
    if (node.inlineStyle.color) text = node.inlineStyle.color;
    if (node.inlineStyle.strokeWidth) strokeWidth = node.inlineStyle.strokeWidth;
  }
  return { fill, stroke, text, strokeWidth };
}

function renderFlowNodeSvg(
  n: FlowNode,
  classDefs: Record<string, FlowStyleProps>,
  prefix: string,
  forPrint?: boolean
): string {
  const { fill, stroke, text, strokeWidth } = resolveNodePalette(n, classDefs);
  const x = n.x - n.w / 2;
  const y = n.y - n.h / 2;
  const w = n.w;
  const h = n.h;
  const cx = n.x;
  const cy = n.y;
  const filterAttr = forPrint ? '' : ` filter="url(#${prefix}-shadow)"`;

  let shapeSvg = '';
  let textCenterY = cy;

  switch (n.shape) {
    case 'rect':
      shapeSvg = `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${w}" height="${h}" rx="5" ry="5" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}"${filterAttr} />`;
      break;
    case 'round':
      shapeSvg = `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${w}" height="${h}" rx="12" ry="12" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}"${filterAttr} />`;
      break;
    case 'stadium':
      shapeSvg = `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${w}" height="${h}" rx="${(h / 2).toFixed(1)}" ry="${(h / 2).toFixed(1)}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}"${filterAttr} />`;
      break;
    case 'subroutine': {
      const bar = 9;
      shapeSvg = `<g${filterAttr}>
        <rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${w}" height="${h}" rx="9" ry="9" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" />
        <line x1="${(x + bar).toFixed(1)}" y1="${y.toFixed(1)}" x2="${(x + bar).toFixed(1)}" y2="${(y + h).toFixed(1)}" stroke="${escapeXml(stroke)}" stroke-width="1.3" opacity="0.65" />
        <line x1="${(x + w - bar).toFixed(1)}" y1="${y.toFixed(1)}" x2="${(x + w - bar).toFixed(1)}" y2="${(y + h).toFixed(1)}" stroke="${escapeXml(stroke)}" stroke-width="1.3" opacity="0.65" />
      </g>`;
      break;
    }
    case 'cylinder': {
      const rx = (w / 2).toFixed(1);
      const ry = 8;
      const bodyTop = y + ry;
      const bodyBot = y + h - ry;
      textCenterY = cy + 3;
      shapeSvg = `<g${filterAttr}>
        <path d="M ${x.toFixed(1)} ${bodyTop.toFixed(1)} L ${x.toFixed(1)} ${bodyBot.toFixed(1)} A ${rx} ${ry} 0 0 0 ${(x + w).toFixed(1)} ${bodyBot.toFixed(1)} L ${(x + w).toFixed(1)} ${bodyTop.toFixed(1)} Z" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" />
        <ellipse cx="${cx.toFixed(1)}" cy="${bodyTop.toFixed(1)}" rx="${rx}" ry="${ry}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" />
      </g>`;
      break;
    }
    case 'circle':
      shapeSvg = `<circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${(w / 2).toFixed(1)}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}"${filterAttr} />`;
      break;
    case 'double-circle':
      shapeSvg = `<g${filterAttr}>
        <circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${(w / 2).toFixed(1)}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" />
        <circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${Math.max(10, w / 2 - 5).toFixed(1)}" fill="none" stroke="${escapeXml(stroke)}" stroke-width="1.3" />
      </g>`;
      break;
    case 'diamond': {
      const pts = `${cx.toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${cy.toFixed(1)} ${cx.toFixed(1)},${(y + h).toFixed(1)} ${x.toFixed(1)},${cy.toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'hexagon': {
      const ins = 15;
      const pts = `${(x + ins).toFixed(1)},${y.toFixed(1)} ${(x + w - ins).toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${cy.toFixed(1)} ${(x + w - ins).toFixed(1)},${(y + h).toFixed(1)} ${(x + ins).toFixed(1)},${(y + h).toFixed(1)} ${x.toFixed(1)},${cy.toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'lean-right': {
      const sk = 14;
      const pts = `${(x + sk).toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${y.toFixed(1)} ${(x + w - sk).toFixed(1)},${(y + h).toFixed(1)} ${x.toFixed(1)},${(y + h).toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'lean-left': {
      const sk = 14;
      const pts = `${x.toFixed(1)},${y.toFixed(1)} ${(x + w - sk).toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${(y + h).toFixed(1)} ${(x + sk).toFixed(1)},${(y + h).toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'trapezoid': {
      const sk = 13;
      const pts = `${(x + sk).toFixed(1)},${y.toFixed(1)} ${(x + w - sk).toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${(y + h).toFixed(1)} ${x.toFixed(1)},${(y + h).toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'inv-trapezoid': {
      const sk = 13;
      const pts = `${x.toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${y.toFixed(1)} ${(x + w - sk).toFixed(1)},${(y + h).toFixed(1)} ${(x + sk).toFixed(1)},${(y + h).toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
    case 'asymmetric': {
      const ins = 13;
      const pts = `${x.toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${y.toFixed(1)} ${(x + w).toFixed(1)},${(y + h).toFixed(1)} ${x.toFixed(1)},${(y + h).toFixed(1)} ${(x + ins).toFixed(1)},${cy.toFixed(1)}`;
      shapeSvg = `<polygon points="${pts}" fill="${escapeXml(fill)}" stroke="${escapeXml(stroke)}" stroke-width="${escapeXml(strokeWidth)}" stroke-linejoin="round"${filterAttr} />`;
      break;
    }
  }

  return `<g class="onyx-flow-node" data-node-id="${escapeXml(n.id)}">
    ${shapeSvg}
    ${renderSvgTextLines(n.lines, cx, textCenterY, {
      fontSize: 13,
      fontWeight: n.shape === 'diamond' ? 700 : 600,
      fill: text,
      lineHeight: 18.5,
    })}
  </g>`;
}

/* ───────────────────── Companion Renderers: Sequence / Mindmap / Gantt ───────────────────── */

function renderSequenceDiagramSvg(
  code: string,
  theme: FlowThemeName,
  curve: FlowCurveMode,
  options: { forPrint?: boolean; svgIdPrefix?: string }
): DiagramRenderResult {
  const prefix = options.svgIdPrefix || `onyxseq-${svgInstanceSeq++}`;
  const { cleanedLines } = parseDiagramMeta(code);
  const participants: Array<{ id: string; label: string }> = [];
  const pSet = new Set<string>();
  const messages: Array<
    | { kind: 'msg'; from: string; to: string; text: string; dotted: boolean }
    | { kind: 'note'; over: string[]; text: string }
  > = [];

  const ensureP = (id: string, label?: string) => {
    const cleanId = id.trim();
    if (!cleanId) return;
    if (!pSet.has(cleanId)) {
      pSet.add(cleanId);
      participants.push({ id: cleanId, label: label ? stripOuterQuotes(label) : cleanId });
    } else if (label) {
      const found = participants.find((p) => p.id === cleanId);
      if (found) found.label = stripOuterQuotes(label);
    }
  };

  for (const raw of cleanedLines) {
    const line = raw.trim();
    if (!line || line.startsWith('%%') || /^sequenceDiagram\b/i.test(line)) continue;
    const pM = /^(?:participant|actor)\s+([^\s]+)(?:\s+as\s+(.+))?$/i.exec(line);
    if (pM) {
      ensureP(pM[1], pM[2]);
      continue;
    }
    const nM = /^Note\s+(?:right of|left of|over)\s+([^:]+):\s*(.+)$/i.exec(line);
    if (nM) {
      const targets = nM[1].split(',').map((s) => s.trim());
      targets.forEach((t) => ensureP(t));
      messages.push({ kind: 'note', over: targets, text: nM[2].trim() });
      continue;
    }
    const mM = /^([^\s-]+)\s*(-->>|->>|-->|->|-\)|--\))\s*\+?-?([^\s:]+)\s*:\s*(.*)$/.exec(line);
    if (mM) {
      ensureP(mM[1]);
      ensureP(mM[3]);
      messages.push({
        kind: 'msg',
        from: mM[1].trim(),
        to: mM[3].trim(),
        text: mM[4].trim(),
        dotted: mM[2].startsWith('--'),
      });
    }
  }

  if (participants.length === 0) {
    return {
      svg: renderEmptyHintSvg('시퀀스 다이어그램 참여자와 메시지를 입력해 주세요.', 380, 120),
      kind: 'sequence',
      direction: 'LR',
      theme,
      curve,
      nodeCount: 0,
      edgeCount: 0,
      width: 380,
      height: 120,
      warnings: [],
    };
  }

  const colGap = 158;
  const padX = 48;
  const topY = 26;
  const boxH = 40;
  const rowStep = 50;
  const colCenter = new Map<string, number>();
  participants.forEach((p, idx) => {
    colCenter.set(p.id, padX + 64 + idx * colGap);
  });

  const canvasW = Math.max(360, padX * 2 + 128 + (participants.length - 1) * colGap);
  const msgStartY = topY + boxH + 34;
  const msgEndY = msgStartY + Math.max(1, messages.length) * rowStep + 18;
  const canvasH = msgEndY + boxH + 26;

  const lifelines = participants
    .map((p) => {
      const cx = colCenter.get(p.id)!;
      return `<line x1="${cx}" y1="${topY + boxH}" x2="${cx}" y2="${msgEndY}" stroke="var(--flow-border, #cbd5e1)" stroke-width="1.5" stroke-dasharray="5 4" />`;
    })
    .join('\n');

  const actorBoxes = (yPos: number) =>
    participants
      .map((p) => {
        const cx = colCenter.get(p.id)!;
        const w = Math.max(106, Math.ceil(measureLinePx(p.label, 0.95) + 28));
        return `<rect x="${(cx - w / 2).toFixed(1)}" y="${yPos}" width="${w}" height="${boxH}" rx="10" fill="var(--flow-surface, #ffffff)" stroke="var(--flow-accent, #2383e2)" stroke-width="1.7" />
        ${renderSvgTextLines([p.label], cx, yPos + boxH / 2, {
          fontSize: 12.5,
          fontWeight: 700,
          fill: 'var(--flow-text, #1e293b)',
        })}`;
      })
      .join('\n');

  const rowsSvg = messages
    .map((m, idx) => {
      const y = msgStartY + idx * rowStep;
      if (m.kind === 'note') {
        const cx =
          m.over.reduce((s, id) => s + (colCenter.get(id) ?? 100), 0) / Math.max(1, m.over.length);
        const w = Math.max(120, Math.ceil(measureLinePx(m.text, 0.88) + 26));
        return `<rect x="${(cx - w / 2).toFixed(1)}" y="${y - 16}" width="${w}" height="32" rx="8" fill="var(--flow-warn-fill, #fffbeb)" stroke="var(--flow-warn-stroke, #d97706)" stroke-width="1.3" />
        ${renderSvgTextLines([m.text], cx, y, {
          fontSize: 11.5,
          fontWeight: 600,
          fill: 'var(--flow-text, #1e293b)',
        })}`;
      }
      const x1 = colCenter.get(m.from) ?? 80;
      const x2 = colCenter.get(m.to) ?? 200;
      const dash = m.dotted ? ' stroke-dasharray="5 4"' : '';
      const midX = (x1 + x2) / 2;
      const pillW = Math.min(colGap * 1.4, Math.max(44, Math.ceil(measureLinePx(m.text, 0.85) + 18)));
      const lineSvg =
        Math.abs(x1 - x2) < 2
          ? `<path d="M ${x1} ${y - 8} C ${x1 + 54} ${y - 8}, ${x1 + 54} ${y + 16}, ${x1 + 3} ${y + 16}" fill="none" stroke="var(--flow-edge, #64748b)" stroke-width="1.8"${dash} marker-end="url(#${prefix}-arr-default)" />`
          : `<line x1="${x1}" y1="${y}" x2="${x2}" y2="${y}" stroke="var(--flow-edge, #64748b)" stroke-width="1.8"${dash} marker-end="url(#${prefix}-arr-default)" />`;
      const lblSvg = m.text
        ? `<rect x="${(midX - pillW / 2).toFixed(1)}" y="${y - 23}" width="${pillW}" height="19" rx="7" fill="var(--flow-pill-bg, #ffffff)" stroke="var(--flow-border, #e2e8f0)" stroke-width="1" />
           ${renderSvgTextLines([m.text], midX, y - 13.5, {
             fontSize: 11,
             fontWeight: 600,
             fill: 'var(--flow-text, #334155)',
           })}`
        : '';
      return `${lineSvg}${lblSvg}`;
    })
    .join('\n');

  const svg = [
    `<svg xmlns="http://www.w3.org/2000/svg" class="onyx-flow-svg" viewBox="0 0 ${canvasW} ${canvasH}" width="${canvasW}" height="${canvasH}">`,
    buildFlowDefs(prefix, options.forPrint),
    lifelines,
    rowsSvg,
    actorBoxes(topY),
    actorBoxes(msgEndY),
    `</svg>`,
  ].join('\n');

  return {
    svg,
    kind: 'sequence',
    direction: 'SEQ',
    theme,
    curve,
    nodeCount: participants.length,
    edgeCount: messages.length,
    width: canvasW,
    height: canvasH,
    warnings: [],
  };
}

interface MindNode {
  label: string;
  indent: number;
  children: MindNode[];
  y: number;
  x: number;
  w: number;
  h: number;
  branchIdx: number;
}

function renderMindmapSvg(
  code: string,
  theme: FlowThemeName,
  curve: FlowCurveMode,
  _options: { forPrint?: boolean; svgIdPrefix?: string }
): DiagramRenderResult {
  const { cleanedLines } = parseDiagramMeta(code);
  const items: Array<{ indent: number; label: string }> = [];
  for (const raw of cleanedLines) {
    if (!raw.trim() || raw.trim().startsWith('%%') || /^mindmap\b/i.test(raw.trim())) continue;
    const indent = raw.search(/\S/);
    let label = raw.trim();
    label = label
      .replace(/^root\(\(([\s\S]+)\)\)$/i, '$1')
      .replace(/^\(\(([\s\S]+)\)\)$/, '$1')
      .replace(/^\[([\s\S]+)\]$/, '$1')
      .replace(/^\(([\s\S]+)\)$/, '$1');
    items.push({ indent, label: stripOuterQuotes(label) });
  }

  if (items.length === 0) {
    return {
      svg: renderEmptyHintSvg('마인드맵 계층 항목을 입력해 주세요.', 380, 120),
      kind: 'mindmap',
      direction: 'MIND',
      theme,
      curve,
      nodeCount: 0,
      edgeCount: 0,
      width: 380,
      height: 120,
      warnings: [],
    };
  }

  const root: MindNode = {
    label: items[0].label,
    indent: items[0].indent,
    children: [],
    x: 0,
    y: 0,
    w: Math.max(108, Math.ceil(measureLinePx(items[0].label, 1.05) + 36)),
    h: 46,
    branchIdx: 0,
  };
  const stack: MindNode[] = [root];
  let nodeCount = 1;
  for (let i = 1; i < items.length; i++) {
    const it = items[i];
    const node: MindNode = {
      label: it.label,
      indent: it.indent,
      children: [],
      x: 0,
      y: 0,
      w: Math.max(84, Math.ceil(measureLinePx(it.label, 0.95) + 28)),
      h: 34,
      branchIdx: 0,
    };
    while (stack.length > 1 && stack[stack.length - 1].indent >= it.indent) {
      stack.pop();
    }
    const parent = stack[stack.length - 1];
    node.branchIdx = parent === root ? parent.children.length : parent.branchIdx;
    parent.children.push(node);
    stack.push(node);
    nodeCount++;
  }

  const palette = ['#2563eb', '#0f7b6c', '#d97706', '#7c3aed', '#e11d48', '#0891b2'];
  let leafY = 34;
  let maxRight = 240;

  const layoutTree = (node: MindNode, depth: number, startX: number) => {
    node.x = startX + node.w / 2;
    maxRight = Math.max(maxRight, node.x + node.w / 2);
    if (node.children.length === 0) {
      node.y = leafY + node.h / 2;
      leafY += node.h + 14;
      return;
    }
    const childStartX = startX + node.w + 56;
    for (const c of node.children) {
      layoutTree(c, depth + 1, childStartX);
    }
    node.y = (node.children[0].y + node.children[node.children.length - 1].y) / 2;
  };
  layoutTree(root, 0, 32);

  const canvasW = Math.max(360, Math.ceil(maxRight + 36));
  const canvasH = Math.max(150, Math.ceil(leafY + 24));
  const paths: string[] = [];
  const boxes: string[] = [];

  const emitTree = (node: MindNode, depth: number) => {
    const color = depth === 0 ? 'var(--flow-accent, #2563eb)' : palette[node.branchIdx % palette.length];
    const bg = depth === 0 ? 'var(--flow-accent-fill, #eff6ff)' : 'var(--flow-surface, #ffffff)';
    for (const c of node.children) {
      const cCol = palette[c.branchIdx % palette.length];
      const x1 = node.x + node.w / 2;
      const x2 = c.x - c.w / 2;
      const mx = (x1 + x2) / 2;
      paths.push(
        `<path d="M ${x1.toFixed(1)} ${node.y.toFixed(1)} C ${mx.toFixed(1)} ${node.y.toFixed(1)}, ${mx.toFixed(1)} ${c.y.toFixed(1)}, ${x2.toFixed(1)} ${c.y.toFixed(1)}" fill="none" stroke="${cCol}" stroke-width="2.1" opacity="0.82" />`
      );
      emitTree(c, depth + 1);
    }
    boxes.push(
      `<rect x="${(node.x - node.w / 2).toFixed(1)}" y="${(node.y - node.h / 2).toFixed(1)}" width="${node.w}" height="${node.h}" rx="${depth === 0 ? 22 : 10}" fill="${bg}" stroke="${color}" stroke-width="${depth === 0 ? 2.2 : 1.5}" />
       ${renderSvgTextLines([node.label], node.x, node.y, {
         fontSize: depth === 0 ? 13.5 : 12,
         fontWeight: depth <= 1 ? 700 : 500,
         fill: 'var(--flow-text, #1e293b)',
       })}`
    );
  };
  emitTree(root, 0);

  const svg = `<svg xmlns="http://www.w3.org/2000/svg" class="onyx-flow-svg" viewBox="0 0 ${canvasW} ${canvasH}" width="${canvasW}" height="${canvasH}">${paths.join('')}${boxes.join('')}</svg>`;
  return {
    svg,
    kind: 'mindmap',
    direction: 'MINDMAP',
    theme,
    curve,
    nodeCount,
    edgeCount: Math.max(0, nodeCount - 1),
    width: canvasW,
    height: canvasH,
    warnings: [],
  };
}

function renderGanttSvg(
  code: string,
  theme: FlowThemeName,
  curve: FlowCurveMode
): DiagramRenderResult {
  const { cleanedLines } = parseDiagramMeta(code);
  let title = '프로젝트 일정';
  let curSection = '기본';
  const tasks: Array<{ section: string; label: string; status: 'done' | 'active' | 'crit' | 'normal'; start: number; dur: number }> = [];
  let cursor = 0;

  for (const raw of cleanedLines) {
    const line = raw.trim();
    if (!line || line.startsWith('%%') || /^(?:gantt|dateFormat|axisFormat)\b/i.test(line)) continue;
    const tM = /^title\s+(.+)$/i.exec(line);
    if (tM) {
      title = tM[1].trim();
      continue;
    }
    const sM = /^section\s+(.+)$/i.exec(line);
    if (sM) {
      curSection = sM[1].trim();
      continue;
    }
    const colon = line.indexOf(':');
    if (colon > 0) {
      const label = line.slice(0, colon).trim();
      const meta = line.slice(colon + 1).split(',').map((x) => x.trim().toLowerCase());
      let status: 'done' | 'active' | 'crit' | 'normal' = 'normal';
      if (meta.includes('done')) status = 'done';
      else if (meta.includes('crit')) status = 'crit';
      else if (meta.includes('active')) status = 'active';
      const durToken = meta.find((m) => /^\d+[dhw]$/.test(m));
      const dur = durToken ? Math.max(1, parseInt(durToken, 10)) : 5;
      tasks.push({ section: curSection, label, status, start: cursor, dur });
      cursor += Math.max(2, Math.round(dur * 0.7));
    }
  }

  if (tasks.length === 0) {
    return {
      svg: renderEmptyHintSvg('간트 차트 작업(`작업명 :active, 5d`)을 입력해 주세요.', 380, 120),
      kind: 'gantt',
      direction: 'GANTT',
      theme,
      curve,
      nodeCount: 0,
      edgeCount: 0,
      width: 380,
      height: 120,
      warnings: [],
    };
  }

  const maxEnd = Math.max(10, ...tasks.map((t) => t.start + t.dur));
  const labelW = 160;
  const chartW = 380;
  const canvasW = labelW + chartW + 32;
  const rowH = 34;
  const topY = 46;
  const canvasH = topY + tasks.length * rowH + 22;

  const bars = tasks
    .map((t, idx) => {
      const y = topY + idx * rowH;
      const bx = labelW + (t.start / maxEnd) * chartW;
      const bw = Math.max(24, (t.dur / maxEnd) * chartW);
      const fill =
        t.status === 'done'
          ? 'var(--flow-ok-stroke, #0f7b6c)'
          : t.status === 'crit'
            ? '#e11d48'
            : t.status === 'active'
              ? 'var(--flow-accent, #2383e2)'
              : '#64748b';
      return `<rect x="12" y="${y}" width="${canvasW - 24}" height="${rowH - 6}" rx="6" fill="var(--flow-subg-bg, rgba(241,245,249,0.5))" />
      <text x="22" y="${y + 18}" font-size="11.5" font-weight="600" fill="var(--flow-text, #1e293b)" font-family="'Pretendard', sans-serif">${escapeXml(t.label)}</text>
      <rect x="${bx.toFixed(1)}" y="${y + 4}" width="${bw.toFixed(1)}" height="${rowH - 14}" rx="7" fill="${fill}" />`;
    })
    .join('\n');

  const svg = `<svg xmlns="http://www.w3.org/2000/svg" class="onyx-flow-svg" viewBox="0 0 ${canvasW} ${canvasH}" width="${canvasW}" height="${canvasH}">
    <text x="${(canvasW / 2).toFixed(1)}" y="26" text-anchor="middle" font-size="14" font-weight="800" fill="var(--flow-text, #0f172a)" font-family="'Pretendard', sans-serif">${escapeXml(title)}</text>
    ${bars}
  </svg>`;

  return {
    svg,
    kind: 'gantt',
    direction: 'GANTT',
    theme,
    curve,
    nodeCount: tasks.length,
    edgeCount: 0,
    width: canvasW,
    height: canvasH,
    warnings: [],
  };
}

function renderEmptyHintSvg(msg: string, w: number, h: number): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" class="onyx-flow-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">
    <rect x="8" y="8" width="${w - 16}" height="${h - 16}" rx="12" fill="var(--flow-surface, #f8fafc)" stroke="var(--flow-border, #cbd5e1)" stroke-dasharray="6 4" stroke-width="1.5" />
    ${renderSvgTextLines([msg], w / 2, h / 2, {
      fontSize: 12.5,
      fontWeight: 600,
      fill: 'var(--flow-muted, #64748b)',
    })}
  </svg>`;
}

export function renderDiagramSvg(
  code: string,
  options: { forPrint?: boolean; svgIdPrefix?: string } = {}
): DiagramRenderResult {
  const { theme, curve } = parseDiagramMeta(code);
  const first = firstSignificantLine(code);

  if (/^sequenceDiagram\b/i.test(first)) {
    return renderSequenceDiagramSvg(code, theme, curve, options);
  }
  if (/^mindmap\b/i.test(first)) {
    return renderMindmapSvg(code, theme, curve, options);
  }
  if (/^gantt\b/i.test(first)) {
    return renderGanttSvg(code, theme, curve);
  }

  const parsed = parseFlowchart(code);
  return renderFlowchartSvg(parsed, options);
}

export interface RenderDiagramCardOptions {
  forPrint?: boolean;
  editorActions?: boolean;
  diagramIndex?: number;
  nodeIdAttr?: string;
  collapsedCode?: boolean;
  zoomPct?: number;
}

export function renderDiagramCardHtml(code: string, options: RenderDiagramCardOptions = {}): string {
  const res = renderDiagramSvg(code, { forPrint: options.forPrint });
  const kindBadge =
    res.kind === 'flowchart'
      ? `🔀 FLOWCHART · ${res.direction}`
      : res.kind === 'sequence'
        ? '🔁 SEQUENCE'
        : res.kind === 'mindmap'
          ? '🧠 MINDMAP'
          : res.kind === 'gantt'
            ? '📅 GANTT'
            : '📊 DIAGRAM';

  const nodeIdChunk = options.nodeIdAttr ? ` data-nodeid="${escapeXml(options.nodeIdAttr)}"` : '';
  const idxChunk =
    typeof options.diagramIndex === 'number' ? ` data-diagram-index="${options.diagramIndex}"` : '';
  const encoded = encodeFlowCode(code);
  const zoom = options.zoomPct || 100;

  if (options.forPrint) {
    return `<div class="onyx-flow-card onyx-flow-print" data-flow-theme="${escapeXml(res.theme)}"${nodeIdChunk}>
      <div class="onyx-flow-viewport">${res.svg}</div>
    </div>`;
  }

  const isFlow = res.kind === 'flowchart';
  const showEd = options.editorActions !== false;
  const metaCount =
    res.nodeCount > 0 ? `<span class="onyx-flow-stat">${res.nodeCount}노드 · ${res.edgeCount}연결</span>` : '';

  return `<div class="onyx-flow-card" contenteditable="false" data-flow-theme="${escapeXml(res.theme)}" data-flow-curve="${escapeXml(res.curve)}" data-flow-zoom="${zoom}" data-flow-code="${escapeXml(encoded)}"${idxChunk}${nodeIdChunk}>
    <div class="onyx-flow-toolbar">
      <div class="onyx-flow-toolbar-left">
        <span class="onyx-flow-badge">${escapeXml(kindBadge)}</span>
        ${metaCount}
      </div>
      <div class="onyx-flow-toolbar-actions">
        ${showEd && isFlow ? `<button type="button" class="onyx-flow-btn" data-flow-action="dir" title="흐름 방향 전환 (TB → LR → BT → RL)">↕ ${escapeXml(res.direction)}</button>` : ''}
        ${showEd && isFlow ? `<button type="button" class="onyx-flow-btn" data-flow-action="curve" title="선 모양 전환 (곡선 ↔ 직각)">${res.curve === 'ortho' ? '📐 직각' : '〰 곡선'}</button>` : ''}
        ${showEd ? `<button type="button" class="onyx-flow-btn" data-flow-action="theme" title="다이어그램 컬러 테마 변경 (${escapeXml(FLOW_THEME_LABELS[res.theme])})">🎨</button>` : ''}
        <button type="button" class="onyx-flow-btn" data-flow-action="zoom-out" title="축소">−</button>
        <button type="button" class="onyx-flow-btn onyx-flow-zoom-label" data-flow-action="zoom-reset" title="원래 크기(100%)">${zoom}%</button>
        <button type="button" class="onyx-flow-btn" data-flow-action="zoom-in" title="확대">+</button>
        <button type="button" class="onyx-flow-btn" data-flow-action="copy-img" title="PNG 이미지 복사 / SVG 저장">🖼 복사</button>
        ${showEd ? `<button type="button" class="onyx-flow-btn" data-flow-action="toggle-code" title="Mermaid 코드 보기/접기">${options.collapsedCode === false ? '🔼 코드숨김' : '✏️ 코드'}</button>` : ''}
        ${showEd ? `<button type="button" class="onyx-flow-btn primary" data-flow-action="builder" title="시각적 플로우차트 빌더로 편집">✨ 빌더</button>` : ''}
      </div>
    </div>
    <div class="onyx-flow-viewport"><div class="onyx-flow-stage" style="${zoom !== 100 ? `zoom: ${(zoom / 100).toFixed(2)};` : ''}">${res.svg}</div></div>
  </div>`;
}

function buildStandaloneSvgMarkup(cardEl: HTMLElement): {
  svgText: string;
  width: number;
  height: number;
} | null {
  const svgEl = cardEl.querySelector('svg.onyx-flow-svg') as SVGSVGElement | null;
  if (!svgEl) return null;
  const clone = svgEl.cloneNode(true) as SVGSVGElement;
  const cs = getComputedStyle(cardEl);
  const vars = [
    '--flow-canvas-bg',
    '--flow-surface',
    '--flow-border',
    '--flow-text',
    '--flow-muted',
    '--flow-edge',
    '--flow-accent',
    '--flow-accent-fill',
    '--flow-ok-fill',
    '--flow-ok-stroke',
    '--flow-ok-pill',
    '--flow-ok-text',
    '--flow-warn-fill',
    '--flow-warn-stroke',
    '--flow-warn-pill',
    '--flow-warn-text',
    '--flow-db-fill',
    '--flow-db-stroke',
    '--flow-sub-fill',
    '--flow-sub-stroke',
    '--flow-subg-bg',
    '--flow-subg-border',
    '--flow-subg-pill',
    '--flow-subg-text',
    '--flow-pill-bg',
  ];

  let xml = new XMLSerializer().serializeToString(clone);
  for (const v of vars) {
    const val = cs.getPropertyValue(v).trim();
    if (!val) continue;
    const escName = v.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    xml = xml.replace(new RegExp(`var\\(${escName}(?:,[^)]+)?\\)`, 'g'), val);
  }
  const w = Number(svgEl.getAttribute('width')) || svgEl.getBoundingClientRect().width || 600;
  const h = Number(svgEl.getAttribute('height')) || svgEl.getBoundingClientRect().height || 360;
  return { svgText: xml, width: Math.ceil(w), height: Math.ceil(h) };
}

export async function copyDiagramAsPngOrSvg(
  cardEl: HTMLElement,
  onToast: (msg: string, isErr?: boolean) => void
): Promise<void> {
  const built = buildStandaloneSvgMarkup(cardEl);
  if (!built) {
    onToast('복사할 다이어그램 SVG를 찾지 못했습니다.', true);
    return;
  }

  try {
    const svgBlob = new Blob([built.svgText], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(svgBlob);
    try {
      const img = new Image();
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = (e) => reject(e);
        img.src = url;
      });
      const scale = 2;
      const canvas = document.createElement('canvas');
      canvas.width = built.width * scale;
      canvas.height = built.height * scale;
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.fillStyle = getComputedStyle(cardEl).getPropertyValue('--bg-elevated').trim() || '#ffffff';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        ctx.scale(scale, scale);
        ctx.drawImage(img, 0, 0, built.width, built.height);

        const pngBlob = await new Promise<Blob | null>((res) => canvas.toBlob(res, 'image/png'));
        if (pngBlob && typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
          await navigator.clipboard.write([new ClipboardItem({ 'image/png': pngBlob })]);
          onToast('플로우차트 고화질 PNG 이미지를 클립보드에 복사했습니다.');
          return;
        }
        if (pngBlob) {
          const dlUrl = URL.createObjectURL(pngBlob);
          const a = document.createElement('a');
          a.href = dlUrl;
          a.download = `onyx-flowchart-${Date.now()}.png`;
          a.click();
          setTimeout(() => URL.revokeObjectURL(dlUrl), 1500);
          onToast('클립보드 이미지 복사 제한 환경으로 PNG 파일로 저장했습니다.');
          return;
        }
      }
    } finally {
      URL.revokeObjectURL(url);
    }
  } catch {
    // Fallback to SVG text clipboard
  }

  try {
    await navigator.clipboard.writeText(built.svgText);
    onToast('다이어그램 SVG 소스를 클립보드에 복사했습니다.');
  } catch {
    onToast('다이어그램 복사에 실패했습니다.', true);
  }
}

/* ───────────────────── Visual Flowchart Studio Modal ───────────────────── */

export interface FlowchartStarterTemplate {
  id: string;
  label: string;
  code: string;
}

export const FLOWCHART_TEMPLATES: FlowchartStarterTemplate[] = [
  {
    id: 'process-tb',
    label: '🔀 업무 흐름도 (TB)',
    code: `flowchart TB
  Start([업무 요청 접수]) --> Check{예산 · 일정 검토}
  Check -->|승인| Plan[세부 실행 계획]
  Check -->|보완 필요| Revise[요청서 수정]
  Revise --> Check
  Plan --> Build[[실무 개발 / 제작]]
  Build --> QA{최종 품질 검수}
  QA -->|통과| Done([배포 및 완료 보고])
  QA -->|이슈 발견| Build`,
  },
  {
    id: 'arch-tb',
    label: '🏛️ 시스템 아키텍처 (Subgraph TB)',
    code: `flowchart TB
  subgraph Client["클라이언트 (Windows · Android)"]
    direction TB
    UI[Notion 에디터 UI] --> Engine[[로컬 마크다운 & 그래프]]
  end
  subgraph Security["보안 계층"]
    Crypto{{AES-256-GCM 암복호화}}
  end
  subgraph Cloud["원격 저장소 / 동기화"]
    DB[(Synology NAS · 클라우드)]
  end
  Engine --> Crypto
  Crypto -->|양방향 동기화| DB`,
  },
  {
    id: 'approval-tb',
    label: '✅ 결재·분기 흐름 (TB)',
    code: `flowchart TB
  Draft[기안서 작성] --> TeamLead{팀장 1차 검토}
  TeamLead -->|승인| DeptHead{부서장 최종 결재}
  TeamLead -->|반려| Fix[내용 보완]
  Fix --> TeamLead
  DeptHead -->|승인| Archive[(전자문서 보관함)]
  DeptHead -->|보류| Notify([담당자 알림 발송])`,
  },
  {
    id: 'pipeline-lr',
    label: '➡️ 가로 파이프라인 (LR)',
    code: `flowchart LR
  Commit([코드 푸시]) --> Test[Rust · Web 테스트]
  Test -->|통과| Win[Windows EXE 빌드]
  Test -->|통과| And[Android APK · 에뮬레이터]
  Win & And --> Release([GitHub Release 통합 배포])`,
  },
  {
    id: 'sequence',
    label: '🔁 시퀀스 다이어그램',
    code: `sequenceDiagram
  사용자->>앱: 노트 저장 요청
  앱->>암호화모듈: AES-256-GCM 암호화
  암호화모듈-->>동기화엔진: 변경 내역 반영
  동기화엔진-->>NAS: 원격 업로드 완료`,
  },
  {
    id: 'mindmap',
    label: '🧠 마인드맵 (Mindmap)',
    code: `mindmap
  root((Onyx Studio 핵심 기획))
    문서 작성
      Notion 블록 UI
      A4 PDF 출력
      플로우차트 TB
    지식 연결
      위키링크
      구조적 그래프 뷰
    동기화
      Synology NAS
      클라우드 1클릭 연동`,
  },
];

export function openFlowchartStudioModal(opts: {
  initialCode?: string;
  onApply: (code: string) => void;
  onToast?: (msg: string, isErr?: boolean) => void;
}): void {
  const { card, close } = createModalBase(980);
  card.classList.add('onyx-flow-studio-modal');

  let currentCode = (opts.initialCode || '').trim() || FLOWCHART_TEMPLATES[0].code;
  let nextNodeIdx = 1;

  card.innerHTML = `
    <div class="modal-header">
      <div style="display:flex;align-items:center;gap:8px;">
        <span>✨ Onyx 시각적 플로우차트 빌더 (flowchart TB / LR / Subgraph)</span>
      </div>
      <button type="button" class="icon-btn" data-close>✕</button>
    </div>
    <div class="onyx-flow-studio-body">
      <div class="onyx-flow-studio-left">
        <div class="onyx-flow-studio-sec">
          <div class="onyx-flow-studio-label">1. 템플릿 빠른 시작</div>
          <div class="onyx-flow-tpl-row" data-tpl-list></div>
        </div>

        <div class="onyx-flow-studio-sec">
          <div class="onyx-flow-studio-label">2. 레이아웃 · 선 · 테마</div>
          <div style="display:flex;flex-wrap:wrap;gap:6px;" data-layout-controls>
            <button type="button" class="btn btn-sm" data-dir="TB">⬇ 위→아래 (TB)</button>
            <button type="button" class="btn btn-sm" data-dir="LR">➡ 왼→오 (LR)</button>
            <button type="button" class="btn btn-sm" data-dir="BT">⬆ 아래→위 (BT)</button>
            <button type="button" class="btn btn-sm" data-dir="RL">⬅ 오→왼 (RL)</button>
            <button type="button" class="btn btn-sm" data-toggle-curve>〰 곡선/직각</button>
            <button type="button" class="btn btn-sm" data-cycle-theme>🎨 테마</button>
          </div>
        </div>

        <div class="onyx-flow-studio-sec">
          <div class="onyx-flow-studio-label">3. 노드 · 연결 빠른 추가</div>
          <div class="onyx-flow-quick-grid">
            <input type="text" class="text-input input-sm" data-q-from placeholder="출발 노드 (예: 검토)" value="요청 접수" />
            <select class="text-input input-sm" data-q-from-shape>
              <option value="round">[둥근 사각형]</option>
              <option value="stadium">([시작/종료 타원])</option>
              <option value="diamond">{조건 분기}</option>
              <option value="cylinder">[(데이터베이스)]</option>
              <option value="subroutine">[[서브모듈]]</option>
            </select>
            <select class="text-input input-sm" data-q-arrow>
              <option value="-->">--&gt; 실선 화살표</option>
              <option value="-.->">-.-> 점선 화살표</option>
              <option value="==>">==> 굵은 강조</option>
              <option value="<-->">&lt;--> 양방향</option>
            </select>
            <input type="text" class="text-input input-sm" data-q-label placeholder="연결 라벨 (예: 승인)" />
            <input type="text" class="text-input input-sm" data-q-to placeholder="도착 노드 (예: 개발)" value="1차 승인" />
            <select class="text-input input-sm" data-q-to-shape>
              <option value="diamond">{조건 분기}</option>
              <option value="round">[둥근 사각형]</option>
              <option value="stadium">([시작/종료 타원])</option>
              <option value="cylinder">[(데이터베이스)]</option>
              <option value="subroutine">[[서브모듈]]</option>
            </select>
          </div>
          <div style="display:flex;gap:6px;margin-top:6px;flex-wrap:wrap;">
            <button type="button" class="btn btn-sm btn-primary" data-q-add>＋ 연결 추가</button>
            <button type="button" class="btn btn-sm" data-q-sub>📦 서브그래프 블록 추가</button>
          </div>
        </div>

        <div class="onyx-flow-studio-sec" style="flex:1;display:flex;flex-direction:column;min-height:170px;">
          <div class="onyx-flow-studio-label">4. 다이어그램 코드 (실시간 미리보기 연동)</div>
          <textarea class="onyx-flow-code-area" data-code-input spellcheck="false"></textarea>
        </div>
      </div>

      <div class="onyx-flow-studio-right">
        <div class="onyx-flow-studio-label" style="display:flex;justify-content:space-between;align-items:center;">
          <span>실시간 벡터 렌더링 미리보기</span>
          <span class="muted" style="font-size:11px;" data-studio-stats></span>
        </div>
        <div class="onyx-flow-studio-preview" data-preview-host></div>
      </div>
    </div>
    <div class="modal-footer" style="justify-content:space-between;">
      <div style="display:flex;gap:6px;">
        <button type="button" class="btn btn-sm" data-studio-copy-png>🖼 PNG 복사</button>
        <button type="button" class="btn btn-sm" data-studio-dl-svg>💾 SVG 저장</button>
      </div>
      <div style="display:flex;gap:8px;">
        <button type="button" class="btn" data-cancel>취소</button>
        <button type="button" class="btn btn-primary" data-apply>✅ 문서에 적용</button>
      </div>
    </div>
  `;

  const tplWrap = card.querySelector('[data-tpl-list]')!;
  const codeInput = card.querySelector('[data-code-input]') as HTMLTextAreaElement;
  const previewHost = card.querySelector('[data-preview-host]') as HTMLElement;
  const statsEl = card.querySelector('[data-studio-stats]') as HTMLElement;

  const refreshPreview = () => {
    currentCode = codeInput.value;
    const res = renderDiagramSvg(currentCode, { svgIdPrefix: 'studio-prev' });
    previewHost.innerHTML = `<div class="onyx-flow-card" data-flow-theme="${escapeXml(res.theme)}" style="margin:0;height:100%;border:none;box-shadow:none;">
      <div class="onyx-flow-viewport" style="max-height:none;flex:1;">${res.svg}</div>
    </div>`;
    statsEl.textContent = `${res.kind.toUpperCase()} · ${res.direction} · ${FLOW_THEME_LABELS[res.theme]} · ${res.nodeCount}노드 ${res.edgeCount}연결`;
  };

  codeInput.value = currentCode;
  refreshPreview();
  codeInput.addEventListener('input', refreshPreview);

  for (const tpl of FLOWCHART_TEMPLATES) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'btn btn-sm';
    b.textContent = tpl.label;
    b.addEventListener('click', () => {
      codeInput.value = tpl.code;
      refreshPreview();
    });
    tplWrap.appendChild(b);
  }

  card.querySelectorAll<HTMLElement>('[data-dir]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const dir = btn.getAttribute('data-dir') as 'TB' | 'LR' | 'BT' | 'RL';
      codeInput.value = setFlowchartDirection(codeInput.value, dir);
      refreshPreview();
    });
  });

  card.querySelector('[data-toggle-curve]')?.addEventListener('click', () => {
    const res = toggleFlowchartCurve(codeInput.value);
    codeInput.value = res.nextCode;
    refreshPreview();
  });

  card.querySelector('[data-cycle-theme]')?.addEventListener('click', () => {
    const res = cycleFlowchartTheme(codeInput.value);
    codeInput.value = res.nextCode;
    refreshPreview();
  });

  const formatNodeExpr = (idMap: Map<string, string>, label: string, shape: FlowShape): string => {
    const clean = label.trim() || '단계';
    const existing = idMap.get(clean);
    if (existing) return existing;
    const safeId = `N${nextNodeIdx++}`;
    idMap.set(clean, safeId);
    switch (shape) {
      case 'stadium':
        return `${safeId}(["${clean}"])`;
      case 'diamond':
        return `${safeId}{"${clean}"}`;
      case 'cylinder':
        return `${safeId}[("${clean}")]`;
      case 'subroutine':
        return `${safeId}[["${clean}"]]`;
      default:
        return `${safeId}["${clean}"]`;
    }
  };

  card.querySelector('[data-q-add]')?.addEventListener('click', () => {
    const parsed = parseFlowchart(codeInput.value);
    const labelToId = new Map<string, string>();
    for (const n of parsed.nodes) {
      labelToId.set(n.label.trim(), n.id);
      labelToId.set(n.id, n.id);
    }
    const fromTxt = (card.querySelector('[data-q-from]') as HTMLInputElement).value;
    const toTxt = (card.querySelector('[data-q-to]') as HTMLInputElement).value;
    const fromShape = (card.querySelector('[data-q-from-shape]') as HTMLSelectElement).value as FlowShape;
    const toShape = (card.querySelector('[data-q-to-shape]') as HTMLSelectElement).value as FlowShape;
    const arrow = (card.querySelector('[data-q-arrow]') as HTMLSelectElement).value;
    const edgeLbl = (card.querySelector('[data-q-label]') as HTMLInputElement).value.trim();

    const leftExpr = formatNodeExpr(labelToId, fromTxt, fromShape);
    const rightExpr = formatNodeExpr(labelToId, toTxt, toShape);
    const edgeExpr = edgeLbl ? `${arrow}|${edgeLbl}|` : arrow;

    const base = codeInput.value.trimEnd();
    const hasHeader = /^(?:flowchart|graph)\b/im.test(base);
    codeInput.value = `${hasHeader ? base : 'flowchart TB\n' + base}\n  ${leftExpr} ${edgeExpr} ${rightExpr}`;
    refreshPreview();
  });

  card.querySelector('[data-q-sub]')?.addEventListener('click', () => {
    const gIdx = nextNodeIdx++;
    const base = codeInput.value.trimEnd();
    codeInput.value = `${base}\n  subgraph Group${gIdx}["하위 모듈 그룹 ${gIdx}"]\n    G${gIdx}A["세부 작업 A"] --> G${gIdx}B["세부 작업 B"]\n  end`;
    refreshPreview();
  });

  card.querySelector('[data-studio-copy-png]')?.addEventListener('click', () => {
    const cEl = previewHost.querySelector('.onyx-flow-card') as HTMLElement | null;
    if (cEl) void copyDiagramAsPngOrSvg(cEl, opts.onToast || (() => {}));
  });

  card.querySelector('[data-studio-dl-svg]')?.addEventListener('click', () => {
    const cEl = previewHost.querySelector('.onyx-flow-card') as HTMLElement | null;
    const built = cEl ? buildStandaloneSvgMarkup(cEl) : null;
    if (!built) return;
    const blob = new Blob([built.svgText], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `onyx-flowchart-${Date.now()}.svg`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1500);
    opts.onToast?.('SVG 벡터 파일을 저장했습니다.');
  });

  card.querySelector('[data-close]')?.addEventListener('click', close);
  card.querySelector('[data-cancel]')?.addEventListener('click', close);
  card.querySelector('[data-apply]')?.addEventListener('click', () => {
    const finalCode = codeInput.value.trim();
    if (!finalCode) return;
    close();
    opts.onApply(finalCode);
  });
}
