//! Interoperability with the public Postman Collection v2.0/v2.1 and environment JSON formats,
//! plus Fetchly's own native export format.

use crate::model::*;
use serde_json::{json, Value};

pub const SCHEMA_V21: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// What an imported file turned out to contain.
pub enum Imported {
    Collection(Collection),
    Environment(Environment),
    Workspace(Workspace),
    Request(Request),
}

/// Detect the format of a pasted/opened document and import it.
pub fn import_any(text: &str) -> Result<Imported, String> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("curl") {
        return crate::curl::parse_curl(text).map(Imported::Request);
    }
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Not valid JSON or cURL: {e}"))?;
    if v.get("fetchly_workspace").is_some() {
        let ws: Workspace = serde_json::from_value(v["fetchly_workspace"].clone()).map_err(|e| e.to_string())?;
        return Ok(Imported::Workspace(ws));
    }
    if v.get("fetchly_collection").is_some() {
        let c: Collection = serde_json::from_value(v["fetchly_collection"].clone()).map_err(|e| e.to_string())?;
        return Ok(Imported::Collection(reassign_ids(c)));
    }
    if v.get("info").is_some() && v.get("item").is_some() {
        return import_collection(&v).map(Imported::Collection);
    }
    if v.get("values").is_some() && v.get("name").is_some() {
        return Ok(Imported::Environment(import_environment(&v)));
    }
    Err("Unrecognised format. Supported: Postman collection v2.0/v2.1, Postman environment, Fetchly export, cURL".into())
}

fn reassign_ids(mut c: Collection) -> Collection {
    fn walk(items: &mut [Item]) {
        for it in items {
            match it {
                Item::Request(r) => r.id = new_id(),
                Item::Folder(f) => {
                    f.id = new_id();
                    walk(&mut f.items);
                }
            }
        }
    }
    c.id = new_id();
    walk(&mut c.items);
    c
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn description(v: &Value) -> String {
    match v.get("description") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(o)) => o.get("content").map(s).unwrap_or_default(),
        _ => String::new(),
    }
}

fn kv_list(v: Option<&Value>) -> Vec<KeyValue> {
    v.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|e| KeyValue {
                    enabled: !e.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false)
                        && e.get("enabled").and_then(|d| d.as_bool()).unwrap_or(true),
                    key: e.get("key").map(s).unwrap_or_default(),
                    value: e.get("value").map(s).unwrap_or_default(),
                    description: description(e),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Postman scripts are JavaScript; Fetchly scripts use Rhai. Imported scripts are kept,
/// commented out, so nothing breaks and they can be ported by hand.
fn import_script(events: Option<&Value>, listen: &str) -> String {
    let Some(arr) = events.and_then(|e| e.as_array()) else { return String::new() };
    let mut out = String::new();
    for ev in arr.iter().filter(|e| e.get("listen").and_then(|l| l.as_str()) == Some(listen)) {
        let exec = &ev["script"]["exec"];
        let lines: Vec<String> = match exec {
            Value::Array(a) => a.iter().map(s).collect(),
            Value::String(st) => st.lines().map(String::from).collect(),
            _ => vec![],
        };
        if lines.iter().all(|l| l.trim().is_empty()) {
            continue;
        }
        out.push_str("// Imported JavaScript (not executed) — port to a Fetchly script:\n");
        for l in lines {
            out.push_str("// ");
            out.push_str(&l);
            out.push('\n');
        }
    }
    out
}

fn auth_param(arr: &Value, key: &str) -> String {
    match arr {
        Value::Array(a) => a.iter().find(|p| p.get("key").and_then(|k| k.as_str()) == Some(key)).and_then(|p| p.get("value")).map(s).unwrap_or_default(),
        Value::Object(o) => o.get(key).map(s).unwrap_or_default(),
        _ => String::new(),
    }
}

fn import_auth(v: Option<&Value>, default: Auth) -> Auth {
    let Some(a) = v else { return default };
    if a.is_null() {
        return default;
    }
    let ty = a.get("type").and_then(|t| t.as_str()).unwrap_or("noauth");
    let p = |k: &str| a.get(ty).map(|x| auth_param(x, k)).unwrap_or_default();
    match ty {
        "basic" => Auth::Basic { username: p("username"), password: p("password") },
        "bearer" => Auth::Bearer { token: p("token") },
        "apikey" => Auth::ApiKey {
            key: p("key"),
            value: p("value"),
            location: if p("in") == "query" { ApiKeyLocation::Query } else { ApiKeyLocation::Header },
        },
        "oauth2" => Auth::OAuth2 { access_token: p("accessToken"), header_prefix: { let h = p("headerPrefix"); if h.is_empty() { "Bearer".into() } else { h } } },
        "noauth" => Auth::None,
        _ => default,
    }
}

fn import_url(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Object(o) => {
            if let Some(raw) = o.get("raw").and_then(|r| r.as_str()) {
                return raw.to_string();
            }
            let proto = o.get("protocol").and_then(|p| p.as_str()).map(|p| format!("{p}://")).unwrap_or_default();
            let join = |k: &str, sep: &str| match o.get(k) {
                Some(Value::Array(a)) => a.iter().map(s).collect::<Vec<_>>().join(sep),
                Some(other) => s(other),
                None => String::new(),
            };
            let mut url = format!("{proto}{}", join("host", "."));
            let path = join("path", "/");
            if !path.is_empty() {
                url.push('/');
                url.push_str(&path);
            }
            url
        }
        _ => String::new(),
    }
}

fn import_body(b: Option<&Value>) -> Body {
    let Some(b) = b else { return Body::None };
    match b.get("mode").and_then(|m| m.as_str()).unwrap_or("") {
        "raw" => {
            let lang = b.pointer("/options/raw/language").and_then(|l| l.as_str()).unwrap_or("text");
            let language = match lang {
                "json" => RawLanguage::Json,
                "xml" => RawLanguage::Xml,
                "html" => RawLanguage::Html,
                "javascript" => RawLanguage::JavaScript,
                _ => RawLanguage::Text,
            };
            Body::Raw { language, text: b.get("raw").map(s).unwrap_or_default() }
        }
        "urlencoded" => Body::UrlEncoded(kv_list(b.get("urlencoded"))),
        "formdata" => Body::FormData(
            b.get("formdata")
                .and_then(|f| f.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|f| {
                            let is_file = f.get("type").and_then(|t| t.as_str()) == Some("file");
                            FormField {
                                enabled: !f.get("disabled").and_then(|d| d.as_bool()).unwrap_or(false),
                                key: f.get("key").map(s).unwrap_or_default(),
                                value: if is_file {
                                    match f.get("src") {
                                        Some(Value::Array(a)) => a.first().map(s).unwrap_or_default(),
                                        Some(x) => s(x),
                                        None => String::new(),
                                    }
                                } else {
                                    f.get("value").map(s).unwrap_or_default()
                                },
                                kind: if is_file { FormFieldKind::File } else { FormFieldKind::Text },
                                content_type: f.get("contentType").map(s).unwrap_or_default(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "file" => Body::Binary { path: b.pointer("/file/src").map(s).unwrap_or_default() },
        "graphql" => Body::GraphQL {
            query: b.pointer("/graphql/query").map(s).unwrap_or_default(),
            variables: b.pointer("/graphql/variables").map(s).unwrap_or_default(),
        },
        _ => Body::None,
    }
}

fn import_items(arr: Option<&Value>) -> Vec<Item> {
    let Some(arr) = arr.and_then(|a| a.as_array()) else { return vec![] };
    arr.iter()
        .map(|it| {
            let name = it.get("name").map(s).unwrap_or_else(|| "Untitled".into());
            if it.get("item").is_some() && it.get("request").is_none() {
                Item::Folder(Folder {
                    id: new_id(),
                    name,
                    items: import_items(it.get("item")),
                    auth: import_auth(it.get("auth"), Auth::Inherit),
                    pre_request_script: import_script(it.get("event"), "prerequest"),
                    test_script: import_script(it.get("event"), "test"),
                    description: description(it),
                })
            } else {
                let r = &it["request"];
                let (method, url, headers, body, auth, desc) = if let Value::String(u) = r {
                    (HttpMethod::Get, u.clone(), vec![], Body::None, Auth::Inherit, String::new())
                } else {
                    (
                        r.get("method").and_then(|m| m.as_str()).and_then(HttpMethod::parse).unwrap_or_default(),
                        r.get("url").map(import_url).unwrap_or_default(),
                        kv_list(r.get("header")),
                        import_body(r.get("body")),
                        import_auth(r.get("auth"), Auth::Inherit),
                        description(r),
                    )
                };
                let mut params = crate::url::params_from_url(&url, &[]);
                // disabled query params live only in the structured url
                if let Some(q) = r.pointer("/url/query") {
                    params.extend(kv_list(Some(q)).into_iter().filter(|p| !p.enabled));
                }
                Item::Request(Request {
                    id: new_id(),
                    name,
                    method,
                    url,
                    params,
                    headers,
                    body,
                    auth,
                    pre_request_script: import_script(it.get("event"), "prerequest"),
                    test_script: import_script(it.get("event"), "test"),
                    settings: RequestSettings::default(),
                    description: desc,
                })
            }
        })
        .collect()
}

pub fn import_collection(v: &Value) -> Result<Collection, String> {
    let info = v.get("info").ok_or("Missing `info`")?;
    Ok(Collection {
        id: new_id(),
        name: info.get("name").map(s).unwrap_or_else(|| "Imported Collection".into()),
        description: description(info),
        items: import_items(v.get("item")),
        variables: kv_list(v.get("variable")),
        auth: import_auth(v.get("auth"), Auth::None),
        pre_request_script: import_script(v.get("event"), "prerequest"),
        test_script: import_script(v.get("event"), "test"),
    })
}

pub fn import_environment(v: &Value) -> Environment {
    Environment { id: new_id(), name: v.get("name").map(s).unwrap_or_else(|| "Imported".into()), variables: kv_list(v.get("values")) }
}

// ---------------------------------------------------------------- export

fn export_kv(list: &[KeyValue]) -> Value {
    Value::Array(
        list.iter()
            .filter(|k| !k.is_blank())
            .map(|k| {
                let mut o = json!({ "key": k.key, "value": k.value });
                if !k.enabled {
                    o["disabled"] = json!(true);
                }
                if !k.description.is_empty() {
                    o["description"] = json!(k.description);
                }
                o
            })
            .collect(),
    )
}

fn export_auth(a: &Auth) -> Option<Value> {
    let p = |k: &str, v: &str| json!({ "key": k, "value": v, "type": "string" });
    Some(match a {
        Auth::Inherit => return None,
        Auth::None => json!({ "type": "noauth" }),
        Auth::Basic { username, password } => json!({ "type": "basic", "basic": [p("username", username), p("password", password)] }),
        Auth::Bearer { token } => json!({ "type": "bearer", "bearer": [p("token", token)] }),
        Auth::ApiKey { key, value, location } => json!({ "type": "apikey", "apikey": [p("key", key), p("value", value), p("in", if *location == ApiKeyLocation::Query { "query" } else { "header" })] }),
        Auth::OAuth2 { access_token, header_prefix } => json!({ "type": "oauth2", "oauth2": [p("accessToken", access_token), p("headerPrefix", header_prefix), p("addTokenTo", "header")] }),
    })
}

fn export_events(pre: &str, test: &str) -> Value {
    let mut ev = vec![];
    for (listen, src) in [("prerequest", pre), ("test", test)] {
        if !src.trim().is_empty() {
            ev.push(json!({ "listen": listen, "script": { "type": "text/x-rhai", "exec": src.lines().collect::<Vec<_>>() } }));
        }
    }
    Value::Array(ev)
}

fn export_body(b: &Body) -> Option<Value> {
    Some(match b {
        Body::None => return None,
        Body::Raw { language, text } => json!({ "mode": "raw", "raw": text, "options": { "raw": { "language": language.label().to_lowercase() } } }),
        Body::UrlEncoded(f) => json!({ "mode": "urlencoded", "urlencoded": export_kv(f) }),
        Body::FormData(f) => json!({ "mode": "formdata", "formdata": f.iter().map(|f| {
            let mut o = if f.kind == FormFieldKind::File { json!({ "key": f.key, "type": "file", "src": f.value }) } else { json!({ "key": f.key, "value": f.value, "type": "text" }) };
            if !f.enabled { o["disabled"] = json!(true); }
            if !f.content_type.is_empty() { o["contentType"] = json!(f.content_type); }
            o
        }).collect::<Vec<_>>() }),
        Body::Binary { path } => json!({ "mode": "file", "file": { "src": path } }),
        Body::GraphQL { query, variables } => json!({ "mode": "graphql", "graphql": { "query": query, "variables": variables } }),
    })
}

fn export_url(url: &str, params: &[KeyValue]) -> Value {
    let (base, _, _) = crate::url::split_url(url);
    let (protocol, rest) = base.split_once("://").map(|(p, r)| (Some(p), r)).unwrap_or((None, base.as_str()));
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let mut o = json!({
        "raw": url,
        "host": host.split('.').collect::<Vec<_>>(),
        "path": path.split('/').filter(|p| !p.is_empty()).collect::<Vec<_>>(),
    });
    if let Some(p) = protocol {
        o["protocol"] = json!(p);
    }
    if !params.is_empty() {
        o["query"] = export_kv(params);
    }
    o
}

fn export_items(items: &[Item]) -> Value {
    Value::Array(
        items
            .iter()
            .map(|it| match it {
                Item::Folder(f) => {
                    let mut o = json!({ "name": f.name, "item": export_items(&f.items), "event": export_events(&f.pre_request_script, &f.test_script) });
                    if let Some(a) = export_auth(&f.auth) {
                        o["auth"] = a;
                    }
                    if !f.description.is_empty() {
                        o["description"] = json!(f.description);
                    }
                    o
                }
                Item::Request(r) => {
                    let mut req = json!({ "method": r.method.as_str(), "header": export_kv(&r.headers), "url": export_url(&r.url, &r.params) });
                    if let Some(b) = export_body(&r.body) {
                        req["body"] = b;
                    }
                    if let Some(a) = export_auth(&r.auth) {
                        req["auth"] = a;
                    }
                    if !r.description.is_empty() {
                        req["description"] = json!(r.description);
                    }
                    json!({ "name": r.name, "request": req, "response": [], "event": export_events(&r.pre_request_script, &r.test_script) })
                }
            })
            .collect(),
    )
}

/// Export as a Postman v2.1 collection document.
pub fn export_collection(c: &Collection) -> String {
    let mut v = json!({
        "info": { "_postman_id": c.id, "name": c.name, "description": c.description, "schema": SCHEMA_V21 },
        "item": export_items(&c.items),
        "variable": export_kv(&c.variables),
        "event": export_events(&c.pre_request_script, &c.test_script),
    });
    if let Some(a) = export_auth(&c.auth) {
        v["auth"] = a;
    }
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

pub fn export_environment(e: &Environment) -> String {
    let values: Vec<Value> = e
        .variables
        .iter()
        .filter(|v| !v.is_blank())
        .map(|v| json!({ "key": v.key, "value": v.value, "enabled": v.enabled, "type": "default" }))
        .collect();
    serde_json::to_string_pretty(&json!({ "id": e.id, "name": e.name, "values": values, "_postman_variable_scope": "environment" }))
        .unwrap_or_default()
}

/// Full-fidelity Fetchly workspace backup.
pub fn export_workspace(ws: &Workspace) -> String {
    serde_json::to_string_pretty(&json!({ "fetchly_workspace": ws })).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "info": {"name": "Demo", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
      "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{token}}", "type": "string"}]},
      "variable": [{"key": "base", "value": "https://api.example.com"}],
      "item": [
        {"name": "Users", "item": [
          {"name": "List", "request": {"method": "GET", "header": [{"key": "Accept", "value": "application/json"}],
            "url": {"raw": "{{base}}/users?page=1", "host": ["{{base}}"], "path": ["users"], "query": [{"key": "page", "value": "1"}, {"key": "x", "value": "y", "disabled": true}]}},
            "event": [{"listen": "test", "script": {"exec": ["pm.test('ok', () => {})"]}}]},
          {"name": "Create", "request": {"method": "POST", "url": "{{base}}/users",
            "body": {"mode": "raw", "raw": "{\"a\":1}", "options": {"raw": {"language": "json"}}}}}
        ]}
      ]
    }"#;

    #[test]
    fn import_roundtrip() {
        let Imported::Collection(c) = import_any(SAMPLE).unwrap() else { panic!() };
        assert_eq!(c.name, "Demo");
        assert!(matches!(c.auth, Auth::Bearer { .. }));
        let reqs = c.all_requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0].params.len(), 2);
        assert!(reqs[0].test_script.contains("// pm.test"));
        assert!(matches!(reqs[1].body, Body::Raw { language: RawLanguage::Json, .. }));

        let exported = export_collection(&c);
        let Imported::Collection(c2) = import_any(&exported).unwrap() else { panic!() };
        assert_eq!(c2.all_requests().len(), 2);
        assert_eq!(c2.all_requests()[1].method, HttpMethod::Post);
    }

    #[test]
    fn env_roundtrip() {
        let mut e = Environment::new("Dev");
        e.variables.push(KeyValue::new("a", "1"));
        let Imported::Environment(e2) = import_any(&export_environment(&e)).unwrap() else { panic!() };
        assert_eq!(e2.variables[0].value, "1");
    }
}
