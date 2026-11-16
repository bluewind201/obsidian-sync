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
        "text/javascript; charset=utf-8"
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

fn generate_onyx_rgba(size: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity((size * size * 4) as usize);
    let half = (size as f32 - 1.0) * 0.5;
    let max_d = size.max(1) as f32;
    let lerp = |a: f32, b: f32, t: f32| -> u8 { (a + (b - a) * t).round().clamp(0.0, 255.0) as u8 };
    let blend = |bg: [u8; 4], fg: [u8; 3], alpha: f32| -> [u8; 4] {
        let a = alpha.clamp(0.0, 1.0);
        [
            lerp(bg[0] as f32, fg[0] as f32, a),
            lerp(bg[1] as f32, fg[1] as f32, a),
            lerp(bg[2] as f32, fg[2] as f32, a),
            bg[3],
        ]
    };

    for y in 0..size {
        for x in 0..size {
            let nx = if half > 0.0 {
                (x as f32 - half) / half
            } else {
                0.0
            };
            let ny = if half > 0.0 {
                (y as f32 - half) / half
            } else {
                0.0
            };
            let qx = (nx.abs() - 0.68).max(0.0);
            let qy = (ny.abs() - 0.68).max(0.0);
            let sdf = (qx * qx + qy * qy).sqrt() - 0.28;
            let px_edge = 2.0 / max_d;
            let bg_alpha = (0.5 - sdf / px_edge).clamp(0.0, 1.0);

            if bg_alpha <= 0.0 {
                v.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            let t = ((ny + 1.0) * 0.5).clamp(0.0, 1.0);
            let mut px = [
                lerp(15.0, 30.0, t),
                lerp(23.0, 41.0, t),
                lerp(42.0, 59.0, t),
                (bg_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
            ];

            let rim = 1.0 - ((sdf + 0.04).abs() / 0.04).clamp(0.0, 1.0);
            if rim > 0.0 && bg_alpha > 0.9 {
                px = blend(px, [56, 189, 248], rim * 0.35);
            }

            let gem_d = (nx.abs() * 1.24 + ny.abs()) - 0.66;
            let gem_cov = (0.5 - gem_d / (px_edge * 1.2)).clamp(0.0, 1.0);
            if gem_cov > 0.0 && bg_alpha > 0.9 {
                let facet = match (nx >= 0.0, ny >= 0.0) {
                    (false, false) => [56, 189, 248],
                    (true, false) => [129, 140, 248],
                    (false, true) => [2, 132, 199],
                    (true, true) => [99, 102, 241],
                };
                px = blend(px, facet, gem_cov);

                let core_d = (nx.abs() * 1.24 + ny.abs()) - 0.25;
                let core_cov = (0.5 - core_d / (px_edge * 1.2)).clamp(0.0, 1.0);
                if core_cov > 0.0 {
                    px = blend(px, [248, 250, 252], core_cov * 0.92);
                }
            }

            v.extend_from_slice(&px);
        }
    }
    v
}

fn encode_dib_icon(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let img_bytes = size * size * 4;
    // BITMAPINFOHEADER (40 bytes)
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight (XOR + AND mask)
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&img_bytes.to_le_bytes()); // biSizeImage
    out.extend_from_slice(&[0u8; 16]); // biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant

    // Bottom-up BGRA XOR bitmap
    for row in (0..size).rev() {
        for col in 0..size {
            let i = ((row * size + col) * 4) as usize;
            out.push(rgba[i + 2]);
            out.push(rgba[i + 1]);
            out.push(rgba[i]);
            out.push(rgba[i + 3]);
        }
    }

    // Monochrome AND mask (zeros, 32-bit aligned rows; alpha channel carries transparency)
    let mask_stride = size.div_ceil(32) * 4;
    out.resize(out.len() + (mask_stride * size) as usize, 0);
    out
}

fn append_res_entry(buf: &mut Vec<u8>, type_id: u16, name_id: u16, mem_flags: u16, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // DataSize
    buf.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    buf.extend_from_slice(&[0xff, 0xff]);
    buf.extend_from_slice(&type_id.to_le_bytes()); // TYPE ordinal
    buf.extend_from_slice(&[0xff, 0xff]);
    buf.extend_from_slice(&name_id.to_le_bytes()); // NAME ordinal
    buf.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    buf.extend_from_slice(&mem_flags.to_le_bytes()); // MemoryFlags
    buf.extend_from_slice(&0x0409u16.to_le_bytes()); // LanguageId (en-US)
    buf.extend_from_slice(&0u32.to_le_bytes()); // Version
    buf.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    buf.extend_from_slice(data);
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn write_win32_icon_res(res_path: &Path) {
    let mut res = Vec::new();
    // Standard 32-byte Win32 .res prologue entry
    res.extend_from_slice(&[
        0, 0, 0, 0, 32, 0, 0, 0, 0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
    ]);

    let sizes: &[u32] = &[64, 48, 32, 16];
    let mut grp = Vec::new();
    grp.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    grp.extend_from_slice(&1u16.to_le_bytes()); // idType (1 = icon)
    grp.extend_from_slice(&(sizes.len() as u16).to_le_bytes()); // idCount

    for (idx, &sz) in sizes.iter().enumerate() {
        let icon_id = (idx + 1) as u16;
        let rgba = generate_onyx_rgba(sz);
        let dib = encode_dib_icon(sz, &rgba);
        append_res_entry(&mut res, 3, icon_id, 0x1010, &dib); // RT_ICON = 3

        grp.push(if sz >= 256 { 0 } else { sz as u8 }); // bWidth
        grp.push(if sz >= 256 { 0 } else { sz as u8 }); // bHeight
        grp.push(0); // bColorCount
        grp.push(0); // bReserved
        grp.extend_from_slice(&1u16.to_le_bytes()); // wPlanes
        grp.extend_from_slice(&32u16.to_le_bytes()); // wBitCount
        grp.extend_from_slice(&(dib.len() as u32).to_le_bytes()); // dwBytesInRes
        grp.extend_from_slice(&icon_id.to_le_bytes()); // nID
    }

    append_res_entry(&mut res, 14, 1, 0x1030, &grp); // RT_GROUP_ICON = 14
    fs::write(res_path, res).expect("failed to write Windows .res icon resource");
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

    // Embed Win32 application icon into Windows executables (OnyxStudio.exe)
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let res_path = out_dir.join("onyx_studio.res");
        write_win32_icon_res(&res_path);
        println!("cargo:rustc-link-arg-bins={}", res_path.display());
    }
}
