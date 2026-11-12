//! WebView 커스텀 프로토콜(`app://localhost/api/*`, `app://localhost/vault-media`) JSON API 라우터.

use crate::platform::{self, SharedTrigger};
use crate::sync_controller::{self, App, SyncSnapshotDto};
use crate::vault::{self, NotePropertyDto, WorkspaceManager};
use anyhow::{bail, Context, Result};
use obsidian_sync_core::{AppConfig, BackendConfig, ProfileKind};
use percent_encoding::percent_decode_str;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct BackendState {
    pub sync: App,
    pub workspace: WorkspaceManager,
}

impl BackendState {
    pub fn new(trigger: SharedTrigger) -> Self {
        let (config_path, data_dir) = sync_controller::app_dirs();
        let sync = App::new(config_path, data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        Self { sync, workspace }
    }
}

#[derive(Serialize)]
struct ApiOk<T: Serialize> {
    ok: bool,
    data: T,
}

#[derive(Serialize)]
struct ApiErr {
    ok: bool,
    error: String,
}

fn ok_json<T: Serialize>(data: T) -> Result<Value> {
    Ok(serde_json::to_value(data)?)
}

fn parse_body<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    let trimmed_empty = body.iter().all(|b| b.is_ascii_whitespace());
    let slice = if trimmed_empty { b"{}" as &[u8] } else { body };
    serde_json::from_slice(slice).context("요청 JSON 형식이 올바르지 않습니다")
}

// ───────────────────────────── 요청 페이로드 정의 ─────────────────────────────

#[derive(Deserialize)]
struct PathOnlyReq {
    path: String,
}

#[derive(Deserialize)]
struct VaultOnlyReq {
    vault: String,
}

#[derive(Deserialize)]
struct VaultPollReq {
    #[serde(default)]
    vault: Option<String>,
}

#[derive(Deserialize)]
struct VaultPathReq {
    vault: String,
    path: String,
}

#[derive(Deserialize)]
struct SaveNoteReq {
    vault: String,
    path: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    properties: Vec<NotePropertyDto>,
    body: String,
}

#[derive(Deserialize)]
struct CreateNoteReq {
    vault: String,
    #[serde(default)]
    folder: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    properties: Vec<NotePropertyDto>,
}

#[derive(Deserialize)]
struct DailyNoteReq {
    vault: String,
    #[serde(default = "vault::default_daily_folder")]
    daily_folder: String,
}

#[derive(Deserialize)]
struct CreateFolderReq {
    vault: String,
    #[serde(default)]
    parent: String,
    name: String,
}

#[derive(Deserialize)]
struct RenameReq {
    vault: String,
    path: String,
    new_name: String,
}

#[derive(Deserialize)]
struct MoveReq {
    vault: String,
    path: String,
    #[serde(default)]
    target_folder: String,
}

#[derive(Deserialize)]
struct TrashRestoreReq {
    vault: String,
    trash_name: String,
}

#[derive(Deserialize)]
struct LinkMentionReq {
    vault: String,
    source_path: String,
    target_title: String,
}

#[derive(Deserialize)]
struct SearchReq {
    vault: String,
    #[serde(default)]
    query: String,
}

#[derive(Deserialize)]
struct GraphReq {
    vault: String,
    #[serde(default)]
    center_path: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    include_tags: Option<bool>,
}

#[derive(Deserialize)]
struct TemplatesReq {
    vault: String,
    #[serde(default = "vault::default_templates_folder")]
    templates_folder: String,
}

#[derive(Deserialize)]
struct UploadAttachmentReq {
    vault: String,
    filename: String,
    data_base64: String,
}

#[derive(Deserialize)]
struct SaveConfigReq {
    cfg: AppConfig,
    #[serde(default)]
    start_after_save: bool,
}

#[derive(Deserialize)]
struct AllowDeletesReq {
    #[serde(default)]
    profile_id: String,
}

#[derive(Deserialize)]
struct ResetProfileReq {
    profile_id: String,
}

#[derive(Deserialize)]
struct NewProfileReq {
    kind: ProfileKind,
    #[serde(default)]
    copy_synology_from: Option<BackendConfig>,
}

#[derive(Deserialize)]
struct BackendOnlyReq {
    backend: BackendConfig,
}

#[derive(Deserialize)]
struct SubmitOtpReq {
    backend: BackendConfig,
    otp: String,
}

#[derive(Deserialize)]
struct NasBrowseOpenReq {
    backend: BackendConfig,
    #[serde(default)]
    initial_folder: String,
}

#[derive(Deserialize)]
struct NasBrowseStepReq {
    backend: BackendConfig,
    path: String,
    #[serde(default)]
    create: Option<String>,
    #[serde(default)]
    otp: Option<String>,
}

#[derive(Deserialize)]
struct SetAutostartReq {
    enable: bool,
}

// ───────────────────────────── 엔드포인트 라우터 ─────────────────────────────

fn bump_revision(state: &Arc<Mutex<BackendState>>) -> u64 {
    state.lock().unwrap().workspace.watcher.bump()
}

fn current_revision(state: &Arc<Mutex<BackendState>>) -> u64 {
    state.lock().unwrap().workspace.watcher.revision()
}

fn dispatch(state: &Arc<Mutex<BackendState>>, endpoint: &str, body: &[u8]) -> Result<Value> {
    match endpoint {
        // ── 워크스페이스 & 볼트 관리 ──
        "/api/bootstrap" => {
            let (ws_state, rev, sync_snap) = {
                let mut st = state.lock().unwrap();
                let BackendState { sync, workspace } = &mut *st;
                workspace.state.sync_vaults_with_config(&sync.cfg);
                let _ = workspace.save_and_refresh_watcher();
                let rev = workspace.watcher.revision();
                let snap = sync.snapshot();
                (workspace.state.clone(), rev, snap)
            };
            let vault_scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault(p, rev).ok());

            Ok(json!({
                "workspace": ws_state,
                "vault_scan": vault_scan,
                "sync": sync_snap,
                "vault_revision": rev,
            }))
        }

        "/api/workspace/save" => {
            let raw: Value = parse_body(body)?;
            let patch = raw.get("workspace").unwrap_or(&raw);
            let mut st = state.lock().unwrap();
            st.workspace.state.merge_json(patch);
            st.workspace.save_and_refresh_watcher()?;
            ok_json(&st.workspace.state)
        }

        "/api/workspace/open_vault_dialog" => {
            let picked = platform::pick_folder();
            let (ws_state, rev) = {
                let mut st = state.lock().unwrap();
                if let Some(ref p) = picked {
                    st.workspace.state.add_or_select_vault(p);
                    st.workspace.save_and_refresh_watcher()?;
                }
                (st.workspace.state.clone(), st.workspace.watcher.revision())
            };
            let vault_scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault(p, rev).ok());

            Ok(json!({
                "picked": picked,
                "workspace": ws_state,
                "vault_scan": vault_scan,
            }))
        }

        "/api/workspace/create_sample_vault" => {
            let data_dir: PathBuf = state.lock().unwrap().workspace.data_dir.clone();
            let sample_path = vault::create_sample_vault(&data_dir)?;
            let sample_str = sample_path.to_string_lossy().to_string();
            let (ws_state, rev) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.add_or_select_vault(&sample_str);
                st.workspace.save_and_refresh_watcher()?;
                (st.workspace.state.clone(), st.workspace.watcher.revision())
            };
            let scan = vault::scan_vault(&sample_path, rev)?;
            Ok(json!({
                "path": sample_str,
                "workspace": ws_state,
                "vault_scan": scan,
            }))
        }

        "/api/workspace/select_vault" => {
            let req: PathOnlyReq = parse_body(body)?;
            let clean = req.path.trim().to_string();
            if clean.is_empty() || !Path::new(&clean).is_dir() {
                bail!("선택한 볼트 폴더가 존재하지 않습니다: {clean}");
            }
            let (ws_state, rev) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.add_or_select_vault(&clean);
                st.workspace.save_and_refresh_watcher()?;
                (st.workspace.state.clone(), st.workspace.watcher.revision())
            };
            let scan = vault::scan_vault(Path::new(&clean), rev).ok();
            Ok(json!({
                "workspace": ws_state,
                "vault_scan": scan,
            }))
        }

        "/api/workspace/remove_vault" => {
            let req: PathOnlyReq = parse_body(body)?;
            let (ws_state, rev) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.remove_vault(&req.path);
                st.workspace.save_and_refresh_watcher()?;
                (st.workspace.state.clone(), st.workspace.watcher.revision())
            };
            let scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault(p, rev).ok());
            Ok(json!({
                "workspace": ws_state,
                "vault_scan": scan,
            }))
        }

        "/api/vault/scan" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let rev = current_revision(state);
            let scan = vault::scan_vault(Path::new(req.vault.trim()), rev)?;
            ok_json(scan)
        }

        "/api/vault/poll" => {
            let req: VaultPollReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            if let Some(ref vp) = req.vault {
                if !vp.trim().is_empty()
                    && st.workspace.state.active_vault.as_deref() == Some(vp.trim())
                {
                    st.workspace.watcher.watch(Some(vp.trim()));
                }
            }
            let rev = st.workspace.watcher.revision();
            let sync: SyncSnapshotDto = st.sync.snapshot();
            Ok(json!({
                "vault_revision": rev,
                "sync": sync,
            }))
        }

        // ── 노트 및 파일 작업 ──
        "/api/note/read" => {
            let req: VaultPathReq = parse_body(body)?;
            let doc = vault::read_note(Path::new(req.vault.trim()), &req.path)?;
            ok_json(doc)
        }

        "/api/note/save" => {
            let req: SaveNoteReq = parse_body(body)?;
            let doc = vault::save_note(
                Path::new(req.vault.trim()),
                &req.path,
                req.icon.as_deref(),
                req.cover.as_deref(),
                &req.properties,
                &req.body,
            )?;
            bump_revision(state);
            ok_json(doc)
        }

        "/api/note/create" => {
            let req: CreateNoteReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let (path, doc) = vault::create_note(
                root,
                &req.folder,
                &req.title,
                req.body.as_deref(),
                req.icon.as_deref(),
                &req.properties,
            )?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "path": path,
                "doc": doc,
                "scan": scan,
            }))
        }

        "/api/note/duplicate" => {
            let req: VaultPathReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let path = vault::duplicate_note(root, &req.path)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "path": path,
                "scan": scan,
            }))
        }

        "/api/note/daily" => {
            let req: DailyNoteReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let (path, doc) = vault::open_daily_note(root, &req.daily_folder)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "path": path,
                "doc": doc,
                "scan": scan,
            }))
        }

        "/api/vault/create_folder" => {
            let req: CreateFolderReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let path = vault::create_folder(root, &req.parent, &req.name)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "path": path,
                "scan": scan,
            }))
        }

        "/api/vault/rename" => {
            let req: RenameReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let res = vault::rename_entry(root, &req.path, &req.new_name)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "new_path": res.new_path,
                "updated_links": res.updated_links,
                "scan": scan,
            }))
        }

        "/api/vault/move" => {
            let req: MoveReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let new_path = vault::move_entry(root, &req.path, &req.target_folder)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "new_path": new_path,
                "scan": scan,
            }))
        }

        "/api/vault/delete" => {
            let req: VaultPathReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            vault::delete_to_trash(root, &req.path)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({ "scan": scan }))
        }

        "/api/vault/trash_list" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let items = vault::list_trash(Path::new(req.vault.trim()))?;
            Ok(json!({ "items": items }))
        }

        "/api/vault/trash_restore" => {
            let req: TrashRestoreReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let restored_path = vault::restore_trash(root, &req.trash_name)?;
            let rev = bump_revision(state);
            let scan = vault::scan_vault(root, rev)?;
            Ok(json!({
                "restored_path": restored_path,
                "scan": scan,
            }))
        }

        "/api/vault/trash_empty" => {
            let req: VaultOnlyReq = parse_body(body)?;
            vault::empty_trash(Path::new(req.vault.trim()))?;
            Ok(json!({}))
        }

        "/api/vault/link_mention" => {
            let req: LinkMentionReq = parse_body(body)?;
            vault::link_unlinked_mention(
                Path::new(req.vault.trim()),
                &req.source_path,
                &req.target_title,
            )?;
            bump_revision(state);
            Ok(json!({}))
        }

        "/api/vault/search" => {
            let req: SearchReq = parse_body(body)?;
            let hits = vault::search_vault(Path::new(req.vault.trim()), &req.query)?;
            Ok(json!({ "hits": hits }))
        }

        "/api/vault/graph" => {
            let req: GraphReq = parse_body(body)?;
            let graph = vault::build_graph(
                Path::new(req.vault.trim()),
                req.center_path.as_deref(),
                req.depth,
                req.include_tags.unwrap_or(false),
            )?;
            ok_json(graph)
        }

        "/api/vault/templates" => {
            let req: TemplatesReq = parse_body(body)?;
            let templates =
                vault::list_templates(Path::new(req.vault.trim()), &req.templates_folder)?;
            Ok(json!({ "templates": templates }))
        }

        "/api/vault/upload_attachment" => {
            let req: UploadAttachmentReq = parse_body(body)?;
            let res = vault::upload_attachment(
                Path::new(req.vault.trim()),
                &req.filename,
                &req.data_base64,
            )?;
            bump_revision(state);
            ok_json(res)
        }

        "/api/vault/pick_attachment" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let res = vault::pick_attachment(Path::new(req.vault.trim()))?;
            if res.picked {
                bump_revision(state);
            }
            ok_json(res)
        }

        // ── 동기화 서비스 & 원격 저장소 제어 ──
        "/api/sync/state" => {
            let mut st = state.lock().unwrap();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/save_config" => {
            let req: SaveConfigReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            let saved_ok = st.sync.save_config(req.cfg, req.start_after_save);
            if saved_ok {
                let cfg_clone = st.sync.cfg.clone();
                st.workspace.state.sync_vaults_with_config(&cfg_clone);
                let _ = st.workspace.save_and_refresh_watcher();
            }
            ok_json(st.sync.snapshot())
        }

        "/api/sync/start" => {
            let mut st = state.lock().unwrap();
            st.sync.start_service();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/stop" => {
            let mut st = state.lock().unwrap();
            st.sync.stop_service();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/sync_now" => {
            let mut st = state.lock().unwrap();
            st.sync.sync_now();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/allow_deletes" => {
            let req: AllowDeletesReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.allow_deletes(&req.profile_id);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/reset_profile" => {
            let req: ResetProfileReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.reset_profile(req.profile_id);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/new_profile" => {
            let req: NewProfileReq = parse_body(body)?;
            let st = state.lock().unwrap();
            let profile = st.sync.new_profile(req.kind, req.copy_synology_from);
            Ok(json!({ "profile": profile }))
        }

        "/api/sync/test_connection" => {
            let req: BackendOnlyReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.test_connection(req.backend);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/submit_otp" => {
            let req: SubmitOtpReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.submit_otp(req.backend, req.otp);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/forget_device" => {
            let req: BackendOnlyReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.forget_device(&req.backend);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/nas_browse_open" => {
            let req: NasBrowseOpenReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.nas_browse_open(req.backend, req.initial_folder);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/nas_browse_step" => {
            let req: NasBrowseStepReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync
                .nas_browse_step(req.backend, req.path, req.create, req.otp);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/nas_browse_close" => {
            let mut st = state.lock().unwrap();
            st.sync.nas_browse_close();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/oauth_start" => {
            let req: BackendOnlyReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.oauth_start(req.backend);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/oauth_cancel" => {
            let mut st = state.lock().unwrap();
            st.sync.oauth_cancel();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/oauth_logout" => {
            let req: BackendOnlyReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.oauth_logout(&req.backend);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/set_autostart" => {
            let req: SetAutostartReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            st.sync.set_autostart(req.enable);
            ok_json(st.sync.snapshot())
        }

        "/api/sync/pick_folder" => {
            let path = platform::pick_folder();
            Ok(json!({ "path": path }))
        }

        "/api/sync/open_path" => {
            let req: PathOnlyReq = parse_body(body)?;
            let p = req.path.trim();
            if p.is_empty() {
                bail!("열 경로가 비어 있습니다");
            }
            open::that(p).with_context(|| format!("폴더를 열지 못했습니다: {p}"))?;
            Ok(json!({}))
        }

        "/api/sync/open_data_dir" => {
            let dir = state.lock().unwrap().sync.data_dir.clone();
            let _ = std::fs::create_dir_all(&dir);
            open::that(&dir).context("설정 폴더를 열지 못했습니다")?;
            Ok(json!({}))
        }

        "/api/sync/clear_message" => {
            let mut st = state.lock().unwrap();
            st.sync.clear_message();
            ok_json(st.sync.snapshot())
        }

        other => bail!("알 수 없는 API 경로입니다: {other}"),
    }
}

pub fn handle_api(state: &Arc<Mutex<BackendState>>, endpoint: &str, body: &[u8]) -> Vec<u8> {
    match dispatch(state, endpoint, body) {
        Ok(data) => serde_json::to_vec(&ApiOk { ok: true, data }).unwrap_or_else(|e| {
            serde_json::to_vec(&ApiErr {
                ok: false,
                error: format!("{e:#}"),
            })
            .unwrap_or_default()
        }),
        Err(e) => serde_json::to_vec(&ApiErr {
            ok: false,
            error: format!("{e:#}"),
        })
        .unwrap_or_default(),
    }
}

fn media_mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

pub fn handle_vault_media(query: Option<&str>) -> Option<(&'static str, Vec<u8>)> {
    let q = query?;
    let mut vault_param: Option<String> = None;
    let mut path_param: Option<String> = None;

    for pair in q.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let decoded = percent_decode_str(v).decode_utf8_lossy().to_string();
        match k {
            "vault" => vault_param = Some(decoded),
            "path" => path_param = Some(decoded),
            _ => {}
        }
    }

    let vault_root = PathBuf::from(vault_param?.trim());
    let rel = path_param?;
    let resolved = vault::resolve_vault_media_file(&vault_root, &rel)?;
    let bytes = std::fs::read(&resolved).ok()?;
    let mime = media_mime_for(&resolved);
    Some((mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obsidian_sync_core::Profile;

    #[test]
    fn api_router_and_vault_media_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let vault_dir = tmp.path().join("vault");
        std::fs::create_dir_all(&vault_dir).unwrap();

        let trigger: SharedTrigger = Arc::new(Mutex::new(None));
        let sync = App::new(data_dir.join("config.json"), data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        let state = Arc::new(Mutex::new(BackendState { sync, workspace }));

        // 1) save_config 시 Obsidian 프로필이 자동으로 볼트 목록에 동기화되는지 검증
        let mut cfg = AppConfig::default();
        cfg.profiles.push(Profile {
            name: "내 노트".into(),
            kind: ProfileKind::Obsidian,
            local_path: vault_dir.to_string_lossy().into(),
            backend: BackendConfig::LocalFolder {
                path: tmp.path().join("remote").to_string_lossy().into(),
            },
            ..Default::default()
        });
        let save_req = serde_json::to_vec(&json!({
            "cfg": cfg,
            "start_after_save": false
        }))
        .unwrap();
        let resp_bytes = handle_api(&state, "/api/sync/save_config", &save_req);
        let resp: Value = serde_json::from_slice(&resp_bytes).unwrap();
        assert_eq!(resp["ok"], true);

        // 2) bootstrap 호출로 자동 등록된 볼트 확인
        let boot_bytes = handle_api(&state, "/api/bootstrap", b"");
        let boot: Value = serde_json::from_slice(&boot_bytes).unwrap();
        assert_eq!(boot["ok"], true);
        assert_eq!(
            boot["data"]["workspace"]["vaults"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        // 3) 노트 생성 및 첨부파일 업로드 + /vault-media 조회 검증
        let create_req = serde_json::to_vec(&json!({
            "vault": vault_dir.to_string_lossy(),
            "title": "첫 노트",
            "body": "안녕하세요!"
        }))
        .unwrap();
        let created: Value =
            serde_json::from_slice(&handle_api(&state, "/api/note/create", &create_req)).unwrap();
        assert_eq!(created["ok"], true);
        assert_eq!(created["data"]["path"], "첫 노트.md");

        // 1x1 투명 PNG base64
        let png_b64 = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        let up_req = serde_json::to_vec(&json!({
            "vault": vault_dir.to_string_lossy(),
            "filename": "pixel.png",
            "data_base64": png_b64
        }))
        .unwrap();
        let uploaded: Value =
            serde_json::from_slice(&handle_api(&state, "/api/vault/upload_attachment", &up_req))
                .unwrap();
        assert_eq!(uploaded["ok"], true);
        let media_url = uploaded["data"]["media_url"].as_str().unwrap();
        let query = media_url.split_once('?').unwrap().1;
        let (mime, bytes) = handle_vault_media(Some(query)).unwrap();
        assert_eq!(mime, "image/png");
        assert!(!bytes.is_empty());
    }
}
