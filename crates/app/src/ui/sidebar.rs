//! Left sidebar: collections tree (drag-to-reorder), environments, history.

use crate::actions;
use crate::components::dropdown_menu::{DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger};
use crate::store::*;
use crate::ui::common::{method_class, method_short, Empty};
use dioxus::prelude::*;
use fetchly_core::model::*;
use lucide_dioxus as icons;

#[component]
pub fn Sidebar() -> Element {
    let mode = UI.read().sidebar_mode;
    let width = SETTINGS.read().sidebar_width;
    let filter = UI.read().filter.clone();
    let seg = |m: SidebarMode| if mode == m { "on" } else { "" };
    rsx! {
        aside { class: "sidebar", style: "width: {width}px",
            div { class: "sb-top",
                div { class: "seg full",
                    button { class: seg(SidebarMode::Collections), onclick: move |_| UI.write().sidebar_mode = SidebarMode::Collections, icons::LibraryBig { size: 14 } "Collections" }
                    button { class: seg(SidebarMode::Environments), onclick: move |_| UI.write().sidebar_mode = SidebarMode::Environments, icons::Layers { size: 14 } "Envs" }
                    button { class: seg(SidebarMode::History), onclick: move |_| UI.write().sidebar_mode = SidebarMode::History, icons::RotateCcwClock { size: 14 } "History" }
                }
                div { class: "sb-tools",
                    div { class: "search grow",
                        icons::Search { size: 14 }
                        input { class: "input sm", placeholder: "Search", value: "{filter}", oninput: move |e| UI.write().filter = e.value() }
                    }
                    match mode {
                        SidebarMode::Collections => rsx! {
                            button { class: "btn sm", title: "New collection", onclick: move |_| new_collection(), icons::Plus { size: 14 } "New" }
                        },
                        SidebarMode::Environments => rsx! {
                            button { class: "btn sm", title: "New environment", onclick: move |_| new_environment(), icons::Plus { size: 14 } "New" }
                        },
                        SidebarMode::History => rsx! {
                            button { class: "btn sm ghost", title: "Clear history", onclick: move |_| WS.write().history.clear(), icons::Trash { size: 14 } "Clear" }
                        },
                    }
                }
            }
            match mode {
                SidebarMode::Collections => rsx! { CollectionsTree {} },
                SidebarMode::Environments => rsx! { Environments {} },
                SidebarMode::History => rsx! { HistoryList {} },
            }
        }
    }
}

pub fn new_collection() {
    let c = Collection::new("New Collection");
    UI.write().renaming = Some((c.id.clone(), c.name.clone()));
    UI.write().sidebar_mode = SidebarMode::Collections;
    WS.write().collections.push(c);
}

pub fn new_environment() {
    let e = Environment::new("New Environment");
    let id = e.id.clone();
    WS.write().environments.push(e);
    open_dialog(Dialog::Environment(Some(id)));
}

fn matches_filter(f: &str, item: &Item) -> bool {
    if f.is_empty() {
        return true;
    }
    match item {
        Item::Request(r) => r.name.to_lowercase().contains(f) || r.url.to_lowercase().contains(f),
        Item::Folder(fo) => fo.name.to_lowercase().contains(f) || fo.items.iter().any(|i| matches_filter(f, i)),
    }
}

// ------------------------------------------------------------------ menu actions

#[derive(Clone, Copy, PartialEq, Debug)]
enum Act {
    Open,
    AddRequest,
    AddFolder,
    Run,
    Edit,
    Rename,
    Duplicate,
    Export,
    Delete,
}

fn perform(act: Act, cid: &str, item: Option<&str>) {
    UI.write().menu_open = None;
    let ws = WS.read();
    let Some(c) = ws.collection(cid) else { return };
    let name = match item {
        Some(id) => c.find_item(id).map(|i| i.name().to_string()).unwrap_or_default(),
        None => c.name.clone(),
    };
    let is_folder = item.is_some_and(|id| c.folder(id).is_some());
    drop(ws);
    match act {
        Act::Open => {
            if let Some(id) = item {
                open_request(cid, id);
            }
        }
        Act::AddRequest => {
            let req = Request { name: "New Request".into(), auth: Auth::Inherit, ..Default::default() };
            let rid = req.id.clone();
            if let Some(list) = WS.write().collection_mut(cid).and_then(|c| c.container_mut(item)) {
                list.push(Item::Request(req));
            }
            expand(cid);
            if let Some(f) = item {
                expand(f);
            }
            open_request(cid, &rid);
            UI.write().renaming = Some((rid, "New Request".into()));
        }
        Act::AddFolder => {
            let f = Folder::new("New Folder");
            let fid = f.id.clone();
            if let Some(list) = WS.write().collection_mut(cid).and_then(|c| c.container_mut(item)) {
                list.push(Item::Folder(f));
            }
            expand(cid);
            if let Some(f) = item {
                expand(f);
            }
            UI.write().renaming = Some((fid, "New Folder".into()));
        }
        Act::Run => {
            {
                let mut r = actions::RUNNER.write();
                r.collection_id = Some(cid.into());
                r.folder_id = if is_folder { item.map(String::from) } else { None };
            }
            open_dialog(Dialog::Runner);
        }
        Act::Edit => open_dialog(Dialog::ItemSettings(match item {
            Some(f) => SettingsTarget::Folder(cid.into(), f.into()),
            None => SettingsTarget::Collection(cid.into()),
        })),
        Act::Rename => UI.write().renaming = Some((item.unwrap_or(cid).to_string(), name)),
        Act::Duplicate => match item {
            Some(id) => {
                if let Some(c) = WS.write().collection_mut(cid) {
                    c.duplicate_item(id);
                }
            }
            None => {
                let Some(c) = WS.read().collection(cid).cloned() else { return };
                let items = c.items.iter().map(|i| {
                    let mut d = i.duplicate();
                    d.set_name(i.name().into());
                    d
                });
                let copy = Collection { id: new_id(), name: format!("{} Copy", c.name), items: items.collect(), ..c };
                WS.write().collections.push(copy);
            }
        },
        Act::Export => actions::export_collection(cid),
        Act::Delete => open_dialog(Dialog::ConfirmDelete { cid: cid.into(), item: item.map(String::from), name }),
    }
}

fn expand(id: &str) {
    UI.write().expanded.insert(id.to_string());
}

fn menu_items(kind: RowKind) -> Vec<(Act, &'static str)> {
    match kind {
        RowKind::Collection => vec![
            (Act::AddRequest, "Add request"),
            (Act::AddFolder, "Add folder"),
            (Act::Run, "Run collection"),
            (Act::Edit, "Edit settings"),
            (Act::Rename, "Rename"),
            (Act::Duplicate, "Duplicate"),
            (Act::Export, "Export"),
            (Act::Delete, "Delete"),
        ],
        RowKind::Folder => vec![
            (Act::AddRequest, "Add request"),
            (Act::AddFolder, "Add folder"),
            (Act::Run, "Run folder"),
            (Act::Edit, "Edit settings"),
            (Act::Rename, "Rename"),
            (Act::Duplicate, "Duplicate"),
            (Act::Delete, "Delete"),
        ],
        RowKind::Request => vec![(Act::Open, "Open in tab"), (Act::Rename, "Rename"), (Act::Duplicate, "Duplicate"), (Act::Delete, "Delete")],
    }
}

fn act_icon(a: Act) -> Element {
    match a {
        Act::Open => rsx! { icons::SquareArrowOutUpRight { size: 14 } },
        Act::AddRequest => rsx! { icons::FilePlus { size: 14 } },
        Act::AddFolder => rsx! { icons::FolderPlus { size: 14 } },
        Act::Run => rsx! { icons::Play { size: 14 } },
        Act::Edit => rsx! { icons::Settings2 { size: 14 } },
        Act::Rename => rsx! { icons::Pencil { size: 14 } },
        Act::Duplicate => rsx! { icons::Copy { size: 14 } },
        Act::Export => rsx! { icons::Download { size: 14 } },
        Act::Delete => rsx! { icons::Trash { size: 14 } },
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum RowKind {
    Collection,
    Folder,
    Request,
}

/// "…" actions menu for a tree row; also opened by right-clicking the row.
#[component]
fn RowMenu(row_id: String, cid: String, item: Option<String>, kind: RowKind) -> Element {
    let open = UI.read().menu_open.as_deref() == Some(row_id.as_str());
    let rid = row_id.clone();
    let items = menu_items(kind);
    rsx! {
        div { class: "actions", "data-open": "{open}",
            onclick: move |e| e.stop_propagation(),
            DropdownMenu {
                open: Some(open),
                on_open_change: move |v: bool| UI.write().menu_open = if v { Some(rid.clone()) } else { None },
                DropdownMenuTrigger { class: "icon-btn sm", title: "More actions", icons::Ellipsis { size: 15 } }
                DropdownMenuContent {
                    for (i , (act , label)) in items.into_iter().enumerate() {
                        if act == Act::Delete { div { class: "menu-sep" } }
                        DropdownMenuItem::<String> {
                            value: label.to_string(),
                            index: i,
                            on_select: {
                                let (cid, item) = (cid.clone(), item.clone());
                                move |_| perform(act, &cid, item.as_deref())
                            },
                            span { class: if act == Act::Delete { "menu-item danger" } else { "menu-item" }, {act_icon(act)} "{label}" }
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ drag & drop

#[derive(Clone, PartialEq)]
enum DropTarget {
    Collection(String),
    Item { cid: String, id: String, folder: bool },
}

fn place_for(y: f64, target: &DropTarget, payload: &DragPayload) -> Option<DropPlace> {
    match (payload, target) {
        (DragPayload::Collection(_), DropTarget::Collection(_)) => Some(if y < 15.0 { DropPlace::Before } else { DropPlace::After }),
        (DragPayload::Collection(_), _) => None,
        (DragPayload::Item { .. }, DropTarget::Collection(_)) => Some(DropPlace::Inside),
        (DragPayload::Item { .. }, DropTarget::Item { folder: true, .. }) => Some(if y < 8.0 {
            DropPlace::Before
        } else if y > 22.0 {
            DropPlace::After
        } else {
            DropPlace::Inside
        }),
        (DragPayload::Item { .. }, DropTarget::Item { .. }) => Some(if y < 15.0 { DropPlace::Before } else { DropPlace::After }),
    }
}

fn target_id(t: &DropTarget) -> &str {
    match t {
        DropTarget::Collection(c) => c,
        DropTarget::Item { id, .. } => id,
    }
}

fn on_drag_over(e: &DragEvent, target: &DropTarget) {
    e.prevent_default();
    let Some(payload) = UI.read().drag.clone() else { return };
    let y = e.element_coordinates().y;
    let hint = place_for(y, target, &payload).map(|place| DropHint { target: target_id(target).to_string(), place });
    if UI.read().drop_hint != hint {
        UI.write().drop_hint = hint;
    }
}

fn on_drop(e: &DragEvent, target: &DropTarget) {
    e.prevent_default();
    let payload = UI.read().drag.clone();
    let hint = UI.read().drop_hint.clone();
    {
        let mut ui = UI.write();
        ui.drag = None;
        ui.drop_hint = None;
    }
    let (Some(payload), Some(hint)) = (payload, hint) else { return };
    if hint.target != target_id(target) {
        return;
    }
    let result = match (&payload, target) {
        (DragPayload::Collection(id), DropTarget::Collection(tid)) => {
            WS.write().move_collection(id, tid, hint.place);
            Ok(())
        }
        (DragPayload::Item { id, .. }, DropTarget::Collection(tcid)) => {
            let r = WS.write().move_item(id, tcid, None, DropPlace::Inside);
            expand(tcid);
            r
        }
        (DragPayload::Item { id, .. }, DropTarget::Item { cid, id: tid, .. }) => {
            let r = WS.write().move_item(id, cid, Some(tid), hint.place);
            if hint.place == DropPlace::Inside {
                expand(tid);
            }
            r
        }
        _ => Ok(()),
    };
    match result {
        Ok(()) => refresh_origins(),
        Err(e) => toast_error(e),
    }
}

fn row_class(base: &str, id: &str, selected: bool) -> String {
    let ui = UI.read();
    let mut c = format!("tree-row {base}");
    if selected {
        c.push_str(" selected");
    }
    let dragging = match &ui.drag {
        Some(DragPayload::Collection(x)) => x == id,
        Some(DragPayload::Item { id: x, .. }) => x == id,
        None => false,
    };
    if dragging {
        c.push_str(" dragging");
    }
    if let Some(h) = ui.drop_hint.as_ref().filter(|h| h.target == id) {
        c.push_str(match h.place {
            DropPlace::Before => " drop-before",
            DropPlace::After => " drop-after",
            DropPlace::Inside => " drop-inside",
        });
    }
    c
}

// ------------------------------------------------------------------ tree

#[component]
fn CollectionsTree() -> Element {
    let ws = WS.read();
    let filter = UI.read().filter.to_lowercase();
    let dragging = UI.read().drag.is_some();
    if ws.collections.is_empty() {
        return rsx! {
            Empty { title: "No collections yet", text: "Group your requests into collections to save and share them.", icon: rsx! { icons::LibraryBig { size: 22 } },
                div { class: "row",
                    button { class: "btn primary sm", onclick: move |_| new_collection(), icons::Plus { size: 14 } "Create collection" }
                    button { class: "btn sm", onclick: move |_| open_dialog(Dialog::Import), icons::Import { size: 14 } "Import" }
                }
            }
        };
    }
    let cols: Vec<Collection> = ws
        .collections
        .iter()
        .filter(|c| filter.is_empty() || c.name.to_lowercase().contains(&filter) || c.items.iter().any(|i| matches_filter(&filter, i)))
        .cloned()
        .collect();
    drop(ws);
    rsx! {
        div { class: if dragging { "tree dragging" } else { "tree" },
            for c in cols {
                CollectionNode { key: "{c.id}", collection: c, filter: filter.clone() }
            }
        }
    }
}

#[component]
fn CollectionNode(collection: Collection, filter: String) -> Element {
    let c = collection;
    let cid = c.id.clone();
    let open = !filter.is_empty() || UI.read().expanded.contains(&cid);
    let renaming = UI.read().renaming.clone().filter(|(id, _)| id == &cid);
    let target = DropTarget::Collection(cid.clone());
    let class = row_class("collection", &cid, false);
    rsx! {
        div {
            class,
            style: "padding-left: 4px",
            draggable: renaming.is_none(),
            ondragstart: { let cid = cid.clone(); move |e: DragEvent| {
                let _ = e.data_transfer().set_data("text/plain", &cid);
                UI.write().drag = Some(DragPayload::Collection(cid.clone()));
            }},
            ondragend: move |_| { let mut ui = UI.write(); ui.drag = None; ui.drop_hint = None; },
            ondragover: { let t = target.clone(); move |e: DragEvent| on_drag_over(&e, &t) },
            ondrop: { let t = target.clone(); move |e: DragEvent| on_drop(&e, &t) },
            onclick: { let cid = cid.clone(); move |_| toggle(&cid) },
            oncontextmenu: { let cid = cid.clone(); move |e: MouseEvent| { e.prevent_default(); UI.write().menu_open = Some(cid.clone()); } },
            span { class: if open { "chev open" } else { "chev" }, icons::ChevronRight { size: 14 } }
            span { class: "ico", if open { icons::FolderOpen { size: 15 } } else { icons::LibraryBig { size: 15 } } }
            if let Some((_, buf)) = renaming.clone() {
                RenameInput { value: buf, cid: cid.clone(), item: None }
            } else {
                span { class: "label", ondoubleclick: { let (cid, name) = (cid.clone(), c.name.clone()); move |e: MouseEvent| { e.stop_propagation(); UI.write().renaming = Some((cid.clone(), name.clone())); } }, "{c.name}" }
                RowMenu { row_id: cid.clone(), cid: cid.clone(), item: None, kind: RowKind::Collection }
            }
        }
        if open {
            div { class: "tree-children",
                if c.items.is_empty() {
                    div { class: "tree-row", style: "padding-left: 30px; cursor: default",
                        span { class: "muted small", "Empty collection · " }
                        button { class: "btn ghost sm", onclick: { let cid = cid.clone(); move |_| perform(Act::AddRequest, &cid, None) }, "Add a request" }
                    }
                }
                {render_items(&cid, &c.items, 1, &filter)}
            }
        }
    }
}

fn toggle(id: &str) {
    let mut ui = UI.write();
    if !ui.expanded.remove(id) {
        ui.expanded.insert(id.to_string());
    }
}

fn render_items(cid: &str, items: &[Item], depth: usize, filter: &str) -> Element {
    let active_rid = {
        let a = ACTIVE.read().clone();
        TABS.read().iter().find(|t| t.id == a).and_then(|t| t.origin.as_ref()).map(|o| o.request_id.clone())
    };
    let pad = 4 + depth * 14;
    let visible: Vec<Item> = items.iter().filter(|i| matches_filter(filter, i)).cloned().collect();
    rsx! {
        for item in visible {
            {
                let id = item.id().to_string();
                let cid_s = cid.to_string();
                let renaming = UI.read().renaming.clone().filter(|(rid, _)| rid == &id);
                match item {
                    Item::Folder(f) => {
                        let open = !filter.is_empty() || UI.read().expanded.contains(&f.id);
                        let target = DropTarget::Item { cid: cid_s.clone(), id: id.clone(), folder: true };
                        let class = row_class("", &id, false);
                        rsx! {
                            div { key: "{id}",
                                div {
                                    class,
                                    style: "padding-left: {pad}px",
                                    draggable: renaming.is_none(),
                                    ondragstart: { let (cid, id) = (cid_s.clone(), id.clone()); move |e: DragEvent| {
                                        e.stop_propagation();
                                        let _ = e.data_transfer().set_data("text/plain", &id);
                                        UI.write().drag = Some(DragPayload::Item { cid: cid.clone(), id: id.clone() });
                                    }},
                                    ondragend: move |_| { let mut ui = UI.write(); ui.drag = None; ui.drop_hint = None; },
                                    ondragover: { let t = target.clone(); move |e: DragEvent| on_drag_over(&e, &t) },
                                    ondrop: { let t = target.clone(); move |e: DragEvent| on_drop(&e, &t) },
                                    onclick: { let id = id.clone(); move |_| toggle(&id) },
                                    oncontextmenu: { let id = id.clone(); move |e: MouseEvent| { e.prevent_default(); UI.write().menu_open = Some(id.clone()); } },
                                    span { class: if open { "chev open" } else { "chev" }, icons::ChevronRight { size: 14 } }
                                    span { class: "ico", if open { icons::FolderOpen { size: 15 } } else { icons::Folder { size: 15 } } }
                                    if let Some((_, buf)) = renaming.clone() {
                                        RenameInput { value: buf, cid: cid_s.clone(), item: Some(id.clone()) }
                                    } else {
                                        span { class: "label", ondoubleclick: { let (id, name) = (id.clone(), f.name.clone()); move |e: MouseEvent| { e.stop_propagation(); UI.write().renaming = Some((id.clone(), name.clone())); } }, "{f.name}" }
                                        RowMenu { row_id: id.clone(), cid: cid_s.clone(), item: Some(id.clone()), kind: RowKind::Folder }
                                    }
                                }
                                if open {
                                    if f.items.is_empty() {
                                        div { class: "tree-row", style: "padding-left: {pad + 34}px; cursor: default", span { class: "muted small", "Empty folder — drag requests here" } }
                                    }
                                    {render_items(&cid_s, &f.items, depth + 1, filter)}
                                }
                            }
                        }
                    }
                    Item::Request(r) => {
                        let selected = active_rid.as_deref() == Some(id.as_str());
                        let target = DropTarget::Item { cid: cid_s.clone(), id: id.clone(), folder: false };
                        let class = row_class("", &id, selected);
                        let mclass = method_class(r.method);
                        let short = method_short(r.method);
                        rsx! {
                            div {
                                key: "{id}",
                                class,
                                style: "padding-left: {pad}px",
                                title: "{r.url}",
                                draggable: renaming.is_none(),
                                ondragstart: { let (cid, id) = (cid_s.clone(), id.clone()); move |e: DragEvent| {
                                    let _ = e.data_transfer().set_data("text/plain", &id);
                                    UI.write().drag = Some(DragPayload::Item { cid: cid.clone(), id: id.clone() });
                                }},
                                ondragend: move |_| { let mut ui = UI.write(); ui.drag = None; ui.drop_hint = None; },
                                ondragover: { let t = target.clone(); move |e: DragEvent| on_drag_over(&e, &t) },
                                ondrop: { let t = target.clone(); move |e: DragEvent| on_drop(&e, &t) },
                                onclick: { let (cid, id) = (cid_s.clone(), id.clone()); move |_| open_request(&cid, &id) },
                                oncontextmenu: { let id = id.clone(); move |e: MouseEvent| { e.prevent_default(); UI.write().menu_open = Some(id.clone()); } },
                                span { class: "{mclass}", "{short}" }
                                if let Some((_, buf)) = renaming.clone() {
                                    RenameInput { value: buf, cid: cid_s.clone(), item: Some(id.clone()) }
                                } else {
                                    span { class: "label", ondoubleclick: { let (id, name) = (id.clone(), r.name.clone()); move |e: MouseEvent| { e.stop_propagation(); UI.write().renaming = Some((id.clone(), name.clone())); } }, "{r.name}" }
                                    RowMenu { row_id: id.clone(), cid: cid_s.clone(), item: Some(id.clone()), kind: RowKind::Request }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RenameInput(value: String, cid: String, item: Option<String>) -> Element {
    let commit = {
        let (cid, item) = (cid.clone(), item.clone());
        move || {
            let Some((_, name)) = UI.write().renaming.take() else { return };
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            if let Some(c) = WS.write().collection_mut(&cid) {
                match &item {
                    None => c.name = name.clone(),
                    Some(id) => {
                        if let Some(it) = c.find_item_mut(id) {
                            it.set_name(name.clone());
                        }
                    }
                }
            }
            if let Some(id) = &item {
                for t in TABS.write().iter_mut().filter(|t| t.origin.as_ref().is_some_and(|o| &o.request_id == id)) {
                    t.request.name = name.clone();
                }
            }
        }
    };
    let commit2 = commit.clone();
    rsx! {
        input {
            class: "input sm rename grow",
            value: "{value}",
            onmounted: move |e| async move {
                let _ = e.set_focus(true).await;
            },
            onclick: move |e| e.stop_propagation(),
            oninput: move |e| {
                if let Some(r) = UI.write().renaming.as_mut() {
                    r.1 = e.value();
                }
            },
            onkeydown: move |e| {
                if e.key() == Key::Enter {
                    commit();
                } else if e.key() == Key::Escape {
                    UI.write().renaming = None;
                }
            },
            onblur: move |_| commit2(),
        }
    }
}

// ------------------------------------------------------------------ environments

#[component]
fn Environments() -> Element {
    let ws = WS.read();
    let filter = UI.read().filter.to_lowercase();
    let active = ws.active_environment.clone();
    let envs: Vec<Environment> = ws.environments.iter().filter(|e| filter.is_empty() || e.name.to_lowercase().contains(&filter)).cloned().collect();
    let globals = ws.globals.iter().filter(|g| !g.key.is_empty()).count();
    drop(ws);
    rsx! {
        div { class: "tree",
            div { class: "tree-row", style: "padding-left: 8px", onclick: move |_| open_dialog(Dialog::Environment(None)),
                span { class: "ico", icons::Globe { size: 15 } }
                span { class: "label", "Globals" }
                span { class: "badge", "{globals}" }
            }
            div { class: "tree-group-label", "Environments" }
            if envs.is_empty() {
                div { class: "muted small", style: "padding: 4px 8px", "No environments yet. Use New to create one for variables like {{{{base_url}}}}." }
            }
            for e in envs {
                {
                    let is_active = active.as_deref() == Some(e.id.as_str());
                    let (id1, id2) = (e.id.clone(), e.id.clone());
                    let n = e.variables.iter().filter(|v| !v.key.is_empty()).count();
                    rsx! {
                        div { key: "{e.id}", class: if is_active { "tree-row env-row selected" } else { "tree-row env-row" }, style: "padding-left: 8px",
                            onclick: move |_| open_dialog(Dialog::Environment(Some(id1.clone()))),
                            span { class: "ico", if is_active { icons::CircleCheck { size: 15, color: "var(--success)" } } else { icons::Layers { size: 15 } } }
                            span { class: "label", "{e.name}" }
                            span { class: "badge", "{n}" }
                            if !is_active {
                                button { class: "btn ghost sm use", onclick: move |ev| { ev.stop_propagation(); WS.write().active_environment = Some(id2.clone()); }, "Use" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ history

#[component]
fn HistoryList() -> Element {
    let ws = WS.read();
    let filter = UI.read().filter.to_lowercase();
    if ws.history.is_empty() {
        return rsx! { Empty { title: "No history yet", text: "Requests you send will show up here.", icon: rsx! { icons::RotateCcwClock { size: 22 } } } };
    }
    let mut groups: Vec<(String, Vec<(usize, HistoryEntry)>)> = vec![];
    for (i, h) in ws.history.iter().enumerate() {
        if !filter.is_empty() && !h.request.url.to_lowercase().contains(&filter) && !h.request.name.to_lowercase().contains(&filter) {
            continue;
        }
        let day = h.timestamp.with_timezone(&chrono::Local).format("%A, %-d %B").to_string();
        match groups.last_mut() {
            Some((d, v)) if *d == day => v.push((i, h.clone())),
            _ => groups.push((day, vec![(i, h.clone())])),
        }
    }
    drop(ws);
    rsx! {
        div { class: "tree",
            for (day , entries) in groups {
                div { key: "{day}", class: "tree-group-label", "{day}" }
                for (i , h) in entries {
                    {
                        let time = h.timestamp.with_timezone(&chrono::Local).format("%H:%M").to_string();
                        let mclass = method_class(h.request.method);
                        let short = method_short(h.request.method);
                        let req = h.request.clone();
                        rsx! {
                            div { key: "{h.id}", class: "tree-row", style: "padding-left: 4px", title: "{h.request.url}",
                                onclick: move |_| {
                                    let mut r = req.clone();
                                    r.id = new_id();
                                    push_tab(Tab::new(r, None));
                                },
                                span { class: "{mclass}", "{short}" }
                                span { class: "label mono small", "{h.request.url}" }
                                if let Some(s) = h.status {
                                    span { class: if s < 400 { "xs ok" } else { "xs bad" }, "{s}" }
                                }
                                span { class: "xs subtle", "{time}" }
                                button { class: "icon-btn sm danger", title: "Remove", onclick: move |e| { e.stop_propagation(); WS.write().history.remove(i); }, icons::X { size: 13 } }
                            }
                        }
                    }
                }
            }
        }
    }
}
