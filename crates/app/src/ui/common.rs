//! Small shared widgets: highlighted inputs, code editor, key/value tables.

use crate::highlight::{self, Lang};
use dioxus::prelude::*;
use fetchly_core::model::{HttpMethod, KeyValue};
use fetchly_core::vars::VarScope;
use lucide_dioxus as icons;

pub fn method_class(m: HttpMethod) -> String {
    format!("method m-{}", m.as_str())
}

pub fn method_short(m: HttpMethod) -> &'static str {
    match m {
        HttpMethod::Delete => "DEL",
        HttpMethod::Options => "OPT",
        other => other.as_str(),
    }
}

pub fn status_class(status: u16) -> String {
    format!("status-pill status-{}", (status / 100).clamp(2, 5))
}

/// Single-line input that highlights `{{variables}}` (green = defined, red = undefined).
#[component]
pub fn VarInput(
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] placeholder: String,
    #[props(default)] cell: bool,
    scope: ReadSignal<VarScope>,
    #[props(default)] onenter: Option<EventHandler<()>>,
) -> Element {
    let html = highlight::vars_html(&value, &scope.read());
    let class = if cell { "hl-input cell" } else { "hl-input" };
    rsx! {
        div { class,
            div { class: "hl", dangerous_inner_html: "{html}" }
            input {
                value: "{value}",
                placeholder: "{placeholder}",
                spellcheck: false,
                autocomplete: "off",
                oninput: move |e| oninput.call(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        if let Some(h) = &onenter {
                            h.call(());
                        }
                    }
                },
            }
        }
    }
}

/// Multi-line editor with syntax highlighting and line numbers.
#[component]
pub fn CodeEditor(
    value: String,
    oninput: EventHandler<String>,
    #[props(default)] script: bool,
    #[props(default = 10)] min_lines: usize,
    #[props(default)] placeholder: String,
) -> Element {
    let lang = if script { Lang::Script } else { Lang::Data };
    let html = highlight::to_html(&value, lang);
    let lines = value.split('\n').count().max(min_lines);
    let longest = value.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
    let gutter: String = (1..=value.split('\n').count()).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
    let height = lines * 20 + 22;
    rsx! {
        div { class: "code-wrap",
            div { class: "code-inner", style: "height: {height}px; width: max(100%, calc({longest}ch + 80px));",
                div { class: "code-gutter", "{gutter}" }
                pre { dangerous_inner_html: "{html}" }
                textarea {
                    value: "{value}",
                    spellcheck: false,
                    autocomplete: "off",
                    placeholder: "{placeholder}",
                    "wrap": "off",
                    oninput: move |e| oninput.call(e.value()),
                }
            }
        }
    }
}

/// Read-only highlighted/escaped HTML view.
#[component]
pub fn CodeView(html: String, #[props(default)] wrap: bool) -> Element {
    let class = if wrap { "code-view selectable wrap" } else { "code-view selectable" };
    rsx! { pre { class, dangerous_inner_html: "{html}" } }
}

/// Editable key/value table with a trailing "new row", bulk-edit mode and optional descriptions.
#[component]
pub fn KvTable(
    rows: Vec<KeyValue>,
    onchange: EventHandler<Vec<KeyValue>>,
    #[props(default = true)] description: bool,
    #[props(default)] title: String,
    scope: Option<Signal<VarScope>>,
) -> Element {
    let mut bulk = use_signal(|| false);
    let mut bulk_text = use_signal(String::new);
    let n = rows.len();
    let render_rows = rows.clone();

    let set_field = {
        let rows = rows.clone();
        move |i: usize, f: u8, v: String| {
            let mut r = rows.clone();
            if i == r.len() {
                r.push(KeyValue::default());
            }
            match f {
                0 => r[i].key = v,
                1 => r[i].value = v,
                _ => r[i].description = v,
            }
            while r.last().is_some_and(|x| x.is_blank()) {
                r.pop();
            }
            onchange.call(r);
        }
    };

    rsx! {
        div {
            div { class: "kv-toolbar",
                if !title.is_empty() { span { class: "title", "{title}" } }
                div { class: "spacer" }
                button {
                    class: "btn ghost sm",
                    onclick: {
                        let rows = rows.clone();
                        move |_| {
                            if !bulk() {
                                let text = rows.iter().map(|r| format!("{}{}:{}", if r.enabled { "" } else { "//" }, r.key, r.value)).collect::<Vec<_>>().join("\n");
                                bulk_text.set(text);
                            }
                            bulk.toggle();
                        }
                    },
                    if bulk() { icons::Table { size: 14 } } else { icons::TextCursorInput { size: 14 } }
                    if bulk() { "Key-Value Edit" } else { "Bulk Edit" }
                }
            }
            if bulk() {
                textarea {
                    class: "input mono",
                    rows: 8,
                    spellcheck: false,
                    placeholder: "key:value — one per line, prefix with // to disable",
                    value: "{bulk_text}",
                    oninput: {
                        let rows = rows.clone();
                        move |e: FormEvent| {
                            let text = e.value();
                            bulk_text.set(text.clone());
                            let parsed: Vec<KeyValue> = text
                                .lines()
                                .filter(|l| !l.trim().is_empty())
                                .map(|l| {
                                    let (enabled, l) = match l.trim_start().strip_prefix("//") {
                                        Some(rest) => (false, rest),
                                        None => (true, l),
                                    };
                                    let (k, v) = l.split_once(':').unwrap_or((l, ""));
                                    let desc = rows.iter().find(|r| r.key == k.trim()).map(|r| r.description.clone()).unwrap_or_default();
                                    KeyValue { enabled, key: k.trim().to_string(), value: v.trim().to_string(), description: desc }
                                })
                                .collect();
                            onchange.call(parsed);
                        }
                    },
                }
            } else {
                table { class: "kv",
                    thead {
                        tr {
                            th { class: "c-check" }
                            th { "Key" }
                            th { "Value" }
                            if description { th { "Description" } }
                            th { class: "c-act" }
                        }
                    }
                    tbody {
                        for (i , row) in render_rows.iter().cloned().chain(std::iter::once(KeyValue::default())).enumerate() {
                            tr { key: "{i}", class: if !row.enabled && i < n { "disabled" },
                                td { class: "c-check",
                                    if i < n {
                                        input {
                                            r#type: "checkbox",
                                            class: "check",
                                            checked: row.enabled,
                                            onchange: {
                                                let rows = rows.clone();
                                                move |_| {
                                                    let mut r = rows.clone();
                                                    r[i].enabled = !r[i].enabled;
                                                    onchange.call(r);
                                                }
                                            },
                                        }
                                    }
                                }
                                td {
                                    if let Some(scope) = scope {
                                        VarInput { value: row.key.clone(), placeholder: if i == n { "Key" } else { "" }, cell: true, scope, oninput: { let f = set_field.clone(); move |v| f(i, 0, v) } }
                                    } else {
                                        input { class: "input", value: "{row.key}", placeholder: if i == n { "Key" } else { "" }, spellcheck: false, oninput: { let f = set_field.clone(); move |e: FormEvent| f(i, 0, e.value()) } }
                                    }
                                }
                                td {
                                    if let Some(scope) = scope {
                                        VarInput { value: row.value.clone(), placeholder: if i == n { "Value" } else { "" }, cell: true, scope, oninput: { let f = set_field.clone(); move |v| f(i, 1, v) } }
                                    } else {
                                        input { class: "input", value: "{row.value}", placeholder: if i == n { "Value" } else { "" }, spellcheck: false, oninput: { let f = set_field.clone(); move |e: FormEvent| f(i, 1, e.value()) } }
                                    }
                                }
                                if description {
                                    td {
                                        input { class: "input", value: "{row.description}", placeholder: if i == n { "Description" } else { "" }, oninput: { let f = set_field.clone(); move |e: FormEvent| f(i, 2, e.value()) } }
                                    }
                                }
                                td { class: "c-act",
                                    if i < n {
                                        button {
                                            class: "icon-btn sm danger",
                                            title: "Remove",
                                            onclick: {
                                                let rows = rows.clone();
                                                move |_| {
                                                    let mut r = rows.clone();
                                                    r.remove(i);
                                                    onchange.call(r);
                                                }
                                            },
                                            icons::Trash { size: 14 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A minimal empty-state block.
#[component]
pub fn Empty(title: String, #[props(default)] text: String, icon: Element, #[props(default)] children: Element) -> Element {
    rsx! {
        div { class: "empty",
            div { class: "empty-icon", {icon} }
            h3 { "{title}" }
            if !text.is_empty() { div { class: "small", "{text}" } }
            {children}
        }
    }
}
