//! Application state as Dioxus global signals, plus persistence.
//!
//! State is split into several signals so that, for example, typing in the request
//! editor (TABS) does not re-render the response viewer (RESULTS).

use dioxus::prelude::*;
use dioxus::core::Task;
use fetchly_core::cookies::{self, StoredCookie};
use fetchly_core::http::{PreparedRequest, ResponseData, Transport};
use fetchly_core::model::*;
use fetchly_core::script::TestResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

// ------------------------------------------------------------------ types

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub collection_id: String,
    pub request_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReqSection {
    #[default]
    Params,
    Auth,
    Headers,
    Body,
    PreRequest,
    Tests,
    Settings,
    Docs,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub request: Request,
    pub origin: Option<Origin>,
    #[serde(default)]
    pub section: ReqSection,
}

impl Tab {
    pub fn new(request: Request, origin: Option<Origin>) -> Self {
        Self { id: new_id(), request, origin, section: ReqSection::Params }
    }
    pub fn blank() -> Self {
        Self::new(Request { auth: Auth::Inherit, ..Default::default() }, None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub use_proxy: bool,
    pub proxy_endpoint: String,
    #[serde(default)]
    pub proxy_token: String,
    #[serde(default = "one")]
    pub zoom: f32,
    pub wrap_response: bool,
    pub save_history: bool,
    #[serde(default = "default_sidebar")]
    pub sidebar_width: f32,
}

fn one() -> f32 {
    1.0
}
fn default_sidebar() -> f32 {
    290.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            use_proxy: crate::platform::IS_WEB,
            proxy_endpoint: crate::platform::default_proxy_endpoint(),
            proxy_token: String::new(),
            zoom: 1.0,
            wrap_response: true,
            save_history: true,
            sidebar_width: default_sidebar(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    Xml,
    Html,
    Text,
    Binary,
}

impl BodyKind {
    pub fn label(&self) -> &'static str {
        match self {
            BodyKind::Json => "JSON",
            BodyKind::Xml => "XML",
            BodyKind::Html => "HTML",
            BodyKind::Text => "Text",
            BodyKind::Binary => "Binary",
        }
    }
    pub fn ext(&self) -> &'static str {
        match self {
            BodyKind::Json => "json",
            BodyKind::Xml => "xml",
            BodyKind::Html => "html",
            BodyKind::Text => "txt",
            BodyKind::Binary => "bin",
        }
    }
}

/// Everything shown in the response pane after a send.
#[derive(Debug)]
pub struct ResultView {
    pub response: Option<ResponseData>,
    pub error: Option<String>,
    pub prepared: Option<PreparedRequest>,
    pub kind: BodyKind,
    pub pretty: String,
    pub raw: String,
    /// Pre-rendered, escaped, syntax-highlighted HTML of `pretty`.
    pub pretty_html: String,
    pub tests: Vec<TestResult>,
    pub script_error: Option<String>,
}

impl PartialEq for ResultView {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl ResultView {
    pub fn error(msg: String, prepared: Option<PreparedRequest>) -> Self {
        Self {
            response: None,
            error: Some(msg),
            prepared,
            kind: BodyKind::Text,
            pretty: String::new(),
            raw: String::new(),
            pretty_html: String::new(),
            tests: vec![],
            script_error: None,
        }
    }

    pub fn from_response(resp: ResponseData, prepared: PreparedRequest) -> Self {
        let ct = resp.header("content-type").unwrap_or("").to_ascii_lowercase();
        let is_text = std::str::from_utf8(&resp.body).is_ok();
        let raw = if is_text { resp.body_text() } else { String::new() };
        let trimmed = raw.trim_start();
        let kind = if !is_text {
            BodyKind::Binary
        } else if ct.contains("json") || ((trimmed.starts_with('{') || trimmed.starts_with('[')) && serde_json::from_str::<serde_json::Value>(&raw).is_ok()) {
            BodyKind::Json
        } else if ct.contains("html") {
            BodyKind::Html
        } else if ct.contains("xml") || trimmed.starts_with("<?xml") {
            BodyKind::Xml
        } else {
            BodyKind::Text
        };
        let big = raw.len() > 4 * 1024 * 1024;
        let pretty = if big {
            raw.clone()
        } else {
            match kind {
                BodyKind::Json => crate::highlight::pretty_json(&raw).unwrap_or_else(|| raw.clone()),
                BodyKind::Xml | BodyKind::Html => crate::highlight::pretty_xml(&raw),
                _ => raw.clone(),
            }
        };
        let pretty_html = if pretty.len() <= 1024 * 1024 && matches!(kind, BodyKind::Json | BodyKind::Xml | BodyKind::Html) {
            crate::highlight::to_html(&pretty, crate::highlight::Lang::Data)
        } else {
            crate::highlight::escape(&pretty)
        };
        Self { response: Some(resp), error: None, prepared: Some(prepared), kind, pretty, raw, pretty_html, tests: vec![], script_error: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMode {
    Collections,
    Environments,
    History,
}

/// A thing being dragged in the sidebar tree.
#[derive(Clone, Debug, PartialEq)]
pub enum DragPayload {
    Collection(String),
    Item { cid: String, id: String },
}

/// Where the current drag would drop.
#[derive(Clone, Debug, PartialEq)]
pub struct DropHint {
    /// Collection id, or item id.
    pub target: String,
    pub place: DropPlace,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsTarget {
    Collection(String),
    Folder(String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Import,
    SaveRequest { tab_id: String },
    Code,
    ConfirmCloseTab { tab_id: String },
    ConfirmDelete { cid: String, item: Option<String>, name: String },
    ItemSettings(SettingsTarget),
    /// `None` = globals
    Environment(Option<String>),
    Settings,
    Cookies,
    Runner,
    Shortcuts,
    ScriptHelp,
}

pub struct UiState {
    pub sidebar_mode: SidebarMode,
    pub filter: String,
    /// Item / collection being renamed and its draft name.
    pub renaming: Option<(String, String)>,
    pub expanded: HashSet<String>,
    pub dialog: Option<Dialog>,
    pub show_console: bool,
    pub drag: Option<DragPayload>,
    pub drop_hint: Option<DropHint>,
    /// Row whose actions menu is open (sidebar).
    pub menu_open: Option<String>,
    /// Fraction of the main area used by the request editor.
    pub split: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleEntry {
    pub time: chrono::DateTime<chrono::Local>,
    pub kind: ConsoleKind,
    pub text: String,
    pub details: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleKind {
    Request,
    Log,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
}

// ------------------------------------------------------------------ signals

pub static WS: GlobalSignal<Workspace> = Signal::global(|| LOADED.with(|l| l.borrow_mut().take()).map(|p| p.workspace).unwrap_or_else(sample_workspace));
pub static TABS: GlobalSignal<Vec<Tab>> = Signal::global(|| {
    let t = STARTUP.with(|s| s.borrow().tabs.clone());
    if t.is_empty() { vec![Tab::blank()] } else { t }
});
pub static ACTIVE: GlobalSignal<String> = Signal::global(|| {
    let active = STARTUP.with(|s| s.borrow().active_tab);
    let tabs = TABS.peek();
    tabs.get(active).or(tabs.first()).map(|t| t.id.clone()).unwrap_or_default()
});
pub static SETTINGS: GlobalSignal<Settings> = Signal::global(|| STARTUP.with(|s| s.borrow().settings.clone()));
pub static RESULTS: GlobalSignal<HashMap<String, Rc<ResultView>>> = Signal::global(HashMap::new);
pub static PENDING: GlobalSignal<HashMap<String, Task>> = Signal::global(HashMap::new);
pub static UI: GlobalSignal<UiState> = Signal::global(|| UiState {
    sidebar_mode: SidebarMode::Collections,
    filter: String::new(),
    renaming: None,
    expanded: STARTUP.with(|s| s.borrow().expanded.clone()),
    dialog: None,
    show_console: false,
    drag: None,
    drop_hint: None,
    menu_open: None,
    split: 0.48,
});
pub static CONSOLE: GlobalSignal<Vec<ConsoleEntry>> = Signal::global(Vec::new);
pub static TOASTS: GlobalSignal<Vec<Toast>> = Signal::global(Vec::new);

// ------------------------------------------------------------------ persistence

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Persisted {
    pub workspace: Workspace,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub settings: Settings,
    #[serde(default)]
    pub expanded: HashSet<String>,
    #[serde(default)]
    pub cookies: Vec<StoredCookie>,
}

thread_local! {
    static LOADED: std::cell::RefCell<Option<Persisted>> = const { std::cell::RefCell::new(None) };
    static STARTUP: std::cell::RefCell<Persisted> = std::cell::RefCell::new(Persisted::default());
}

/// Load saved state before the UI starts. Returns true on first run.
pub fn init() -> bool {
    let loaded = crate::platform::load_state().and_then(|s| serde_json::from_str::<Persisted>(&s).ok());
    let first_run = loaded.is_none();
    let p = loaded.unwrap_or_default();
    {
        let mut jar = cookies::global().write().unwrap();
        jar.cookies = p.cookies.clone();
        jar.purge_expired();
    }
    STARTUP.with(|s| *s.borrow_mut() = p.clone());
    if !first_run {
        LOADED.with(|l| *l.borrow_mut() = Some(p));
    }
    first_run
}

pub fn snapshot() -> String {
    let tabs = TABS.read().clone();
    let active = ACTIVE.read().clone();
    let p = Persisted {
        workspace: WS.read().clone(),
        active_tab: tabs.iter().position(|t| t.id == active).unwrap_or(0),
        tabs,
        settings: SETTINGS.read().clone(),
        expanded: UI.read().expanded.clone(),
        cookies: cookies::global().read().unwrap().cookies.clone(),
    };
    serde_json::to_string(&p).unwrap_or_default()
}

// ------------------------------------------------------------------ helpers

static TOAST_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn toast(msg: impl Into<String>) {
    push_toast(msg.into(), false);
}

pub fn toast_error(msg: impl Into<String>) {
    push_toast(msg.into(), true);
}

fn push_toast(text: String, error: bool) {
    let id = TOAST_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    TOASTS.write().push(Toast { id, text, error });
    spawn(async move {
        crate::platform::sleep_ms(3500).await;
        TOASTS.write().retain(|t| t.id != id);
    });
}

pub fn log(kind: ConsoleKind, text: impl Into<String>, details: Option<String>) {
    let mut c = CONSOLE.write();
    c.push(ConsoleEntry { time: chrono::Local::now(), kind, text: text.into(), details });
    if c.len() > 1000 {
        c.drain(..200);
    }
}

pub fn open_dialog(d: Dialog) {
    UI.write().dialog = Some(d);
}

pub fn close_dialog() {
    UI.write().dialog = None;
}

pub fn active_index() -> usize {
    let a = ACTIVE.read().clone();
    TABS.read().iter().position(|t| t.id == a).unwrap_or(0)
}

pub fn saved_request(origin: &Origin) -> Option<Request> {
    WS.read().collection(&origin.collection_id)?.find_request(&origin.request_id).cloned()
}

pub fn is_dirty(tab: &Tab) -> bool {
    match &tab.origin {
        Some(o) => saved_request(o).as_ref() != Some(&tab.request),
        None => !tab.request.url.is_empty(),
    }
}

fn is_pristine(tab: &Tab) -> bool {
    tab.origin.is_none() && tab.request.url.is_empty() && !RESULTS.read().contains_key(&tab.id) && !PENDING.read().contains_key(&tab.id)
}

/// Open a tab, replacing the active one if it's an untouched blank tab.
pub fn push_tab(tab: Tab) {
    let id = tab.id.clone();
    let i = active_index();
    let replace = TABS.read().get(i).is_some_and(is_pristine);
    if replace {
        TABS.write()[i] = tab;
    } else {
        TABS.write().push(tab);
    }
    *ACTIVE.write() = id;
}

pub fn new_tab() {
    let t = Tab::blank();
    let id = t.id.clone();
    TABS.write().push(t);
    *ACTIVE.write() = id;
}

pub fn open_request(cid: &str, rid: &str) {
    let existing = TABS.read().iter().find(|t| t.origin.as_ref().is_some_and(|o| o.request_id == rid)).map(|t| t.id.clone());
    if let Some(id) = existing {
        *ACTIVE.write() = id;
        return;
    }
    let Some(req) = WS.read().collection(cid).and_then(|c| c.find_request(rid)).cloned() else { return };
    push_tab(Tab::new(req, Some(Origin { collection_id: cid.into(), request_id: rid.into() })));
}

pub fn close_tab(tab_id: &str, force: bool) {
    let Some(i) = TABS.read().iter().position(|t| t.id == tab_id) else { return };
    if !force && is_dirty(&TABS.read()[i]) {
        open_dialog(Dialog::ConfirmCloseTab { tab_id: tab_id.into() });
        return;
    }
    if let Some(task) = PENDING.write().remove(tab_id) {
        task.cancel();
    }
    RESULTS.write().remove(tab_id);
    TABS.write().remove(i);
    if TABS.read().is_empty() {
        TABS.write().push(Tab::blank());
    }
    if *ACTIVE.read() == tab_id {
        let n = TABS.read().len();
        let id = TABS.read()[i.min(n - 1)].id.clone();
        *ACTIVE.write() = id;
    }
}

/// Save a tab back to its collection, or ask where to save it.
pub fn save_tab(tab_id: &str) {
    let Some(tab) = TABS.read().iter().find(|t| t.id == tab_id).cloned() else { return };
    if let Some(o) = &tab.origin {
        let mut ws = WS.write();
        if let Some(slot) = ws.collection_mut(&o.collection_id).and_then(|c| c.find_request_mut(&o.request_id)) {
            *slot = tab.request.clone();
            drop(ws);
            toast("Request saved");
            return;
        }
    }
    open_dialog(Dialog::SaveRequest { tab_id: tab_id.into() });
}

/// Keep tabs pointing at the right collection after a move; detach tabs whose request is gone.
pub fn refresh_origins() {
    let ws = WS.read();
    let mut tabs = TABS.write();
    for t in tabs.iter_mut() {
        if let Some(o) = &mut t.origin {
            match ws.collections.iter().find(|c| c.find_request(&o.request_id).is_some()) {
                Some(c) => o.collection_id = c.id.clone(),
                None => t.origin = None,
            }
        }
    }
}

pub fn transport() -> Transport {
    let s = SETTINGS.read();
    if s.use_proxy {
        Transport::Proxy { endpoint: s.proxy_endpoint.clone(), token: s.proxy_token.clone() }
    } else {
        Transport::Direct
    }
}

/// A welcome collection so the app is useful on first launch.
pub fn sample_workspace() -> Workspace {
    let mut c = Collection::new("Sample APIs");
    c.description = "Example requests against public test APIs. Explore, send, and tweak them.".into();
    c.variables = vec![KeyValue::new("jsonplaceholder", "https://jsonplaceholder.typicode.com")];

    let mut posts = Folder::new("Posts");
    posts.items = vec![
        Item::Request(Request {
            name: "List posts".into(),
            url: "{{jsonplaceholder}}/posts?_limit=5".into(),
            params: vec![KeyValue::new("_limit", "5")],
            test_script: "pm.test(\"Status is 200\", || pm.response.to_have_status(200));\npm.test(\"Returns 5 posts\", || pm.expect(pm.response.json()).to_have_length(5));\npm.test(\"Fast response\", || pm.expect(pm.response.responseTime).to_be_below(2000));\n\n// Save the first post id for the next request\npm.collectionVariables.set(\"post_id\", pm.response.json()[0].id);".into(),
            ..Default::default()
        }),
        Item::Request(Request {
            name: "Get post".into(),
            url: "{{jsonplaceholder}}/posts/{{post_id}}".into(),
            test_script: "let post = pm.response.json();\npm.test(\"Has a title\", || pm.expect(post).to_have_property(\"title\"));\nconsole.log(post.title);".into(),
            ..Default::default()
        }),
        Item::Request(Request {
            name: "Create post".into(),
            method: HttpMethod::Post,
            url: "{{jsonplaceholder}}/posts".into(),
            body: Body::Raw { language: RawLanguage::Json, text: "{\n  \"title\": \"Hello from Fetchly\",\n  \"body\": \"Sent at {{$isoTimestamp}}\",\n  \"userId\": {{$randomInt}}\n}".into() },
            test_script: "pm.test(\"Created\", || pm.expect(pm.response.code).to_equal(201));".into(),
            ..Default::default()
        }),
    ];
    let mut misc = Folder::new("HTTP basics");
    misc.items = vec![
        Item::Request(Request { name: "Echo headers".into(), url: "https://httpbin.org/headers".into(), headers: vec![KeyValue::new("X-Request-Id", "{{$guid}}")], ..Default::default() }),
        Item::Request(Request { name: "Basic auth".into(), url: "https://httpbin.org/basic-auth/user/passwd".into(), auth: Auth::Basic { username: "user".into(), password: "passwd".into() }, ..Default::default() }),
        Item::Request(Request { name: "Set a cookie".into(), url: "https://httpbin.org/cookies/set?session=fetchly".into(), ..Default::default() }),
        Item::Request(Request {
            name: "Form upload".into(),
            method: HttpMethod::Post,
            url: "https://httpbin.org/post".into(),
            body: Body::FormData(vec![FormField { enabled: true, key: "greeting".into(), value: "hello".into(), ..Default::default() }]),
            ..Default::default()
        }),
        Item::Request(Request {
            name: "GraphQL query".into(),
            method: HttpMethod::Post,
            url: "https://countries.trevorblades.com/".into(),
            body: Body::GraphQL { query: "query ($code: ID!) {\n  country(code: $code) {\n    name\n    capital\n    emoji\n  }\n}".into(), variables: "{ \"code\": \"IN\" }".into() },
            ..Default::default()
        }),
    ];
    c.items = vec![Item::Folder(posts), Item::Folder(misc)];

    let mut env = Environment::new("Development");
    env.variables = vec![KeyValue::new("base_url", "http://localhost:3000"), KeyValue::new("token", "dev-token")];
    Workspace { collections: vec![c], environments: vec![env], globals: vec![], history: vec![], active_environment: None }
}
