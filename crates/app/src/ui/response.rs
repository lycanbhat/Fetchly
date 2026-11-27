//! Response pane.

use crate::highlight::{self, human_size, human_time};
use crate::store::*;
use crate::ui::common::{status_class, CodeView, Empty};
use dioxus::prelude::*;
use lucide_dioxus as icons;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum RespTab {
    Body,
    Headers,
    Cookies,
    Tests,
    Request,
}

#[component]
pub fn ResponsePane() -> Element {
    let active = ACTIVE.read().clone();
    let busy = PENDING.read().contains_key(&active);
    let result = RESULTS.read().get(&active).cloned();
    rsx! {
        div { class: "response",
            if busy {
                div { class: "loading-bar" }
                Empty { title: "Sending request…", icon: rsx! { span { class: "spin", style: "display:grid", icons::LoaderCircle { size: 22 } } },
                    button { class: "btn sm", onclick: move |_| crate::actions::cancel(&active), icons::X { size: 14 } "Cancel" }
                }
            } else if let Some(r) = result {
                ResultViewPane { key: "{active}", result: r }
            } else {
                Empty { title: "No response yet", text: "Enter a URL and press Send to see the response here.", icon: rsx! { icons::Send { size: 22 } },
                    div { class: "row small muted", style: "gap: 14px; margin-top: 4px",
                        span { span { class: "kbd", "{crate::platform::CMD} ↵" } " send" }
                        span { span { class: "kbd", "{crate::platform::CMD} S" } " save" }
                        span { span { class: "kbd", "{crate::platform::CMD} T" } " new tab" }
                    }
                }
            }
        }
    }
}

#[component]
fn ResultViewPane(result: Rc<ResultView>) -> Element {
    let mut tab = use_signal(|| RespTab::Body);
    let mut pretty = use_signal(|| true);
    let mut search = use_signal(String::new);
    let wrap = SETTINGS.read().wrap_response;

    if let Some(err) = &result.error {
        let prepared = result.prepared.clone();
        let req_text = prepared.as_ref().map(|p| format!("{} {}\n{}", p.method, p.url, p.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n"))).unwrap_or_default();
        return rsx! {
            div { class: "scroll", style: "flex:1",
                Empty { title: "Could not send request", text: "{err}", icon: rsx! { icons::CircleX { size: 22, color: "var(--danger)" } } }
                if prepared.is_some() {
                    div { style: "padding: 0 24px 24px",
                        div { class: "section-label", style: "margin-bottom:6px", "Request" }
                        pre { class: "code-view selectable", style: "border:1px solid var(--border); border-radius: 8px", "{req_text}" }
                    }
                }
            }
        };
    }
    let Some(resp) = result.response.clone() else { return rsx! {} };
    let cookies = resp.cookies();
    let n_tests = result.tests.len();
    let n_pass = result.tests.iter().filter(|t| t.passed).count();
    let t = tab();
    let tab_btn = |v: RespTab, label: String| {
        rsx! {
            button { class: if t == v { "utab on" } else { "utab" }, onclick: move |_| tab.set(v), "{label}" }
        }
    };
    let total = resp.size_bytes();
    let header_bytes = total - resp.body.len();

    rsx! {
        div { class: "resp-head",
            div { class: "utabs",
                {tab_btn(RespTab::Body, "Body".into())}
                {tab_btn(RespTab::Headers, format!("Headers ({})", resp.headers.len()))}
                {tab_btn(RespTab::Cookies, if cookies.is_empty() { "Cookies".into() } else { format!("Cookies ({})", cookies.len()) })}
                {tab_btn(RespTab::Tests, if n_tests > 0 { format!("Tests ({n_pass}/{n_tests})") } else { "Tests".into() })}
                {tab_btn(RespTab::Request, "Request".into())}
            }
            div { class: "resp-meta",
                span { class: "{status_class(resp.status)}", "{resp.status} {resp.status_text}" }
                span { class: "row", style: "gap:4px", icons::Clock { size: 13 } b { "{human_time(resp.time_ms)}" } }
                span { class: "row", style: "gap:4px", title: "Body {human_size(resp.body.len())} · Headers {human_size(header_bytes)}", icons::HardDrive { size: 13 } b { "{human_size(total)}" } }
            }
        }
        if let Some(e) = &result.script_error {
            div { class: "var-warn", style: "padding: 8px 14px; border-bottom: 1px solid var(--border)", icons::TriangleAlert { size: 13 } "{e}" }
        }
        match t {
            RespTab::Body => {
                let binary = result.kind == BodyKind::Binary;
                let text = if pretty() { &result.pretty } else { &result.raw };
                let q = search();
                let (html, hits) = if !q.is_empty() {
                    highlight::mark_matches(text, &q)
                } else if pretty() {
                    (result.pretty_html.clone(), 0)
                } else {
                    (highlight::escape(text), 0)
                };
                let hits_label = format!("{hits} match{}", if hits == 1 { "" } else { "es" });
                let r2 = result.clone();
                let r3 = result.clone();
                rsx! {
                    div { class: "resp-tools",
                        div { class: "seg",
                            button { class: if pretty() { "on" } else { "" }, onclick: move |_| pretty.set(true), "Pretty" }
                            button { class: if !pretty() { "on" } else { "" }, onclick: move |_| pretty.set(false), "Raw" }
                        }
                        span { class: "badge", "{result.kind.label()}" }
                        div { class: "search", style: "width: 220px",
                            icons::Search { size: 14 }
                            input { class: "input sm", placeholder: "Find in body", value: "{q}", oninput: move |e| search.set(e.value()) }
                        }
                        if !q.is_empty() { span { class: "small muted", "{hits_label}" } }
                        div { class: "spacer" }
                        button { class: if wrap { "icon-btn on" } else { "icon-btn" }, title: "Wrap lines", onclick: move |_| SETTINGS.write().wrap_response.toggle(), icons::TextAlignStart { size: 15 } }
                        button { class: "icon-btn", title: "Copy body", onclick: move |_| {
                            let text = if pretty() { r2.pretty.clone() } else { r2.raw.clone() };
                            spawn(async move {
                                let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&text).unwrap_or_default());
                                let _ = document::eval(&js).await;
                                toast("Copied to clipboard");
                            });
                        }, icons::Copy { size: 15 } }
                        button { class: "icon-btn", title: "Save response to file", onclick: move |_| {
                            if let Some(resp) = &r3.response {
                                match crate::platform::save_file(&format!("response.{}", r3.kind.ext()), &resp.body) {
                                    Ok(m) => toast(m),
                                    Err(e) if e != "Cancelled" => toast_error(e),
                                    _ => {}
                                }
                            }
                        }, icons::Download { size: 15 } }
                    }
                    div { class: "resp-body",
                        if binary {
                            Empty { title: "Binary response", text: "{human_size(resp.body.len())} — use the download button to save it.", icon: rsx! { icons::FileArchive { size: 22 } } }
                        } else if text.is_empty() {
                            Empty { title: "Empty body", text: "The server returned no content.", icon: rsx! { icons::FileText { size: 22 } } }
                        } else {
                            CodeView { html, wrap }
                        }
                    }
                }
            }
            RespTab::Headers => rsx! {
                div { class: "resp-body",
                    table { class: "table selectable",
                        thead { tr { th { "Name" } th { "Value" } } }
                        tbody {
                            for (i , (k , v)) in resp.headers.iter().enumerate() {
                                tr { key: "{i}", td { class: "k", "{k}" } td { class: "mono small", "{v}" } }
                            }
                        }
                    }
                }
            },
            RespTab::Cookies => rsx! {
                div { class: "resp-body",
                    if cookies.is_empty() {
                        Empty { title: "No cookies", text: "The server didn't set any cookies on this response.", icon: rsx! { icons::Cookie { size: 22 } } }
                    } else {
                        table { class: "table selectable",
                            thead { tr { th { "Name" } th { "Value" } th { "Domain" } th { "Path" } th { "Expires" } th { "HttpOnly" } th { "Secure" } } }
                            tbody {
                                for (i , c) in cookies.into_iter().enumerate() {
                                    tr { key: "{i}",
                                        td { class: "k", "{c.name}" } td { class: "mono small", "{c.value}" } td { "{c.domain}" } td { "{c.path}" }
                                        td { "{c.expires}" } td { if c.http_only { icons::Check { size: 14 } } } td { if c.secure { icons::Check { size: 14 } } }
                                    }
                                }
                            }
                        }
                        div { style: "padding: 10px 14px",
                            button { class: "btn sm", onclick: move |_| open_dialog(Dialog::Cookies), icons::Cookie { size: 14 } "Manage cookies" }
                        }
                    }
                }
            },
            RespTab::Tests => rsx! { TestResults { result: result.clone() } },
            RespTab::Request => {
                let p = result.prepared.clone().unwrap_or_default();
                let headers = p.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n");
                let body: String = p.body_text().chars().take(50_000).collect();
                let final_url = (resp.url != p.url && !resp.url.is_empty()).then(|| resp.url.clone());
                rsx! {
                    div { class: "resp-body",
                        div { style: "padding: 12px 14px 0",
                            if let Some(u) = final_url { div { class: "hint", "Final URL after redirects: {u}" } }
                        }
                        pre { class: "code-view selectable wrap", "{p.method} {p.url}\n{headers}" if !body.is_empty() { "\n\n{body}" } }
                    }
                }
            }
        }
    }
}

#[component]
fn TestResults(result: Rc<ResultView>) -> Element {
    let mut filter = use_signal(|| 0u8);
    if result.tests.is_empty() {
        return rsx! {
            Empty { title: "No tests", text: "Add tests in the request's Tests tab to check responses automatically.", icon: rsx! { icons::FlaskConical { size: 22 } },
                code { class: "mono small", "pm.test(\"Status is 200\", || pm.response.to_have_status(200));" }
            }
        };
    }
    let f = filter();
    let passed = result.tests.iter().filter(|t| t.passed).count();
    let failed = result.tests.len() - passed;
    rsx! {
        div { class: "resp-tools",
            div { class: "seg",
                button { class: if f == 0 { "on" } else { "" }, onclick: move |_| filter.set(0), "All ({result.tests.len()})" }
                button { class: if f == 1 { "on" } else { "" }, onclick: move |_| filter.set(1), "Passed ({passed})" }
                button { class: if f == 2 { "on" } else { "" }, onclick: move |_| filter.set(2), "Failed ({failed})" }
            }
        }
        div { class: "resp-body",
            for (i , t) in result.tests.iter().enumerate().filter(|(_, t)| f == 0 || (f == 1) == t.passed) {
                div { key: "{i}", class: "test-row",
                    if t.passed { span { class: "badge ok", icons::Check { size: 12 } "PASS" } } else { span { class: "badge bad", icons::X { size: 12 } "FAIL" } }
                    div { class: "col", style: "gap:0",
                        span { "{t.name}" }
                        if let Some(e) = &t.error { span { class: "err", "{e}" } }
                    }
                }
            }
        }
    }
}
