use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_files(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let web_path = format!("/{}", rel.to_string_lossy().replace('\\', "/"));
            out.push((web_path, path));
        }
    }
}

fn mime_for(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".woff2") {
        "font/woff2"
    } else if lower.ends_with(".woff") {
        "font/woff"
    } else if lower.ends_with(".ttf") {
        "font/ttf"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist_dir = manifest_dir.join("web-dist");
    println!("cargo:rerun-if-changed=web-dist");

    let mut files = Vec::new();
    if dist_dir.is_dir() {
        collect_files(&dist_dir, &dist_dir, &mut files);
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("web_assets.rs");
    let mut f = fs::File::create(&dest).unwrap();

    writeln!(
        f,
        "pub fn get_web_asset(path: &str) -> Option<(&'static str, &'static [u8])> {{"
    )
    .unwrap();
    writeln!(
        f,
        "    let norm = if path.is_empty() || path == \"/\" {{ \"/index.html\" }} else {{ path }};"
    )
    .unwrap();
    writeln!(f, "    match norm {{").unwrap();
    for (web_path, abs_path) in &files {
        println!("cargo:rerun-if-changed={}", abs_path.display());
        let mime = mime_for(web_path);
        let escaped = abs_path.to_string_lossy().replace('\\', "/");
        writeln!(
            f,
            "        {:?} => Some(({:?}, include_bytes!({:?}))),",
            web_path, mime, escaped
        )
        .unwrap();
    }
    writeln!(f, "        _ => None,").unwrap();
    writeln!(f, "    }}").unwrap();
    writeln!(f, "}}").unwrap();
}
