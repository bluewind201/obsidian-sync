/**
 * Clean Clipboard & Drag-Drop Sanitizer for Onyx Studio.
 *
 * Ensures that pasting or dropping external content:
 * 1) Pastes text strictly as clean plain text (stripping all HTML tags, inline styles,
 *    colors, font attributes, spans, divs, links, bold/italic wrappers, and zero-width junk
 *    so codes never wrap characters).
 * 2) Preserves images as clean images — both binary clipboard images (screenshots,
 *    browser "Copy Image", copied image files) and `<img>` elements inside copied external
 *    HTML articles (automatically decoding `data:image/...` URIs into uploadable Blobs).
 * 3) Allows Onyx Studio's own internal WYSIWYG copy-paste to work naturally unless
 *    plain-text paste (Shift) is requested.
 */

export const ONYX_INTERNAL_CLIPBOARD_MIME = 'application/x-onyx-studio-internal';

export type CleanPasteSegment =
  | { kind: 'text'; text: string }
  | { kind: 'image'; src: string; alt: string };

export type ClassifiedPasteAction =
  | { kind: 'internal' }
  | { kind: 'image-blobs'; blobs: Array<File | Blob> }
  | { kind: 'segments'; segments: CleanPasteSegment[] }
  | { kind: 'plain-text'; text: string }
  | { kind: 'noop' };

const IGNORED_PASTE_TAGS = new Set([
  'script',
  'style',
  'noscript',
  'template',
  'head',
  'meta',
  'link',
  'title',
  'svg',
  'canvas',
  'iframe',
  'object',
  'embed',
  'audio',
  'video',
  'source',
  'track',
]);

const BLOCK_PASTE_TAGS = new Set([
  'address',
  'article',
  'aside',
  'blockquote',
  'dd',
  'details',
  'div',
  'dl',
  'dt',
  'fieldset',
  'figcaption',
  'figure',
  'footer',
  'form',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'header',
  'hr',
  'li',
  'main',
  'nav',
  'ol',
  'p',
  'pre',
  'section',
  'summary',
  'table',
  'tr',
  'ul',
]);

const PARAGRAPH_GAP_TAGS = new Set([
  'p',
  'blockquote',
  'pre',
  'table',
  'figure',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
]);

const LAZY_IMG_ATTRS = [
  'data-lazy-src',
  'data-src',
  'data-original',
  'data-origin-src',
  'data-actualsrc',
  'data-url',
];

/**
 * Normalizes external plain text:
 * - Converts CRLF/CR to LF
 * - Replaces non-breaking spaces (`\u00A0`) with regular spaces
 * - Strips invisible zero-width space (`\u200B`, e.g. Naver SmartEditor) & BOM (`\uFEFF`)
 *   while keeping emoji ZWJ (`\u200D`) intact
 * - Caps consecutive blank lines when `collapseExcessBlankLines` is true so TOAST UI
 *   WYSIWYG never emits `<br>` HTML tags on save.
 */
export function normalizePastedPlainText(
  raw: string,
  opts?: { collapseExcessBlankLines?: boolean }
): string {
  if (!raw) return '';
  let out = raw
    .replace(/\r\n?/g, '\n')
    .replace(/\u00A0/g, ' ')
    .replace(/[\u200B\uFEFF]/g, '');

  if (opts?.collapseExcessBlankLines) {
    out = out.replace(/[ \t]+\n/g, '\n').replace(/\n{3,}/g, '\n\n');
  }
  return out;
}

function isPlaceholderDataImage(url: string): boolean {
  const lower = url.trim().toLowerCase();
  if (!lower.startsWith('data:image/')) return false;
  // Common 1x1 transparent GIF/SVG lazy-load placeholders on blogs
  if (lower.startsWith('data:image/gif;base64,r0lgod') && url.length < 220) {
    return true;
  }
  if (lower.startsWith('data:image/svg+xml') && url.length < 220) {
    return true;
  }
  return false;
}

function isAllowedImageUrl(url: string): boolean {
  const trimmed = url.trim();
  if (!trimmed) return false;
  if (/^https?:\/\/\S+/i.test(trimmed)) return true;
  if (/^data:image\/[a-z0-9.+-]+(?:;[^,]*)?,/i.test(trimmed)) return true;
  if (/^\/vault-media\b/i.test(trimmed)) return true;
  return false;
}

/**
 * Extracts a safe, clean image URL from an `<img>` element while resolving
 * lazy-loaded blog image attributes and rejecting 1x1 tracking pixels or dead `file://` URLs.
 */
export function resolveCleanImageSrc(imgEl: Element): string | null {
  const wAttr = (imgEl.getAttribute('width') || '').trim();
  const hAttr = (imgEl.getAttribute('height') || '').trim();
  if ((wAttr === '1' && hAttr === '1') || (wAttr === '0' && hAttr === '0')) {
    return null;
  }
  if (imgEl.classList?.contains('ProseMirror-separator')) {
    return null;
  }

  const rawSrc = (imgEl.getAttribute('src') || '').trim();
  let lazyCandidate = '';
  for (const attr of LAZY_IMG_ATTRS) {
    const v = (imgEl.getAttribute(attr) || '').trim();
    if (v && isAllowedImageUrl(v) && !isPlaceholderDataImage(v)) {
      lazyCandidate = v;
      break;
    }
  }

  if (lazyCandidate && (!rawSrc || !isAllowedImageUrl(rawSrc) || isPlaceholderDataImage(rawSrc))) {
    return lazyCandidate;
  }
  if (rawSrc && isAllowedImageUrl(rawSrc) && !isPlaceholderDataImage(rawSrc)) {
    return rawSrc;
  }
  return lazyCandidate || null;
}

function cleanImageAlt(rawAlt: string | null): string {
  if (!rawAlt) return 'image';
  const cleaned = rawAlt
    .replace(/[\r\n\t]+/g, ' ')
    .replace(/[\[\]]+/g, '')
    .replace(/[\u200B\uFEFF]/g, '')
    .trim();
  if (!cleaned || cleaned.length > 120) return 'image';
  return cleaned;
}

function isHiddenElementWithoutImages(el: Element): boolean {
  if (el.hasAttribute('hidden')) {
    return !el.querySelector('img');
  }
  const style = el.getAttribute('style') || '';
  if (/\bdisplay\s*:\s*none\b/i.test(style) || /\bvisibility\s*:\s*hidden\b/i.test(style)) {
    return !el.querySelector('img');
  }
  return false;
}

/**
 * Walks an HTML fragment and extracts ONLY:
 * - Clean plain text (all HTML tags, inline styles, links, headings, bold/italic/color attributes stripped)
 * - Clean `<img>` segments in exact document order
 */
export function extractPlainTextAndImagesFromHtml(rawHtml: string): CleanPasteSegment[] {
  if (!rawHtml || !rawHtml.trim()) return [];
  const parser = new DOMParser();
  const doc = parser.parseFromString(rawHtml, 'text/html');
  const segments: CleanPasteSegment[] = [];
  let textBuf = '';
  let preDepth = 0;

  const ensureLineBreak = () => {
    if (textBuf.length > 0 && !textBuf.endsWith('\n')) {
      textBuf += '\n';
    }
  };

  const ensureParagraphGap = () => {
    if (textBuf.length === 0) return;
    if (textBuf.endsWith('\n\n')) return;
    if (textBuf.endsWith('\n')) {
      textBuf += '\n';
    } else {
      textBuf += '\n\n';
    }
  };

  const flushTextBuf = () => {
    const cleaned = textBuf
      .replace(/[ \t]+\n/g, '\n')
      .replace(/\n{3,}/g, '\n\n')
      .replace(/^\n+|\n+$/g, '');
    textBuf = '';
    if (cleaned.length > 0) {
      segments.push({ kind: 'text', text: cleaned });
    }
  };

  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      let val = (node.nodeValue || '').replace(/\u00A0/g, ' ').replace(/[\u200B\uFEFF]/g, '');
      if (!val) return;
      if (preDepth > 0) {
        textBuf += val.replace(/\r\n?/g, '\n');
      } else {
        val = val.replace(/[\r\n\t ]+/g, ' ');
        if (
          val === ' ' &&
          (textBuf.length === 0 ||
            textBuf.endsWith('\n') ||
            textBuf.endsWith(' ') ||
            textBuf.endsWith('\t'))
        ) {
          return;
        }
        textBuf += val;
      }
      return;
    }

    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const el = node as Element;
    const tag = el.tagName.toLowerCase();

    if (IGNORED_PASTE_TAGS.has(tag)) return;
    if (isHiddenElementWithoutImages(el)) return;

    if (tag === 'br') {
      textBuf += '\n';
      return;
    }
    if (tag === 'hr') {
      ensureLineBreak();
      return;
    }
    if (tag === 'img') {
      const src = resolveCleanImageSrc(el);
      if (src) {
        flushTextBuf();
        segments.push({
          kind: 'image',
          src,
          alt: cleanImageAlt(el.getAttribute('alt')),
        });
      }
      return;
    }

    if (tag === 'td' || tag === 'th') {
      for (const child of Array.from(el.childNodes)) {
        walk(child);
      }
      const nextTag = el.nextElementSibling?.tagName.toLowerCase();
      if (
        (nextTag === 'td' || nextTag === 'th') &&
        textBuf.length > 0 &&
        !textBuf.endsWith('\t') &&
        !textBuf.endsWith('\n')
      ) {
        textBuf += '\t';
      }
      return;
    }

    const isBlock = BLOCK_PASTE_TAGS.has(tag);
    const isParaGap = PARAGRAPH_GAP_TAGS.has(tag);

    if (isParaGap) {
      ensureParagraphGap();
    } else if (isBlock) {
      ensureLineBreak();
    }

    if (tag === 'pre') preDepth++;
    for (const child of Array.from(el.childNodes)) {
      walk(child);
    }
    if (tag === 'pre') preDepth--;

    if (isParaGap) {
      ensureParagraphGap();
    } else if (isBlock) {
      ensureLineBreak();
    }
  };

  walk(doc.body);
  flushTextBuf();
  return segments;
}

/**
 * Decodes a `data:image/...` URI into a binary `Blob` + file extension so pasted base64 images
 * can be stored cleanly in the vault's attachments folder instead of bloating Markdown with base64.
 */
export function dataUrlToImageBlob(dataUrl: string): { blob: Blob; ext: string } | null {
  const m = /^data:(image\/([a-z0-9.+-]+))(;base64)?,([\s\S]*)$/i.exec(dataUrl.trim());
  if (!m) return null;
  const mime = m[1].toLowerCase();
  const rawSub = m[2].toLowerCase();
  const isBase64 = Boolean(m[3]);
  const payload = m[4] || '';

  const ext =
    rawSub === 'jpeg'
      ? 'jpg'
      : rawSub === 'svg+xml'
        ? 'svg'
        : rawSub.replace(/[^a-z0-9]/g, '') || 'png';

  try {
    if (isBase64) {
      const bin = atob(payload.replace(/\s+/g, ''));
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) {
        bytes[i] = bin.charCodeAt(i);
      }
      return { blob: new Blob([bytes], { type: mime }), ext };
    } else {
      const decoded = decodeURIComponent(payload);
      return { blob: new Blob([decoded], { type: mime }), ext };
    }
  } catch {
    return null;
  }
}

function collectImageBlobsFromDataTransfer(dt: DataTransfer): Array<File | Blob> {
  const out: Array<File | Blob> = [];
  if (dt.items && dt.items.length > 0) {
    for (const item of Array.from(dt.items)) {
      if (item.kind === 'file' && item.type.toLowerCase().startsWith('image/')) {
        const f = item.getAsFile();
        if (f) out.push(f);
      }
    }
  }
  if (out.length === 0 && dt.files && dt.files.length > 0) {
    for (const f of Array.from(dt.files)) {
      if (
        f.type.toLowerCase().startsWith('image/') ||
        /\.(?:png|jpe?g|gif|webp|bmp|svg)$/i.test(f.name || '')
      ) {
        out.push(f);
      }
    }
  }
  return out;
}

function hasRtfStringItem(dt: DataTransfer): boolean {
  if (!dt.items) return false;
  for (const item of Array.from(dt.items)) {
    if (item.kind === 'string' && item.type.toLowerCase() === 'text/rtf') {
      return true;
    }
  }
  return false;
}

function htmlContainsOnlyImages(rawHtml: string): boolean {
  if (!rawHtml || !/<img\b/i.test(rawHtml)) return false;
  const segments = extractPlainTextAndImagesFromHtml(rawHtml);
  return segments.length > 0 && segments.every((s) => s.kind === 'image');
}

function htmlLooksCleanInternalCopy(rawHtml: string): boolean {
  // Reject foreign inline font-family / mso- style attributes if they ever existed in old notes
  if (/\b(?:font-family|mso-[a-z-]+)\s*:/i.test(rawHtml)) {
    return false;
  }
  return true;
}

/**
 * Classifies a `ClipboardEvent.clipboardData` or `DragEvent.dataTransfer` into a clean paste action.
 */
export function classifyClipboardData(
  dt: DataTransfer | null | undefined,
  opts?: { forcePlainText?: boolean }
): ClassifiedPasteAction {
  if (!dt) return { kind: 'noop' };

  const forcePlain = Boolean(opts?.forcePlainText);
  const rawHtml = dt.getData('text/html') || '';
  const rawPlain = dt.getData('text/plain') || dt.getData('Text') || '';

  // 1) Internal Onyx Studio copy-paste (when Shift is NOT held)
  if (
    !forcePlain &&
    dt.getData(ONYX_INTERNAL_CLIPBOARD_MIME) === '1' &&
    htmlLooksCleanInternalCopy(rawHtml)
  ) {
    return { kind: 'internal' };
  }

  // 2) Binary image paste (screenshots, browser "Copy Image", image file copy)
  //    Skip when `text/rtf` is present (MS Office puts a bitmap snapshot alongside copied text).
  const hasRtf = hasRtfStringItem(dt);
  if (!hasRtf) {
    const imageBlobs = collectImageBlobsFromDataTransfer(dt);
    if (imageBlobs.length > 0 && (!rawPlain.trim() || htmlContainsOnlyImages(rawHtml))) {
      return { kind: 'image-blobs', blobs: imageBlobs };
    }
  }

  // 3) External HTML containing valid `<img>` elements (e.g. web article with text + pictures,
  //    or standalone `<img>` without binary file item): keep images as images, strip all text styles!
  if (!forcePlain && rawHtml && /<img\b/i.test(rawHtml)) {
    const segments = extractPlainTextAndImagesFromHtml(rawHtml);
    if (segments.some((s) => s.kind === 'image')) {
      return { kind: 'segments', segments };
    }
  }

  // 4) External text paste -> strictly plain text (zero HTML tags, zero attributes, zero markdown codes)
  let plain = normalizePastedPlainText(rawPlain);
  if (!plain.trim() && rawHtml.trim()) {
    const segments = extractPlainTextAndImagesFromHtml(rawHtml);
    plain = segments
      .filter((s): s is { kind: 'text'; text: string } => s.kind === 'text')
      .map((s) => s.text)
      .join('\n\n');
  }

  if (!plain) {
    const uriList = dt.getData('text/uri-list') || '';
    if (uriList.trim()) {
      plain = normalizePastedPlainText(
        uriList
          .split(/\r?\n/)
          .filter((l) => l && !l.startsWith('#'))
          .join('\n')
      );
    }
  }

  if (!plain) return { kind: 'noop' };
  return { kind: 'plain-text', text: plain };
}
