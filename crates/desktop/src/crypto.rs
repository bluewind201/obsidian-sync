//! Authenticated note and vault encryption (AES-256-GCM + PBKDF2-HMAC-SHA256).
//!
//! Stores encrypted notes inside standard `.md` files with YAML frontmatter + fenced ciphertext
//! so all 6 Obsidian Sync backends, path syncing, and WikiLinks keep working without exposing
//! note body, title properties, tags, icons, or cover metadata on disk.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;
use std::path::Path;
use zeroize::Zeroizing;

pub const DEFAULT_KDF_ITERATIONS: u32 = 210_000;
const MIN_KDF_ITERATIONS: u32 = 10_000;
const MAX_KDF_ITERATIONS: u32 = 10_000_000;
const SALT_LEN: usize = 16;
const AAD_NOTE_V1: &[u8] = b"OBSIDIAN_SYNC_ENC_V1";
const AAD_CANARY_V1: &[u8] = b"OBSIDIAN_SYNC_CANARY_V1";
const CANARY_PLAINTEXT: &[u8] = b"OBSIDIAN_SYNC_VAULT_OK_V1";
pub const CRYPTO_CONFIG_FILENAME: &str = ".obsidian-sync-crypto.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedPropEntry {
    pub key: String,
    pub value: String,
    pub prop_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedNotePayload {
    pub icon: Option<String>,
    pub cover: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub property_list: Vec<EncryptedPropEntry>,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct EncryptedHeader {
    pub cipher: String,
    pub kdf: String,
    pub kdf_iter: u32,
    pub salt: Vec<u8>,
    pub nonce: Vec<u8>,
    pub hint: Option<String>,
    pub ciphertext_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultCryptoConfig {
    pub version: u32,
    #[serde(default)]
    pub auto_encrypt: bool,
    #[serde(default)]
    pub hint: Option<String>,
    pub kdf_iter: u32,
    pub canary_salt: String,
    pub canary_nonce: String,
    pub canary_ct: String,
}

/// RAM-only cache of unlocked passwords for vaults and individual notes.
/// Never serialized or written to disk; cleared cleanly via `Zeroizing` on lock/drop.
#[derive(Default, Clone)]
pub struct VaultCryptoSession {
    vault_passwords: HashMap<String, Zeroizing<String>>,
    note_passwords: HashMap<(String, String), Zeroizing<String>>,
    note_hints: HashMap<(String, String), Option<String>>,
    manually_locked_notes: HashSet<(String, String)>,
}

fn norm_vault_key(vault_root: &Path) -> String {
    let resolved = fs::canonicalize(vault_root).unwrap_or_else(|_| vault_root.to_path_buf());
    let mut s = resolved.to_string_lossy().trim().replace('\\', "/");
    // Strip Windows extended-length UNC prefix `//?/C:/...` produced by fs::canonicalize
    if let Some(stripped) = s.strip_prefix("//?/") {
        s = stripped.to_string();
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

fn norm_rel_key(rel_path: &str) -> String {
    let mut s = rel_path.trim().replace('\\', "/");
    s = s.trim_matches('/').to_string();
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

impl VaultCryptoSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unlock_vault(&mut self, vault_root: &Path, password: &str) {
        let vk = norm_vault_key(vault_root);
        self.vault_passwords
            .insert(vk.clone(), Zeroizing::new(password.to_string()));
        self.manually_locked_notes.retain(|(v, _)| v != &vk);
    }

    pub fn lock_vault(&mut self, vault_root: &Path) {
        let vk = norm_vault_key(vault_root);
        self.vault_passwords.remove(&vk);
        self.note_passwords.retain(|(v, _), _| v != &vk);
        self.note_hints.retain(|(v, _), _| v != &vk);
        self.manually_locked_notes.retain(|(v, _)| v != &vk);
    }

    pub fn is_vault_unlocked(&self, vault_root: &Path) -> bool {
        let vk = norm_vault_key(vault_root);
        self.vault_passwords.contains_key(&vk)
    }

    pub fn vault_password(&self, vault_root: &Path) -> Option<Zeroizing<String>> {
        let vk = norm_vault_key(vault_root);
        self.vault_passwords.get(&vk).cloned()
    }

    pub fn unlock_note(
        &mut self,
        vault_root: &Path,
        rel_path: &str,
        password: &str,
        hint: Option<String>,
    ) {
        let vk = norm_vault_key(vault_root);
        let rk = norm_rel_key(rel_path);
        let key = (vk, rk);
        self.manually_locked_notes.remove(&key);
        self.note_passwords
            .insert(key.clone(), Zeroizing::new(password.to_string()));
        self.note_hints.insert(key, hint);
    }

    pub fn lock_note(&mut self, vault_root: &Path, rel_path: &str) {
        let vk = norm_vault_key(vault_root);
        let rk = norm_rel_key(rel_path);
        let key = (vk, rk);
        self.note_passwords.remove(&key);
        self.note_hints.remove(&key);
        self.manually_locked_notes.insert(key);
    }

    pub fn rename_note_key(&mut self, vault_root: &Path, old_rel: &str, new_rel: &str) {
        let vk = norm_vault_key(vault_root);
        let old_k = (vk.clone(), norm_rel_key(old_rel));
        let new_k = (vk, norm_rel_key(new_rel));
        if old_k == new_k {
            return;
        }
        if let Some(pw) = self.note_passwords.remove(&old_k) {
            self.note_passwords.insert(new_k.clone(), pw);
        }
        if let Some(h) = self.note_hints.remove(&old_k) {
            self.note_hints.insert(new_k.clone(), h);
        }
        if self.manually_locked_notes.remove(&old_k) {
            self.manually_locked_notes.insert(new_k);
        }
    }

    pub fn rename_folder_prefix(&mut self, vault_root: &Path, old_folder: &str, new_folder: &str) {
        let vk = norm_vault_key(vault_root);
        let old_prefix = format!("{}/", norm_rel_key(old_folder));
        let new_prefix = format!("{}/", norm_rel_key(new_folder));
        let matching_pw: Vec<(String, Zeroizing<String>)> = self
            .note_passwords
            .iter()
            .filter_map(|((v, r), pw)| {
                if v == &vk && r.starts_with(&old_prefix) {
                    Some((r.clone(), pw.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (old_r, pw) in matching_pw {
            self.note_passwords.remove(&(vk.clone(), old_r.clone()));
            let suffix = &old_r[old_prefix.len()..];
            self.note_passwords
                .insert((vk.clone(), format!("{new_prefix}{suffix}")), pw);
        }
        let matching_hints: Vec<(String, Option<String>)> = self
            .note_hints
            .iter()
            .filter_map(|((v, r), h)| {
                if v == &vk && r.starts_with(&old_prefix) {
                    Some((r.clone(), h.clone()))
                } else {
                    None
                }
            })
            .collect();
        for (old_r, h) in matching_hints {
            self.note_hints.remove(&(vk.clone(), old_r.clone()));
            let suffix = &old_r[old_prefix.len()..];
            self.note_hints
                .insert((vk.clone(), format!("{new_prefix}{suffix}")), h);
        }
        let matching_locked: Vec<String> = self
            .manually_locked_notes
            .iter()
            .filter_map(|(v, r)| {
                if v == &vk && r.starts_with(&old_prefix) {
                    Some(r.clone())
                } else {
                    None
                }
            })
            .collect();
        for old_r in matching_locked {
            self.manually_locked_notes
                .remove(&(vk.clone(), old_r.clone()));
            let suffix = &old_r[old_prefix.len()..];
            self.manually_locked_notes
                .insert((vk.clone(), format!("{new_prefix}{suffix}")));
        }
    }

    pub fn remove_note_key(&mut self, vault_root: &Path, rel_path: &str) {
        let vk = norm_vault_key(vault_root);
        let rk = norm_rel_key(rel_path);
        let key = (vk, rk);
        self.note_passwords.remove(&key);
        self.note_hints.remove(&key);
        self.manually_locked_notes.remove(&key);
    }

    pub fn get_password_for_note(
        &self,
        vault_root: &Path,
        rel_path: &str,
    ) -> Option<(Zeroizing<String>, Option<String>)> {
        let vk = norm_vault_key(vault_root);
        let rk = norm_rel_key(rel_path);
        let key = (vk.clone(), rk.clone());
        if self.manually_locked_notes.contains(&key) {
            return None;
        }
        if let Some(pw) = self.note_passwords.get(&key) {
            let hint = self.note_hints.get(&key).cloned().flatten();
            return Some((pw.clone(), hint));
        }
        if let Some(pw) = self.vault_passwords.get(&vk) {
            return Some((pw.clone(), None));
        }
        None
    }

    pub fn vault_fingerprint(&self, vault_root: &Path) -> u64 {
        let vk = norm_vault_key(vault_root);
        let mut hasher = DefaultHasher::new();
        if let Some(vpw) = self.vault_passwords.get(&vk) {
            true.hash(&mut hasher);
            vpw.as_str().hash(&mut hasher);
        } else {
            false.hash(&mut hasher);
        }
        let mut notes: Vec<(&str, &str)> = self
            .note_passwords
            .iter()
            .filter_map(|((v, r), pw)| {
                if v == &vk {
                    Some((r.as_str(), pw.as_str()))
                } else {
                    None
                }
            })
            .collect();
        notes.sort_unstable();
        notes.hash(&mut hasher);
        let mut locked: Vec<&str> = self
            .manually_locked_notes
            .iter()
            .filter_map(|(v, r)| if v == &vk { Some(r.as_str()) } else { None })
            .collect();
        locked.sort_unstable();
        locked.hash(&mut hasher);
        hasher.finish()
    }

    pub fn try_decrypt_with_session(
        &self,
        vault_root: &Path,
        rel_path: &str,
        raw: &str,
    ) -> Option<(EncryptedNotePayload, Zeroizing<String>, Option<String>)> {
        let vk = norm_vault_key(vault_root);
        let rk = norm_rel_key(rel_path);
        let key = (vk.clone(), rk);
        if self.manually_locked_notes.contains(&key) {
            return None;
        }

        if let Some(pw) = self.note_passwords.get(&key) {
            if let Ok(payload) = decrypt_note_markdown(raw, pw) {
                let hint = self.note_hints.get(&key).cloned().flatten();
                return Some((payload, pw.clone(), hint));
            }
        }
        if let Some(vpw) = self.vault_passwords.get(&vk) {
            if let Ok(payload) = decrypt_note_markdown(raw, vpw) {
                let hint = is_encrypted_markdown(raw).and_then(|h| h.hint);
                return Some((payload, vpw.clone(), hint));
            }
        }
        None
    }
}

fn derive_aes_key(password: &str, salt: &[u8], iterations: u32) -> Result<Zeroizing<[u8; 32]>> {
    if password.is_empty() {
        bail!("비밀번호가 비어 있습니다.");
    }
    let iter = NonZeroU32::new(iterations.clamp(MIN_KDF_ITERATIONS, MAX_KDF_ITERATIONS))
        .context("유효하지 않은 KDF 반복 횟수입니다")?;
    let mut key = Zeroizing::new([0u8; 32]);
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iter,
        salt,
        password.as_bytes(),
        &mut *key,
    );
    Ok(key)
}

fn seal_with_password(
    plaintext: &[u8],
    password: &str,
    aad_bytes: &[u8],
    iterations: u32,
) -> Result<(Vec<u8>, [u8; NONCE_LEN], Vec<u8>)> {
    let rng = SystemRandom::new();
    let mut salt = vec![0u8; SALT_LEN];
    rng.fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("난수 솔트 생성에 실패했습니다"))?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("난수 Nonce 생성에 실패했습니다"))?;

    let key_bytes = derive_aes_key(password, &salt, iterations)?;
    let unbound = UnboundKey::new(&AES_256_GCM, &*key_bytes)
        .map_err(|_| anyhow::anyhow!("암호화 키 초기화에 실패했습니다"))?;
    let key = LessSafeKey::new(unbound);

    let mut in_out = Zeroizing::new(plaintext.to_vec());
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    key.seal_in_place_append_tag(nonce, Aad::from(aad_bytes), &mut *in_out)
        .map_err(|_| anyhow::anyhow!("AES-256-GCM 암호화에 실패했습니다"))?;

    Ok((salt, nonce_bytes, in_out.to_vec()))
}

fn open_with_password(
    ciphertext: &[u8],
    password: &str,
    salt: &[u8],
    nonce_bytes: &[u8],
    aad_bytes: &[u8],
    iterations: u32,
) -> Result<Zeroizing<Vec<u8>>> {
    if nonce_bytes.len() != NONCE_LEN {
        bail!("암호화된 Nonce 길이가 올바르지 않습니다.");
    }
    let key_bytes = derive_aes_key(password, salt, iterations)?;
    let unbound = UnboundKey::new(&AES_256_GCM, &*key_bytes)
        .map_err(|_| anyhow::anyhow!("복호화 키 초기화에 실패했습니다"))?;
    let key = LessSafeKey::new(unbound);

    let mut buf = Zeroizing::new(ciphertext.to_vec());
    let nonce_arr: [u8; NONCE_LEN] = nonce_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("암호화된 Nonce 형식이 올바르지 않습니다"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_arr);
    let plain_len = key
        .open_in_place(nonce, Aad::from(aad_bytes), &mut *buf)
        .map_err(|_| {
            anyhow::anyhow!("비밀번호가 올바르지 않거나 암호화된 데이터가 손상되었습니다.")
        })?
        .len();
    buf.truncate(plain_len);
    Ok(buf)
}

fn unquote_yaml_val(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
    {
        let inner = &t[1..t.len() - 1];
        if t.starts_with('"') {
            return inner
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
                .replace("\\n", "\n");
        }
        return inner.replace("''", "'");
    }
    t.to_string()
}

fn quote_yaml_val(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

/// Inspects Markdown content to check whether it uses Obsidian Sync's encrypted `.md` envelope.
pub fn is_encrypted_markdown(raw: &str) -> Option<EncryptedHeader> {
    let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let first_nl = normalized.find('\n')?;
    if normalized[..first_nl].trim_end_matches('\r') != "---" {
        return None;
    }

    let rest = &normalized[first_nl + 1..];
    let mut end_offsets: Option<(usize, usize)> = None;
    let mut cursor = 0usize;
    for chunk in rest.split_inclusive('\n') {
        let trimmed = chunk.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            end_offsets = Some((cursor, cursor + chunk.len()));
            break;
        }
        cursor += chunk.len();
    }
    let (yaml_end, body_start) = end_offsets?;
    let yaml_block = &rest[..yaml_end];
    let after_yaml = &rest[body_start..];

    let mut is_enc = false;
    let mut cipher = String::new();
    let mut kdf = String::new();
    let mut kdf_iter = DEFAULT_KDF_ITERATIONS;
    let mut salt_b64 = String::new();
    let mut nonce_b64 = String::new();
    let mut hint: Option<String> = None;

    for line in yaml_block.lines() {
        let Some((k_raw, v_raw)) = line.split_once(':') else {
            continue;
        };
        let key = k_raw.trim();
        let val = unquote_yaml_val(v_raw);
        match key {
            "obsync_encrypted" => {
                is_enc = val.eq_ignore_ascii_case("true");
            }
            "cipher" => cipher = val,
            "kdf" => kdf = val,
            "kdf_iter" => {
                if let Ok(n) = val.parse::<u32>() {
                    kdf_iter = n.clamp(MIN_KDF_ITERATIONS, MAX_KDF_ITERATIONS);
                }
            }
            "salt" => salt_b64 = val,
            "nonce" => nonce_b64 = val,
            "hint" => {
                if !val.trim().is_empty() {
                    hint = Some(val);
                }
            }
            _ => {}
        }
    }

    if !is_enc || cipher != "AES-256-GCM" || kdf != "PBKDF2-HMAC-SHA256" {
        return None;
    }

    let salt = B64.decode(salt_b64.trim()).ok()?;
    let nonce = B64.decode(nonce_b64.trim()).ok()?;
    if salt.len() != SALT_LEN || nonce.len() != NONCE_LEN {
        return None;
    }

    // Extract fenced ```obsidian-encrypted ... ``` block
    let mut in_fence = false;
    let mut b64_lines = Vec::new();
    for line in after_yaml.lines() {
        let trimmed = line.trim();
        if !in_fence {
            if trimmed.starts_with("```obsidian-encrypted") {
                in_fence = true;
            }
        } else if trimmed.starts_with("```") {
            break;
        } else if !trimmed.is_empty() {
            b64_lines.push(trimmed);
        }
    }

    let ciphertext_b64 = b64_lines.concat();
    if ciphertext_b64.is_empty() {
        return None;
    }

    Some(EncryptedHeader {
        cipher,
        kdf,
        kdf_iter,
        salt,
        nonce,
        hint,
        ciphertext_b64,
    })
}

/// Encrypts a structured note payload into an armored `.md` document with a fresh salt and nonce.
pub fn encrypt_note_payload(
    payload: &EncryptedNotePayload,
    password: &str,
    hint: Option<&str>,
) -> Result<String> {
    if password.trim().is_empty() {
        bail!("암호화에 사용할 비밀번호를 입력해 주세요.");
    }
    let json_bytes =
        Zeroizing::new(serde_json::to_vec(payload).context("노트 페이로드 직렬화에 실패했습니다")?);
    let (salt, nonce, ct) =
        seal_with_password(&json_bytes, password, AAD_NOTE_V1, DEFAULT_KDF_ITERATIONS)?;

    let salt_b64 = B64.encode(&salt);
    let nonce_b64 = B64.encode(nonce);
    let ct_b64 = B64.encode(&ct);

    let mut wrapped_ct = String::with_capacity(ct_b64.len() + ct_b64.len() / 76 + 8);
    for chunk in ct_b64.as_bytes().chunks(76) {
        let line = std::str::from_utf8(chunk).unwrap_or_default();
        wrapped_ct.push_str(line);
        wrapped_ct.push('\n');
    }

    let clean_hint = hint.map(str::trim).filter(|s| !s.is_empty());
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("obsync_encrypted: true\n");
    out.push_str("cipher: AES-256-GCM\n");
    out.push_str("kdf: PBKDF2-HMAC-SHA256\n");
    out.push_str(&format!("kdf_iter: {DEFAULT_KDF_ITERATIONS}\n"));
    out.push_str(&format!("salt: {}\n", quote_yaml_val(&salt_b64)));
    out.push_str(&format!("nonce: {}\n", quote_yaml_val(&nonce_b64)));
    if let Some(h) = clean_hint {
        out.push_str(&format!("hint: {}\n", quote_yaml_val(h)));
    }
    out.push_str("---\n\n");
    out.push_str("```obsidian-encrypted\n");
    out.push_str(&wrapped_ct);
    out.push_str("```\n");
    Ok(out)
}

/// Decrypts an armored encrypted `.md` file using the supplied password.
pub fn decrypt_note_markdown(raw: &str, password: &str) -> Result<EncryptedNotePayload> {
    if password.is_empty() {
        bail!("복호화 비밀번호를 입력해 주세요.");
    }
    let hdr = is_encrypted_markdown(raw).context("유효한 암호화 노트 형식이 아닙니다.")?;
    let ct = B64
        .decode(hdr.ciphertext_b64.trim())
        .context("암호화된 본문(Base64)을 디코딩할 수 없습니다")?;

    let plain = open_with_password(
        &ct,
        password,
        &hdr.salt,
        &hdr.nonce,
        AAD_NOTE_V1,
        hdr.kdf_iter,
    )?;

    let payload: EncryptedNotePayload =
        serde_json::from_slice(&plain).context("복호화된 노트 JSON 구조를 해석할 수 없습니다")?;
    Ok(payload)
}

pub fn load_vault_crypto_config(vault_root: &Path) -> Option<VaultCryptoConfig> {
    let path = vault_root.join(CRYPTO_CONFIG_FILENAME);
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_vault_crypto_config(
    vault_root: &Path,
    password: &str,
    auto_encrypt: bool,
    hint: Option<&str>,
) -> Result<VaultCryptoConfig> {
    if password.trim().is_empty() {
        bail!("볼트 암호화 비밀번호를 입력해 주세요.");
    }
    let (salt, nonce, ct) = seal_with_password(
        CANARY_PLAINTEXT,
        password,
        AAD_CANARY_V1,
        DEFAULT_KDF_ITERATIONS,
    )?;
    let cfg = VaultCryptoConfig {
        version: 1,
        auto_encrypt,
        hint: hint
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        kdf_iter: DEFAULT_KDF_ITERATIONS,
        canary_salt: B64.encode(&salt),
        canary_nonce: B64.encode(nonce),
        canary_ct: B64.encode(&ct),
    };
    let path = vault_root.join(CRYPTO_CONFIG_FILENAME);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&cfg)?)?;
    fs::rename(&tmp, &path)?;
    Ok(cfg)
}

pub fn verify_vault_password(cfg: &VaultCryptoConfig, password: &str) -> bool {
    let Ok(salt) = B64.decode(cfg.canary_salt.trim()) else {
        return false;
    };
    let Ok(nonce) = B64.decode(cfg.canary_nonce.trim()) else {
        return false;
    };
    let Ok(ct) = B64.decode(cfg.canary_ct.trim()) else {
        return false;
    };
    let Ok(plain) = open_with_password(&ct, password, &salt, &nonce, AAD_CANARY_V1, cfg.kdf_iter)
    else {
        return false;
    };
    &*plain == CANARY_PLAINTEXT
}

pub fn remove_vault_crypto_config(vault_root: &Path) -> Result<()> {
    let path = vault_root.join(CRYPTO_CONFIG_FILENAME);
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Builds an encrypted-payload property map from `(key, value, prop_type)` entries.
pub fn props_map_from_entries(
    entries: &[EncryptedPropEntry],
) -> BTreeMap<String, serde_json::Value> {
    let mut map = BTreeMap::new();
    for p in entries {
        map.insert(
            p.key.clone(),
            serde_json::json!({
                "value": p.value,
                "prop_type": p.prop_type,
            }),
        );
    }
    map
}

pub fn entries_from_payload(payload: &EncryptedNotePayload) -> Vec<EncryptedPropEntry> {
    if !payload.property_list.is_empty() {
        return payload.property_list.clone();
    }
    payload
        .properties
        .iter()
        .map(|(k, v)| {
            if let Some(obj) = v.as_object() {
                EncryptedPropEntry {
                    key: k.clone(),
                    value: obj
                        .get("value")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    prop_type: obj
                        .get("prop_type")
                        .and_then(|x| x.as_str())
                        .unwrap_or("text")
                        .to_string(),
                }
            } else {
                EncryptedPropEntry {
                    key: k.clone(),
                    value: v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string()),
                    prop_type: if k.eq_ignore_ascii_case("tags") {
                        "tags".into()
                    } else {
                        "text".into()
                    },
                }
            }
        })
        .collect()
}

/// Validates and writes a file atomically via a temporary sibling file.
pub fn atomic_write_string(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_name = format!(
        ".{}.obsync.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("note")
    );
    let tmp = path.with_file_name(tmp_name);
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Avoids clippy/compiler unused warnings on aead import if any refactor occurs.
#[allow(dead_code)]
fn _aead_tag_len() -> usize {
    aead::AES_256_GCM.tag_len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip_and_fresh_nonce() {
        let entries = vec![
            EncryptedPropEntry {
                key: "tags".into(),
                value: "secret, 보안".into(),
                prop_type: "tags".into(),
            },
            EncryptedPropEntry {
                key: "date".into(),
                value: "2026-11-15".into(),
                prop_type: "date".into(),
            },
        ];
        let payload = EncryptedNotePayload {
            icon: Some("🔒".into()),
            cover: Some("linear-gradient(135deg, #2383e2 0%, #6dd5ed 100%)".into()),
            properties: props_map_from_entries(&entries),
            property_list: entries.clone(),
            body: "# 기밀 문서\n\n한글 본문과 [[위키링크]] 그리고 **굵은 글씨** 테스트입니다."
                .into(),
        };

        let armored1 =
            encrypt_note_payload(&payload, "MyStrongPass!123", Some("사내 노트")).unwrap();
        let armored2 =
            encrypt_note_payload(&payload, "MyStrongPass!123", Some("사내 노트")).unwrap();

        assert_ne!(
            armored1, armored2,
            "Each encryption must use a fresh salt/nonce"
        );
        assert!(!armored1.contains("기밀 문서"));
        assert!(!armored1.contains("한글 본문"));

        let hdr = is_encrypted_markdown(&armored1).expect("header should parse");
        assert_eq!(hdr.cipher, "AES-256-GCM");
        assert_eq!(hdr.hint.as_deref(), Some("사내 노트"));

        let decoded = decrypt_note_markdown(&armored1, "MyStrongPass!123").unwrap();
        assert_eq!(decoded.icon.as_deref(), Some("🔒"));
        assert_eq!(
            decoded.cover.as_deref(),
            Some("linear-gradient(135deg, #2383e2 0%, #6dd5ed 100%)")
        );
        assert_eq!(decoded.body, payload.body);
        assert_eq!(entries_from_payload(&decoded), entries);
    }

    #[test]
    fn wrong_password_and_tampered_ciphertext_fail() {
        let payload = EncryptedNotePayload {
            icon: None,
            cover: None,
            properties: BTreeMap::new(),
            property_list: Vec::new(),
            body: "Top secret payload".into(),
        };
        let armored = encrypt_note_payload(&payload, "correct-password", None).unwrap();

        // Wrong password
        assert!(decrypt_note_markdown(&armored, "wrong-password").is_err());

        // Tamper ciphertext byte inside fenced block
        let mut tampered = armored.clone();
        let idx = tampered.rfind("```").unwrap() - 3;
        let replacement = if &tampered[idx..idx + 1] == "A" {
            "B"
        } else {
            "A"
        };
        tampered.replace_range(idx..idx + 1, replacement);
        assert!(decrypt_note_markdown(&tampered, "correct-password").is_err());
    }

    #[test]
    fn vault_crypto_config_canary_and_session() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = save_vault_crypto_config(tmp.path(), "VaultMaster#2026", true, Some("마스터 키"))
            .unwrap();
        assert!(cfg.auto_encrypt);
        assert!(verify_vault_password(&cfg, "VaultMaster#2026"));
        assert!(!verify_vault_password(&cfg, "wrong"));

        let loaded = load_vault_crypto_config(tmp.path()).unwrap();
        assert!(verify_vault_password(&loaded, "VaultMaster#2026"));

        let mut session = VaultCryptoSession::new();
        session.unlock_vault(tmp.path(), "VaultMaster#2026");
        assert!(session.is_vault_unlocked(tmp.path()));

        let payload = EncryptedNotePayload {
            icon: None,
            cover: None,
            properties: BTreeMap::new(),
            property_list: Vec::new(),
            body: "Vault note".into(),
        };
        let armored = encrypt_note_payload(&payload, "VaultMaster#2026", None).unwrap();
        let (dec, _, _) = session
            .try_decrypt_with_session(tmp.path(), "일일업무/2026-11-15.md", &armored)
            .expect("vault session should decrypt");
        assert_eq!(dec.body, "Vault note");

        session.lock_vault(tmp.path());
        assert!(!session.is_vault_unlocked(tmp.path()));
        assert!(session
            .try_decrypt_with_session(tmp.path(), "일일업무/2026-11-15.md", &armored)
            .is_none());

        remove_vault_crypto_config(tmp.path()).unwrap();
        assert!(load_vault_crypto_config(tmp.path()).is_none());
    }
}
