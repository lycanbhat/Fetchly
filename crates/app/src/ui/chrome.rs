//! Header, request tab bar, console and footer.

use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger};
use crate::platform;
use crate::store::*;
use crate::ui::common::{method_class, method_short};
use dioxus::prelude::*;
use fetchly_core::postman;
use lucide_dioxus as icons;

pub const AUTHOR_URL: &str = "https://akshaybhat.online";

#[component]
pub fn Header() -> Element {
    let ws = WS.read();
    let envs: Vec<(String, String)> = ws.environments.iter().map(|e| (e.id.clone(), e.name.clone())).collect();
    let active_env = ws.active_environment.clone().unwrap_or_default();
    drop(ws);
    let theme = SETTINGS.read().theme;
    let mut new_open = use_signal(|| false);
    rsx! {
        header { class: "header",
            div { class: "brand",
                img { class: "brand-mark on-dark", src: crate::brand::MARK_DARK, alt: "" }
                img { class: "brand-mark on-light", src: crate::brand::MARK_LIGHT, alt: "" }
                "Fetchly"
            }
            DropdownMenu {
                open: Some(new_open()),
                on_open_change: move |v| new_open.set(v),
                DropdownMenuTrigger { class: "btn sm", icons::Plus { size: 14 } "New" icons::ChevronDown { size: 13 } }
                DropdownMenuContent {
                    DropdownMenuItem::<u8> { value: 0, index: 0usize, on_select: move |_| { new_tab(); new_open.set(false) },
                        span { class: "menu-item", icons::FilePlus { size: 14 } "Request" span { class: "spacer" } span { class: "kbd", "{platform::CMD}T" } }
                    }
                    DropdownMenuItem::<u8> { value: 1, index: 1usize, on_select: move |_| { crate::ui::sidebar::new_collection(); new_open.set(false) },
                        span { class: "menu-item", icons::LibraryBig { size: 14 } "Collection" }
                    }
                    DropdownMenuItem::<u8> { value: 2, index: 2usize, on_select: move |_| { crate::ui::sidebar::new_environment(); new_open.set(false) },
                        span { class: "menu-item", icons::Layers { size: 14 } "Environment" }
                    }
                }
            }
            button { class: "btn sm ghost", onclick: move |_| open_dialog(Dialog::Import), icons::Import { size: 14 } "Import" }
            button { class: "btn sm ghost", onclick: move |_| open_dialog(Dialog::Runner), icons::Play { size: 14 } "Runner" }
            button { class: "btn sm ghost", onclick: move |_| open_dialog(Dialog::Cookies), icons::Cookie { size: 14 } "Cookies" }
            div { class: "spacer" }
            div { class: "row", style: "gap: 4px",
                icons::Layers { size: 14, color: "var(--fg-subtle)" }
                select {
                    class: "select-native input sm",
                    style: "width: 190px",
                    value: "{active_env}",
                    onchange: move |e| {
                        let v = e.value();
                        WS.write().active_environment = if v.is_empty() { None } else { Some(v) };
                    },
                    option { value: "", selected: active_env.is_empty(), "No environment" }
                    for (id , name) in envs {
                        option { key: "{id}", value: "{id}", selected: id == active_env, "{name}" }
                    }
                }
                button {
                    class: "icon-btn",
                    title: "Edit active environment",
                    onclick: move |_| {
                        let id = WS.read().active_environment.clone();
                        open_dialog(Dialog::Environment(id));
                    },
                    icons::Eye { size: 16 }
                }
            }
            div { class: "sep", style: "width:1px;height:20px;background:var(--border)" }
            button {
                class: "icon-btn",
                title: "Toggle theme",
                onclick: move |_| {
                    let next = match SETTINGS.read().theme {
                        ThemeChoice::System => ThemeChoice::Dark,
                        ThemeChoice::Dark => ThemeChoice::Light,
                        ThemeChoice::Light => ThemeChoice::System,
                    };
                    SETTINGS.write().theme = next;
                },
                match theme {
                    ThemeChoice::System => rsx! { icons::Monitor { size: 16 } },
                    ThemeChoice::Dark => rsx! { icons::Moon { size: 16 } },
                    ThemeChoice::Light => rsx! { icons::Sun { size: 16 } },
                }
            }
            button { class: "icon-btn", title: "Settings", onclick: move |_| open_dialog(Dialog::Settings), icons::Settings { size: 16 } }
        }
    }
}

#[component]
pub fn TabBar() -> Element {
    let tabs = TABS.read().clone();
    let active = ACTIVE.read().clone();
    let pending = PENDING.read().keys().cloned().collect::<Vec<_>>();
    rsx! {
        div { class: "tabbar",
            div { class: "tabbar-scroll",
                for t in tabs {
                    {
                        let on = t.id == active;
                        let dirty = is_dirty(&t);
                        let busy = pending.contains(&t.id);
                        let (id1, id2, id3) = (t.id.clone(), t.id.clone(), t.id.clone());
                        let mclass = method_class(t.request.method);
                        let short = method_short(t.request.method);
                        rsx! {
                            div {
                                key: "{t.id}",
                                class: if on { "rtab on" } else { "rtab" },
                                title: "{t.request.name}",
                                onclick: move |_| *ACTIVE.write() = id1.clone(),
                                onauxclick: move |e: PointerEvent| {
                                    if e.trigger_button() == Some(dioxus::html::input_data::MouseButton::Auxiliary) {
                                        close_tab(&id2, false);
                                    }
                                },
                                span { class: "{mclass}", "{short}" }
                                span { class: "name", "{t.request.name}" }
                                if busy {
                                    span { class: "spin", style: "display:grid", icons::LoaderCircle { size: 13 } }
                                } else if dirty {
                                    span { class: "dot", title: "Unsaved changes" }
                                }
                                button {
                                    class: "icon-btn sm close",
                                    title: "Close tab",
                                    onclick: move |e| {
                                        e.stop_propagation();
                                        close_tab(&id3, false);
                                    },
                                    icons::X { size: 13 }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "new",
                button { class: "icon-btn", title: "New tab ({platform::CMD}T)", onclick: move |_| new_tab(), icons::Plus { size: 16 } }
            }
        }
    }
}

#[component]
pub fn Console() -> Element {
    let entries = CONSOLE.read().clone();
    rsx! {
        div { class: "console",
            div { class: "console-head",
                icons::SquareTerminal { size: 14 }
                "Console"
                span { class: "badge", "{entries.len()}" }
                div { class: "spacer" }
                button { class: "btn ghost sm", onclick: move |_| CONSOLE.write().clear(), icons::Eraser { size: 13 } "Clear" }
                button { class: "icon-btn sm", title: "Close", onclick: move |_| UI.write().show_console = false, icons::X { size: 14 } }
            }
            div { class: "console-body selectable",
                if entries.is_empty() {
                    div { class: "console-line log", "Requests, script logs (console.log / print) and errors appear here." }
                }
                for (i , e) in entries.into_iter().enumerate() {
                    {
                        let class = match e.kind {
                            ConsoleKind::Request => "console-line request",
                            ConsoleKind::Log => "console-line log",
                            ConsoleKind::Error => "console-line error",
                        };
                        let t = e.time.format("%H:%M:%S").to_string();
                        rsx! {
                            if let Some(d) = e.details {
                                details { key: "{i}",
                                    summary { class, span { class: "t", "{t}" } span { "{e.text}" } }
                                    pre { "{d}" }
                                }
                            } else {
                                div { key: "{i}", class, span { class: "t", "{t}" } span { "{e.text}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Footer() -> Element {
    let errors = CONSOLE.read().iter().filter(|e| e.kind == ConsoleKind::Error).count();
    let show = UI.read().show_console;
    let s = SETTINGS.read();
    let mode = if s.use_proxy {
        "Proxy".to_string()
    } else if platform::IS_WEB {
        "Browser".to_string()
    } else {
        "Direct".to_string()
    };
    let mode_title = if s.use_proxy { format!("Requests are relayed through {}", s.proxy_endpoint) } else { "Requests are sent directly".to_string() };
    drop(s);
    rsx! {
        footer { class: "footer",
            button {
                class: if show { "btn ghost sm on" } else { "btn ghost sm" },
                style: "margin-left:-8px",
                onclick: move |_| UI.write().show_console.toggle(),
                icons::SquareTerminal { size: 13 }
                "Console"
                if errors > 0 { span { class: "badge bad", "{errors}" } }
            }
            div { class: "sep" }
            span { class: "row", style: "gap:5px", title: "{mode_title}", icons::Wifi { size: 12 } "{mode}" }
            div { class: "spacer" }
            button { class: "btn ghost sm", style: "height:22px", onclick: move |_| open_dialog(Dialog::Shortcuts), icons::Keyboard { size: 13 } "Shortcuts" }
            div { class: "sep" }
            span { "v{env!(\"CARGO_PKG_VERSION\")}" }
            div { class: "sep" }
            a {
                class: "credit",
                href: AUTHOR_URL,
                target: "_blank",
                rel: "noopener noreferrer",
                title: AUTHOR_URL,
                "Developed by "
                b { style: "font-weight:600; color: var(--fg)", "Akshay Bhat" }
                span { class: "heart", style: "display:grid", icons::ArrowUpRight { size: 12 } }
            }
        }
    }
}

#[component]
pub fn Toasts() -> Element {
    let toasts = TOASTS.read().clone();
    rsx! {
        div { class: "toasts",
            for t in toasts {
                div { key: "{t.id}", class: if t.error { "toast error" } else { "toast" },
                    if t.error { icons::CircleX { size: 16, color: "var(--danger)" } } else { icons::CircleCheck { size: 16, color: "var(--success)" } }
                    span { "{t.text}" }
                }
            }
        }
    }
}

/// Export the full workspace as a backup file.
pub fn export_workspace() {
    let content = postman::export_workspace(&WS.read());
    crate::actions::export_file("fetchly-workspace.json", &content);
}
