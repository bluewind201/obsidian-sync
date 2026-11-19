//! Onyx Studio 네이티브 A4 인쇄 & PDF 내보내기 엔진 (Zero `WebView2Loader.dll`).
//! - ISO A4(210×297mm 세로 / 297×210mm 가로), 사용자 지정 여백(cm), 표 테두리/음영, 벡터 플로우차트(`<svg>`),
//!   다단 레이아웃, 콜아웃 및 볼트 내장 이미지(Base64 data URI)를 1:1 인쇄용 HTML로 변환합니다.
//! - PDF 저장 시 Microsoft Edge/Chrome Headless `--print-to-pdf`(창 숨김)로 고해상도 A4 PDF를 출력하며,
//!   브라우저가 없는 환경에서도 순수 Rust `%PDF-1.4` 내장 생성기로 폴백 동작합니다.

use crate::flowchart;
use crate::platform;
use crate::vault::{self, NotePropertyDto};
use anyhow::{Context, Result};
use base64::Engine as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct PagePrintConfig {
    pub landscape: bool,
    pub margin_top_cm: f64,
    pub margin_bottom_cm: f64,
    pub margin_left_cm: f64,
    pub margin_right_cm: f64,
    pub include_title: bool,
    pub include_properties: bool,
}

impl Default for PagePrintConfig {
    fn default() -> Self {
        Self {
            landscape: false,
            margin_top_cm: 2.0,
            margin_bottom_cm: 1.0,
            margin_left_cm: 1.5,
            margin_right_cm: 1.5,
            include_title: true,
            include_properties: false,
        }
    }
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn mime_for_ext(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

fn resolve_img_src_for_print(vault_root: Option<&Path>, target: &str) -> String {
    let clean = target
        .trim()
        .trim_matches(|c| c == '<' || c == '>' || c == '"');
    if clean.starts_with("http://") || clean.starts_with("https://") || clean.starts_with("data:") {
        return clean.to_string();
    }
    let decoded = percent_encoding::percent_decode_str(clean)
        .decode_utf8_lossy()
        .replace('\\', "/");
    if let Some(vr) = vault_root {
        if let Some(abs) = vault::resolve_vault_media_file(vr, &decoded) {
            if let Ok(bytes) = std::fs::read(&abs) {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let mime = mime_for_ext(&abs);
                return format!("data:{mime};base64,{b64}");
            }
        }
    }
    clean.to_string()
}

fn render_inline_html(raw: &str, vault_root: Option<&Path>) -> String {
    let mut out = String::new();
    let mut rest = raw;

    while !rest.is_empty() {
        let next_embed = rest.find("![[");
        let next_md_img = rest.find("![");
        let next_link = rest.find('[');
        let next_wiki = rest.find("[[");
        let next_code = rest.find('`');
        let next_bold = rest.find("**");
        let next_mark = rest.find("==");

        let earliest = [
            next_embed.map(|p| (p, "embed_img")),
            next_md_img
                .filter(|&p| next_embed != Some(p))
                .map(|p| (p, "md_img")),
            next_wiki
                .filter(|&p| next_embed.map(|e| e + 1) != Some(p))
                .map(|p| (p, "wiki")),
            next_link
                .filter(|&p| {
                    next_wiki != Some(p)
                        && next_embed.map(|e| e + 1) != Some(p)
                        && next_md_img.map(|e| e + 1) != Some(p)
                })
                .map(|p| (p, "link")),
            next_code.map(|p| (p, "code")),
            next_bold.map(|p| (p, "bold")),
            next_mark.map(|p| (p, "mark")),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(pos, _)| *pos);

        let Some((pos, kind)) = earliest else {
            out.push_str(&escape_html(rest));
            break;
        };

        if pos > 0 {
            out.push_str(&escape_html(&rest[..pos]));
            rest = &rest[pos..];
        }

        match kind {
            "embed_img" => {
                if let Some(end) = rest[3..].find("]]") {
                    let inner = &rest[3..3 + end];
                    let (target, opt_w) = match inner.split_once('|') {
                        Some((t, w)) => (t.trim(), w.trim().parse::<u32>().ok()),
                        None => (inner.trim(), None),
                    };
                    let src = resolve_img_src_for_print(vault_root, target);
                    let width_attr = opt_w
                        .map(|w| format!(r#" style="max-width:{w}px;""#))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        r#"<img src="{}" alt="{}"{}/>"#,
                        escape_html(&src),
                        escape_html(target),
                        width_attr
                    ));
                    rest = &rest[3 + end + 2..];
                } else {
                    out.push_str("![[");
                    rest = &rest[3..];
                }
            }
            "md_img" => {
                if let Some(mid) = rest[2..].find("](") {
                    let after = &rest[2 + mid + 2..];
                    if let Some(end_p) = after.find(')') {
                        let alt = rest[2..2 + mid].trim();
                        let target = after[..end_p].trim();
                        let src = resolve_img_src_for_print(vault_root, target);
                        out.push_str(&format!(
                            r#"<img src="{}" alt="{}"/>"#,
                            escape_html(&src),
                            escape_html(alt)
                        ));
                        rest = &after[end_p + 1..];
                    } else {
                        out.push_str("![");
                        rest = &rest[2..];
                    }
                } else {
                    out.push_str("![");
                    rest = &rest[2..];
                }
            }
            "wiki" => {
                if let Some(end) = rest[2..].find("]]") {
                    let inner = &rest[2..2 + end];
                    let display = inner.split('|').last().unwrap_or(inner).trim();
                    out.push_str(&format!(
                        r#"<span class="wiki-chip">[[{}]]</span>"#,
                        escape_html(display)
                    ));
                    rest = &rest[2 + end + 2..];
                } else {
                    out.push_str("[[");
                    rest = &rest[2..];
                }
            }
            "link" => {
                if let Some(mid) = rest[1..].find("](") {
                    let after = &rest[1 + mid + 2..];
                    if let Some(end_p) = after.find(')') {
                        let txt = &rest[1..1 + mid];
                        let href = after[..end_p].trim();
                        out.push_str(&format!(
                            r#"<a href="{}">{}</a>"#,
                            escape_html(href),
                            escape_html(txt)
                        ));
                        rest = &after[end_p + 1..];
                    } else {
                        out.push('[');
                        rest = &rest[1..];
                    }
                } else {
                    out.push('[');
                    rest = &rest[1..];
                }
            }
            "code" => {
                if let Some(end) = rest[1..].find('`') {
                    out.push_str(&format!("<code>{}</code>", escape_html(&rest[1..1 + end])));
                    rest = &rest[1 + end + 1..];
                } else {
                    out.push('`');
                    rest = &rest[1..];
                }
            }
            "bold" => {
                if let Some(end) = rest[2..].find("**") {
                    out.push_str(&format!(
                        "<strong>{}</strong>",
                        escape_html(&rest[2..2 + end])
                    ));
                    rest = &rest[2 + end + 2..];
                } else {
                    out.push_str("**");
                    rest = &rest[2..];
                }
            }
            "mark" => {
                if let Some(end) = rest[2..].find("==") {
                    out.push_str(&format!("<mark>{}</mark>", escape_html(&rest[2..2 + end])));
                    rest = &rest[2 + end + 2..];
                } else {
                    out.push_str("==");
                    rest = &rest[2..];
                }
            }
            _ => break,
        }
    }

    out
}

fn render_markdown_body_to_html(md: &str, vault_root: Option<&Path>) -> String {
    let mut html = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Page break marker
        if trimmed.contains("page-break") || trimmed == "<!-- pagebreak -->" {
            html.push_str(r#"<div class="page-break"></div>"#);
            i += 1;
            continue;
        }

        // Preserve structural multi-column or details HTML blocks cleanly
        if trimmed.starts_with("<div class=\"doc-cols") {
            let mut cols_class = "doc-cols doc-cols-2";
            if trimmed.contains("doc-cols-3") {
                cols_class = "doc-cols doc-cols-3";
            }
            html.push_str(&format!(r#"<div class="{cols_class}">"#));
            i += 1;
            continue;
        }
        if trimmed.starts_with("<div class=\"doc-col\"") {
            html.push_str(r#"<div class="doc-col">"#);
            i += 1;
            continue;
        }
        if trimmed == "</div>" {
            html.push_str("</div>");
            i += 1;
            continue;
        }

        // Fenced code or vector flowchart SVG
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let lang = trimmed.trim_start_matches(['`', '~']).trim();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                let l = lines[i];
                if l.trim().starts_with("```") || l.trim().starts_with("~~~") {
                    i += 1;
                    break;
                }
                code_lines.push(l);
                i += 1;
            }
            let code_body = code_lines.join("\n");
            if flowchart::is_diagram_code_block(lang, &code_body) {
                if let Some(svg) = flowchart::render_diagram_to_svg(&code_body) {
                    html.push_str(&format!(r#"<div class="diagram-card">{svg}</div>"#));
                    continue;
                }
            }
            html.push_str(&format!(
                "<pre><code>{}</code></pre>",
                escape_html(&code_body)
            ));
            continue;
        }

        // Headings
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            let inner = trimmed[hashes..].trim();
            html.push_str(&format!(
                "<h{hashes}>{}</h{hashes}>",
                render_inline_html(inner, vault_root)
            ));
            i += 1;
            continue;
        }

        // Horizontal rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            html.push_str("<hr/>");
            i += 1;
            continue;
        }

        // Table
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() {
                let tl = lines[i].trim();
                if tl.starts_with('|') && tl.ends_with('|') {
                    let cells: Vec<String> = tl[1..tl.len() - 1]
                        .split('|')
                        .map(|c| c.trim().to_string())
                        .collect();
                    let is_sep = cells.iter().all(|c| {
                        !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
                    });
                    if !is_sep {
                        rows.push(cells);
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            if !rows.is_empty() {
                html.push_str("<table><thead><tr>");
                for c in &rows[0] {
                    html.push_str(&format!("<th>{}</th>", render_inline_html(c, vault_root)));
                }
                html.push_str("</tr></thead><tbody>");
                for row in rows.iter().skip(1) {
                    html.push_str("<tr>");
                    for c in row {
                        html.push_str(&format!("<td>{}</td>", render_inline_html(c, vault_root)));
                    }
                    html.push_str("</tr>");
                }
                html.push_str("</tbody></table>");
            }
            continue;
        }

        // Blockquote & Notion Callout
        if let Some(first_q) = trimmed.strip_prefix('>') {
            let mut q_lines = vec![first_q.trim_start()];
            i += 1;
            while i < lines.len() {
                if let Some(q) = lines[i].trim().strip_prefix('>') {
                    q_lines.push(q.trim_start());
                    i += 1;
                } else {
                    break;
                }
            }
            let first = q_lines[0];
            if first.starts_with("[!") {
                if let Some(end) = first.find(']') {
                    let kind = first[2..end].to_lowercase();
                    let title = first[end + 1..].trim();
                    let head = if title.is_empty() {
                        kind.to_uppercase()
                    } else {
                        title.to_string()
                    };
                    html.push_str(&format!(
                        r#"<div class="callout callout-{}"><div class="callout-title">{}</div>"#,
                        escape_html(&kind),
                        escape_html(&head)
                    ));
                    for ql in q_lines.iter().skip(1) {
                        html.push_str(&format!("<p>{}</p>", render_inline_html(ql, vault_root)));
                    }
                    html.push_str("</div>");
                    continue;
                }
            }
            html.push_str("<blockquote>");
            for ql in &q_lines {
                html.push_str(&format!("<p>{}</p>", render_inline_html(ql, vault_root)));
            }
            html.push_str("</blockquote>");
            continue;
        }

        // Unordered / Checklist
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            html.push_str("<ul>");
            loop {
                let item = lines[i]
                    .trim()
                    .strip_prefix("- ")
                    .or_else(|| lines[i].trim().strip_prefix("* "));
                let Some(it) = item else { break };
                if let Some(task) = it.strip_prefix("[ ] ") {
                    html.push_str(&format!(
                        r#"<li class="task">☐ {}</li>"#,
                        render_inline_html(task, vault_root)
                    ));
                } else if let Some(task) =
                    it.strip_prefix("[x] ").or_else(|| it.strip_prefix("[X] "))
                {
                    html.push_str(&format!(
                        r#"<li class="task">☑ {}</li>"#,
                        render_inline_html(task, vault_root)
                    ));
                } else {
                    html.push_str(&format!("<li>{}</li>", render_inline_html(it, vault_root)));
                }
                i += 1;
                if i >= lines.len() {
                    break;
                }
            }
            html.push_str("</ul>");
            continue;
        }

        // Standalone image line
        if trimmed.starts_with("![") {
            html.push_str(&format!(
                r#"<div class="img-block">{}</div>"#,
                render_inline_html(trimmed, vault_root)
            ));
            i += 1;
            continue;
        }

        html.push_str(&format!(
            "<p>{}</p>",
            render_inline_html(trimmed, vault_root)
        ));
        i += 1;
    }

    html
}

/// A4 인쇄 및 PDF 출력용 독립 HTML5 문서를 생성합니다.
pub fn build_printable_a4_html(
    title: &str,
    icon: Option<&str>,
    properties: &[NotePropertyDto],
    markdown_body: &str,
    vault_root: Option<&Path>,
    cfg: &PagePrintConfig,
    auto_print_dialog: bool,
) -> String {
    let size_css = if cfg.landscape {
        "297mm 210mm"
    } else {
        "210mm 297mm"
    };
    let title_block = if cfg.include_title {
        let ic = icon
            .map(|s| format!("{} ", escape_html(s)))
            .unwrap_or_default();
        format!(r#"<h1 class="doc-title">{ic}{}</h1>"#, escape_html(title))
    } else {
        String::new()
    };

    let props_block = if cfg.include_properties && !properties.is_empty() {
        let mut rows = String::new();
        for p in properties {
            if p.key.trim().is_empty() {
                continue;
            }
            rows.push_str(&format!(
                "<tr><th>{}</th><td>{}</td></tr>",
                escape_html(&p.key),
                escape_html(&p.value)
            ));
        }
        if rows.is_empty() {
            String::new()
        } else {
            format!(r#"<table class="props-table">{rows}</table>"#)
        }
    } else {
        String::new()
    };

    let body_html = render_markdown_body_to_html(markdown_body, vault_root);
    let onload_script = if auto_print_dialog {
        "<script>window.onload=function(){setTimeout(function(){window.print();},250);};</script>"
    } else {
        ""
    };

    format!(
        r#"<!doctype html>
<html lang="ko">
<head>
<meta charset="utf-8"/>
<title>{doc_title}</title>
<style>
@page {{
  size: {size_css};
  margin: {m_top}cm {m_right}cm {m_bot}cm {m_left}cm;
}}
* {{ box-sizing: border-box; -webkit-print-color-adjust: exact !important; print-color-adjust: exact !important; }}
body {{
  margin: 0;
  padding: 0;
  font-family: 'Pretendard', 'Malgun Gothic', 'Apple SD Gothic Neo', sans-serif;
  font-size: 10pt;
  line-height: 1.62;
  color: #1e293b;
  background: #ffffff;
}}
.doc-title {{
  margin: 0 0 12px;
  font-size: 20pt;
  font-weight: 800;
  color: #111827;
  border-bottom: 1.5px solid #e2e8f0;
  padding-bottom: 8px;
}}
.props-table {{ margin: 0 0 14px; font-size: 9pt; }}
h1 {{ font-size: 16.5pt; margin: 16px 0 6px; }}
h2 {{ font-size: 13.5pt; margin: 14px 0 5px; }}
h3 {{ font-size: 11.5pt; margin: 11px 0 4px; }}
p {{ margin: 5px 0; }}
ul {{ margin: 5px 0 8px 20px; padding: 0; }}
li.task {{ list-style: none; margin-left: -14px; }}
table {{
  border-collapse: collapse !important;
  width: 100%;
  margin: 10px 0 !important;
  border: 1px solid #94a3b8 !important;
  page-break-inside: avoid;
}}
th, td {{
  border: 1px solid #cbd5e1 !important;
  padding: 6px 9px !important;
  color: #1e293b !important;
  background: #ffffff !important;
  text-align: left;
  vertical-align: top;
}}
th {{
  background: #f1f5f9 !important;
  color: #0f172a !important;
  font-weight: 700 !important;
  border-bottom: 1.5px solid #94a3b8 !important;
}}
code {{
  background: #f6f5f3;
  color: #eb5757;
  padding: 1px 4px;
  border-radius: 3px;
  font-family: Consolas, monospace;
  font-size: 9pt;
}}
pre {{
  background: #f6f5f3;
  border: 1px solid #e2e8f0;
  border-radius: 6px;
  padding: 10px 12px;
  white-space: pre-wrap;
  word-break: break-word;
}}
pre code {{ background: transparent; color: #1e293b; padding: 0; }}
mark {{ background: #fef08a; padding: 1px 3px; border-radius: 3px; }}
blockquote {{
  border-left: 3.5px solid #2383e2;
  background: #f8fafc;
  padding: 8px 12px;
  margin: 10px 0;
  border-radius: 0 6px 6px 0;
}}
.callout {{
  border: 1px solid #bfdbfe;
  border-left: 4px solid #2383e2;
  background: #eff6ff;
  border-radius: 6px;
  padding: 8px 12px;
  margin: 10px 0;
  page-break-inside: avoid;
}}
.callout-warning, .callout-caution {{ border-color: #fde68a; border-left-color: #d97706; background: #fffbeb; }}
.callout-danger, .callout-error {{ border-color: #fecaca; border-left-color: #dc2626; background: #fef2f2; }}
.callout-tip, .callout-success {{ border-color: #a7f3d0; border-left-color: #059669; background: #ecfdf5; }}
.callout-title {{ font-weight: 700; margin-bottom: 3px; }}
.doc-cols {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 14px; margin: 10px 0; }}
.doc-cols-3 {{ grid-template-columns: repeat(3, 1fr); }}
.doc-col {{ border: 1px solid #e2e8f0; border-radius: 6px; padding: 8px 10px; }}
.wiki-chip {{ color: #2383e2; background: #e8f3fe; padding: 1px 5px; border-radius: 3px; font-weight: 600; }}
.img-block img, img {{ max-width: 100%; height: auto; border-radius: 6px; margin: 6px 0; }}
.diagram-card {{ border: 1px solid #cbd5e1; border-radius: 8px; padding: 8px; margin: 10px 0; page-break-inside: avoid; }}
.page-break {{ page-break-before: always; break-before: page; }}
</style>
{onload_script}
</head>
<body>
{title_block}
{props_block}
{body_html}
</body>
</html>"#,
        doc_title = escape_html(title),
        size_css = size_css,
        m_top = cfg.margin_top_cm,
        m_right = cfg.margin_right_cm,
        m_bot = cfg.margin_bottom_cm,
        m_left = cfg.margin_left_cm,
        title_block = title_block,
        props_block = props_block,
        body_html = body_html,
        onload_script = onload_script,
    )
}

fn find_headless_chromium() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
    ];
    for p in candidates {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    None
}

fn render_html_to_pdf_headless(html_path: &Path, pdf_dest: &Path) -> Result<()> {
    let browser = find_headless_chromium().context("Headless Chromium/Edge 미설치")?;
    let mut cmd = Command::new(&browser);
    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    cmd.args([
        "--headless",
        "--disable-gpu",
        "--no-pdf-header-footer",
        "--allow-file-access-from-files",
        &format!("--print-to-pdf={}", pdf_dest.display()),
        &url,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output()?;
    if !out.status.success() || !pdf_dest.is_file() || std::fs::metadata(pdf_dest)?.len() < 400 {
        anyhow::bail!("Headless PDF 생성 실패");
    }
    Ok(())
}

fn pdf_escape_ascii(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let c = if ch.is_ascii() && !ch.is_control() {
            ch
        } else {
            '?'
        };
        if matches!(c, '(' | ')' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 브라우저가 없는 환경에서도 표준 규격 A4 `%PDF-1.4` 문서를 생성하는 순수 Rust 폴백 라이터입니다.
pub fn write_pure_rust_a4_pdf(
    dest: &Path,
    title: &str,
    markdown_body: &str,
    cfg: &PagePrintConfig,
) -> Result<()> {
    let (page_w, page_h) = if cfg.landscape {
        (841.89f64, 595.28f64)
    } else {
        (595.28f64, 841.89f64)
    };
    let cm_to_pt = 28.3464567;
    let m_left = cfg.margin_left_cm * cm_to_pt;
    let m_top = cfg.margin_top_cm * cm_to_pt;
    let m_bot = cfg.margin_bottom_cm * cm_to_pt;

    let mut content = String::new();
    content.push_str("BT\n/F1 16 Tf\n");
    let mut y = page_h - m_top - 16.0;
    content.push_str(&format!(
        "1 0 0 1 {:.2} {:.2} Tm ({}) Tj\n",
        m_left,
        y,
        pdf_escape_ascii(title)
    ));
    y -= 24.0;
    content.push_str("/F1 10 Tf\n");

    let mut pages_streams: Vec<String> = Vec::new();
    for line in markdown_body.lines() {
        if y < m_bot + 14.0 {
            content.push_str("ET\n");
            pages_streams.push(std::mem::take(&mut content));
            content.push_str("BT\n/F1 10 Tf\n");
            y = page_h - m_top - 10.0;
        }
        let clean = line.trim();
        if !clean.is_empty() {
            content.push_str(&format!(
                "1 0 0 1 {:.2} {:.2} Tm ({}) Tj\n",
                m_left,
                y,
                pdf_escape_ascii(clean)
            ));
        }
        y -= 14.5;
    }
    content.push_str("ET\n");
    pages_streams.push(content);

    // Build standard PDF 1.4 objects
    let mut pdf = Vec::<u8>::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets: Vec<usize> = Vec::new();
    let mut push_obj = |id: usize, body: &[u8]| {
        offsets.resize(id + 1, 0);
        offsets[id] = pdf.len();
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    };

    let n_pages = pages_streams.len();
    let kids: Vec<String> = (0..n_pages).map(|i| format!("{} 0 R", 4 + i * 2)).collect();
    push_obj(1, b"<< /Type /Catalog /Pages 2 0 R >>");
    push_obj(
        2,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            n_pages
        )
        .as_bytes(),
    );
    push_obj(3, b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>");

    for (i, stream) in pages_streams.iter().enumerate() {
        let page_id = 4 + i * 2;
        let cont_id = 5 + i * 2;
        push_obj(
            page_id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w:.2} {page_h:.2}] /Resources << /Font << /F1 3 0 R >> >> /Contents {cont_id} 0 R >>"
            )
            .as_bytes(),
        );
        let stream_bytes = stream.as_bytes();
        let mut cont_body = format!("<< /Length {} >>\nstream\n", stream_bytes.len()).into_bytes();
        cont_body.extend_from_slice(stream_bytes);
        cont_body.extend_from_slice(b"endstream");
        push_obj(cont_id, &cont_body);
    }

    let xref_pos = pdf.len();
    let max_id = 3 + n_pages * 2;
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", max_id + 1).as_bytes());
    for id in 1..=max_id {
        let off = offsets.get(id).copied().unwrap_or(0);
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            max_id + 1,
            xref_pos
        )
        .as_bytes(),
    );

    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut f = std::fs::File::create(dest)?;
    f.write_all(&pdf)?;
    Ok(())
}

/// 저장 다이얼로그를 열어 현재 노트를 A4 PDF 파일로 저장합니다.
pub fn export_note_pdf(
    default_filename: &str,
    title: &str,
    icon: Option<&str>,
    properties: &[NotePropertyDto],
    markdown_body: &str,
    vault_root: Option<&Path>,
    cfg: &PagePrintConfig,
) -> Result<Option<PathBuf>> {
    let Some(dest) = platform::save_pdf_dialog(default_filename) else {
        return Ok(None);
    };

    let tmp_html =
        std::env::temp_dir().join(format!("onyx-studio-export-{}.html", std::process::id()));
    let html = build_printable_a4_html(
        title,
        icon,
        properties,
        markdown_body,
        vault_root,
        cfg,
        false,
    );
    std::fs::write(&tmp_html, html.as_bytes())?;

    let headless_ok = render_html_to_pdf_headless(&tmp_html, &dest).is_ok();
    let _ = std::fs::remove_file(&tmp_html);
    if !headless_ok {
        write_pure_rust_a4_pdf(&dest, title, markdown_body, cfg)?;
    }
    Ok(Some(dest))
}

/// 시스템 인쇄 대화상자(`Ctrl+P` / `🖨️`)를 A4 여백 규격에 맞춰 엽니다.
pub fn print_note_a4(
    title: &str,
    icon: Option<&str>,
    properties: &[NotePropertyDto],
    markdown_body: &str,
    vault_root: Option<&Path>,
    cfg: &PagePrintConfig,
) -> Result<()> {
    let tmp_html = std::env::temp_dir().join("onyx-studio-print.html");
    let html = build_printable_a4_html(
        title,
        icon,
        properties,
        markdown_body,
        vault_root,
        cfg,
        true,
    );
    std::fs::write(&tmp_html, html.as_bytes())?;
    open::that(&tmp_html)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_html_includes_a4_margins_tables_and_svg_flowchart() {
        let cfg = PagePrintConfig {
            landscape: false,
            margin_top_cm: 2.0,
            margin_bottom_cm: 1.0,
            margin_left_cm: 1.5,
            margin_right_cm: 1.5,
            include_title: true,
            include_properties: true,
        };
        let props = vec![NotePropertyDto {
            key: "분류".into(),
            value: "설계".into(),
            prop_type: "text".into(),
        }];
        let md = "| 항목 | 값 |\n| --- | --- |\n| 테스트 | 정상 |\n\n```flowchart\nflowchart TB\n    A[시작] --> B[완료]\n```\n";
        let html =
            build_printable_a4_html("테스트 노트", Some("📘"), &props, md, None, &cfg, false);
        assert!(html.contains("size: 210mm 297mm;"));
        assert!(html.contains("margin: 2cm 1.5cm 1cm 1.5cm;"));
        assert!(html.contains("#cbd5e1"));
        assert!(html.contains("<svg "));
        assert!(html.contains("테스트 노트"));
    }

    #[test]
    fn pure_rust_a4_pdf_writes_valid_pdf_header_and_eof() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("sample.pdf");
        let cfg = PagePrintConfig::default();
        write_pure_rust_a4_pdf(&out, "Onyx Studio Report", "Line 1\nLine 2", &cfg).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let txt = String::from_utf8_lossy(&bytes);
        assert!(txt.starts_with("%PDF-1.4"));
        assert!(txt.contains("%%EOF"));
        assert!(txt.contains("Onyx Studio Report"));
    }
}
