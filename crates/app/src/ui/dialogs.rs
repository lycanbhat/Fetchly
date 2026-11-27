//! All modal dialogs, built on the Dioxus `Dialog` component.

use crate::actions::{self, RUNNER};
use crate::components::dialog::{Dialog as DxDialog, DialogDescription, DialogTitle};
use crate::components::switch::Switch;
use crate::highlight::{self, human_time, Lang};
use crate::platform;
use crate::store::*;
use crate::ui::common::{method_class, method_short, CodeView, Empty, KvTable};
use crate::ui::editor::{AuthEditor, ScriptEditor, SettingRow};
use dioxus::prelude::*;
use fetchly_core::codegen::{self, Language};
use fetchly_core::cookies::{self, StoredCookie};
use fetchly_core::model::*;
use fetchly_core::postman;
use lucide_dioxus as icons;

#[component]
pub fn Dialogs() -> Element {
    let d = UI.read().dialog.clone();
    match d {
        None => rsx! {},
        Some(d) => rsx! {
            match d {
                Dialog::Import => rsx! { ImportDialog {} },
                Dialog::SaveRequest { tab_id } => rsx! { SaveDialog { tab_id } },
                Dialog::Code => rsx! { CodeDialog {} },
                Dialog::ConfirmCloseTab { tab_id } => rsx! { ConfirmCloseTab { tab_id } },
                Dialog::ConfirmDelete { cid, item, name } => rsx! { ConfirmDelete { cid, item, name } },
                Dialog::ItemSettings(target) => rsx! { ItemSettings { target } },
                Dialog::Environment(id) => rsx! { EnvironmentDialog { env_id: id } },
                Dialog::Settings => rsx! { SettingsDialog {} },
                Dialog::Cookies => rsx! { CookieManager {} },
                Dialog::Runner => rsx! { RunnerDialog {} },
                Dialog::Shortcuts => rsx! { ShortcutsDialog {} },
                Dialog::ScriptHelp => rsx! { ScriptHelpDialog {} },
            }
        },
    }
}

/// Dialog frame: title, optional description, close button, body.
#[component]
fn Frame(title: String, #[props(default)] description: String, #[props(default)] size: String, icon: Option<Element>, children: Element) -> Element {
    rsx! {
        DxDialog {
            open: Some(true),
            on_open_change: move |open: bool| if !open { close_dialog() },
            class: "{size}",
            div { class: "dialog-head",
                if let Some(i) = icon { div { class: "empty-icon", style: "width:34px;height:34px;border-radius:9px", {i} } }
                div { class: "grow",
                    DialogTitle { "{title}" }
                    if !description.is_empty() { DialogDescription { "{description}" } }
                }
                button { class: "icon-btn", title: "Close", onclick: move |_| close_dialog(), icons::X { size: 16 } }
            }
            {children}
        }
    }
}

// ------------------------------------------------------------------ import

#[component]
fn ImportDialog() -> Element {
    let mut text = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut do_import = move |t: String| match actions::import_text(&t) {
        Ok(msg) => {
            toast(msg);
            close_dialog();
        }
        Err(e) => error.set(Some(e)),
    };
    rsx! {
        Frame { title: "Import", description: "Postman collections (v2.0 / v2.1) and environments, Fetchly backups, or cURL commands.", size: "lg", icon: rsx! { icons::Import { size: 17 } },
            div { class: "dialog-body",
                button {
                    class: "btn", style: "height: 64px; border-style: dashed; justify-content: center; font-size: 13px",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some((_, t)) = platform::pick_text_file().await {
                                do_import(t);
                            }
                        });
                    },
                    icons::Upload { size: 16 } "Choose a file to import"
                }
                div { class: "row muted small", div { class: "divider grow" } "or paste" div { class: "divider grow" } }
                textarea { class: "input mono", rows: 12, spellcheck: false, value: "{text}",
                    placeholder: "{{ \"info\": {{ … }}, \"item\": [ … ] }}\n\ncurl https://api.example.com/users -H 'Accept: application/json'",
                    oninput: move |e| { text.set(e.value()); error.set(None); } }
                if let Some(e) = error() { div { class: "var-warn", style: "color: var(--danger)", icons::CircleX { size: 14 } "{e}" } }
            }
            div { class: "dialog-foot",
                button { class: "btn", onclick: move |_| close_dialog(), "Cancel" }
                button { class: "btn primary", disabled: text().trim().is_empty(), onclick: move |_| do_import(text()), icons::Import { size: 14 } "Import" }
            }
        }
    }
}

// ------------------------------------------------------------------ save request

/// (folder id, "A / B") for every folder, depth-first.
pub fn folder_paths(items: &[Item], prefix: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for it in items {
        if let Item::Folder(f) = it {
            let path = if prefix.is_empty() { f.name.clone() } else { format!("{prefix} / {}", f.name) };
            out.push((f.id.clone(), path.clone()));
            out.extend(folder_paths(&f.items, &path));
        }
    }
    out
}

#[component]
fn SaveDialog(tab_id: String) -> Element {
    let initial = TABS.read().iter().find(|t| t.id == tab_id).map(|t| {
        if t.request.name == "Untitled Request" && !t.request.url.is_empty() { fetchly_core::curl::derive_name(&t.request.url) } else { t.request.name.clone() }
    });
    let mut name = use_signal(|| initial.unwrap_or_default());
    let mut cid = use_signal(|| WS.read().collections.first().map(|c| c.id.clone()).unwrap_or_default());
    let mut folder = use_signal(String::new);
    let mut new_coll = use_signal(String::new);
    let collections: Vec<(String, String)> = WS.read().collections.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
    let folders = WS.read().collection(&cid()).map(|c| folder_paths(&c.items, "")).unwrap_or_default();
    let save = move |_| {
        let mut target = cid();
        if target.is_empty() {
            let n = if new_coll().trim().is_empty() { "New Collection".to_string() } else { new_coll().trim().to_string() };
            let c = Collection::new(n);
            target = c.id.clone();
            WS.write().collections.push(c);
        }
        let Some(mut req) = TABS.read().iter().find(|t| t.id == tab_id).map(|t| t.request.clone()) else { return };
        req.id = new_id();
        req.name = if name().trim().is_empty() { "Untitled Request".into() } else { name().trim().to_string() };
        let rid = req.id.clone();
        let f = folder();
        let pushed = WS.write().collection_mut(&target).and_then(|c| c.container_mut(if f.is_empty() { None } else { Some(&f) })).map(|list| list.push(Item::Request(req.clone()))).is_some();
        if pushed {
            if let Some(t) = TABS.write().iter_mut().find(|t| t.id == tab_id) {
                t.request = req;
                t.origin = Some(Origin { collection_id: target.clone(), request_id: rid });
            }
            UI.write().expanded.insert(target);
            if !f.is_empty() {
                UI.write().expanded.insert(f);
            }
            toast("Request saved");
        }
        close_dialog();
    };
    rsx! {
        Frame { title: "Save request", description: "Choose a collection, and optionally a folder, to save this request into.", size: "sm", icon: rsx! { icons::Save { size: 17 } },
            div { class: "dialog-body",
                div { class: "field", label { "Name" } input { class: "input", value: "{name}", oninput: move |e| name.set(e.value()) } }
                div { class: "field", label { "Collection" }
                    select { class: "select-native input", onchange: move |e| { cid.set(e.value()); folder.set(String::new()); },
                        for (id , n) in collections { option { key: "{id}", value: "{id}", selected: id == cid(), "{n}" } }
                        option { value: "", selected: cid().is_empty(), "+ New collection…" }
                    }
                }
                if cid().is_empty() {
                    div { class: "field", label { "New collection name" } input { class: "input", placeholder: "My API", value: "{new_coll}", oninput: move |e| new_coll.set(e.value()) } }
                } else if !folders.is_empty() {
                    div { class: "field", label { "Folder" }
                        select { class: "select-native input", onchange: move |e| folder.set(e.value()),
                            option { value: "", "(collection root)" }
                            for (id , p) in folders { option { key: "{id}", value: "{id}", selected: id == folder(), "{p}" } }
                        }
                    }
                }
            }
            div { class: "dialog-foot",
                button { class: "btn", onclick: move |_| close_dialog(), "Cancel" }
                button { class: "btn primary", onclick: save, icons::Save { size: 14 } "Save" }
            }
        }
    }
}

// ------------------------------------------------------------------ code generation

#[component]
fn CodeDialog() -> Element {
    let mut lang = use_signal(|| Language::Curl);
    let active = ACTIVE.read().clone();
    let tab = TABS.read().iter().find(|t| t.id == active).cloned();
    let code: Result<String, String> = match &tab {
        None => Err("No request".into()),
        Some(t) => {
            let cid = t.origin.as_ref().map(|o| o.collection_id.clone());
            let plan = fetchly_core::pipeline::plan(&WS.read(), &t.request, cid.as_deref());
            fetchly_core::http::prepare(&plan.request, &plan.auth, &plan.scope).map(|p| codegen::generate(lang(), &plan.request, &p, &plan.scope))
        }
    };
    let script = !matches!(lang(), Language::HttpRaw);
    rsx! {
        Frame { title: "Code snippet", description: "Variables are resolved with the active environment.", size: "lg", icon: rsx! { icons::CodeXml { size: 17 } },
            div { class: "row",
                select { class: "select-native input", style: "width: 240px",
                    onchange: move |e| if let Some(l) = Language::ALL.into_iter().find(|l| l.label() == e.value()) { lang.set(l) },
                    for l in Language::ALL { option { key: "{l.label()}", value: "{l.label()}", selected: l == lang(), "{l.label()}" } }
                }
                div { class: "spacer" }
                if let Ok(c) = code.clone() {
                    button { class: "btn", onclick: move |_| {
                        let c = c.clone();
                        spawn(async move {
                            let _ = document::eval(&format!("navigator.clipboard.writeText({})", serde_json::to_string(&c).unwrap_or_default())).await;
                            toast("Snippet copied");
                        });
                    }, icons::Copy { size: 14 } "Copy" }
                }
            }
            div { class: "dialog-body", style: "border: 1px solid var(--border); border-radius: 8px; background: var(--bg-subtle)",
                match code {
                    Ok(c) => rsx! { CodeView { html: if script { highlight::to_html(&c, Lang::Script) } else { highlight::escape(&c) } } },
                    Err(e) => rsx! { div { class: "var-warn", style: "padding: 12px; color: var(--danger)", "{e}" } },
                }
            }
        }
    }
}

// ------------------------------------------------------------------ confirmations

#[component]
fn ConfirmCloseTab(tab_id: String) -> Element {
    let (a, b) = (tab_id.clone(), tab_id.clone());
    rsx! {
        Frame { title: "Unsaved changes", description: "Save your changes to this request before closing it?", size: "sm", icon: rsx! { icons::TriangleAlert { size: 17 } },
            div { class: "dialog-foot",
                button { class: "btn", onclick: move |_| close_dialog(), "Cancel" }
                button { class: "btn", onclick: move |_| { close_dialog(); close_tab(&a, true); }, "Don't save" }
                button { class: "btn primary", onclick: move |_| {
                    close_dialog();
                    let has_origin = TABS.read().iter().find(|t| t.id == b).is_some_and(|t| t.origin.is_some());
                    save_tab(&b);
                    if has_origin { close_tab(&b, true); }
                }, "Save" }
            }
        }
    }
}

#[component]
fn ConfirmDelete(cid: String, item: Option<String>, name: String) -> Element {
    rsx! {
        Frame { title: "Delete “{name}”?", description: "This can't be undone.", size: "sm", icon: rsx! { icons::Trash { size: 17 } },
            div { class: "dialog-foot",
                button { class: "btn", onclick: move |_| close_dialog(), "Cancel" }
                button { class: "btn danger", onclick: move |_| {
                    match &item {
                        None => WS.write().collections.retain(|c| c.id != cid),
                        Some(id) => { if let Some(c) = WS.write().collection_mut(&cid) { c.remove_item(id); } }
                    }
                    refresh_origins();
                    close_dialog();
                }, icons::Trash { size: 14 } "Delete" }
            }
        }
    }
}

// ------------------------------------------------------------------ collection / folder settings

fn with_target<R>(t: &SettingsTarget, f: impl FnOnce(&mut String, &mut String, &mut Auth, &mut String, &mut String, Option<&mut Vec<KeyValue>>) -> R) -> Option<R> {
    let mut ws = WS.write();
    match t {
        SettingsTarget::Collection(cid) => ws.collection_mut(cid).map(|c| f(&mut c.name, &mut c.description, &mut c.auth, &mut c.pre_request_script, &mut c.test_script, Some(&mut c.variables))),
        SettingsTarget::Folder(cid, fid) => match ws.collection_mut(cid).and_then(|c| c.find_item_mut(fid)) {
            Some(Item::Folder(fo)) => Some(f(&mut fo.name, &mut fo.description, &mut fo.auth, &mut fo.pre_request_script, &mut fo.test_script, None)),
            _ => None,
        },
    }
}

#[component]
fn ItemSettings(target: SettingsTarget) -> Element {
    let mut section = use_signal(|| 0usize);
    let snapshot = {
        let ws = WS.read();
        match &target {
            SettingsTarget::Collection(cid) => ws.collection(cid).map(|c| (c.name.clone(), c.description.clone(), c.auth.clone(), c.pre_request_script.clone(), c.test_script.clone(), Some(c.variables.clone()))),
            SettingsTarget::Folder(cid, fid) => ws.collection(cid).and_then(|c| c.folder(fid)).map(|f| (f.name.clone(), f.description.clone(), f.auth.clone(), f.pre_request_script.clone(), f.test_script.clone(), None)),
        }
    };
    let Some((name, desc, auth, pre, test, vars)) = snapshot else { return rsx! {} };
    let is_coll = matches!(target, SettingsTarget::Collection(_));
    let cid = match &target {
        SettingsTarget::Collection(c) | SettingsTarget::Folder(c, _) => c.clone(),
    };
    let scope_val = fetchly_core::pipeline::build_scope(&WS.read(), Some(&cid));
    let scope = use_signal(|| scope_val);
    let mut tabs = vec!["Overview", "Authorization", "Pre-request", "Tests"];
    if is_coll {
        tabs.push("Variables");
    }
    let (t1, t2, t3, t4, t5, t6) = (target.clone(), target.clone(), target.clone(), target.clone(), target.clone(), target.clone());
    let icon = if is_coll { rsx! { icons::LibraryBig { size: 17 } } } else { rsx! { icons::Folder { size: 17 } } };
    let description = if is_coll { "Collection settings apply to every request inside it." } else { "Folder settings apply to every request inside it." };
    rsx! {
        Frame { title: "{name}", description, size: "xl", icon,
            div { class: "utabs",
                for (i , t) in tabs.into_iter().enumerate() {
                    button { key: "{t}", class: if section() == i { "utab on" } else { "utab" }, onclick: move |_| section.set(i), "{t}" }
                }
            }
            div { class: "dialog-body",
                match section() {
                    0 => rsx! {
                        div { class: "field", label { "Name" } input { class: "input", value: "{name}", oninput: move |e| { with_target(&t1, |n, _, _, _, _, _| *n = e.value()); } } }
                        div { class: "field", label { "Description" } textarea { class: "input", rows: 8, value: "{desc}", oninput: move |e| { with_target(&t2, |_, d, _, _, _, _| *d = e.value()); } } }
                    },
                    1 => rsx! { AuthEditor { auth, allow_inherit: !is_coll, scope, onchange: move |a| { with_target(&t3, |_, _, au, _, _, _| *au = a); } } },
                    2 => rsx! { ScriptEditor { value: pre, tests: false, onchange: move |s| { with_target(&t4, |_, _, _, p, _, _| *p = s); } } },
                    3 => rsx! { ScriptEditor { value: test, tests: true, onchange: move |s| { with_target(&t5, |_, _, _, _, t, _| *t = s); } } },
                    _ => rsx! {
                        div { class: "hint", "Collection variables are available as {{{{name}}}} in every request of this collection." }
                        KvTable { rows: vars.unwrap_or_default(), description: true, onchange: move |rows| { with_target(&t6, |_, _, _, _, _, v| if let Some(v) = v { *v = rows }); } }
                    },
                }
            }
        }
    }
}

// ------------------------------------------------------------------ environments

#[component]
fn EnvironmentDialog(env_id: Option<String>) -> Element {
    let ws = WS.read();
    let data = match &env_id {
        None => Some(("Globals".to_string(), ws.globals.clone())),
        Some(id) => ws.environments.iter().find(|e| &e.id == id).map(|e| (e.name.clone(), e.variables.clone())),
    };
    let active = env_id.is_some() && ws.active_environment == env_id;
    drop(ws);
    let Some((name, vars)) = data else {
        return rsx! {
            Frame { title: "No environment selected", size: "sm",
                div { class: "dialog-body",
                    div { class: "hint", "Pick an environment in the header or create one to manage its variables." }
                    div { class: "row",
                        button { class: "btn primary", onclick: move |_| crate::ui::sidebar::new_environment(), icons::Plus { size: 14 } "New environment" }
                        button { class: "btn", onclick: move |_| open_dialog(Dialog::Environment(None)), icons::Globe { size: 14 } "Edit globals" }
                    }
                }
            }
        };
    };
    let (e1, e2, e3, e4, e5) = (env_id.clone(), env_id.clone(), env_id.clone(), env_id.clone(), env_id.clone());
    let title = if env_id.is_none() { "Globals".to_string() } else { format!("Environment · {name}") };
    let description = if env_id.is_none() { "Global variables are available in every request." } else { "Variables here are used while this environment is active." };
    let icon = if env_id.is_none() { rsx! { icons::Globe { size: 17 } } } else { rsx! { icons::Layers { size: 17 } } };
    rsx! {
        Frame { title, description, size: "lg", icon,
            if env_id.is_some() {
                div { class: "row",
                    input { class: "input grow", value: "{name}", oninput: move |e| {
                        if let Some(env) = WS.write().environments.iter_mut().find(|x| Some(&x.id) == e1.as_ref()) { env.name = e.value(); }
                    } }
                    if active {
                        span { class: "badge ok", icons::Check { size: 12 } "Active" }
                    } else {
                        button { class: "btn", onclick: move |_| WS.write().active_environment = e2.clone(), "Set active" }
                    }
                }
            }
            div { class: "dialog-body",
                KvTable { rows: vars, description: false, onchange: move |rows| {
                    let mut ws = WS.write();
                    match &e3 {
                        None => ws.globals = rows,
                        Some(id) => if let Some(env) = ws.environments.iter_mut().find(|x| &x.id == id) { env.variables = rows },
                    }
                } }
            }
            if env_id.is_some() {
                div { class: "dialog-foot",
                    button { class: "btn ghost", style: "color: var(--danger); margin-right: auto", onclick: move |_| {
                        let id = e4.clone().unwrap_or_default();
                        let mut ws = WS.write();
                        ws.environments.retain(|e| e.id != id);
                        if ws.active_environment.as_deref() == Some(id.as_str()) { ws.active_environment = None; }
                        drop(ws);
                        close_dialog();
                    }, icons::Trash { size: 14 } "Delete" }
                    button { class: "btn", onclick: {
                        let e5 = e5.clone();
                        move |_| if let Some(e) = WS.read().environments.iter().find(|x| Some(&x.id) == e5.as_ref()).cloned() {
                            WS.write().environments.push(Environment { id: new_id(), name: format!("{} Copy", e.name), ..e });
                            toast("Environment duplicated");
                        }
                    }, icons::Copy { size: 14 } "Duplicate" }
                    button { class: "btn", onclick: move |_| if let Some(e) = WS.read().environments.iter().find(|x| Some(&x.id) == env_id.as_ref()).cloned() {
                        actions::export_file(&format!("{}.postman_environment.json", e.name.replace(' ', "_")), &postman::export_environment(&e));
                    }, icons::Download { size: 14 } "Export" }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ settings

#[component]
fn SettingsDialog() -> Element {
    let s = SETTINGS.read().clone();
    rsx! {
        Frame { title: "Settings", size: "lg", icon: rsx! { icons::Settings { size: 17 } },
            div { class: "dialog-body",
                div { class: "section-label", "Appearance" }
                SettingRow { title: "Theme", text: "Follow the system, or force light / dark.",
                    div { class: "seg",
                        for (t , label) in [(ThemeChoice::System, "System"), (ThemeChoice::Light, "Light"), (ThemeChoice::Dark, "Dark")] {
                            button { key: "{label}", class: if s.theme == t { "on" } else { "" }, onclick: move |_| SETTINGS.write().theme = t, "{label}" }
                        }
                    }
                }
                SettingRow { title: "Interface scale", text: "{(s.zoom * 100.0).round()}%",
                    div { class: "row",
                        button { class: "icon-btn", onclick: move |_| { let z = (SETTINGS.read().zoom - 0.1).max(0.7); SETTINGS.write().zoom = z; }, icons::ZoomOut { size: 16 } }
                        button { class: "btn sm", onclick: move |_| SETTINGS.write().zoom = 1.0, "Reset" }
                        button { class: "icon-btn", onclick: move |_| { let z = (SETTINGS.read().zoom + 0.1).min(1.6); SETTINGS.write().zoom = z; }, icons::ZoomIn { size: 16 } }
                    }
                }
                SettingRow { title: "Wrap response lines", text: "Soft-wrap long lines in the response body.",
                    Switch { checked: Some(s.wrap_response), on_checked_change: move |v| SETTINGS.write().wrap_response = v }
                }
                div { class: "section-label", style: "margin-top: 8px", "Network" }
                SettingRow { title: "Use Fetchly proxy", text: if platform::IS_WEB { "Browsers block most cross-origin API calls (CORS). Keep this on when using fetchly-server." } else { "Optional on desktop; requests normally go out directly." },
                    Switch { checked: Some(s.use_proxy), on_checked_change: move |v| SETTINGS.write().use_proxy = v }
                }
                if s.use_proxy {
                    div { class: "form-grid",
                        label { "Proxy endpoint" }
                        input { class: "input mono", value: "{s.proxy_endpoint}", oninput: move |e| SETTINGS.write().proxy_endpoint = e.value() }
                        label { "Proxy token" }
                        input { class: "input", r#type: "password", placeholder: "Only if FETCHLY_PROXY_TOKEN is set", value: "{s.proxy_token}", oninput: move |e| SETTINGS.write().proxy_token = e.value() }
                    }
                }
                div { class: "section-label", style: "margin-top: 8px", "Data" }
                SettingRow { title: "Save request history", text: "Keep a list of every request you send.",
                    Switch { checked: Some(s.save_history), on_checked_change: move |v| SETTINGS.write().save_history = v }
                }
                SettingRow { title: "Workspace backup", text: "Export everything (collections, environments, globals, history) to one file.",
                    div { class: "row",
                        button { class: "btn", onclick: move |_| WS.write().history.clear(), icons::Eraser { size: 14 } "Clear history" }
                        button { class: "btn", onclick: move |_| crate::ui::chrome::export_workspace(), icons::Download { size: 14 } "Export backup" }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ cookie manager

#[derive(Clone, PartialEq)]
struct CookieDraft {
    original: Option<(String, String, String)>, // domain, name, path
    name: String,
    value: String,
    domain: String,
    path: String,
    expires: String,
    secure: bool,
    http_only: bool,
}

impl CookieDraft {
    fn new(domain: &str) -> Self {
        Self { original: None, name: String::new(), value: String::new(), domain: domain.into(), path: "/".into(), expires: String::new(), secure: false, http_only: false }
    }
    fn from(c: &StoredCookie) -> Self {
        Self {
            original: Some((c.domain.clone(), c.name.clone(), c.path.clone())),
            name: c.name.clone(),
            value: c.value.clone(),
            domain: c.domain.clone(),
            path: c.path.clone(),
            expires: c.expires.and_then(|e| chrono::DateTime::from_timestamp(e, 0)).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default(),
            secure: c.secure,
            http_only: c.http_only,
        }
    }
}

fn parse_local_time(s: &str) -> Result<Option<i64>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    for fmt in ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(n) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            if let Some(dt) = n.and_local_timezone(chrono::Local).single() {
                return Ok(Some(dt.timestamp()));
            }
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(d.and_hms_opt(0, 0, 0).and_then(|n| n.and_local_timezone(chrono::Local).single()).map(|d| d.timestamp()));
    }
    Err("Use the format YYYY-MM-DD HH:MM, or leave empty for a session cookie".into())
}

#[component]
fn CookieManager() -> Element {
    // Cookies live in a non-reactive global jar; `version` re-renders after edits.
    let mut version = use_signal(|| 0u32);
    let _ = version();
    let mut selected = use_signal(String::new);
    let mut new_domain = use_signal(String::new);
    let mut draft = use_signal(|| None::<CookieDraft>);
    let mut error = use_signal(|| None::<String>);
    let mut filter = use_signal(String::new);

    let jar = cookies::global().read().unwrap().clone();
    let mut domains = jar.domains();
    let sel = selected();
    if !sel.is_empty() && !domains.contains(&sel) {
        domains.push(sel.clone());
        domains.sort();
    }
    let f = filter().to_lowercase();
    let domains: Vec<String> = domains.into_iter().filter(|d| f.is_empty() || d.contains(&f)).collect();
    let current = if sel.is_empty() { domains.first().cloned().unwrap_or_default() } else { sel };
    let list: Vec<StoredCookie> = jar.for_domain(&current).into_iter().cloned().collect();
    let total = jar.cookies.len();

    let save_draft = move |_| {
        let Some(d) = draft() else { return };
        if d.name.trim().is_empty() || d.domain.trim().is_empty() {
            error.set(Some("Name and domain are required".into()));
            return;
        }
        let expires = match parse_local_time(&d.expires) {
            Ok(e) => e,
            Err(e) => {
                error.set(Some(e));
                return;
            }
        };
        let mut jar = cookies::global().write().unwrap();
        if let Some((dom, name, path)) = &d.original {
            jar.remove(dom, name, path);
        }
        let mut c = StoredCookie::new(&d.domain, d.name.trim(), &d.value);
        c.path = if d.path.trim().is_empty() { "/".into() } else { d.path.trim().into() };
        c.expires = expires;
        c.secure = d.secure;
        c.http_only = d.http_only;
        let dom = c.domain.clone();
        jar.upsert(c);
        drop(jar);
        selected.set(dom);
        draft.set(None);
        error.set(None);
        version += 1;
    };

    let description = if platform::IS_WEB && !SETTINGS.read().use_proxy {
        "Cookies are stored per domain and sent with matching requests. Without the proxy, the browser manages cookies itself."
    } else {
        "Cookies are stored per domain and sent automatically with matching requests."
    };
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let list_label = format!("{} cookie{}", list.len(), plural(list.len()));
    let total_label = format!("{} cookie{} in total", total, plural(total));
    rsx! {
        Frame { title: "Cookies", description,
            size: "xl", icon: rsx! { icons::Cookie { size: 17 } },
            div { class: "cookie-layout",
                div { class: "cookie-domains",
                    div { style: "padding: 8px; display: flex; flex-direction: column; gap: 6px; border-bottom: 1px solid var(--border)",
                        div { class: "search", icons::Search { size: 14 } input { class: "input sm", placeholder: "Filter domains", value: "{filter}", oninput: move |e| filter.set(e.value()) } }
                        div { class: "row", style: "gap: 4px",
                            input { class: "input sm grow", placeholder: "Add domain, e.g. api.example.com", value: "{new_domain}", oninput: move |e| new_domain.set(e.value()),
                                onkeydown: move |e| if e.key() == Key::Enter {
                                    let d = cookies::normalize_domain(&new_domain());
                                    if !d.is_empty() { selected.set(d.clone()); draft.set(Some(CookieDraft::new(&d))); new_domain.set(String::new()); }
                                } }
                            button { class: "icon-btn", title: "Add domain", onclick: move |_| {
                                let d = cookies::normalize_domain(&new_domain());
                                if !d.is_empty() { selected.set(d.clone()); draft.set(Some(CookieDraft::new(&d))); new_domain.set(String::new()); }
                            }, icons::Plus { size: 16 } }
                        }
                    }
                    div { class: "list",
                        if domains.is_empty() { div { class: "hint", style: "padding: 8px", "No cookies yet. They appear here after a server sets them, or add one manually." } }
                        for d in domains {
                            {
                                let n = jar.for_domain(&d).len();
                                let (d1, d2) = (d.clone(), d.clone());
                                rsx! {
                                    div { key: "{d}", class: if d == current { "domain-item on" } else { "domain-item" }, onclick: move |_| { selected.set(d1.clone()); draft.set(None); },
                                        icons::Globe { size: 14 }
                                        span { class: "grow truncate", "{d}" }
                                        span { class: "badge", "{n}" }
                                        button { class: "icon-btn sm danger", title: "Remove all cookies for this domain", onclick: move |e| {
                                            e.stop_propagation();
                                            cookies::global().write().unwrap().clear_domain(&d2);
                                            version += 1;
                                        }, icons::Trash { size: 13 } }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "cookie-list",
                    if current.is_empty() {
                        Empty { title: "No cookies", text: "Send a request that sets cookies, or add a domain on the left.", icon: rsx! { icons::Cookie { size: 22 } } }
                    } else {
                        div { class: "row",
                            span { class: "strong", "{current}" }
                            span { class: "badge", "{list_label}" }
                            div { class: "spacer" }
                            button { class: "btn sm", onclick: { let c = current.clone(); move |_| { draft.set(Some(CookieDraft::new(&c))); error.set(None); } }, icons::Plus { size: 14 } "Add cookie" }
                        }
                        if let Some(d) = draft() {
                            div { class: "cookie-card", style: "border-color: var(--accent)",
                                div { class: "cookie-edit",
                                    div { class: "field", label { "Name" } input { class: "input sm mono", value: "{d.name}", oninput: move |e| draft.with_mut(|x| if let Some(x) = x { x.name = e.value() }) } }
                                    div { class: "field", label { "Value" } input { class: "input sm mono", value: "{d.value}", oninput: move |e| draft.with_mut(|x| if let Some(x) = x { x.value = e.value() }) } }
                                    div { class: "field", label { "Domain" } input { class: "input sm", value: "{d.domain}", oninput: move |e| draft.with_mut(|x| if let Some(x) = x { x.domain = e.value() }) } }
                                    div { class: "field", label { "Path" } input { class: "input sm", value: "{d.path}", oninput: move |e| draft.with_mut(|x| if let Some(x) = x { x.path = e.value() }) } }
                                    div { class: "field full", label { "Expires (local time — empty = session cookie)" } input { class: "input sm", placeholder: "2027-01-31 18:00", value: "{d.expires}", oninput: move |e| draft.with_mut(|x| if let Some(x) = x { x.expires = e.value() }) } }
                                    div { class: "row full", style: "gap: 18px",
                                        label { class: "row small", style: "gap:6px", input { r#type: "checkbox", class: "check", checked: d.secure, onchange: move |_| draft.with_mut(|x| if let Some(x) = x { x.secure = !x.secure }) } "Secure" }
                                        label { class: "row small", style: "gap:6px", input { r#type: "checkbox", class: "check", checked: d.http_only, onchange: move |_| draft.with_mut(|x| if let Some(x) = x { x.http_only = !x.http_only }) } "HttpOnly" }
                                        div { class: "spacer" }
                                        button { class: "btn sm", onclick: move |_| { draft.set(None); error.set(None); }, "Cancel" }
                                        button { class: "btn sm primary", onclick: save_draft, icons::Check { size: 14 } "Save" }
                                    }
                                }
                                if let Some(e) = error() { div { class: "var-warn", style: "color: var(--danger)", icons::CircleX { size: 13 } "{e}" } }
                            }
                        }
                        if list.is_empty() && draft().is_none() {
                            div { class: "hint", "No cookies stored for this domain yet." }
                        }
                        for c in list {
                            {
                                let (c1, c2) = (c.clone(), c.clone());
                                let raw = c.to_header_string();
                                let ck = format!("{}|{}", c.name, c.path);
                                rsx! {
                                    div { key: "{ck}", class: "cookie-card",
                                        div { class: "row",
                                            span { class: "mono strong", "{c.name}" }
                                            if c.secure { span { class: "badge", icons::Lock { size: 11 } "Secure" } }
                                            if c.http_only { span { class: "badge", "HttpOnly" } }
                                            if c.expires.is_none() { span { class: "badge info", "Session" } }
                                            div { class: "spacer" }
                                            button { class: "icon-btn sm", title: "Edit", onclick: move |_| { draft.set(Some(CookieDraft::from(&c1))); error.set(None); }, icons::Pencil { size: 13 } }
                                            button { class: "icon-btn sm danger", title: "Delete", onclick: move |_| {
                                                cookies::global().write().unwrap().remove(&c2.domain, &c2.name, &c2.path);
                                                version += 1;
                                            }, icons::Trash { size: 13 } }
                                        }
                                        div { class: "raw selectable", "{raw}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "dialog-foot",
                span { class: "hint", style: "margin-right: auto", "{total_label}" }
                button { class: "btn ghost", style: "color: var(--danger)", disabled: total == 0, onclick: move |_| { cookies::global().write().unwrap().clear(); version += 1; }, icons::Trash { size: 14 } "Clear all" }
                button { class: "btn primary", onclick: move |_| close_dialog(), "Done" }
            }
        }
    }
}

// ------------------------------------------------------------------ collection runner

#[component]
fn RunnerDialog() -> Element {
    let r = RUNNER.read().clone();
    let ws = WS.read();
    let collections: Vec<(String, String)> = ws.collections.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
    let cid = r.collection_id.clone().or_else(|| collections.first().map(|c| c.0.clone()));
    let folders = cid.as_deref().and_then(|c| ws.collection(c)).map(|c| folder_paths(&c.items, "")).unwrap_or_default();
    let queue = actions::runner_queue(&ws, cid.as_deref(), r.folder_id.as_deref());
    let env = ws.active_env().map(|e| e.name.clone()).unwrap_or_else(|| "No environment".into());
    drop(ws);
    if r.collection_id.is_none() && cid.is_some() {
        RUNNER.write().collection_id = cid.clone();
    }
    let passed: usize = r.results.iter().map(|x| x.tests.iter().filter(|t| t.passed).count()).sum();
    let failed: usize = r.results.iter().map(|x| x.tests.iter().filter(|t| !t.passed).count()).sum();
    let errors = r.results.iter().filter(|x| x.error.is_some()).count();
    let elapsed = r.finished.unwrap_or_else(platform::now_secs) - r.started;
    let (it, its, idx, tot) = r.progress;
    let pct = if tot == 0 { 0.0 } else { (((it.saturating_sub(1)) * tot + idx) as f64 / (its * tot) as f64 * 100.0).min(100.0) };
    let running = r.running;
    let run_label = format!("Run {} request{}", queue.len(), if queue.len() == 1 { "" } else { "s" });
    let req_label = if r.results.is_empty() { "Requests".to_string() } else { format!("Requests · {elapsed:.1}s") };
    let empty_title = if running { "Starting…" } else { "Ready to run" };
    rsx! {
        Frame { title: "Collection Runner", description: "Run every request in a collection or folder, in order, with its tests.", size: "xl", icon: rsx! { icons::Play { size: 17 } },
            div { class: "runner",
                div { class: "runner-config",
                    div { class: "field", label { "Collection" }
                        select { class: "select-native input", disabled: running, onchange: move |e| { let mut w = RUNNER.write(); w.collection_id = Some(e.value()); w.folder_id = None; },
                            for (id , n) in collections { option { key: "{id}", value: "{id}", selected: Some(&id) == cid.as_ref(), "{n}" } }
                        }
                    }
                    if !folders.is_empty() {
                        div { class: "field", label { "Folder" }
                            select { class: "select-native input", disabled: running, onchange: move |e| { let v = e.value(); RUNNER.write().folder_id = if v.is_empty() { None } else { Some(v) }; },
                                option { value: "", "(entire collection)" }
                                for (id , p) in folders { option { key: "{id}", value: "{id}", selected: r.folder_id.as_ref() == Some(&id), "{p}" } }
                            }
                        }
                    }
                    div { class: "row",
                        div { class: "field grow", label { "Iterations" } input { class: "input", r#type: "number", min: "1", disabled: running, value: "{r.iterations}", oninput: move |e| RUNNER.write().iterations = e.value().parse().unwrap_or(1).max(1) } }
                        div { class: "field grow", label { "Delay (ms)" } input { class: "input", r#type: "number", min: "0", disabled: running, value: "{r.delay_ms}", oninput: move |e| RUNNER.write().delay_ms = e.value().parse().unwrap_or(0) } }
                    }
                    label { class: "row small", style: "gap: 8px",
                        input { r#type: "checkbox", class: "check", checked: r.stop_on_failure, disabled: running, onchange: move |_| { let v = !RUNNER.read().stop_on_failure; RUNNER.write().stop_on_failure = v; } }
                        "Stop on first failure"
                    }
                    div { class: "callout", icons::Layers { size: 14 } span { "Environment: " b { "{env}" } } }
                    div { class: "section-label", "Run order ({queue.len()})" }
                    div { class: "col", style: "gap: 2px; max-height: 200px; overflow: auto",
                        for q in queue.iter() {
                            div { key: "{q.id}", class: "row small", span { class: "{method_class(q.method)}", style: "width: 42px; text-align: right", "{method_short(q.method)}" } span { class: "truncate", "{q.name}" } }
                        }
                    }
                    if running {
                        button { class: "btn", onclick: move |_| actions::stop_run(), icons::Square { size: 14 } "Stop" }
                    } else {
                        button { class: "btn accent", disabled: queue.is_empty(), onclick: move |_| actions::start_run(), icons::Play { size: 14 } "{run_label}" }
                    }
                }
                div { class: "col", style: "min-height: 0",
                    div { class: "run-summary",
                        div { class: "stat", div { class: "v ok", "{passed}" } div { class: "l", "Tests passed" } }
                        div { class: "stat", div { class: "v bad", "{failed}" } div { class: "l", "Tests failed" } }
                        div { class: "stat", div { class: if errors > 0 { "v warn" } else { "v" }, "{errors}" } div { class: "l", "Errors" } }
                        div { class: "stat", div { class: "v", "{r.results.len()}" } div { class: "l", "{req_label}" } }
                    }
                    if running {
                        div { class: "row small muted", span { "Iteration {it}/{its} · request {idx}/{tot}" } }
                        div { class: "progress", div { style: "width: {pct}%" } }
                    }
                    div { class: "runner-results", style: "flex: 1",
                        if r.results.is_empty() {
                            Empty { title: empty_title, text: "Results for each request and its tests will stream in here.", icon: rsx! { icons::ListChecks { size: 22 } } }
                        }
                        for (i , res) in r.results.iter().enumerate() {
                            div { key: "{i}", class: "run-item",
                                div { class: "row",
                                    if res.ok() { icons::CircleCheck { size: 15, color: "var(--success)" } } else { icons::CircleX { size: 15, color: "var(--danger)" } }
                                    span { class: "{method_class(res.method)}", "{method_short(res.method)}" }
                                    span { class: "strong truncate grow", "{res.name}" }
                                    if r.iterations > 1 { span { class: "badge", "#{res.iteration}" } }
                                    if let Some(s) = res.status { span { class: "{crate::ui::common::status_class(s)}", "{s}" } }
                                    if let Some(t) = res.time_ms { span { class: "small muted", "{human_time(t)}" } }
                                }
                                div { class: "xs subtle mono truncate", style: "margin: 2px 0 0 22px", "{res.url}" }
                                if let Some(e) = &res.error { div { class: "small bad", style: "margin-left: 22px", "{e}" } }
                                if !res.tests.is_empty() {
                                    div { class: "tests",
                                        for (j , t) in res.tests.iter().enumerate() {
                                            div { key: "{j}", class: "row",
                                                if t.passed { span { class: "ok", "✓" } } else { span { class: "bad", "✕" } }
                                                span { "{t.name}" }
                                                if let Some(e) = &t.error { span { class: "subtle", "— {e}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !r.results.is_empty() && !running {
                        div { class: "row", style: "justify-content: flex-end",
                            button { class: "btn sm", onclick: move |_| actions::export_file("fetchly-run-results.json", &actions::run_results_json(&RUNNER.read())), icons::Download { size: 14 } "Export results" }
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ help

#[component]
fn ShortcutsDialog() -> Element {
    let cmd = platform::CMD;
    let rows = [
        (format!("{cmd} Enter"), "Send request"),
        (format!("{cmd} S"), "Save request"),
        (format!("{cmd} T"), "New tab"),
        (format!("{cmd} O"), "Import"),
        ("Enter (in URL bar)".to_string(), "Send request"),
        ("Double-click".to_string(), "Rename in sidebar"),
        ("Right-click".to_string(), "Sidebar item actions"),
        ("Drag & drop".to_string(), "Reorder / move requests, folders and collections"),
        ("Middle-click tab".to_string(), "Close tab"),
    ];
    rsx! {
        Frame { title: "Keyboard shortcuts", size: "sm", icon: rsx! { icons::Keyboard { size: 17 } },
            div { class: "dialog-body",
                table { class: "table",
                    tbody {
                        for (k , v) in rows {
                            tr { key: "{k}", td { span { class: "kbd", "{k}" } } td { "{v}" } }
                        }
                    }
                }
                div { class: "hint row", style: "gap: 6px",
                    "Fetchly v{env!(\"CARGO_PKG_VERSION\")} · Developed by"
                    a { href: crate::ui::chrome::AUTHOR_URL, target: "_blank", style: "color: var(--fg)", "Akshay Bhat" }
                }
            }
        }
    }
}

const SCRIPT_HELP: &str = r#"// Fetchly scripts are written in Rhai — a small, safe, JavaScript-like language.
// Closures use `|| expr` or `|| { statements }` instead of `function () {}`.

// ---- Variables -------------------------------------------------------------
pm.environment.set("key", "value");      // active environment
pm.environment.get("key");               // () if missing
pm.environment.unset("key");
pm.globals.set("key", "value");
pm.collectionVariables.set("key", "value");
pm.variables.get("key");                 // resolves local > env > collection > global
pm.variables.set("key", "value");        // local: this run only
pm.variables.replaceIn("Hello {{name}}");

// ---- Request (editable in pre-request scripts) -------------------------------
pm.request.url = pm.request.url + "?debug=1";
pm.request.method;                       // "GET"
pm.request.headers.upsert("X-Trace", uuid());
pm.request.headers.remove("X-Unwanted");
pm.request.body = to_json(#{ name: "fetchly" });

// ---- Response (test scripts) ---------------------------------------------
pm.response.code;                        // 200
pm.response.status;                      // "OK"
pm.response.responseTime;                // ms
pm.response.text();
pm.response.json();                      // maps/arrays: data.items[0].id
pm.response.headers.get("Content-Type");
pm.response.to_have_status(200);
pm.response.to_have_header("ETag");

// ---- Tests -----------------------------------------------------------------
pm.test("Status is 200", || pm.response.to_have_status(200));
pm.test("Has items", || {
    let data = pm.response.json();
    pm.expect(data.items).to_have_length(3);
    pm.expect(data.total).to_be_above(0);
    pm.expect(data.name).not.to_equal("");
});
pm.test("Simple boolean form", pm.response.code < 400);

// expect(...) assertions:
//   to_equal, to_eql, to_be_above, to_be_below, to_include, to_have_property,
//   to_be_true, to_be_false, to_exist, to_be_a("string"|"number"|...),
//   to_have_length, to_match("wild*card"), and `.not` to negate.

// ---- Collection runner ------------------------------------------------------
pm.execution.setNextRequest("Request name");   // jump
pm.execution.setNextRequest(());               // stop the run
pm.execution.skipRequest();                    // pre-request only

// ---- Utilities ---------------------------------------------------------------
console.log(value);  print(value);
to_json(value);  parse_json(text);
base64_encode(text);  base64_decode(text);
timestamp();  uuid();
"#;

#[component]
fn ScriptHelpDialog() -> Element {
    rsx! {
        Frame { title: "Scripting reference", description: "The pm API available in pre-request and test scripts.", size: "lg", icon: rsx! { icons::BookOpen { size: 17 } },
            div { class: "dialog-body", style: "border: 1px solid var(--border); border-radius: 8px; background: var(--bg-subtle)",
                CodeView { html: highlight::to_html(SCRIPT_HELP, Lang::Script) }
            }
        }
    }
}
