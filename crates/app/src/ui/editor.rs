//! Request editor: name, URL bar, and the Params / Auth / Headers / Body / Scripts / Settings / Docs sections.

use crate::actions;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger};
use crate::components::switch::Switch;
use crate::platform;
use crate::store::*;
use crate::ui::common::{method_class, CodeEditor, KvTable, VarInput};
use dioxus::prelude::*;
use fetchly_core::model::*;
use fetchly_core::url::{params_from_url, url_from_params};
use fetchly_core::vars::{referenced_variables, VarScope};
use lucide_dioxus as icons;

/// Apply a change to the request in tab `id`.
fn edit(id: &str, f: impl FnOnce(&mut Request)) {
    if let Some(t) = TABS.write().iter_mut().find(|t| t.id == id) {
        f(&mut t.request);
    }
}

#[component]
pub fn RequestEditor() -> Element {
    let active = ACTIVE.read().clone();
    let Some(tab) = TABS.read().iter().find(|t| t.id == active).cloned() else { return rsx! {} };
    let cid = tab.origin.as_ref().map(|o| o.collection_id.clone());
    let scope_val = fetchly_core::pipeline::build_scope(&WS.read(), cid.as_deref());
    let mut scope = use_signal(VarScope::default);
    if scope.peek().globals != scope_val.globals || scope.peek().environment != scope_val.environment || scope.peek().collection != scope_val.collection {
        scope.set(scope_val.clone());
    }
    let breadcrumb = cid.as_deref().and_then(|c| WS.read().collection(c).map(|c| c.name.clone()));
    let dirty = is_dirty(&tab);
    let busy = PENDING.read().contains_key(&tab.id);
    let missing: Vec<String> = {
        let mut v: Vec<String> = referenced_variables(&tab.request.url).into_iter().filter(|n| scope_val.get(n).is_none()).collect();
        v.dedup();
        v
    };
    let missing_label = format!("Undefined variable{}: ", if missing.len() > 1 { "s" } else { "" });
    let missing_list = missing.iter().map(|m| format!("{{{{{m}}}}}")).collect::<Vec<_>>().join(", ");
    let id = tab.id.clone();
    let r = tab.request.clone();
    let mut method_open = use_signal(|| false);

    rsx! {
        div { class: "editor-head",
            div { class: "crumbs",
                if let Some(b) = breadcrumb {
                    span { class: "path row", style: "gap:4px", icons::LibraryBig { size: 13 } "{b}" icons::ChevronRight { size: 13 } }
                }
                input {
                    class: "input ghost title grow",
                    value: "{r.name}",
                    oninput: { let id = id.clone(); move |e: FormEvent| edit(&id, |r| r.name = e.value()) },
                }
                button { class: "btn sm", title: "Generate code", onclick: move |_| open_dialog(Dialog::Code), icons::CodeXml { size: 14 } "Code" }
                button {
                    class: if dirty { "btn sm primary" } else { "btn sm" },
                    title: "Save ({platform::CMD}S)",
                    onclick: { let id = id.clone(); move |_| save_tab(&id) },
                    icons::Save { size: 14 }
                    "Save"
                }
            }
            div { class: "urlbar",
                DropdownMenu {
                    open: Some(method_open()),
                    on_open_change: move |v| method_open.set(v),
                    DropdownMenuTrigger { class: "method-trigger {method_class(r.method)}",
                        "{r.method.as_str()}"
                        icons::ChevronDown { size: 14 }
                    }
                    DropdownMenuContent { style: "min-width: 140px",
                        for (i , m) in HttpMethod::ALL.into_iter().enumerate() {
                            DropdownMenuItem::<String> {
                                value: m.as_str().to_string(),
                                index: i,
                                on_select: { let id = id.clone(); move |_| { edit(&id, |r| r.method = m); method_open.set(false); } },
                                span { class: "{method_class(m)}", style: "font-size:12px", "{m.as_str()}" }
                            }
                        }
                    }
                }
                VarInput {
                    value: r.url.clone(),
                    placeholder: "Enter a URL or paste a cURL command",
                    scope,
                    oninput: {
                        let id = id.clone();
                        let origin = tab.origin.is_some();
                        move |v: String| {
                            let trimmed = v.trim_start().to_string();
                            if trimmed.starts_with("curl ") {
                                if let Ok(mut parsed) = fetchly_core::curl::parse_curl(&trimmed) {
                                    edit(&id, |r| {
                                        parsed.id = r.id.clone();
                                        if origin {
                                            parsed.name = r.name.clone();
                                        }
                                        *r = parsed;
                                    });
                                    toast("Imported cURL command");
                                    return;
                                }
                            }
                            edit(&id, |r| {
                                r.params = params_from_url(&v, &r.params);
                                r.url = v;
                            });
                        }
                    },
                    onenter: { let id = id.clone(); move |_| actions::send(id.clone()) },
                }
                if busy {
                    button { class: "btn send", onclick: { let id = id.clone(); move |_| actions::cancel(&id) }, icons::X { size: 15 } "Cancel" }
                } else {
                    button { class: "btn accent send", title: "Send ({platform::CMD}↵)", onclick: { let id = id.clone(); move |_| actions::send(id.clone()) }, icons::Send { size: 15 } "Send" }
                }
            }
            if !missing.is_empty() {
                div { class: "var-warn",
                    icons::TriangleAlert { size: 13 }
                    "{missing_label}"
                    span { class: "mono", "{missing_list}" }
                }
            }
        }
        SectionTabs { tab_id: id.clone(), section: tab.section, request: r.clone() }
        div { class: "editor-body",
            match tab.section {
                ReqSection::Params => rsx! {
                    KvTable {
                        title: "Query Params",
                        rows: r.params.clone(),
                        scope,
                        onchange: { let id = id.clone(); move |rows: Vec<KeyValue>| edit(&id, |r| { r.url = url_from_params(&r.url, &rows); r.params = rows; }) },
                    }
                },
                ReqSection::Auth => rsx! {
                    AuthEditor { auth: r.auth.clone(), allow_inherit: true, scope, onchange: { let id = id.clone(); move |a| edit(&id, |r| r.auth = a) } }
                },
                ReqSection::Headers => rsx! {
                    KvTable { title: "Headers", rows: r.headers.clone(), scope, onchange: { let id = id.clone(); move |rows| edit(&id, |r| r.headers = rows) } }
                    div { class: "row", style: "margin-top: 10px; flex-wrap: wrap",
                        span { class: "hint", "Presets:" }
                        for (k , v) in [("Content-Type", "application/json"), ("Accept", "application/json"), ("Cache-Control", "no-cache"), ("Authorization", "Bearer {{token}}")] {
                            button { class: "btn ghost sm mono", style: "font-size:11.5px", onclick: { let id = id.clone(); move |_| edit(&id, |r| set_var(&mut r.headers, k, v)) }, "{k}: {v}" }
                        }
                    }
                    div { class: "hint", style: "margin-top: 6px", "Generated automatically unless you set them: User-Agent, Accept, Content-Type (from the body), Authorization (from Auth), Cookie (from the cookie jar)." }
                },
                ReqSection::Body => rsx! { BodyEditor { tab_id: id.clone(), body: r.body.clone(), scope } },
                ReqSection::PreRequest => rsx! { ScriptEditor { value: r.pre_request_script.clone(), tests: false, onchange: { let id = id.clone(); move |s| edit(&id, |r| r.pre_request_script = s) } } },
                ReqSection::Tests => rsx! { ScriptEditor { value: r.test_script.clone(), tests: true, onchange: { let id = id.clone(); move |s| edit(&id, |r| r.test_script = s) } } },
                ReqSection::Settings => rsx! { RequestSettingsPane { tab_id: id.clone(), settings: r.settings.clone() } },
                ReqSection::Docs => rsx! {
                    div { class: "field",
                        label { "Description" }
                        textarea { class: "input", rows: 10, placeholder: "Describe what this request does, its inputs and outputs…", value: "{r.description}",
                            oninput: { let id = id.clone(); move |e: FormEvent| edit(&id, |r| r.description = e.value()) } }
                    }
                },
            }
        }
    }
}

#[component]
fn SectionTabs(tab_id: String, section: ReqSection, request: Request) -> Element {
    let count = |rows: &[KeyValue]| rows.iter().filter(|r| r.enabled && !r.key.is_empty()).count();
    let params = count(&request.params);
    let headers = count(&request.headers);
    let has_body = request.body != Body::None;
    let has_pre = !request.pre_request_script.trim().is_empty();
    let has_tests = !request.test_script.trim().is_empty();
    let tabs: Vec<(ReqSection, &str, Option<String>, bool)> = vec![
        (ReqSection::Params, "Params", (params > 0).then(|| params.to_string()), false),
        (ReqSection::Auth, "Authorization", None, false),
        (ReqSection::Headers, "Headers", (headers > 0).then(|| headers.to_string()), false),
        (ReqSection::Body, "Body", None, has_body),
        (ReqSection::PreRequest, "Pre-request", None, has_pre),
        (ReqSection::Tests, "Tests", None, has_tests),
        (ReqSection::Settings, "Settings", None, false),
        (ReqSection::Docs, "Docs", None, false),
    ];
    rsx! {
        div { class: "utabs sections",
            for (s , label , count , dot) in tabs {
                button {
                    key: "{label}",
                    class: if s == section { "utab on" } else { "utab" },
                    onclick: { let id = tab_id.clone(); move |_| { if let Some(t) = TABS.write().iter_mut().find(|t| t.id == id) { t.section = s; } } },
                    "{label}"
                    if let Some(c) = count { span { class: "count", "{c}" } }
                    if dot { span { class: "dot", style: "width:6px;height:6px;background:var(--success)" } }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ auth

#[component]
pub fn AuthEditor(auth: Auth, allow_inherit: bool, scope: ReadSignal<VarScope>, onchange: EventHandler<Auth>) -> Element {
    let kinds: Vec<(&str, Auth)> = {
        let mut v = vec![];
        if allow_inherit {
            v.push(("inherit", Auth::Inherit));
        }
        v.extend([
            ("none", Auth::None),
            ("basic", Auth::Basic { username: String::new(), password: String::new() }),
            ("bearer", Auth::Bearer { token: String::new() }),
            ("apikey", Auth::ApiKey { key: String::new(), value: String::new(), location: ApiKeyLocation::Header }),
            ("oauth2", Auth::OAuth2 { access_token: String::new(), header_prefix: "Bearer".into() }),
        ]);
        v
    };
    let current = kinds.iter().find(|(_, a)| std::mem::discriminant(a) == std::mem::discriminant(&auth)).map(|(k, _)| *k).unwrap_or("none");
    let kinds2 = kinds.clone();
    let a = auth.clone();
    rsx! {
        div { class: "form-grid",
            label { "Auth type" }
            select {
                class: "select-native input",
                value: "{current}",
                onchange: move |e| {
                    if let Some((_, k)) = kinds2.iter().find(|(k, _)| *k == e.value()) {
                        onchange.call(k.clone());
                    }
                },
                for (k , kind) in kinds {
                    option { key: "{k}", value: "{k}", selected: k == current, "{kind.label()}" }
                }
            }
            match a {
                Auth::Inherit => rsx! { span {} div { class: "callout", icons::Info { size: 15 } "The Authorization header will come from the parent folder or collection." } },
                Auth::None => rsx! { span {} div { class: "callout", icons::Info { size: 15 } "This request does not use any authorization." } },
                Auth::Basic { username, password } => rsx! {
                    label { "Username" }
                    VarInput { value: username.clone(), scope, oninput: { let p = password.clone(); move |v| onchange.call(Auth::Basic { username: v, password: p.clone() }) } }
                    label { "Password" }
                    input { class: "input", r#type: "password", value: "{password}", oninput: { let u = username.clone(); move |e: FormEvent| onchange.call(Auth::Basic { username: u.clone(), password: e.value() }) } }
                },
                Auth::Bearer { token } => rsx! {
                    label { "Token" }
                    VarInput { value: token.clone(), placeholder: "Token or {{variable}}", scope, oninput: move |v| onchange.call(Auth::Bearer { token: v }) }
                },
                Auth::ApiKey { key, value, location } => rsx! {
                    label { "Key" }
                    VarInput { value: key.clone(), placeholder: "X-API-Key", scope, oninput: { let v2 = value.clone(); move |v| onchange.call(Auth::ApiKey { key: v, value: v2.clone(), location }) } }
                    label { "Value" }
                    VarInput { value: value.clone(), scope, oninput: { let k = key.clone(); move |v| onchange.call(Auth::ApiKey { key: k.clone(), value: v, location }) } }
                    label { "Add to" }
                    div { class: "seg",
                        button { class: if location == ApiKeyLocation::Header { "on" } else { "" }, onclick: { let (k, v) = (key.clone(), value.clone()); move |_| onchange.call(Auth::ApiKey { key: k.clone(), value: v.clone(), location: ApiKeyLocation::Header }) }, "Header" }
                        button { class: if location == ApiKeyLocation::Query { "on" } else { "" }, onclick: { let (k, v) = (key.clone(), value.clone()); move |_| onchange.call(Auth::ApiKey { key: k.clone(), value: v.clone(), location: ApiKeyLocation::Query }) }, "Query params" }
                    }
                },
                Auth::OAuth2 { access_token, header_prefix } => rsx! {
                    label { "Access token" }
                    VarInput { value: access_token.clone(), placeholder: "Token or {{variable}}", scope, oninput: { let p = header_prefix.clone(); move |v| onchange.call(Auth::OAuth2 { access_token: v, header_prefix: p.clone() }) } }
                    label { "Header prefix" }
                    input { class: "input", value: "{header_prefix}", oninput: { let t = access_token.clone(); move |e: FormEvent| onchange.call(Auth::OAuth2 { access_token: t.clone(), header_prefix: e.value() }) } }
                    span {}
                    div { class: "hint", "Tip: fetch a token with a request, then store it from its Tests script: pm.environment.set(\"token\", pm.response.json().access_token);" }
                },
            }
        }
    }
}

// ------------------------------------------------------------------ body

#[component]
fn BodyEditor(tab_id: String, body: Body, scope: Signal<VarScope>) -> Element {
    let current = body.kind_label();
    let kinds: [(&str, Body); 6] = [
        ("none", Body::None),
        ("form-data", Body::FormData(vec![])),
        ("x-www-form-urlencoded", Body::UrlEncoded(vec![])),
        ("raw", Body::Raw { language: RawLanguage::Json, text: String::new() }),
        ("binary", Body::Binary { path: String::new() }),
        ("GraphQL", Body::GraphQL { query: String::new(), variables: String::new() }),
    ];
    let id = tab_id.clone();
    let set = move |b: Body| edit(&id, |r| r.body = b);
    let set2 = set.clone();
    rsx! {
        div { class: "radio-row",
            for (label , blank) in kinds {
                label { key: "{label}", class: if label == current { "on" } else { "" },
                    input { r#type: "radio", name: "body-{tab_id}", checked: label == current,
                        onchange: { let set = set.clone(); move |_| set(blank.clone()) } }
                    "{label}"
                }
            }
            if let Body::Raw { language, text } = body.clone() {
                div { class: "spacer" }
                select {
                    class: "select-native input sm",
                    style: "width: 130px",
                    onchange: { let set = set.clone(); let text = text.clone(); move |e: FormEvent| {
                        let lang = RawLanguage::ALL.into_iter().find(|l| l.label() == e.value()).unwrap_or_default();
                        set(Body::Raw { language: lang, text: text.clone() });
                    }},
                    for l in RawLanguage::ALL {
                        option { key: "{l.label()}", value: "{l.label()}", selected: l == language, "{l.label()}" }
                    }
                }
                if language == RawLanguage::Json {
                    button { class: "btn sm", onclick: { let set = set.clone(); let text = text.clone(); move |_| {
                        if let Some(p) = crate::highlight::pretty_json(&text) { set(Body::Raw { language, text: p }); } else { toast_error("Body is not valid JSON"); }
                    }}, icons::WandSparkles { size: 14 } "Beautify" }
                }
            }
        }
        match body {
            Body::None => rsx! { div { class: "callout", icons::Info { size: 15 } "This request does not have a body." } },
            Body::Raw { language, text } => {
                let invalid = (language == RawLanguage::Json && !text.trim().is_empty())
                    .then(|| serde_json::from_str::<serde_json::Value>(&scope.read().resolve(&text)).err())
                    .flatten();
                rsx! {
                    CodeEditor { value: text.clone(), script: language == RawLanguage::JavaScript, min_lines: 12, oninput: move |t| set2(Body::Raw { language, text: t }) }
                    if let Some(e) = invalid {
                        div { class: "var-warn", style: "margin-top:8px", icons::TriangleAlert { size: 13 } "Invalid JSON: {e}" }
                    }
                }
            }
            Body::UrlEncoded(rows) => rsx! { KvTable { rows, scope, onchange: move |rows| set2(Body::UrlEncoded(rows)) } },
            Body::FormData(fields) => rsx! { FormDataTable { fields, scope, onchange: move |f| set2(Body::FormData(f)) } },
            Body::Binary { path } => rsx! {
                div { class: "row",
                    input { class: "input grow mono", placeholder: "/path/to/file", value: "{path}", oninput: { let set = set2.clone(); move |e: FormEvent| set(Body::Binary { path: e.value() }) } }
                    if platform::CAN_PICK_PATHS {
                        button { class: "btn", onclick: { let set = set2.clone(); move |_| if let Some(p) = platform::pick_any_path() { set(Body::Binary { path: p }) } }, icons::FolderOpen { size: 14 } "Choose file" }
                    }
                }
                if platform::IS_WEB { div { class: "hint", style: "margin-top:8px", "Binary file bodies are only available in the desktop app." } }
            },
            Body::GraphQL { query, variables } => rsx! {
                div { style: "display:grid; grid-template-columns: 3fr 2fr; gap: 12px",
                    div { class: "col",
                        span { class: "section-label", "Query" }
                        CodeEditor { value: query.clone(), script: true, min_lines: 14, oninput: { let set = set2.clone(); let v = variables.clone(); move |q| set(Body::GraphQL { query: q, variables: v.clone() }) } }
                    }
                    div { class: "col",
                        span { class: "section-label", "Variables (JSON)" }
                        CodeEditor { value: variables.clone(), min_lines: 14, oninput: { let set = set2.clone(); let q = query.clone(); move |v| set(Body::GraphQL { query: q.clone(), variables: v }) } }
                    }
                }
            },
        }
    }
}

#[component]
fn FormDataTable(fields: Vec<FormField>, scope: Signal<VarScope>, onchange: EventHandler<Vec<FormField>>) -> Element {
    let n = fields.len();
    let rows: Vec<FormField> = fields.iter().cloned().chain(std::iter::once(FormField { enabled: true, ..Default::default() })).collect();
    let update = {
        let fields = fields.clone();
        move |i: usize, f: Box<dyn FnOnce(&mut FormField)>| {
            let mut v = fields.clone();
            if i == v.len() {
                v.push(FormField { enabled: true, ..Default::default() });
            }
            f(&mut v[i]);
            while v.last().is_some_and(|x| x.key.is_empty() && x.value.is_empty()) {
                v.pop();
            }
            onchange.call(v);
        }
    };
    rsx! {
        table { class: "kv",
            thead { tr { th { class: "c-check" } th { "Key" } th { class: "c-kind", "Type" } th { "Value" } th { style: "width:140px", "Content-Type" } th { class: "c-act" } } }
            tbody {
                for (i , f) in rows.into_iter().enumerate() {
                    tr { key: "{i}", class: if !f.enabled && i < n { "disabled" },
                        td { class: "c-check",
                            if i < n {
                                input { r#type: "checkbox", class: "check", checked: f.enabled, onchange: { let u = update.clone(); move |_| u(i, Box::new(|f| f.enabled = !f.enabled)) } }
                            }
                        }
                        td { VarInput { value: f.key.clone(), cell: true, placeholder: if i == n { "Key" } else { "" }, scope, oninput: { let u = update.clone(); move |v: String| u(i, Box::new(move |f| f.key = v)) } } }
                        td { class: "c-kind",
                            select { class: "select-native input",
                                onchange: { let u = update.clone(); move |e: FormEvent| { let file = e.value() == "file"; u(i, Box::new(move |f| f.kind = if file { FormFieldKind::File } else { FormFieldKind::Text })) } },
                                option { value: "text", selected: f.kind == FormFieldKind::Text, "Text" }
                                option { value: "file", selected: f.kind == FormFieldKind::File, "File" }
                            }
                        }
                        td {
                            if f.kind == FormFieldKind::File {
                                div { class: "row", style: "gap:0",
                                    input { class: "input mono grow", placeholder: "file path", value: "{f.value}", oninput: { let u = update.clone(); move |e: FormEvent| { let v = e.value(); u(i, Box::new(move |f| f.value = v)) } } }
                                    if platform::CAN_PICK_PATHS {
                                        button { class: "icon-btn", title: "Choose file", onclick: { let u = update.clone(); move |_| if let Some(p) = platform::pick_any_path() { u(i, Box::new(move |f| f.value = p)) } }, icons::FolderOpen { size: 14 } }
                                    }
                                }
                            } else {
                                VarInput { value: f.value.clone(), cell: true, placeholder: if i == n { "Value" } else { "" }, scope, oninput: { let u = update.clone(); move |v: String| u(i, Box::new(move |f| f.value = v)) } }
                            }
                        }
                        td { input { class: "input", placeholder: "auto", value: "{f.content_type}", oninput: { let u = update.clone(); move |e: FormEvent| { let v = e.value(); u(i, Box::new(move |f| f.content_type = v)) } } } }
                        td { class: "c-act",
                            if i < n {
                                button { class: "icon-btn sm danger", onclick: { let fields = fields.clone(); move |_| { let mut v = fields.clone(); v.remove(i); onchange.call(v); } }, icons::Trash { size: 14 } }
                            }
                        }
                    }
                }
            }
        }
        if platform::IS_WEB && fields.iter().any(|f| f.kind == FormFieldKind::File) {
            div { class: "hint", style: "margin-top:8px", "File fields are only supported in the desktop app." }
        }
    }
}

// ------------------------------------------------------------------ scripts

const PRE_SNIPPETS: &[(&str, &str)] = &[
    ("Set an environment variable", "pm.environment.set(\"variable_key\", \"variable_value\");"),
    ("Set a global variable", "pm.globals.set(\"variable_key\", \"variable_value\");"),
    ("Set a collection variable", "pm.collectionVariables.set(\"variable_key\", \"variable_value\");"),
    ("Get an environment variable", "let value = pm.environment.get(\"variable_key\");"),
    ("Add a request header", "pm.request.headers.upsert(\"X-Request-Id\", uuid());"),
    ("Timestamp variable", "pm.variables.set(\"now\", timestamp());"),
    ("Log to console", "console.log(pm.request.url);"),
    ("Skip this request", "pm.execution.skipRequest();"),
];

const TEST_SNIPPETS: &[(&str, &str)] = &[
    ("Status code is 200", "pm.test(\"Status code is 200\", || pm.response.to_have_status(200));"),
    ("Response time below 200ms", "pm.test(\"Response time is less than 200ms\", || pm.expect(pm.response.responseTime).to_be_below(200));"),
    ("Body contains string", "pm.test(\"Body contains string\", || pm.expect(pm.response.text()).to_include(\"string_you_want_to_search\"));"),
    ("JSON value check", "pm.test(\"JSON value check\", || {\n    let data = pm.response.json();\n    pm.expect(data.value).to_equal(100);\n});"),
    ("Content-Type header present", "pm.test(\"Content-Type is present\", || pm.response.to_have_header(\"Content-Type\"));"),
    ("Status code is one of", "pm.test(\"Successful POST\", || pm.expect([201, 202]).to_include(pm.response.code));"),
    ("Save JSON field to env", "pm.environment.set(\"token\", pm.response.json().token);"),
    ("Set next request (runner)", "pm.execution.setNextRequest(\"Request name\");"),
    ("Log response", "console.log(pm.response.json());"),
];

#[component]
pub fn ScriptEditor(value: String, tests: bool, onchange: EventHandler<String>) -> Element {
    let snippets = if tests { TEST_SNIPPETS } else { PRE_SNIPPETS };
    let v = value.clone();
    rsx! {
        div { class: "script-layout",
            div { class: "col",
                div { class: "hint",
                    if tests { "Runs after the response arrives. " } else { "Runs before the request is sent. " }
                    "Scripts use Rhai, a JavaScript-like language, with a pm API. "
                    button { class: "btn ghost sm", style: "height:20px;padding:0 4px", onclick: move |_| open_dialog(Dialog::ScriptHelp), "Reference" icons::ArrowUpRight { size: 12 } }
                }
                CodeEditor { value: value.clone(), script: true, min_lines: 16, oninput: move |s| onchange.call(s) }
            }
            div { class: "snippets",
                div { class: "section-label", style: "padding: 4px 8px", "Snippets" }
                for (label , code) in snippets.iter() {
                    button {
                        key: "{label}",
                        title: "{code}",
                        onclick: {
                            let v = v.clone();
                            move |_| {
                                let mut s = v.clone();
                                if !s.is_empty() && !s.ends_with('\n') {
                                    s.push('\n');
                                }
                                s.push_str(code);
                                s.push('\n');
                                onchange.call(s);
                            }
                        },
                        icons::Plus { size: 13 }
                        "{label}"
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ settings

#[component]
fn RequestSettingsPane(tab_id: String, settings: RequestSettings) -> Element {
    let (a, b, c) = (tab_id.clone(), tab_id.clone(), tab_id.clone());
    rsx! {
        div { class: "col", style: "gap: 14px; max-width: 640px",
            SettingRow { title: "Follow redirects", text: "Automatically follow HTTP 3xx responses.",
                Switch { checked: Some(settings.follow_redirects), on_checked_change: move |v| edit(&a, |r| r.settings.follow_redirects = v) }
            }
            SettingRow { title: "Verify SSL certificates", text: if platform::IS_WEB { "Applies when requests go through the Fetchly proxy." } else { "Turn off to call servers with self-signed certificates." },
                Switch { checked: Some(settings.verify_ssl), on_checked_change: move |v| edit(&b, |r| r.settings.verify_ssl = v) }
            }
            SettingRow { title: "Request timeout", text: "Milliseconds. 0 means no timeout.",
                input { class: "input", style: "width: 120px", r#type: "number", min: "0", value: "{settings.timeout_ms}",
                    oninput: move |e| edit(&c, |r| r.settings.timeout_ms = e.value().parse().unwrap_or(0)) }
            }
        }
    }
}

#[component]
pub fn SettingRow(title: String, text: String, children: Element) -> Element {
    rsx! {
        div { class: "row", style: "justify-content: space-between; gap: 24px; padding: 10px 0; border-bottom: 1px solid var(--border)",
            div { class: "col", style: "gap: 2px",
                span { style: "font-weight:500", "{title}" }
                span { class: "hint", "{text}" }
            }
            {children}
        }
    }
}
