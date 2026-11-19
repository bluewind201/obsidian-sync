//! Pure Native Rust Clipboard Sanitizer & Image Extractor (Zero WebView2 / Zero External DLL).
//!
//! Guarantees that external copy-paste in Onyx Studio:
//! 1) Pastes text strictly as clean plain text (stripping all HTML tags, inline `style`/`color`/`font`
//!    attributes, zero-width spaces `\u{200B}`, and BOM `\u{FEFF}`).
//! 2) Pastes screenshots (`Win+Shift+S`, PrintScreen `CF_DIB` / `PNG`), browser "Copy Image",
//!    and copied image files (`CF_HDROP`) as real vault attachment images.
//! 3) Preserves `<img>` pictures while stripping text HTML attributes when copying mixed external
//!    web articles containing both text and pictures.

use base64::Engine;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanPasteSegment {
    Text(String),
    Image { src: String, alt: String },
}

#[derive(Clone, Debug)]
pub enum ClipboardPasteAction {
    ImageFiles(Vec<PathBuf>),
    ImageBytes { bytes: Vec<u8>, ext: String },
    Segments(Vec<CleanPasteSegment>),
    PlainText(String),
}

/// Normalizes external plain text:
/// - Converts CRLF/CR to LF
/// - Replaces non-breaking spaces (`\u{00A0}`) with regular spaces
/// - Removes invisible zero-width spaces (`\u{200B}`) & BOM (`\u{FEFF}`) while keeping emoji ZWJ (`\u{200D}`)
pub fn sanitize_external_plain_text(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        match ch {
            '\u{00A0}' => out.push(' '),
            '\u{200B}' | '\u{FEFF}' => {}
            _ => out.push(ch),
        }
    }
    out
}

/// Strips inline HTML wrapper tags (`<span ...>`, `<div ...>`, `<font ...>`, `<mark ...>`, etc.)
/// from existing note text while preserving Markdown images `![alt](...)` and `![[...]]`.
pub fn strip_html_wrappers_preserving_images(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(rel_end) = raw[i..].find('>') {
                let tag_full = &raw[i + 1..i + rel_end];
                let trimmed = tag_full.trim().trim_start_matches('/').trim();
                let tag_name: String = trimmed
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                if tag_name == "br" {
                    out.push('\n');
                    i += rel_end + 1;
                    continue;
                }
                if matches!(
                    tag_name.as_str(),
                    "span"
                        | "div"
                        | "font"
                        | "mark"
                        | "u"
                        | "b"
                        | "strong"
                        | "i"
                        | "em"
                        | "s"
                        | "del"
                        | "p"
                        | "a"
                        | "sup"
                        | "sub"
                        | "kbd"
                        | "section"
                        | "article"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                ) {
                    i += rel_end + 1;
                    continue;
                }
            }
        }
        let ch = raw[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    sanitize_external_plain_text(&out)
}

pub fn decode_html_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(semi) = rest[..rest.len().min(14)].find(';') {
            let entity = &rest[1..semi];
            let decoded = match entity {
                "nbsp" | "#160" | "#xa0" | "#xA0" => Some(' '),
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" | "#x27" => Some('\''),
                "#8203" | "#x200b" | "#x200B" | "ZeroWidthSpace" => None,
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => {
                    entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => {
                    out.push('&');
                    rest = &rest[1..];
                    continue;
                }
            };
            if let Some(ch) = decoded {
                if ch != '\u{200B}' && ch != '\u{FEFF}' {
                    out.push(if ch == '\u{00A0}' { ' ' } else { ch });
                }
            }
            rest = &rest[semi + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn slice_cf_html_fragment(raw: &str) -> &str {
    const START_MARK: &str = "<!--StartFragment-->";
    const END_MARK: &str = "<!--EndFragment-->";
    if let Some(s) = raw.find(START_MARK) {
        let after = &raw[s + START_MARK.len()..];
        if let Some(e) = after.find(END_MARK) {
            return &after[..e];
        }
    }
    if let Some(body_idx) = raw.to_ascii_lowercase().find("<body") {
        let from_body = &raw[body_idx..];
        if let Some(gt) = from_body.find('>') {
            let inner = &from_body[gt + 1..];
            if let Some(end_body) = inner.to_ascii_lowercase().rfind("</body>") {
                return &inner[..end_body];
            }
            return inner;
        }
    }
    if let Some(html_idx) = raw.find('<') {
        return &raw[html_idx..];
    }
    raw
}

fn extract_attr_value(tag_attrs: &str, attr_name: &str) -> Option<String> {
    let lower = tag_attrs.to_ascii_lowercase();
    let target = attr_name.to_ascii_lowercase();
    let mut search_from = 0usize;
    while let Some(rel) = lower[search_from..].find(&target) {
        let pos = search_from + rel;
        let prev_ok = pos == 0
            || lower.as_bytes()[pos - 1].is_ascii_whitespace()
            || matches!(lower.as_bytes()[pos - 1], b'"' | b'\'');
        let after_idx = pos + target.len();
        if prev_ok {
            let tail = tag_attrs[after_idx..].trim_start();
            if let Some(val_part) = tail.strip_prefix('=') {
                let val_part = val_part.trim_start();
                if let Some(q) = val_part.chars().next() {
                    if q == '"' || q == '\'' {
                        let inner = &val_part[q.len_utf8()..];
                        if let Some(end_q) = inner.find(q) {
                            return Some(decode_html_entities(&inner[..end_q]));
                        }
                    } else {
                        let unquoted: String = val_part
                            .chars()
                            .take_while(|c| !c.is_whitespace() && *c != '>')
                            .collect();
                        return Some(decode_html_entities(&unquoted));
                    }
                }
            }
        }
        search_from = after_idx;
    }
    None
}

fn is_placeholder_data_img(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if !lower.starts_with("data:image/") {
        return false;
    }
    (lower.starts_with("data:image/gif;base64,r0lgod") && url.len() < 220)
        || (lower.starts_with("data:image/svg+xml") && url.len() < 220)
}

fn is_allowed_img_url(url: &str) -> bool {
    let t = url.trim();
    let lower = t.to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("data:image/")
}

fn resolve_img_from_attrs(attrs: &str) -> Option<(String, String)> {
    let w = extract_attr_value(attrs, "width").unwrap_or_default();
    let h = extract_attr_value(attrs, "height").unwrap_or_default();
    if (w.trim() == "1" && h.trim() == "1") || (w.trim() == "0" && h.trim() == "0") {
        return None;
    }
    if let Some(cls) = extract_attr_value(attrs, "class") {
        if cls.contains("ProseMirror-separator") {
            return None;
        }
    }

    let raw_src = extract_attr_value(attrs, "src")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let mut lazy = String::new();
    for k in [
        "data-lazy-src",
        "data-src",
        "data-original",
        "data-origin-src",
        "data-actualsrc",
        "data-url",
    ] {
        if let Some(v) = extract_attr_value(attrs, k) {
            let vt = v.trim();
            if is_allowed_img_url(vt) && !is_placeholder_data_img(vt) {
                lazy = vt.to_string();
                break;
            }
        }
    }

    let chosen = if !lazy.is_empty()
        && (raw_src.is_empty()
            || !is_allowed_img_url(&raw_src)
            || is_placeholder_data_img(&raw_src))
    {
        lazy
    } else if is_allowed_img_url(&raw_src) && !is_placeholder_data_img(&raw_src) {
        raw_src
    } else {
        lazy
    };

    if chosen.is_empty() {
        return None;
    }

    let alt_raw = extract_attr_value(attrs, "alt").unwrap_or_default();
    let alt_clean: String = alt_raw
        .chars()
        .map(|c| {
            if matches!(c, '\r' | '\n' | '\t' | '[' | ']') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let alt = if alt_clean.is_empty() || alt_clean.len() > 120 {
        "이미지".to_string()
    } else {
        alt_clean
    };
    Some((chosen, alt))
}

/// Extracts ordered plain-text & image segments from an HTML / `CF_HTML` string,
/// stripping 100% of inline/block HTML attributes and formatting wrappers around text.
pub fn extract_plain_text_and_images_from_html(raw_html: &str) -> Vec<CleanPasteSegment> {
    let frag = slice_cf_html_fragment(raw_html);
    if frag.trim().is_empty() {
        return Vec::new();
    }

    let mut segments: Vec<CleanPasteSegment> = Vec::new();
    let mut text_buf = String::new();
    let mut in_pre = 0usize;
    let bytes = frag.as_bytes();
    let mut i = 0usize;

    let flush_text = |buf: &mut String, out: &mut Vec<CleanPasteSegment>| {
        let mut cleaned = String::new();
        for line in buf.split('\n') {
            if !cleaned.is_empty() {
                cleaned.push('\n');
            }
            cleaned.push_str(line.trim_end_matches([' ', '\t']));
        }
        while cleaned.contains("\n\n\n") {
            cleaned = cleaned.replace("\n\n\n", "\n\n");
        }
        let trimmed = cleaned.trim_matches('\n').to_string();
        buf.clear();
        if !trimmed.is_empty() {
            out.push(CleanPasteSegment::Text(trimmed));
        }
    };

    let ensure_newline = |buf: &mut String| {
        if !buf.is_empty() && !buf.ends_with('\n') {
            buf.push('\n');
        }
    };

    let ensure_para_gap = |buf: &mut String| {
        if buf.is_empty() || buf.ends_with("\n\n") {
            return;
        }
        if buf.ends_with('\n') {
            buf.push('\n');
        } else {
            buf.push_str("\n\n");
        }
    };

    while i < bytes.len() {
        if bytes[i] == b'<' {
            if frag[i..].starts_with("<!--") {
                if let Some(end_c) = frag[i + 4..].find("-->") {
                    i += 4 + end_c + 3;
                    continue;
                }
            }
            if let Some(rel_gt) = frag[i..].find('>') {
                let inside = &frag[i + 1..i + rel_gt];
                let trimmed = inside.trim();
                let is_closing = trimmed.starts_with('/');
                let name_slice = trimmed.trim_start_matches('/').trim_start();
                let tag: String = name_slice
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric())
                    .collect::<String>()
                    .to_ascii_lowercase();
                let attrs = &name_slice[tag.len()..];

                i += rel_gt + 1;

                if !is_closing
                    && matches!(
                        tag.as_str(),
                        "script" | "style" | "noscript" | "template" | "head" | "svg"
                    )
                {
                    let close_pat = format!("</{tag}");
                    if let Some(close_rel) = frag[i..].to_ascii_lowercase().find(&close_pat) {
                        i += close_rel;
                        if let Some(close_gt) = frag[i..].find('>') {
                            i += close_gt + 1;
                        }
                    }
                    continue;
                }

                if tag == "br" && !is_closing {
                    text_buf.push('\n');
                    continue;
                }
                if tag == "hr" && !is_closing {
                    ensure_newline(&mut text_buf);
                    continue;
                }
                if tag == "img" && !is_closing {
                    if let Some((src, alt)) = resolve_img_from_attrs(attrs) {
                        flush_text(&mut text_buf, &mut segments);
                        segments.push(CleanPasteSegment::Image { src, alt });
                    }
                    continue;
                }
                if tag == "pre" {
                    if is_closing {
                        in_pre = in_pre.saturating_sub(1);
                        ensure_para_gap(&mut text_buf);
                    } else {
                        ensure_para_gap(&mut text_buf);
                        in_pre += 1;
                    }
                    continue;
                }
                if (tag == "td" || tag == "th") && is_closing {
                    if !text_buf.is_empty()
                        && !text_buf.ends_with('\t')
                        && !text_buf.ends_with('\n')
                    {
                        text_buf.push('\t');
                    }
                    continue;
                }
                if matches!(
                    tag.as_str(),
                    "p" | "blockquote"
                        | "table"
                        | "figure"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                ) {
                    ensure_para_gap(&mut text_buf);
                    continue;
                }
                if matches!(
                    tag.as_str(),
                    "div"
                        | "section"
                        | "article"
                        | "header"
                        | "footer"
                        | "aside"
                        | "li"
                        | "ul"
                        | "ol"
                        | "tr"
                        | "figcaption"
                        | "details"
                        | "summary"
                ) {
                    ensure_newline(&mut text_buf);
                    continue;
                }
                // All inline tags (`span`, `b`, `strong`, `a`, `font`, `mark`, `u`, `i`, `code`, etc.)
                // are silently dropped along with all their attributes!
                continue;
            }
        }

        let next_lt = frag[i..].find('<').unwrap_or(frag.len() - i);
        let raw_chunk = &frag[i..i + next_lt];
        i += next_lt;

        let decoded = decode_html_entities(raw_chunk);
        if decoded.is_empty() {
            continue;
        }
        if in_pre > 0 {
            text_buf.push_str(&sanitize_external_plain_text(&decoded));
        } else {
            let mut collapsed = String::with_capacity(decoded.len());
            let mut prev_ws = text_buf.is_empty()
                || text_buf.ends_with('\n')
                || text_buf.ends_with(' ')
                || text_buf.ends_with('\t');
            for ch in decoded.chars() {
                if matches!(ch, '\u{200B}' | '\u{FEFF}') {
                    continue;
                }
                if ch.is_whitespace() || ch == '\u{00A0}' {
                    if !prev_ws {
                        collapsed.push(' ');
                        prev_ws = true;
                    }
                } else {
                    collapsed.push(ch);
                    prev_ws = false;
                }
            }
            text_buf.push_str(&collapsed);
        }
    }

    flush_text(&mut text_buf, &mut segments);
    segments
}

/// Decodes a `data:image/<ext>;base64,...` URI into raw image bytes + file extension.
pub fn decode_data_image_uri(data_uri: &str) -> Option<(Vec<u8>, String)> {
    let trimmed = data_uri.trim();
    let rest = trimmed.strip_prefix("data:image/")?;
    let (meta, payload) = rest.split_once(',')?;
    let meta_lower = meta.to_ascii_lowercase();
    let sub = meta_lower.split(';').next().unwrap_or("png");
    let ext = match sub {
        "jpeg" | "jpg" => "jpg",
        "svg+xml" => "svg",
        "gif" => "gif",
        "webp" => "webp",
        "bmp" => "bmp",
        _ => "png",
    }
    .to_string();

    if meta_lower.contains(";base64") {
        let clean_b64: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(clean_b64.as_bytes())
            .ok()?;
        Some((bytes, ext))
    } else {
        let decoded = percent_encoding::percent_decode_str(payload).collect::<Vec<u8>>();
        Some((decoded, ext))
    }
}

/// Converts raw Win32 `CF_DIB` (`BITMAPINFOHEADER` + pixel buffer) bytes into lossless PNG bytes.
pub fn dib_bytes_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?) as usize;
    if header_size < 40 || dib.len() < header_size {
        return None;
    }
    let width_i32 = i32::from_le_bytes(dib[4..8].try_into().ok()?);
    let height_i32 = i32::from_le_bytes(dib[8..12].try_into().ok()?);
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);

    if width_i32 <= 0 || width_i32 > 16384 || height_i32 == 0 || height_i32.unsigned_abs() > 16384 {
        return None;
    }
    if bit_count != 24 && bit_count != 32 {
        return None;
    }
    // BI_RGB = 0, BI_BITFIELDS = 3
    if compression != 0 && compression != 3 {
        return None;
    }

    let width = width_i32 as u32;
    let height = height_i32.unsigned_abs();
    let top_down = height_i32 < 0;

    let extra_masks = if compression == 3 && header_size == 40 {
        12usize
    } else {
        0usize
    };
    let pixel_offset = header_size + extra_masks;
    if dib.len() <= pixel_offset {
        return None;
    }
    let pixels = &dib[pixel_offset..];
    let bytes_per_px = (bit_count / 8) as usize;
    let row_stride = (width as usize * bytes_per_px).div_ceil(4) * 4;
    if pixels.len()
        < row_stride * (height as usize).saturating_sub(1) + width as usize * bytes_per_px
    {
        return None;
    }

    let mut rgba = vec![255u8; (width * height * 4) as usize];
    let mut any_non_zero_alpha = false;

    for y in 0..height as usize {
        let src_row = if top_down { y } else { height as usize - 1 - y };
        let row_start = src_row * row_stride;
        for x in 0..width as usize {
            let p = row_start + x * bytes_per_px;
            if p + bytes_per_px > pixels.len() {
                break;
            }
            let b = pixels[p];
            let g = pixels[p + 1];
            let r = pixels[p + 2];
            let a = if bytes_per_px == 4 {
                let alpha = pixels[p + 3];
                if alpha != 0 {
                    any_non_zero_alpha = true;
                }
                alpha
            } else {
                255
            };
            let dst = (y * width as usize + x) * 4;
            rgba[dst] = r;
            rgba[dst + 1] = g;
            rgba[dst + 2] = b;
            rgba[dst + 3] = a;
        }
    }

    // Many Win32 GDI apps write 32-bit DIBs with the 4th byte left at 0x00 (XRGB instead of ARGB).
    if bytes_per_px == 4 && !any_non_zero_alpha {
        for chunk in rgba.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
    }

    let img = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

#[cfg(windows)]
mod win_imp {
    use super::*;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_DIB_FMT: u32 = 8;
    const CF_UNICODETEXT_FMT: u32 = 13;
    const CF_HDROP_FMT: u32 = 15;

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn read_raw_format_bytes(fmt: u32) -> Option<Vec<u8>> {
        if fmt == 0 || IsClipboardFormatAvailable(fmt) == 0 {
            return None;
        }
        let h: HANDLE = GetClipboardData(fmt);
        if h.is_null() {
            return None;
        }
        let size = GlobalSize(h);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(h) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        GlobalUnlock(h);
        Some(data)
    }

    unsafe fn read_unicode_text() -> Option<String> {
        if IsClipboardFormatAvailable(CF_UNICODETEXT_FMT) == 0 {
            return None;
        }
        let h: HANDLE = GetClipboardData(CF_UNICODETEXT_FMT);
        if h.is_null() {
            return None;
        }
        let byte_len = GlobalSize(h);
        if byte_len < 2 {
            return None;
        }
        let ptr = GlobalLock(h) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let max_u16 = byte_len / 2;
        let slice = std::slice::from_raw_parts(ptr, max_u16);
        let end = slice.iter().position(|&c| c == 0).unwrap_or(max_u16);
        let text = String::from_utf16_lossy(&slice[..end]);
        GlobalUnlock(h);
        Some(text)
    }

    unsafe fn read_hdrop_image_files() -> Option<Vec<PathBuf>> {
        if IsClipboardFormatAvailable(CF_HDROP_FMT) == 0 {
            return None;
        }
        let h: HANDLE = GetClipboardData(CF_HDROP_FMT);
        if h.is_null() {
            return None;
        }
        let hdrop = h as HDROP;
        let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, std::ptr::null_mut(), 0);
        if count == 0 {
            return None;
        }
        let mut out = Vec::new();
        for idx in 0..count {
            let len = DragQueryFileW(hdrop, idx, std::ptr::null_mut(), 0);
            if len == 0 {
                continue;
            }
            let mut buf = vec![0u16; (len + 1) as usize];
            let copied = DragQueryFileW(hdrop, idx, buf.as_mut_ptr(), len + 1);
            if copied > 0 {
                let p = PathBuf::from(String::from_utf16_lossy(&buf[..copied as usize]));
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    ext.as_str(),
                    "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
                ) && p.is_file()
                {
                    out.push(p);
                }
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub fn read_windows_clipboard(force_plain_text: bool) -> Option<ClipboardPasteAction> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let _guard = ClipboardGuard;

            let fmt_png = RegisterClipboardFormatW(wide("PNG").as_ptr());
            let fmt_mime_png = RegisterClipboardFormatW(wide("image/png").as_ptr());
            let fmt_rtf = RegisterClipboardFormatW(wide("Rich Text Format").as_ptr());
            let fmt_html = RegisterClipboardFormatW(wide("HTML Format").as_ptr());

            let has_rtf = fmt_rtf != 0 && IsClipboardFormatAvailable(fmt_rtf) != 0;
            let raw_plain = read_unicode_text().unwrap_or_default();
            let raw_html = read_raw_format_bytes(fmt_html)
                .map(|b| {
                    let end = b.iter().position(|&x| x == 0).unwrap_or(b.len());
                    String::from_utf8_lossy(&b[..end]).to_string()
                })
                .unwrap_or_default();

            if !force_plain_text {
                // 1) Explorer copied image file(s)
                if let Some(files) = read_hdrop_image_files() {
                    return Some(ClipboardPasteAction::ImageFiles(files));
                }

                let html_segments = if raw_html.to_ascii_lowercase().contains("<img") {
                    extract_plain_text_and_images_from_html(&raw_html)
                } else {
                    Vec::new()
                };
                let html_only_images = !html_segments.is_empty()
                    && html_segments
                        .iter()
                        .all(|s| matches!(s, CleanPasteSegment::Image { .. }));

                // 2) Screenshot / Snipping Tool / Browser "Copy Image" (skip MS Office RTF text snapshots)
                if !has_rtf && (raw_plain.trim().is_empty() || html_only_images) {
                    if let Some(png_bytes) = read_raw_format_bytes(fmt_png)
                        .or_else(|| read_raw_format_bytes(fmt_mime_png))
                    {
                        if png_bytes.starts_with(b"\x89PNG") {
                            return Some(ClipboardPasteAction::ImageBytes {
                                bytes: png_bytes,
                                ext: "png".into(),
                            });
                        }
                    }
                    if let Some(dib_bytes) = read_raw_format_bytes(CF_DIB_FMT) {
                        if let Some(png_bytes) = dib_bytes_to_png(&dib_bytes) {
                            return Some(ClipboardPasteAction::ImageBytes {
                                bytes: png_bytes,
                                ext: "png".into(),
                            });
                        }
                    }
                }

                // 3) Mixed external web article (text + <img>): plain text + preserved images
                if html_segments
                    .iter()
                    .any(|s| matches!(s, CleanPasteSegment::Image { .. }))
                {
                    return Some(ClipboardPasteAction::Segments(html_segments));
                }
            }

            // 4) External text -> 100% pure plain text
            let mut plain = sanitize_external_plain_text(&raw_plain);
            if plain.trim().is_empty() && !raw_html.trim().is_empty() {
                let segs = extract_plain_text_and_images_from_html(&raw_html);
                plain = segs
                    .into_iter()
                    .filter_map(|s| match s {
                        CleanPasteSegment::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
            if plain.is_empty() {
                None
            } else {
                Some(ClipboardPasteAction::PlainText(plain))
            }
        }
    }
}

#[cfg(windows)]
pub use win_imp::read_windows_clipboard;

#[cfg(not(windows))]
pub fn read_windows_clipboard(_force_plain_text: bool) -> Option<ClipboardPasteAction> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_html_strips_all_text_attributes_and_preserves_images() {
        let cf_html = concat!(
            "Version:0.9\r\nStartHTML:00000090\r\n<!--StartFragment-->",
            "<div class=\"article\" style=\"color: rgb(32, 33, 36); font-size: 16px;\">",
            "<h2 style=\"color:purple\">외부 기사 제목</h2>",
            "<p><span style=\"color:red;background:yellow\">첫 문단&#8203; </span>",
            "<a href=\"https://example.com\"><b>링크 글자</b></a></p>",
            "<img src=\"data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==\" ",
            "data-lazy-src=\"https://images.example.com/chart.png\" alt=\"차트 그림\" style=\"width:600px\"/>",
            "<p><mark>마무리 문단</mark></p>",
            "</div><!--EndFragment-->"
        );
        let segs = extract_plain_text_and_images_from_html(cf_html);
        assert_eq!(
            segs,
            vec![
                CleanPasteSegment::Text("외부 기사 제목\n\n첫 문단 링크 글자".to_string()),
                CleanPasteSegment::Image {
                    src: "https://images.example.com/chart.png".to_string(),
                    alt: "차트 그림".to_string(),
                },
                CleanPasteSegment::Text("마무리 문단".to_string()),
            ]
        );
    }

    #[test]
    fn dib_32bit_converts_to_valid_png() {
        // 2x2 32-bit bottom-up BITMAPINFOHEADER
        let mut dib = vec![0u8; 40 + 16];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2i32.to_le_bytes());
        dib[8..12].copy_from_slice(&2i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        // Blue, Green, Red, Alpha pixels
        dib[40..44].copy_from_slice(&[255, 0, 0, 255]);
        dib[44..48].copy_from_slice(&[0, 255, 0, 255]);
        dib[48..52].copy_from_slice(&[0, 0, 255, 255]);
        dib[52..56].copy_from_slice(&[255, 255, 255, 255]);
        let png = dib_bytes_to_png(&dib).expect("PNG encoded");
        assert!(png.starts_with(b"\x89PNG"));
    }

    #[test]
    fn strip_html_wrappers_cleans_existing_tags_and_keeps_markdown_images() {
        let dirty = "<span style=\"color: rgb(32,33,36); font-size: 16px;\">안녕</span> ![그림](attachments/a.png) <div>하세요</div>";
        let clean = strip_html_wrappers_preserving_images(dirty);
        assert_eq!(clean, "안녕 ![그림](attachments/a.png) 하세요");
    }
}
