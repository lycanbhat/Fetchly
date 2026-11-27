//! Desktop / web differences: storage, timers, files.

pub const IS_WEB: bool = cfg!(target_arch = "wasm32");
pub const CAN_PICK_PATHS: bool = !IS_WEB;
pub const CMD: &str = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" };

const STORAGE_KEY: &str = "fetchly_state_v2";

// ------------------------------------------------------------------ storage

#[cfg(not(target_arch = "wasm32"))]
fn data_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "Fetchly").map(|d| d.data_dir().to_path_buf())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_state() -> Option<String> {
    let dir = data_dir()?;
    if let Ok(s) = std::fs::read_to_string(dir.join("state.json")) {
        return Some(s);
    }
    // Migrate from the earlier egui build, which stored a JSON string inside `app.ron`.
    let ron = std::fs::read_to_string(dir.join("app.ron")).ok()?;
    let start = ron.find("\"fetchly_state_v1\":")? + "\"fetchly_state_v1\":".len();
    let lit = ron[start..].trim_start();
    let end = lit.rfind('"')?;
    serde_json::from_str::<String>(&lit[..=end]).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_state(json: &str) {
    if let Some(dir) = data_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("state.json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, dir.join("state.json"));
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn load_state() -> Option<String> {
    local_storage()?.get_item(STORAGE_KEY).ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn save_state(json: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(STORAGE_KEY, json);
    }
}

#[allow(dead_code)]
fn _key() -> &'static str {
    STORAGE_KEY
}

// ------------------------------------------------------------------ timers

#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms.min(u32::MAX as u64) as u32).await;
}

pub fn now_secs() -> f64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

// ------------------------------------------------------------------ files

/// Ask for a text file to import: (file name, contents).
pub async fn pick_text_file() -> Option<(String, String)> {
    let h = rfd::AsyncFileDialog::new().add_filter("Collections, environments, cURL", &["json", "txt", "sh"]).add_filter("All files", &["*"]).pick_file().await?;
    let bytes = h.read().await;
    Some((h.file_name(), String::from_utf8_lossy(&bytes).into_owned()))
}

/// Ask for any file path (form-data file fields, binary bodies). Desktop only.
pub fn pick_any_path() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new().pick_file().map(|p| p.to_string_lossy().into_owned())
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Save bytes: a save dialog on desktop, a download in the browser.
pub fn save_file(file_name: &str, content: &[u8]) -> Result<String, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match rfd::FileDialog::new().set_file_name(file_name).save_file() {
            Some(path) => {
                std::fs::write(&path, content).map_err(|e| e.to_string())?;
                Ok(format!("Saved to {}", path.display()))
            }
            None => Err("Cancelled".into()),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let arr = js_sys::Uint8Array::from(content);
        let parts = js_sys::Array::new();
        parts.push(&arr.buffer());
        let blob = web_sys::Blob::new_with_buffer_source_sequence(&parts).map_err(|e| format!("{e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
        let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(|e| format!("{e:?}"))?.dyn_into().map_err(|_| "anchor")?;
        a.set_href(&url);
        a.set_download(file_name);
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Ok(format!("Downloaded {file_name}"))
    }
}

pub fn default_proxy_endpoint() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(origin) = web_sys::window().and_then(|w| w.location().origin().ok()) {
            return format!("{origin}/api/proxy");
        }
    }
    "http://localhost:8080/api/proxy".into()
}
