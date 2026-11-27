//! Data model for workspaces, collections, requests and environments.

use serde::{Deserialize, Serialize};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub const ALL: [HttpMethod; 7] = [
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Patch,
        HttpMethod::Delete,
        HttpMethod::Head,
        HttpMethod::Options,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }

    pub fn parse(s: &str) -> Option<HttpMethod> {
        Self::ALL
            .iter()
            .copied()
            .find(|m| m.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A generic enable-able key/value row (params, headers, urlencoded fields, variables).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyValue {
    pub enabled: bool,
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub description: String,
}

impl Default for KeyValue {
    fn default() -> Self {
        Self { enabled: true, key: String::new(), value: String::new(), description: String::new() }
    }
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key: key.into(), value: value.into(), ..Default::default() }
    }
    pub fn is_blank(&self) -> bool {
        self.key.is_empty() && self.value.is_empty() && self.description.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FormFieldKind {
    #[default]
    Text,
    File,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct FormField {
    pub enabled: bool,
    pub key: String,
    /// Text value, or file path for `File` fields.
    pub value: String,
    pub kind: FormFieldKind,
    #[serde(default)]
    pub content_type: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RawLanguage {
    #[default]
    Json,
    Text,
    Xml,
    Html,
    JavaScript,
}

impl RawLanguage {
    pub const ALL: [RawLanguage; 5] =
        [RawLanguage::Json, RawLanguage::Text, RawLanguage::Xml, RawLanguage::Html, RawLanguage::JavaScript];
    pub fn label(&self) -> &'static str {
        match self {
            RawLanguage::Json => "JSON",
            RawLanguage::Text => "Text",
            RawLanguage::Xml => "XML",
            RawLanguage::Html => "HTML",
            RawLanguage::JavaScript => "JavaScript",
        }
    }
    pub fn content_type(&self) -> &'static str {
        match self {
            RawLanguage::Json => "application/json",
            RawLanguage::Text => "text/plain",
            RawLanguage::Xml => "application/xml",
            RawLanguage::Html => "text/html",
            RawLanguage::JavaScript => "application/javascript",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Body {
    #[default]
    None,
    Raw { language: RawLanguage, text: String },
    FormData(Vec<FormField>),
    UrlEncoded(Vec<KeyValue>),
    Binary { path: String },
    GraphQL { query: String, variables: String },
}

impl Body {
    pub fn kind_label(&self) -> &'static str {
        match self {
            Body::None => "none",
            Body::Raw { .. } => "raw",
            Body::FormData(_) => "form-data",
            Body::UrlEncoded(_) => "x-www-form-urlencoded",
            Body::Binary { .. } => "binary",
            Body::GraphQL { .. } => "GraphQL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum Auth {
    /// Use the auth configured on the parent folder / collection.
    #[default]
    Inherit,
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
    ApiKey { key: String, value: String, location: ApiKeyLocation },
    /// OAuth 2.0 with an access token already obtained (sent as `<prefix> <token>`).
    OAuth2 { access_token: String, header_prefix: String },
}

impl Auth {
    pub fn label(&self) -> &'static str {
        match self {
            Auth::Inherit => "Inherit auth from parent",
            Auth::None => "No Auth",
            Auth::Basic { .. } => "Basic Auth",
            Auth::Bearer { .. } => "Bearer Token",
            Auth::ApiKey { .. } => "API Key",
            Auth::OAuth2 { .. } => "OAuth 2.0",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RequestSettings {
    pub follow_redirects: bool,
    pub verify_ssl: bool,
    /// 0 = no timeout
    pub timeout_ms: u64,
}

impl Default for RequestSettings {
    fn default() -> Self {
        Self { follow_redirects: true, verify_ssl: true, timeout_ms: 0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub params: Vec<KeyValue>,
    #[serde(default)]
    pub headers: Vec<KeyValue>,
    #[serde(default)]
    pub body: Body,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub pre_request_script: String,
    #[serde(default)]
    pub test_script: String,
    #[serde(default)]
    pub settings: RequestSettings,
    #[serde(default)]
    pub description: String,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            id: new_id(),
            name: "Untitled Request".into(),
            method: HttpMethod::Get,
            url: String::new(),
            params: vec![],
            headers: vec![],
            body: Body::None,
            auth: Auth::Inherit,
            pre_request_script: String::new(),
            test_script: String::new(),
            settings: RequestSettings::default(),
            description: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub pre_request_script: String,
    #[serde(default)]
    pub test_script: String,
    #[serde(default)]
    pub description: String,
}

impl Folder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            items: vec![],
            auth: Auth::Inherit,
            pre_request_script: String::new(),
            test_script: String::new(),
            description: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Folder(Folder),
    Request(Request),
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Item::Folder(f) => &f.id,
            Item::Request(r) => &r.id,
        }
    }
    pub fn name(&self) -> &str {
        match self {
            Item::Folder(f) => &f.name,
            Item::Request(r) => &r.name,
        }
    }
    pub fn set_name(&mut self, name: String) {
        match self {
            Item::Folder(f) => f.name = name,
            Item::Request(r) => r.name = name,
        }
    }
    /// Deep clone with freshly generated ids.
    pub fn duplicate(&self) -> Item {
        match self {
            Item::Request(r) => Item::Request(Request { id: new_id(), name: format!("{} Copy", r.name), ..r.clone() }),
            Item::Folder(f) => Item::Folder(Folder {
                id: new_id(),
                name: format!("{} Copy", f.name),
                items: f.items.iter().map(|i| {
                    let mut d = i.duplicate();
                    // keep child names unchanged
                    d.set_name(i.name().to_string());
                    d
                }).collect(),
                ..f.clone()
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub variables: Vec<KeyValue>,
    #[serde(default = "default_collection_auth")]
    pub auth: Auth,
    #[serde(default)]
    pub pre_request_script: String,
    #[serde(default)]
    pub test_script: String,
}

fn default_collection_auth() -> Auth {
    Auth::None
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            description: String::new(),
            items: vec![],
            variables: vec![],
            auth: Auth::None,
            pre_request_script: String::new(),
            test_script: String::new(),
        }
    }

    /// Find a request anywhere in this collection.
    pub fn find_request(&self, id: &str) -> Option<&Request> {
        fn walk<'a>(items: &'a [Item], id: &str) -> Option<&'a Request> {
            for it in items {
                match it {
                    Item::Request(r) if r.id == id => return Some(r),
                    Item::Folder(f) => {
                        if let Some(r) = walk(&f.items, id) {
                            return Some(r);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(&self.items, id)
    }

    pub fn find_request_mut(&mut self, id: &str) -> Option<&mut Request> {
        fn walk<'a>(items: &'a mut [Item], id: &str) -> Option<&'a mut Request> {
            for it in items {
                match it {
                    Item::Request(r) if r.id == id => return Some(r),
                    Item::Folder(f) => {
                        if let Some(r) = walk(&mut f.items, id) {
                            return Some(r);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(&mut self.items, id)
    }

    /// The item list of a container: the collection root (`None`) or a folder.
    pub fn container_mut(&mut self, folder_id: Option<&str>) -> Option<&mut Vec<Item>> {
        fn walk<'a>(items: &'a mut Vec<Item>, id: &str) -> Option<&'a mut Vec<Item>> {
            for it in items.iter_mut() {
                if let Item::Folder(f) = it {
                    if f.id == id {
                        return Some(&mut f.items);
                    }
                    if let Some(found) = walk(&mut f.items, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        match folder_id {
            None => Some(&mut self.items),
            Some(id) => walk(&mut self.items, id),
        }
    }

    pub fn find_item_mut(&mut self, id: &str) -> Option<&mut Item> {
        fn walk<'a>(items: &'a mut [Item], id: &str) -> Option<&'a mut Item> {
            for it in items.iter_mut() {
                if it.id() == id {
                    return Some(it);
                }
                if let Item::Folder(f) = it {
                    if let Some(x) = walk(&mut f.items, id) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&mut self.items, id)
    }

    /// Remove an item (request or folder) by id. Returns it if found.
    pub fn remove_item(&mut self, id: &str) -> Option<Item> {
        fn walk(items: &mut Vec<Item>, id: &str) -> Option<Item> {
            if let Some(pos) = items.iter().position(|i| i.id() == id) {
                return Some(items.remove(pos));
            }
            for it in items.iter_mut() {
                if let Item::Folder(f) = it {
                    if let Some(x) = walk(&mut f.items, id) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&mut self.items, id)
    }

    /// Insert a copy of the item with `id` right after the original.
    pub fn duplicate_item(&mut self, id: &str) -> Option<String> {
        fn walk(items: &mut Vec<Item>, id: &str) -> Option<String> {
            if let Some(pos) = items.iter().position(|i| i.id() == id) {
                let dup = items[pos].duplicate();
                let new_id = dup.id().to_string();
                items.insert(pos + 1, dup);
                return Some(new_id);
            }
            for it in items.iter_mut() {
                if let Item::Folder(f) = it {
                    if let Some(x) = walk(&mut f.items, id) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&mut self.items, id)
    }

    /// Chain of folders leading to a request (outermost first), used for auth/script inheritance.
    pub fn ancestors_of(&self, request_id: &str) -> Vec<&Folder> {
        fn walk<'a>(items: &'a [Item], id: &str, path: &mut Vec<&'a Folder>) -> bool {
            for it in items {
                match it {
                    Item::Request(r) if r.id == id => return true,
                    Item::Folder(f) => {
                        path.push(f);
                        if walk(&f.items, id, path) {
                            return true;
                        }
                        path.pop();
                    }
                    _ => {}
                }
            }
            false
        }
        let mut path = vec![];
        walk(&self.items, request_id, &mut path);
        path
    }

    /// All requests in depth-first order (for the collection runner).
    pub fn all_requests(&self) -> Vec<&Request> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Request>) {
            for it in items {
                match it {
                    Item::Request(r) => out.push(r),
                    Item::Folder(f) => walk(&f.items, out),
                }
            }
        }
        let mut out = vec![];
        walk(&self.items, &mut out);
        out
    }

    pub fn folder(&self, id: &str) -> Option<&Folder> {
        fn walk<'a>(items: &'a [Item], id: &str) -> Option<&'a Folder> {
            for it in items {
                if let Item::Folder(f) = it {
                    if f.id == id {
                        return Some(f);
                    }
                    if let Some(x) = walk(&f.items, id) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&self.items, id)
    }
}

/// Where a dragged item lands relative to the drop target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlace {
    Before,
    After,
    /// Into a folder (appended), or into a collection's root.
    Inside,
}

impl Collection {
    /// (parent folder id or None for root, index) of an item.
    pub fn locate(&self, id: &str) -> Option<(Option<String>, usize)> {
        fn walk(items: &[Item], id: &str, parent: Option<&str>) -> Option<(Option<String>, usize)> {
            for (i, it) in items.iter().enumerate() {
                if it.id() == id {
                    return Some((parent.map(String::from), i));
                }
                if let Item::Folder(f) = it {
                    if let Some(x) = walk(&f.items, id, Some(&f.id)) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&self.items, id, None)
    }

    pub fn find_item(&self, id: &str) -> Option<&Item> {
        fn walk<'a>(items: &'a [Item], id: &str) -> Option<&'a Item> {
            for it in items {
                if it.id() == id {
                    return Some(it);
                }
                if let Item::Folder(f) = it {
                    if let Some(x) = walk(&f.items, id) {
                        return Some(x);
                    }
                }
            }
            None
        }
        walk(&self.items, id)
    }
}

fn subtree_ids(item: &Item, out: &mut Vec<String>) {
    out.push(item.id().to_string());
    if let Item::Folder(f) = item {
        for i in &f.items {
            subtree_ids(i, out);
        }
    }
}

impl Workspace {
    /// Move a request or folder next to / into a target.
    /// `target_id = None` means the root of collection `target_cid`.
    pub fn move_item(&mut self, item_id: &str, target_cid: &str, target_id: Option<&str>, place: DropPlace) -> Result<(), String> {
        if target_id == Some(item_id) {
            return Err("Cannot drop an item onto itself".into());
        }
        let src_cid = self.collections.iter().find(|c| c.find_item(item_id).is_some()).map(|c| c.id.clone()).ok_or("Item not found")?;
        let target = self.collection(target_cid).ok_or("Target collection not found")?;
        if let Some(t) = target_id {
            let titem = target.find_item(t).ok_or("Target not found")?;
            if place == DropPlace::Inside && !matches!(titem, Item::Folder(_)) {
                return Err("Can only drop inside a folder".into());
            }
            let mut sub = vec![];
            subtree_ids(self.collection(&src_cid).and_then(|c| c.find_item(item_id)).unwrap(), &mut sub);
            if sub.iter().any(|x| x == t) {
                return Err("Cannot move a folder into itself".into());
            }
        }
        let item = self.collection_mut(&src_cid).and_then(|c| c.remove_item(item_id)).ok_or("Item not found")?;
        let target = self.collection_mut(target_cid).unwrap();
        match (target_id, place) {
            (None, _) => target.items.push(item),
            (Some(t), DropPlace::Inside) => target.container_mut(Some(t)).unwrap().push(item),
            (Some(t), before_after) => {
                let (parent, idx) = target.locate(t).unwrap();
                let list = target.container_mut(parent.as_deref()).unwrap();
                let at = if before_after == DropPlace::After { idx + 1 } else { idx };
                list.insert(at.min(list.len()), item);
            }
        }
        Ok(())
    }

    /// Reorder collections.
    pub fn move_collection(&mut self, id: &str, target_id: &str, place: DropPlace) {
        if id == target_id {
            return;
        }
        let Some(from) = self.collections.iter().position(|c| c.id == id) else { return };
        let c = self.collections.remove(from);
        let Some(to) = self.collections.iter().position(|c| c.id == target_id) else {
            self.collections.insert(from, c);
            return;
        };
        let at = if place == DropPlace::After { to + 1 } else { to };
        self.collections.insert(at, c);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub variables: Vec<KeyValue>,
}

impl Environment {
    pub fn new(name: impl Into<String>) -> Self {
        Self { id: new_id(), name: name.into(), variables: vec![] }
    }
}

/// Set (or add) a variable in a list of key/values.
pub fn set_var(vars: &mut Vec<KeyValue>, key: &str, value: &str) {
    if let Some(v) = vars.iter_mut().find(|v| v.key == key) {
        v.value = value.to_string();
        v.enabled = true;
    } else {
        vars.push(KeyValue::new(key, value));
    }
}

pub fn unset_var(vars: &mut Vec<KeyValue>, key: &str) {
    vars.retain(|v| v.key != key);
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub request: Request,
    pub status: Option<u16>,
    pub time_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Workspace {
    #[serde(default)]
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub environments: Vec<Environment>,
    #[serde(default)]
    pub globals: Vec<KeyValue>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub active_environment: Option<String>,
}

impl Workspace {
    pub const HISTORY_LIMIT: usize = 300;

    pub fn active_env(&self) -> Option<&Environment> {
        let id = self.active_environment.as_ref()?;
        self.environments.iter().find(|e| &e.id == id)
    }
    pub fn active_env_mut(&mut self) -> Option<&mut Environment> {
        let id = self.active_environment.clone()?;
        self.environments.iter_mut().find(|e| e.id == id)
    }
    pub fn collection(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }
    pub fn collection_mut(&mut self, id: &str) -> Option<&mut Collection> {
        self.collections.iter_mut().find(|c| c.id == id)
    }
    /// Find which collection holds a request.
    pub fn locate_request(&self, request_id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.find_request(request_id).is_some())
    }
    pub fn push_history(&mut self, entry: HistoryEntry) {
        self.history.insert(0, entry);
        self.history.truncate(Self::HISTORY_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str) -> Item {
        Item::Request(Request { name: name.into(), ..Default::default() })
    }

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.name().to_string()).collect()
    }

    #[test]
    fn move_items_around() {
        let mut ws = Workspace::default();
        let mut a = Collection::new("A");
        let mut f = Folder::new("F");
        f.items = vec![req("f1")];
        let fid = f.id.clone();
        a.items = vec![req("r1"), req("r2"), Item::Folder(f)];
        let ids: Vec<String> = a.items.iter().map(|i| i.id().to_string()).collect();
        let b = Collection::new("B");
        let (aid, bid) = (a.id.clone(), b.id.clone());
        ws.collections = vec![a, b];

        // reorder within root
        ws.move_item(&ids[1], &aid, Some(&ids[0]), DropPlace::Before).unwrap();
        assert_eq!(names(&ws.collection(&aid).unwrap().items), ["r2", "r1", "F"]);
        // into folder
        ws.move_item(&ids[0], &aid, Some(&fid), DropPlace::Inside).unwrap();
        assert_eq!(names(&ws.collection(&aid).unwrap().folder(&fid).unwrap().items), ["f1", "r1"]);
        // across collections
        ws.move_item(&fid, &bid, None, DropPlace::Inside).unwrap();
        assert_eq!(names(&ws.collection(&bid).unwrap().items), ["F"]);
        assert_eq!(names(&ws.collection(&aid).unwrap().items), ["r2"]);
        // folder into its own child is refused
        let child = ws.collection(&bid).unwrap().folder(&fid).unwrap().items[0].id().to_string();
        assert!(ws.move_item(&fid, &bid, Some(&child), DropPlace::Before).is_err());
        assert!(ws.move_item(&fid, &bid, Some(&fid), DropPlace::Inside).is_err());
        // collections
        ws.move_collection(&bid, &aid, DropPlace::Before);
        assert_eq!(ws.collections[0].name, "B");
    }
}
