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

#[cfg(windows)]
pub type SharedEventProxy = Option<tao::event_loop::EventLoopProxy<crate::AppUserEvent>>;
#[cfg(not(windows))]
pub type SharedEventProxy = Option<()>;

pub struct BackendState {
    pub sync: App,
    pub workspace: WorkspaceManager,
    pub crypto: crate::crypto::VaultCryptoSession,
    pub event_proxy: SharedEventProxy,
}

impl BackendState {
    pub fn new(trigger: SharedTrigger) -> Self {
        Self::new_with_proxy(trigger, None)
    }

    pub fn new_with_proxy(trigger: SharedTrigger, event_proxy: SharedEventProxy) -> Self {
        let (config_path, data_dir) = sync_controller::app_dirs();
        let sync = App::new(config_path, data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        Self {
            sync,
            workspace,
            crypto: crate::crypto::VaultCryptoSession::new(),
            event_proxy,
        }
    }

    pub fn new_at_dir(
        data_dir: PathBuf,
        initial_cfg_json: Option<&str>,
        trigger: SharedTrigger,
    ) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        let config_path = data_dir.join("config.json");
        if !config_path.exists() {
            if let Some(raw) = initial_cfg_json {
                let trimmed = raw.trim();
                if !trimmed.is_empty() {
                    if let Ok(cfg) = AppConfig::from_json(trimmed) {
                        let _ = cfg.save(&config_path);
                    }
                }
            }
        }
        let sync = App::new(config_path, data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        Self {
            sync,
            workspace,
            crypto: crate::crypto::VaultCryptoSession::new(),
            event_proxy: None,
        }
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
    #[serde(default)]
    daily_folder: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct ExportPdfReq {
    #[serde(default)]
    default_filename: Option<String>,
    #[serde(default)]
    landscape: Option<bool>,
    #[serde(default)]
    margin_top_cm: Option<f64>,
    #[serde(default)]
    margin_bottom_cm: Option<f64>,
    #[serde(default)]
    margin_left_cm: Option<f64>,
    #[serde(default)]
    margin_right_cm: Option<f64>,
}

#[derive(Deserialize)]
struct CryptoNotePasswordReq {
    vault: String,
    path: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Deserialize)]
struct CryptoVaultReq {
    vault: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    hint: Option<String>,
    #[serde(default)]
    auto_encrypt: Option<bool>,
}

#[derive(Deserialize)]
struct CryptoAutoEncryptReq {
    vault: String,
    enabled: bool,
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
struct DisconnectProfileReq {
    profile_id: String,
    #[serde(default)]
    remove_vault: bool,
}

#[derive(Deserialize)]
struct PrepareFolderTargetReq {
    base_folder: String,
    #[serde(default)]
    subfolder: String,
}

#[derive(Deserialize)]
struct S3ListBucketsReq {
    endpoint: String,
    region: String,
    access_key: String,
    secret_key: String,
    #[serde(default)]
    create_bucket: Option<String>,
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

fn current_revision_and_crypto(
    state: &Arc<Mutex<BackendState>>,
) -> (u64, crate::crypto::VaultCryptoSession) {
    let st = state.lock().unwrap();
    (st.workspace.watcher.revision(), st.crypto.clone())
}

fn bump_revision_and_crypto(
    state: &Arc<Mutex<BackendState>>,
) -> (u64, crate::crypto::VaultCryptoSession) {
    let st = state.lock().unwrap();
    (st.workspace.watcher.bump(), st.crypto.clone())
}

fn crypto_snapshot(state: &Arc<Mutex<BackendState>>) -> crate::crypto::VaultCryptoSession {
    state.lock().unwrap().crypto.clone()
}

fn dispatch(state: &Arc<Mutex<BackendState>>, endpoint: &str, body: &[u8]) -> Result<Value> {
    match endpoint {
        // ── 워크스페이스 & 볼트 관리 ──
        "/api/bootstrap" => {
            let (ws_state, rev, sync_snap, crypto_snap) = {
                let mut st = state.lock().unwrap();
                let BackendState {
                    sync,
                    workspace,
                    crypto,
                    ..
                } = &mut *st;
                workspace.state.sync_vaults_with_config(&sync.cfg);
                let _ = workspace.save_and_refresh_watcher();
                let rev = workspace.watcher.revision();
                let snap = sync.snapshot();
                (workspace.state.clone(), rev, snap, crypto.clone())
            };
            let vault_scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault_with_session(p, rev, Some(&crypto_snap)).ok());

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
            let (ws_state, rev, crypto_snap) = {
                let mut st = state.lock().unwrap();
                if let Some(ref p) = picked {
                    st.workspace.state.add_or_select_vault(p);
                    st.workspace.save_and_refresh_watcher()?;
                }
                (
                    st.workspace.state.clone(),
                    st.workspace.watcher.revision(),
                    st.crypto.clone(),
                )
            };
            let vault_scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault_with_session(p, rev, Some(&crypto_snap)).ok());

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
            let (ws_state, rev, crypto_snap) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.add_or_select_vault(&sample_str);
                st.workspace.save_and_refresh_watcher()?;
                (
                    st.workspace.state.clone(),
                    st.workspace.watcher.revision(),
                    st.crypto.clone(),
                )
            };
            let scan = vault::scan_vault_with_session(&sample_path, rev, Some(&crypto_snap))?;
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
            let (ws_state, rev, crypto_snap) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.add_or_select_vault(&clean);
                st.workspace.save_and_refresh_watcher()?;
                (
                    st.workspace.state.clone(),
                    st.workspace.watcher.revision(),
                    st.crypto.clone(),
                )
            };
            let scan =
                vault::scan_vault_with_session(Path::new(&clean), rev, Some(&crypto_snap)).ok();
            Ok(json!({
                "workspace": ws_state,
                "vault_scan": scan,
            }))
        }

        "/api/workspace/remove_vault" => {
            let req: PathOnlyReq = parse_body(body)?;
            let (ws_state, rev, crypto_snap) = {
                let mut st = state.lock().unwrap();
                st.workspace.state.remove_vault(&req.path);
                st.workspace.save_and_refresh_watcher()?;
                (
                    st.workspace.state.clone(),
                    st.workspace.watcher.revision(),
                    st.crypto.clone(),
                )
            };
            let scan = ws_state
                .active_vault
                .as_deref()
                .map(Path::new)
                .filter(|p| p.is_dir())
                .and_then(|p| vault::scan_vault_with_session(p, rev, Some(&crypto_snap)).ok());
            Ok(json!({
                "workspace": ws_state,
                "vault_scan": scan,
            }))
        }

        "/api/vault/scan" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let (rev, crypto_snap) = current_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(
                Path::new(req.vault.trim()),
                rev,
                Some(&crypto_snap),
            )?;
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
            let crypto_snap = crypto_snapshot(state);
            let doc = vault::read_note_with_session(
                Path::new(req.vault.trim()),
                &req.path,
                Some(&crypto_snap),
            )?;
            ok_json(doc)
        }

        "/api/note/save" => {
            let req: SaveNoteReq = parse_body(body)?;
            let crypto_snap = crypto_snapshot(state);
            let doc = vault::save_note_with_session(
                Path::new(req.vault.trim()),
                &req.path,
                req.icon.as_deref(),
                req.cover.as_deref(),
                &req.properties,
                &req.body,
                Some(&crypto_snap),
            )?;
            bump_revision(state);
            ok_json(doc)
        }

        "/api/note/create" => {
            let req: CreateNoteReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let crypto_snap = crypto_snapshot(state);
            let (path, doc) = vault::create_note_with_session(
                root,
                &req.folder,
                &req.title,
                req.body.as_deref(),
                req.icon.as_deref(),
                &req.properties,
                Some(&crypto_snap),
            )?;
            let (rev, crypto_after) = bump_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(root, rev, Some(&crypto_after))?;
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
            let (rev, crypto_after) = bump_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(root, rev, Some(&crypto_after))?;
            Ok(json!({
                "path": path,
                "scan": scan,
            }))
        }

        "/api/note/daily" => {
            let req: DailyNoteReq = parse_body(body)?;
            let vault_str = req.vault.trim();
            let root = Path::new(vault_str);
            let (daily_folder, crypto_snap) = {
                let st = state.lock().unwrap();
                let df = match req.daily_folder {
                    Some(f) => f,
                    None => st.workspace.state.resolve_daily_folder(vault_str),
                };
                (df, st.crypto.clone())
            };
            let (path, doc) =
                vault::open_daily_note_with_session(root, &daily_folder, Some(&crypto_snap))?;
            let (rev, crypto_after) = bump_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(root, rev, Some(&crypto_after))?;
            Ok(json!({
                "path": path,
                "doc": doc,
                "scan": scan,
            }))
        }

        "/api/note/export_pdf" => {
            let req: ExportPdfReq = parse_body(body)?;
            let (ws_landscape, ws_top, ws_bottom, ws_left, ws_right, proxy) = {
                let st = state.lock().unwrap();
                (
                    st.workspace.state.page_orientation == "landscape",
                    st.workspace.state.margin_top_cm,
                    st.workspace.state.margin_bottom_cm,
                    st.workspace.state.margin_left_cm,
                    st.workspace.state.margin_right_cm,
                    st.event_proxy.clone(),
                )
            };
            let default_name = req
                .default_filename
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("document.pdf");
            let Some(output_path) = platform::save_pdf_dialog(default_name) else {
                return Ok(json!({ "exported": false, "path": Value::Null }));
            };

            #[cfg(windows)]
            {
                let Some(proxy) = proxy else {
                    bail!("PDF 내보내기 이벤트 채널이 초기화되지 않았습니다.");
                };
                let (tx, rx) = std::sync::mpsc::channel();
                proxy
                    .send_event(crate::AppUserEvent::ExportPdf {
                        output_path: output_path.clone(),
                        landscape: req.landscape.unwrap_or(ws_landscape),
                        margin_top_cm: req.margin_top_cm.unwrap_or(ws_top).clamp(0.0, 6.0),
                        margin_bottom_cm: req.margin_bottom_cm.unwrap_or(ws_bottom).clamp(0.0, 6.0),
                        margin_left_cm: req.margin_left_cm.unwrap_or(ws_left).clamp(0.0, 6.0),
                        margin_right_cm: req.margin_right_cm.unwrap_or(ws_right).clamp(0.0, 6.0),
                        reply: tx,
                    })
                    .map_err(|_| anyhow::anyhow!("PDF 내보내기 요청을 전달할 수 없습니다."))?;

                let res = rx
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .context("PDF 저장 응답 시간이 초과되었습니다.")?;
                if let Err(msg) = res {
                    bail!("{msg}");
                }
                Ok(json!({
                    "exported": true,
                    "path": output_path.to_string_lossy(),
                }))
            }
            #[cfg(not(windows))]
            {
                let _ = (
                    ws_landscape,
                    ws_top,
                    ws_bottom,
                    ws_left,
                    ws_right,
                    proxy,
                    output_path,
                );
                bail!("PDF 직접 내보내기는 Windows WebView2 환경에서 지원됩니다. 인쇄 창에서 PDF로 저장해 주세요.");
            }
        }

        "/api/vault/create_folder" => {
            let req: CreateFolderReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let path = vault::create_folder(root, &req.parent, &req.name)?;
            let (rev, crypto_after) = bump_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(root, rev, Some(&crypto_after))?;
            Ok(json!({
                "path": path,
                "scan": scan,
            }))
        }

        "/api/vault/rename" => {
            let req: RenameReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let res = vault::rename_entry_with_session(
                root,
                &req.path,
                &req.new_name,
                Some(&mut st.crypto),
            )?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({
                "new_path": res.new_path,
                "updated_links": res.updated_links,
                "scan": scan,
            }))
        }

        "/api/vault/move" => {
            let req: MoveReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let new_path = vault::move_entry_with_session(
                root,
                &req.path,
                &req.target_folder,
                Some(&mut st.crypto),
            )?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({
                "new_path": new_path,
                "scan": scan,
            }))
        }

        "/api/vault/delete" => {
            let req: VaultPathReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            vault::delete_to_trash_with_session(root, &req.path, Some(&mut st.crypto))?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
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
            let (rev, crypto_after) = bump_revision_and_crypto(state);
            let scan = vault::scan_vault_with_session(root, rev, Some(&crypto_after))?;
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
            let crypto_snap = crypto_snapshot(state);
            vault::link_unlinked_mention_with_session(
                Path::new(req.vault.trim()),
                &req.source_path,
                &req.target_title,
                Some(&crypto_snap),
            )?;
            bump_revision(state);
            Ok(json!({}))
        }

        "/api/vault/search" => {
            let req: SearchReq = parse_body(body)?;
            let crypto_snap = crypto_snapshot(state);
            let hits = vault::search_vault_with_session(
                Path::new(req.vault.trim()),
                &req.query,
                Some(&crypto_snap),
            )?;
            Ok(json!({ "hits": hits }))
        }

        "/api/vault/graph" => {
            let req: GraphReq = parse_body(body)?;
            let crypto_snap = crypto_snapshot(state);
            let graph = vault::build_graph_with_session(
                Path::new(req.vault.trim()),
                req.center_path.as_deref(),
                req.depth,
                req.include_tags.unwrap_or(false),
                Some(&crypto_snap),
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
            let root = Path::new(req.vault.trim());
            if !root.is_dir() {
                bail!("볼트 폴더가 존재하지 않습니다");
            }
            let picked = platform::pick_image_file();
            let res = vault::save_picked_image_attachment(root, picked)?;
            if res.picked {
                bump_revision(state);
            }
            ok_json(res)
        }

        "/api/vault/pick_file_attachment" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            if !root.is_dir() {
                bail!("볼트 폴더가 존재하지 않습니다");
            }
            let picked = platform::pick_any_file();
            let res = vault::save_picked_file_attachment(root, picked)?;
            if res.picked {
                bump_revision(state);
            }
            ok_json(res)
        }

        // ── 노트/볼트 암호화 및 잠금 관리 ──
        "/api/crypto/encrypt_note" => {
            let req: CryptoNotePasswordReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let doc = vault::encrypt_single_note(
                root,
                &req.path,
                &req.password,
                req.hint.as_deref(),
                &mut st.crypto,
            )?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "doc": doc, "scan": scan }))
        }

        "/api/crypto/decrypt_note" => {
            let req: CryptoNotePasswordReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let doc = vault::decrypt_single_note(root, &req.path, &req.password, &mut st.crypto)?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "doc": doc, "scan": scan }))
        }

        "/api/crypto/unlock_note" => {
            let req: CryptoNotePasswordReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let doc = vault::unlock_single_note(root, &req.path, &req.password, &mut st.crypto)?;
            let rev = st.workspace.watcher.revision();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "doc": doc, "scan": scan }))
        }

        "/api/crypto/lock_note" => {
            let req: VaultPathReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let doc = vault::lock_single_note(root, &req.path, &mut st.crypto)?;
            let rev = st.workspace.watcher.revision();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "doc": doc, "scan": scan }))
        }

        "/api/crypto/encrypt_vault" => {
            let req: CryptoVaultReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let result = vault::encrypt_entire_vault(
                root,
                &req.password,
                req.hint.as_deref(),
                req.auto_encrypt.unwrap_or(false),
                &mut st.crypto,
            )?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "result": result, "scan": scan }))
        }

        "/api/crypto/decrypt_vault" => {
            let req: CryptoVaultReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let result = vault::decrypt_entire_vault(root, &req.password, &mut st.crypto)?;
            let rev = st.workspace.watcher.bump();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "result": result, "scan": scan }))
        }

        "/api/crypto/unlock_vault" => {
            let req: CryptoVaultReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            let result = vault::unlock_entire_vault(root, &req.password, &mut st.crypto)?;
            let rev = st.workspace.watcher.revision();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "result": result, "scan": scan }))
        }

        "/api/crypto/lock_vault" => {
            let req: VaultOnlyReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let mut st = state.lock().unwrap();
            vault::lock_entire_vault(root, &mut st.crypto);
            let rev = st.workspace.watcher.revision();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "scan": scan }))
        }

        "/api/crypto/set_auto_encrypt" => {
            let req: CryptoAutoEncryptReq = parse_body(body)?;
            let root = Path::new(req.vault.trim());
            let st = state.lock().unwrap();
            vault::set_vault_auto_encrypt(root, req.enabled, &st.crypto)?;
            let rev = st.workspace.watcher.revision();
            let scan = vault::scan_vault_with_session(root, rev, Some(&st.crypto))?;
            Ok(json!({ "scan": scan }))
        }

        // ── 동기화 서비스 & 원격 저장소 제어 ──
        "/api/sync/state" => {
            let mut st = state.lock().unwrap();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/save_config" => {
            let req: SaveConfigReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            let old_profiles = st.sync.cfg.profiles.clone();
            let saved_ok = st.sync.save_config(req.cfg, req.start_after_save);
            if saved_ok {
                let cfg_clone = st.sync.cfg.clone();
                // 기존 동기화 프로필의 볼트 폴더가 변경되었으면 구 경로를 워크스페이스에서 정리하고 새 경로 활성화
                for old_p in &old_profiles {
                    if old_p.kind != ProfileKind::Obsidian || old_p.local_path.trim().is_empty() {
                        continue;
                    }
                    let old_norm = old_p.local_path.trim().replace('\\', "/");
                    if let Some(new_p) = cfg_clone.profiles.iter().find(|p| p.id == old_p.id) {
                        let new_norm = new_p.local_path.trim().replace('\\', "/");
                        let changed = if cfg!(windows) {
                            !old_norm.eq_ignore_ascii_case(&new_norm)
                        } else {
                            old_norm != new_norm
                        };
                        if changed {
                            let still_used = cfg_clone.profiles.iter().any(|p| {
                                let pn = p.local_path.trim().replace('\\', "/");
                                if cfg!(windows) {
                                    pn.eq_ignore_ascii_case(&old_norm)
                                } else {
                                    pn == old_norm
                                }
                            });
                            if st
                                .workspace
                                .state
                                .active_vault
                                .as_deref()
                                .map(|a| {
                                    let an = a.trim().replace('\\', "/");
                                    if cfg!(windows) {
                                        an.eq_ignore_ascii_case(&old_norm)
                                    } else {
                                        an == old_norm
                                    }
                                })
                                .unwrap_or(false)
                                && !new_p.local_path.trim().is_empty()
                            {
                                st.workspace.state.active_vault =
                                    Some(new_p.local_path.trim().to_string());
                            }
                            if !still_used {
                                st.workspace.state.remove_vault(&old_p.local_path);
                            }
                        }
                    }
                }
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
            #[cfg(not(target_os = "android"))]
            open::that(p).with_context(|| format!("폴더를 열지 못했습니다: {p}"))?;
            Ok(json!({}))
        }

        "/api/sync/open_data_dir" => {
            let dir = state.lock().unwrap().sync.data_dir.clone();
            let _ = std::fs::create_dir_all(&dir);
            #[cfg(not(target_os = "android"))]
            open::that(&dir).context("설정 폴더를 열지 못했습니다")?;
            Ok(json!({}))
        }

        "/api/sync/clear_message" => {
            let mut st = state.lock().unwrap();
            st.sync.clear_message();
            ok_json(st.sync.snapshot())
        }

        "/api/sync/disconnect_profile" => {
            let req: DisconnectProfileReq = parse_body(body)?;
            let mut st = state.lock().unwrap();
            let removed_path = st.sync.disconnect_profile(&req.profile_id)?;
            if req.remove_vault {
                if let Some(ref lp) = removed_path {
                    let still_used = st
                        .sync
                        .cfg
                        .profiles
                        .iter()
                        .any(|p| p.local_path.eq_ignore_ascii_case(lp));
                    if !still_used {
                        st.workspace.state.remove_vault(lp);
                    }
                }
            }
            st.workspace.save_and_refresh_watcher()?;
            Ok(json!({
                "sync": st.sync.snapshot(),
                "workspace": st.workspace.state,
            }))
        }

        "/api/sync/detect_cloud_folders" => {
            let folders = crate::sync_controller::detect_cloud_folders();
            Ok(json!({ "folders": folders }))
        }

        "/api/sync/prepare_folder_target" => {
            let req: PrepareFolderTargetReq = parse_body(body)?;
            let path =
                crate::sync_controller::prepare_folder_target(&req.base_folder, &req.subfolder)?;
            Ok(json!({ "path": path }))
        }

        "/api/sync/s3_list_buckets" => {
            let req: S3ListBucketsReq = parse_body(body)?;
            let ep = req.endpoint.trim().to_string();
            let reg = if req.region.trim().is_empty() {
                "kr-standard".to_string()
            } else {
                req.region.trim().to_string()
            };
            let ak = req.access_key.trim().to_string();
            let sk = req.secret_key.trim().to_string();
            if ep.is_empty() || ak.is_empty() || sk.is_empty() {
                bail!("Endpoint, Access Key, Secret Key를 모두 입력해 주세요");
            }
            if let Some(new_bucket) = req
                .create_bucket
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                obsidian_sync_core::backend::s3::create_bucket(&ep, &reg, new_bucket, &ak, &sk)?;
            }
            let buckets = obsidian_sync_core::backend::s3::list_buckets(&ep, &reg, &ak, &sk)?;
            Ok(json!({ "buckets": buckets }))
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
        let state = Arc::new(Mutex::new(BackendState {
            sync,
            workspace,
            crypto: crate::crypto::VaultCryptoSession::new(),
            event_proxy: None,
        }));

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

    #[test]
    fn api_router_crypto_and_daily_folder_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let vault_dir = tmp.path().join("vault");
        std::fs::create_dir_all(&vault_dir).unwrap();
        let vault_str = vault_dir.to_string_lossy().to_string();

        let trigger: SharedTrigger = Arc::new(Mutex::new(None));
        let sync = App::new(data_dir.join("config.json"), data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        let state = Arc::new(Mutex::new(BackendState {
            sync,
            workspace,
            crypto: crate::crypto::VaultCryptoSession::new(),
            event_proxy: None,
        }));

        // 1) 워크스페이스에 볼트별 일일노트 폴더("일일업무") 저장 후 daily_folder 생략 요청
        let ws_patch = serde_json::to_vec(&json!({
            "workspace": {
                "daily_folders_by_vault": {
                    &vault_str: "일일업무"
                }
            }
        }))
        .unwrap();
        let ws_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/workspace/save", &ws_patch)).unwrap();
        assert_eq!(ws_resp["ok"], true);

        let daily_req = serde_json::to_vec(&json!({ "vault": &vault_str })).unwrap();
        let daily_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/note/daily", &daily_req)).unwrap();
        assert_eq!(daily_resp["ok"], true);
        let daily_path = daily_resp["data"]["path"].as_str().unwrap().to_string();
        assert!(daily_path.starts_with("일일업무/"));

        // 2) 단일 노트 암호화 -> 잠금 -> 읽기 시 잠금 상태 -> 잠금 해제 -> 복호화
        let enc_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "path": &daily_path,
            "password": "pw-1234",
            "hint": "숫자4자리"
        }))
        .unwrap();
        let enc_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/encrypt_note", &enc_req))
                .unwrap();
        assert_eq!(enc_resp["ok"], true);
        assert_eq!(enc_resp["data"]["doc"]["encrypted"], true);
        assert_eq!(enc_resp["data"]["doc"]["locked"], false);

        let lock_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "path": &daily_path
        }))
        .unwrap();
        let lock_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/lock_note", &lock_req))
                .unwrap();
        assert_eq!(lock_resp["ok"], true);
        assert_eq!(lock_resp["data"]["doc"]["locked"], true);
        assert_eq!(lock_resp["data"]["doc"]["body"], "");

        let unlock_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "path": &daily_path,
            "password": "pw-1234"
        }))
        .unwrap();
        let unlock_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/unlock_note", &unlock_req))
                .unwrap();
        assert_eq!(unlock_resp["ok"], true);
        assert_eq!(unlock_resp["data"]["doc"]["locked"], false);

        // 3) 전체 볼트 암호화 -> 볼트 잠금 -> 볼트 잠금 해제 -> 볼트 복호화
        let dec_note_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/decrypt_note", &unlock_req))
                .unwrap();
        assert_eq!(dec_note_resp["ok"], true);
        assert_eq!(dec_note_resp["data"]["doc"]["encrypted"], false);

        let ev_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "password": "vault-pw",
            "auto_encrypt": true
        }))
        .unwrap();
        let ev_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/encrypt_vault", &ev_req))
                .unwrap();
        assert_eq!(ev_resp["ok"], true);
        assert_eq!(ev_resp["data"]["result"]["encrypted_count"], 1);
        assert_eq!(ev_resp["data"]["scan"]["vault_auto_encrypt"], true);

        let lv_req = serde_json::to_vec(&json!({ "vault": &vault_str })).unwrap();
        let lv_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/lock_vault", &lv_req)).unwrap();
        assert_eq!(lv_resp["ok"], true);
        assert_eq!(lv_resp["data"]["scan"]["vault_unlocked"], false);

        let uv_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "password": "vault-pw"
        }))
        .unwrap();
        let uv_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/unlock_vault", &uv_req))
                .unwrap();
        assert_eq!(uv_resp["ok"], true);
        assert_eq!(uv_resp["data"]["scan"]["vault_unlocked"], true);

        let auto_req = serde_json::to_vec(&json!({
            "vault": &vault_str,
            "enabled": false
        }))
        .unwrap();
        let auto_resp: Value = serde_json::from_slice(&handle_api(
            &state,
            "/api/crypto/set_auto_encrypt",
            &auto_req,
        ))
        .unwrap();
        assert_eq!(auto_resp["ok"], true);
        assert_eq!(auto_resp["data"]["scan"]["vault_auto_encrypt"], false);

        let dv_resp: Value =
            serde_json::from_slice(&handle_api(&state, "/api/crypto/decrypt_vault", &uv_req))
                .unwrap();
        assert_eq!(dv_resp["ok"], true);
        assert_eq!(dv_resp["data"]["result"]["decrypted_count"], 1);
        assert_eq!(dv_resp["data"]["scan"]["vault_has_crypto_config"], false);
    }

    #[test]
    fn api_router_sync_folder_change_and_disconnect_work_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let vault_a = tmp.path().join("VaultA");
        let vault_b = tmp.path().join("VaultB");
        let cloud_base = tmp.path().join("OneDriveMock");
        std::fs::create_dir_all(&vault_a).unwrap();
        std::fs::create_dir_all(&vault_b).unwrap();
        std::fs::create_dir_all(&cloud_base).unwrap();

        let trigger: SharedTrigger = Arc::new(Mutex::new(None));
        let sync = App::new(data_dir.join("config.json"), data_dir.clone(), trigger);
        let workspace = WorkspaceManager::new(data_dir, &sync.cfg);
        let state = Arc::new(Mutex::new(BackendState {
            sync,
            workspace,
            crypto: crate::crypto::VaultCryptoSession::new(),
            event_proxy: None,
        }));

        // 1) prepare_folder_target 검증
        let prep_req = serde_json::to_vec(&json!({
            "base_folder": cloud_base.to_string_lossy(),
            "subfolder": "OnyxVault/Personal"
        }))
        .unwrap();
        let prep_resp: Value = serde_json::from_slice(&handle_api(
            &state,
            "/api/sync/prepare_folder_target",
            &prep_req,
        ))
        .unwrap();
        assert_eq!(prep_resp["ok"], true);
        let remote_target = prep_resp["data"]["path"].as_str().unwrap().to_string();
        assert!(PathBuf::from(&remote_target).is_dir());

        // 2) VaultA 동기화 저장 -> VaultB로 변경 저장 시 active_vault도 VaultB로 갱신됨
        let mut cfg = AppConfig::default();
        cfg.profiles.push(Profile {
            id: "p1".into(),
            name: "Obsidian".into(),
            kind: ProfileKind::Obsidian,
            local_path: vault_a.to_string_lossy().into(),
            backend: BackendConfig::LocalFolder {
                path: remote_target.clone(),
            },
            ..Default::default()
        });
        let save_a = serde_json::to_vec(&json!({ "cfg": cfg, "start_after_save": false })).unwrap();
        let r1: Value =
            serde_json::from_slice(&handle_api(&state, "/api/sync/save_config", &save_a)).unwrap();
        assert_eq!(r1["ok"], true);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .workspace
                .state
                .active_vault
                .as_deref(),
            Some(vault_a.to_string_lossy().as_ref())
        );

        cfg.profiles[0].local_path = vault_b.to_string_lossy().into();
        let save_b = serde_json::to_vec(&json!({ "cfg": cfg, "start_after_save": false })).unwrap();
        let r2: Value =
            serde_json::from_slice(&handle_api(&state, "/api/sync/save_config", &save_b)).unwrap();
        assert_eq!(r2["ok"], true);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .workspace
                .state
                .active_vault
                .as_deref(),
            Some(vault_b.to_string_lossy().as_ref())
        );

        // 3) disconnect_profile 시 동기화 프로필 제거 및 로컬 폴더 보존
        let disc_req = serde_json::to_vec(&json!({
            "profile_id": "p1",
            "remove_vault": false
        }))
        .unwrap();
        let d_resp: Value = serde_json::from_slice(&handle_api(
            &state,
            "/api/sync/disconnect_profile",
            &disc_req,
        ))
        .unwrap();
        assert_eq!(d_resp["ok"], true);
        assert_eq!(
            d_resp["data"]["sync"]["cfg"]["profiles"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(vault_b.is_dir());
    }
}
