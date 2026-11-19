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
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let root_ico = manifest_dir.join("../../icon.ico");
        let local_ico = manifest_dir.join("icons/icon.ico");
        let chosen_ico = if local_ico.is_file() {
            Some(local_ico)
        } else if root_ico.is_file() {
            Some(root_ico)
        } else {
            None
        };

        let mut embedded_custom_icon = false;
        if let (Some(ico_path), Ok("gnu")) =
            (&chosen_ico, env::var("CARGO_CFG_TARGET_ENV").as_deref())
        {
            println!("cargo:rerun-if-changed={}", ico_path.display());
            let rc_path = out_dir.join("onyx_studio_custom.rc");
            let obj_path = out_dir.join("onyx_studio_custom_icon.o");
            let escaped = ico_path.to_string_lossy().replace('\\', "/");
            if fs::write(&rc_path, format!("1 ICON \"{}\"\n", escaped)).is_ok() {
                let status = std::process::Command::new("windres")
                    .args(["-O", "coff", "-i"])
                    .arg(&rc_path)
                    .arg("-o")
                    .arg(&obj_path)
                    .status();
                if matches!(status, Ok(s) if s.success()) && obj_path.is_file() {
                    println!("cargo:rustc-link-arg-bins={}", obj_path.display());
                    embedded_custom_icon = true;
                }
            }
        }

        if !embedded_custom_icon {
            let res_path = out_dir.join("onyx_studio.res");
            write_win32_icon_res(&res_path);
            match env::var("CARGO_CFG_TARGET_ENV").as_deref() {
                Ok("msvc") => {
                    println!("cargo:rustc-link-arg-bins={}", res_path.display());
                }
                Ok("gnu") => {
                    let obj_path = out_dir.join("onyx_studio_icon.o");
                    let status = std::process::Command::new("windres")
                        .args(["-J", "res", "-O", "coff", "-i"])
                        .arg(&res_path)
                        .arg("-o")
                        .arg(&obj_path)
                        .status();
                    if matches!(status, Ok(s) if s.success()) && obj_path.is_file() {
                        println!("cargo:rustc-link-arg-bins={}", obj_path.display());
                    }
                }
                _ => {}
            }
        }

        // On GNU toolchain, `webview2-com-sys` dynamically links `WebView2Loader.dll`.
        // Wrap the 5 loader imports with `--wrap=` + embedded official WebView2Loader.dll bytes
        // so OnyxStudio.exe works as a single standalone EXE without an external DLL beside it.
        if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")
            && env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64")
        {
            if let Some(dll_path) = find_webview2_loader_dll(&out_dir) {
                if let Some((blob_obj, shim_obj)) = compile_webview2_shim(&out_dir, &dll_path) {
                    for sym in [
                        "CompareBrowserVersions",
                        "CreateCoreWebView2Environment",
                        "CreateCoreWebView2EnvironmentWithOptions",
                        "GetAvailableCoreWebView2BrowserVersionString",
                        "GetAvailableCoreWebView2BrowserVersionStringWithOptions",
                    ] {
                        println!("cargo:rustc-link-arg-bins=-Wl,--wrap={sym}");
                    }
                    println!("cargo:rustc-link-arg-bins={}", blob_obj.display());
                    println!("cargo:rustc-link-arg-bins={}", shim_obj.display());
                    println!("cargo:rustc-link-arg-bins=-lkernel32");
                }
            }
        }
    }
}

fn find_webview2_loader_dll(out_dir: &Path) -> Option<PathBuf> {
    if let Some(build_root) = out_dir.parent().and_then(|p| p.parent()) {
        if let Ok(entries) = fs::read_dir(build_root) {
            for ent in entries.flatten() {
                let name = ent.file_name().to_string_lossy().to_string();
                if name.starts_with("webview2-com-sys") {
                    let cand = ent.path().join("out/x64/WebView2Loader.dll");
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
        }
    }
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join(".cargo")))?;
    let reg_src = cargo_home.join("registry/src");
    let indexes = fs::read_dir(reg_src).ok()?;
    for idx in indexes.flatten() {
        let Ok(crates) = fs::read_dir(idx.path()) else {
            continue;
        };
        let mut matches: Vec<PathBuf> = crates
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("webview2-com-sys-")
            })
            .map(|e| e.path().join("x64/WebView2Loader.dll"))
            .filter(|p| p.is_file())
            .collect();
        matches.sort();
        if let Some(last) = matches.pop() {
            return Some(last);
        }
    }
    None
}

fn compile_webview2_shim(out_dir: &Path, dll_path: &Path) -> Option<(PathBuf, PathBuf)> {
    let dll_bytes = fs::read(dll_path).ok()?;
    let dll_len = dll_bytes.len();
    let escaped = dll_path.to_string_lossy().replace('\\', "/");

    let asm_path = out_dir.join("webview2_blob.S");
    let asm_src = format!(
        ".section .rdata,\"dr\"\n\
         .globl WEBVIEW2_LOADER_DLL\n\
         WEBVIEW2_LOADER_DLL:\n\
         \t.incbin \"{escaped}\"\n\
         .globl WEBVIEW2_LOADER_DLL_END\n\
         WEBVIEW2_LOADER_DLL_END:\n"
    );
    fs::write(&asm_path, asm_src).ok()?;

    let c_path = out_dir.join("webview2_shim.c");
    let c_src = format!(
        r#"#define WIN32_LEAN_AND_MEAN
#include <windows.h>

extern const unsigned char WEBVIEW2_LOADER_DLL[];
static const DWORD WEBVIEW2_LOADER_LEN = {dll_len}u;

static CRITICAL_SECTION g_wv_cs;
static LONG g_wv_cs_state = 0;
static HMODULE g_wv_mod = NULL;

static void w_append(wchar_t *dst, DWORD cap, const wchar_t *src) {{
    DWORD i = 0;
    while (i < cap && dst[i]) i++;
    DWORD j = 0;
    while (i + 1 < cap && src[j]) {{
        dst[i++] = src[j++];
    }}
    if (i < cap) dst[i] = 0;
}}

static void u32_to_w(DWORD v, wchar_t *buf) {{
    wchar_t tmp[16];
    int n = 0;
    do {{
        tmp[n++] = (wchar_t)(L'0' + (v % 10));
        v /= 10;
    }} while (v > 0 && n < 15);
    int k = 0;
    while (n > 0) buf[k++] = tmp[--n];
    buf[k] = 0;
}}

static HMODULE ensure_wv(void) {{
    if (g_wv_mod) return g_wv_mod;
    if (InterlockedCompareExchange(&g_wv_cs_state, 1, 0) == 0) {{
        InitializeCriticalSection(&g_wv_cs);
        InterlockedExchange(&g_wv_cs_state, 2);
    }} else {{
        while (InterlockedCompareExchange(&g_wv_cs_state, 2, 2) != 2) Sleep(1);
    }}
    EnterCriticalSection(&g_wv_cs);
    if (!g_wv_mod) {{
        wchar_t exeDir[MAX_PATH];
        DWORD n = GetModuleFileNameW(NULL, exeDir, MAX_PATH);
        if (n > 0 && n < MAX_PATH) {{
            wchar_t *slash = NULL;
            for (DWORD i = 0; exeDir[i]; ++i) {{
                if (exeDir[i] == L'\\' || exeDir[i] == L'/') slash = &exeDir[i];
            }}
            if (slash) {{
                slash[1] = 0;
                w_append(exeDir, MAX_PATH, L"WebView2Loader.dll");
                g_wv_mod = LoadLibraryW(exeDir);
            }}
        }}
        if (!g_wv_mod) {{
            wchar_t baseDir[MAX_PATH];
            baseDir[0] = 0;
            DWORD len = GetEnvironmentVariableW(L"LOCALAPPDATA", baseDir, MAX_PATH - 64);
            if (len > 0 && len < MAX_PATH - 64) {{
                w_append(baseDir, MAX_PATH, L"\\OnyxStudio");
                CreateDirectoryW(baseDir, NULL);
                w_append(baseDir, MAX_PATH, L"\\");
            }} else {{
                GetTempPathW(MAX_PATH - 64, baseDir);
            }}
            wchar_t dllOut[MAX_PATH];
            dllOut[0] = 0;
            w_append(dllOut, MAX_PATH, baseDir);
            w_append(dllOut, MAX_PATH, L"WebView2Loader-0.39.1.dll");
            WIN32_FILE_ATTRIBUTE_DATA fad;
            BOOL need = !GetFileAttributesExW(dllOut, GetFileExInfoStandard, &fad)
                        || fad.nFileSizeHigh != 0
                        || fad.nFileSizeLow != WEBVIEW2_LOADER_LEN;
            if (need) {{
                wchar_t pidBuf[16];
                u32_to_w(GetCurrentProcessId(), pidBuf);
                wchar_t tmpOut[MAX_PATH];
                tmpOut[0] = 0;
                w_append(tmpOut, MAX_PATH, dllOut);
                w_append(tmpOut, MAX_PATH, L".tmp.");
                w_append(tmpOut, MAX_PATH, pidBuf);
                HANDLE h = CreateFileW(tmpOut, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
                if (h != INVALID_HANDLE_VALUE) {{
                    DWORD written = 0;
                    WriteFile(h, WEBVIEW2_LOADER_DLL, WEBVIEW2_LOADER_LEN, &written, NULL);
                    CloseHandle(h);
                    if (!MoveFileExW(tmpOut, dllOut, MOVEFILE_REPLACE_EXISTING)) {{
                        DeleteFileW(tmpOut);
                    }}
                }}
            }}
            g_wv_mod = LoadLibraryW(dllOut);
        }}
    }}
    LeaveCriticalSection(&g_wv_cs);
    return g_wv_mod;
}}

static FARPROC resolve_wv(const char *name) {{
    HMODULE m = ensure_wv();
    return m ? GetProcAddress(m, name) : NULL;
}}

HRESULT __stdcall __wrap_CompareBrowserVersions(LPCWSTR v1, LPCWSTR v2, int *res) {{
    typedef HRESULT (__stdcall *Fn)(LPCWSTR, LPCWSTR, int *);
    Fn f = (Fn)resolve_wv("CompareBrowserVersions");
    return f ? f(v1, v2, res) : E_FAIL;
}}

HRESULT __stdcall __wrap_CreateCoreWebView2Environment(void *handler) {{
    typedef HRESULT (__stdcall *Fn)(void *);
    Fn f = (Fn)resolve_wv("CreateCoreWebView2Environment");
    return f ? f(handler) : E_FAIL;
}}

HRESULT __stdcall __wrap_CreateCoreWebView2EnvironmentWithOptions(LPCWSTR browser, LPCWSTR userData, void *opts, void *handler) {{
    typedef HRESULT (__stdcall *Fn)(LPCWSTR, LPCWSTR, void *, void *);
    Fn f = (Fn)resolve_wv("CreateCoreWebView2EnvironmentWithOptions");
    return f ? f(browser, userData, opts, handler) : E_FAIL;
}}

HRESULT __stdcall __wrap_GetAvailableCoreWebView2BrowserVersionString(LPCWSTR browser, LPWSTR *ver) {{
    typedef HRESULT (__stdcall *Fn)(LPCWSTR, LPWSTR *);
    Fn f = (Fn)resolve_wv("GetAvailableCoreWebView2BrowserVersionString");
    return f ? f(browser, ver) : E_FAIL;
}}

HRESULT __stdcall __wrap_GetAvailableCoreWebView2BrowserVersionStringWithOptions(LPCWSTR browser, void *opts, LPWSTR *ver) {{
    typedef HRESULT (__stdcall *Fn)(LPCWSTR, void *, LPWSTR *);
    Fn f = (Fn)resolve_wv("GetAvailableCoreWebView2BrowserVersionStringWithOptions");
    return f ? f(browser, opts, ver) : E_FAIL;
}}
"#
    );
    fs::write(&c_path, c_src).ok()?;

    let blob_obj = out_dir.join("webview2_blob.o");
    let shim_obj = out_dir.join("webview2_shim.o");
    let st1 = std::process::Command::new("gcc")
        .args(["-c"])
        .arg(&asm_path)
        .arg("-o")
        .arg(&blob_obj)
        .status()
        .ok()?;
    if !st1.success() || !blob_obj.is_file() {
        return None;
    }
    let st2 = std::process::Command::new("gcc")
        .args(["-O2", "-c"])
        .arg(&c_path)
        .arg("-o")
        .arg(&shim_obj)
        .status()
        .ok()?;
    if !st2.success() || !shim_obj.is_file() {
        return None;
    }
    Some((blob_obj, shim_obj))
}
