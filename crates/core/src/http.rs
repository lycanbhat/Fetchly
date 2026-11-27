//! Turning a [`Request`] into bytes on the wire and executing it.

use crate::model::*;
use crate::url::{encode_component, encode_query_for_wire, normalize_url};
use crate::vars::VarScope;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// A fully resolved request, ready to send.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PreparedRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    #[serde(with = "b64")]
    pub body: Vec<u8>,
    #[serde(default)]
    pub follow_redirects: bool,
    #[serde(default)]
    pub verify_ssl: bool,
    #[serde(default)]
    pub timeout_ms: u64,
}

impl PreparedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ResponseData {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    #[serde(with = "b64")]
    pub body: Vec<u8>,
    pub time_ms: f64,
    #[serde(default)]
    pub url: String,
}

impl ResponseData {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
    /// Approximate on-the-wire size of headers + body.
    pub fn size_bytes(&self) -> usize {
        self.body.len() + self.headers.iter().map(|(k, v)| k.len() + v.len() + 4).sum::<usize>()
    }
    pub fn cookies(&self) -> Vec<Cookie> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
            .filter_map(|(_, v)| Cookie::parse(v))
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: String,
    pub http_only: bool,
    pub secure: bool,
}

impl Cookie {
    pub fn parse(header: &str) -> Option<Cookie> {
        let mut parts = header.split(';');
        let (name, value) = parts.next()?.split_once('=')?;
        let mut c = Cookie { name: name.trim().into(), value: value.trim().into(), ..Default::default() };
        for attr in parts {
            let (k, v) = attr.split_once('=').unwrap_or((attr, ""));
            match k.trim().to_ascii_lowercase().as_str() {
                "domain" => c.domain = v.trim().into(),
                "path" => c.path = v.trim().into(),
                "expires" => c.expires = v.trim().into(),
                "max-age" => c.expires = format!("max-age={}", v.trim()),
                "httponly" => c.http_only = true,
                "secure" => c.secure = true,
                _ => {}
            }
        }
        Some(c)
    }
}

mod b64 {
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}

/// Work out the effective auth for a request given its parent chain.
pub fn effective_auth(request: &Request, folders: &[&Folder], collection: Option<&Collection>) -> Auth {
    if request.auth != Auth::Inherit {
        return request.auth.clone();
    }
    for f in folders.iter().rev() {
        if f.auth != Auth::Inherit {
            return f.auth.clone();
        }
    }
    match collection {
        Some(c) if c.auth != Auth::Inherit => c.auth.clone(),
        _ => Auth::None,
    }
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name))
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path).map_err(|e| format!("Could not read file '{path}': {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Err("File uploads from disk are only supported in the desktop app".into())
    }
}

fn guess_mime(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "json" => "application/json",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

/// Resolve variables, auth and body into a [`PreparedRequest`].
pub fn prepare(request: &Request, auth: &Auth, scope: &VarScope) -> Result<PreparedRequest, String> {
    let r = |s: &str| scope.resolve(s);
    if request.url.trim().is_empty() {
        return Err("Enter a URL to send the request".into());
    }
    let mut url = normalize_url(&r(&request.url));
    let mut headers: Vec<(String, String)> = request
        .headers
        .iter()
        .filter(|h| h.enabled && !h.key.trim().is_empty())
        .map(|h| (r(&h.key).trim().to_string(), r(&h.value)))
        .collect();

    // --- auth ---
    match auth {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => {
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", r(username), r(password)));
            if !has_header(&headers, "authorization") {
                headers.push(("Authorization".into(), format!("Basic {token}")));
            }
        }
        Auth::Bearer { token } => {
            if !has_header(&headers, "authorization") {
                headers.push(("Authorization".into(), format!("Bearer {}", r(token))));
            }
        }
        Auth::OAuth2 { access_token, header_prefix } => {
            let prefix = if header_prefix.trim().is_empty() { "Bearer".to_string() } else { r(header_prefix) };
            if !has_header(&headers, "authorization") {
                headers.push(("Authorization".into(), format!("{prefix} {}", r(access_token))));
            }
        }
        Auth::ApiKey { key, value, location } => {
            let (k, v) = (r(key), r(value));
            if !k.is_empty() {
                match location {
                    ApiKeyLocation::Header => headers.push((k, v)),
                    ApiKeyLocation::Query => {
                        let sep = if url.contains('?') { '&' } else { '?' };
                        url = format!("{url}{sep}{}={}", encode_component(&k), encode_component(&v));
                    }
                }
            }
        }
    }

    // --- body ---
    let method = request.method;
    let mut body = Vec::new();
    let set_ct = |headers: &mut Vec<(String, String)>, ct: &str| {
        if !has_header(headers, "content-type") {
            headers.push(("Content-Type".into(), ct.to_string()));
        }
    };
    match &request.body {
        Body::None => {}
        Body::Raw { language, text } => {
            body = r(text).into_bytes();
            if !body.is_empty() {
                set_ct(&mut headers, language.content_type());
            }
        }
        Body::UrlEncoded(fields) => {
            let s = fields
                .iter()
                .filter(|f| f.enabled && !f.key.is_empty())
                .map(|f| format!("{}={}", encode_component(&r(&f.key)), encode_component(&r(&f.value)).replace("%20", "+")))
                .collect::<Vec<_>>()
                .join("&");
            body = s.into_bytes();
            set_ct(&mut headers, "application/x-www-form-urlencoded");
        }
        Body::FormData(fields) => {
            let boundary = format!("----FetchlyBoundary{}", uuid::Uuid::new_v4().simple());
            for f in fields.iter().filter(|f| f.enabled && !f.key.is_empty()) {
                body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
                match f.kind {
                    FormFieldKind::Text => {
                        body.extend_from_slice(
                            format!("Content-Disposition: form-data; name=\"{}\"\r\n", r(&f.key)).as_bytes(),
                        );
                        if !f.content_type.is_empty() {
                            body.extend_from_slice(format!("Content-Type: {}\r\n", f.content_type).as_bytes());
                        }
                        body.extend_from_slice(b"\r\n");
                        body.extend_from_slice(r(&f.value).as_bytes());
                    }
                    FormFieldKind::File => {
                        let path = r(&f.value);
                        let data = read_file(&path)?;
                        let filename = path.rsplit(['/', '\\']).next().unwrap_or("file");
                        let ct = if f.content_type.is_empty() { guess_mime(&path).to_string() } else { f.content_type.clone() };
                        body.extend_from_slice(
                            format!(
                                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                                r(&f.key),
                                filename,
                                ct
                            )
                            .as_bytes(),
                        );
                        body.extend_from_slice(&data);
                    }
                }
                body.extend_from_slice(b"\r\n");
            }
            body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            // multipart content type must carry our boundary, so always override.
            headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type"));
            headers.push(("Content-Type".into(), format!("multipart/form-data; boundary={boundary}")));
        }
        Body::Binary { path } => {
            if !path.is_empty() {
                let p = r(path);
                body = read_file(&p)?;
                set_ct(&mut headers, guess_mime(&p));
            }
        }
        Body::GraphQL { query, variables } => {
            let vars_text = r(variables);
            let vars: serde_json::Value = if vars_text.trim().is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_str(&vars_text).map_err(|e| format!("GraphQL variables are not valid JSON: {e}"))?
            };
            let mut obj = serde_json::json!({ "query": r(query) });
            if !vars.is_null() {
                obj["variables"] = vars;
            }
            body = serde_json::to_vec(&obj).unwrap_or_default();
            set_ct(&mut headers, "application/json");
        }
    }

    if !has_header(&headers, "user-agent") {
        headers.push(("User-Agent".into(), format!("Fetchly/{}", env!("CARGO_PKG_VERSION"))));
    }
    if !has_header(&headers, "accept") {
        headers.push(("Accept".into(), "*/*".into()));
    }

    Ok(PreparedRequest {
        method: method.as_str().to_string(),
        url: encode_query_for_wire(&url),
        headers,
        body,
        follow_redirects: request.settings.follow_redirects,
        verify_ssl: request.settings.verify_ssl,
        timeout_ms: request.settings.timeout_ms,
    })
}

/// How a request is transported.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Transport {
    /// Send directly from this process (desktop) or from the browser (web, subject to CORS).
    #[default]
    Direct,
    /// Relay through a Fetchly server's `/api/proxy` endpoint (bypasses browser CORS limits).
    Proxy {
        endpoint: String,
        /// Sent as `X-Fetchly-Token` when the server requires one.
        #[serde(default)]
        token: String,
    },
}

/// Execute a prepared request.
pub async fn execute(req: PreparedRequest, transport: Transport) -> Result<ResponseData, String> {
    match transport {
        Transport::Direct => direct::send(req).await,
        Transport::Proxy { endpoint, token } => send_via_proxy(req, &endpoint, &token).await,
    }
}

/// Resolve a redirect `Location` against the URL that returned it.
pub fn resolve_location(base: &str, location: &str) -> String {
    let loc = location.trim();
    if loc.contains("://") {
        return loc.to_string();
    }
    let (scheme, rest) = base.split_once("://").unwrap_or(("http", base));
    if let Some(stripped) = loc.strip_prefix("//") {
        return format!("{scheme}://{stripped}");
    }
    let host_end = rest.find('/').unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..host_end]);
    if loc.starts_with('/') {
        return format!("{origin}{loc}");
    }
    let path = rest[host_end..].split(['?', '#']).next().unwrap_or("");
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    format!("{origin}{dir}{loc}")
}

/// Relay through the proxy. Redirects are followed here, hop by hop, so cookies set on
/// intermediate responses land in the app's jar exactly as in the desktop app.
async fn send_via_proxy(req: PreparedRequest, endpoint: &str, token: &str) -> Result<ResponseData, String> {
    let follow = req.follow_redirects;
    let user_cookie = req.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie"));
    let start = web_time::Instant::now();
    let mut cur = req;
    cur.follow_redirects = false;
    for hop in 0..=10 {
        if !user_cookie {
            cur.headers.retain(|(k, _)| !k.eq_ignore_ascii_case("cookie"));
            if let Some(c) = crate::cookies::global().read().unwrap().header_for(&cur.url) {
                cur.headers.push(("Cookie".into(), c));
            }
        }
        let mut resp = proxy_once(&cur, endpoint, token).await?;
        {
            let mut jar = crate::cookies::global().write().unwrap();
            for (_, v) in resp.headers.iter().filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie")) {
                jar.store(&cur.url, v);
            }
        }
        let location = resp.header("location").map(String::from);
        if follow && (300..400).contains(&resp.status) && resp.status != 304 && hop < 10 {
            if let Some(loc) = location {
                if matches!(resp.status, 301..=303) && cur.method != "GET" && cur.method != "HEAD" {
                    cur.method = "GET".into();
                    cur.body.clear();
                    cur.headers.retain(|(k, _)| !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("content-length"));
                }
                cur.url = resolve_location(&cur.url, &loc);
                continue;
            }
        }
        resp.url = cur.url.clone();
        resp.time_ms = start.elapsed().as_secs_f64() * 1000.0;
        return Ok(resp);
    }
    Err("Too many redirects".into())
}

async fn proxy_once(req: &PreparedRequest, endpoint: &str, token: &str) -> Result<ResponseData, String> {
    let client = reqwest::Client::new();
    let payload = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    let mut rb = client.post(endpoint).header("content-type", "application/json");
    if !token.is_empty() {
        rb = rb.header("x-fetchly-token", token);
    }
    let resp = rb.body(payload).send().await.map_err(|e| format!("Could not reach proxy at {endpoint}: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(String::from_utf8_lossy(&bytes).into_owned());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid proxy response: {e}"))
}

#[cfg(not(target_arch = "wasm32"))]
pub mod direct {
    use super::*;
    use std::sync::Arc;

    /// Forget every cookie in the shared jar.
    pub fn clear_cookies() {
        crate::cookies::global().write().unwrap().clear();
    }

    /// Send using the app's shared cookie jar.
    pub async fn send(req: PreparedRequest) -> Result<ResponseData, String> {
        send_with(req, true).await
    }

    /// Send without any cookie jar (used by the multi-user proxy server).
    pub async fn send_without_cookies(req: PreparedRequest) -> Result<ResponseData, String> {
        send_with(req, false).await
    }

    async fn send_with(req: PreparedRequest, use_jar: bool) -> Result<ResponseData, String> {
        let mut builder = reqwest::Client::builder().redirect(if req.follow_redirects {
                reqwest::redirect::Policy::limited(10)
            } else {
                reqwest::redirect::Policy::none()
            })
            .tls_danger_accept_invalid_certs(!req.verify_ssl);
        if use_jar {
            builder = builder.cookie_provider(Arc::new(crate::cookies::GlobalJarProvider));
        }
        if req.timeout_ms > 0 {
            builder = builder.timeout(std::time::Duration::from_millis(req.timeout_ms));
        }
        let client = builder.build().map_err(|e| e.to_string())?;
        let method = reqwest::Method::from_bytes(req.method.as_bytes()).map_err(|e| e.to_string())?;
        let mut rb = client.request(method, &req.url);
        for (k, v) in &req.headers {
            rb = rb.header(k, v);
        }
        if !req.body.is_empty() {
            rb = rb.body(req.body.clone());
        }
        let start = web_time::Instant::now();
        let resp = rb.send().await.map_err(describe_error)?;
        let status = resp.status();
        let url = resp.url().to_string();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        let body = resp.bytes().await.map_err(describe_error)?.to_vec();
        Ok(ResponseData {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            headers,
            body,
            time_ms: start.elapsed().as_secs_f64() * 1000.0,
            url,
        })
    }

    fn describe_error(e: reqwest::Error) -> String {
        use std::error::Error as _;
        let mut msg = e.to_string();
        let mut src = e.source();
        while let Some(s) = src {
            msg.push_str(": ");
            msg.push_str(&s.to_string());
            src = s.source();
        }
        if e.is_timeout() {
            msg = format!("Request timed out ({msg})");
        } else if e.is_connect() {
            msg = format!("Could not connect ({msg})");
        }
        msg
    }
}

#[cfg(target_arch = "wasm32")]
pub mod direct {
    use super::*;

    pub fn clear_cookies() {}

    pub async fn send(req: PreparedRequest) -> Result<ResponseData, String> {
        let client = reqwest::Client::new();
        let method = reqwest::Method::from_bytes(req.method.as_bytes()).map_err(|e| e.to_string())?;
        let mut rb = client.request(method, &req.url);
        for (k, v) in &req.headers {
            // Browsers refuse to let scripts set these; skip rather than fail.
            if k.eq_ignore_ascii_case("user-agent") || k.eq_ignore_ascii_case("cookie") || k.eq_ignore_ascii_case("host") {
                continue;
            }
            rb = rb.header(k, v);
        }
        if !req.body.is_empty() {
            rb = rb.body(req.body.clone());
        }
        let start = web_time::Instant::now();
        let resp = rb.send().await.map_err(|e| {
            format!(
                "{e}. The browser blocked or could not complete this request — usually CORS. \
                 Enable the Fetchly proxy in Settings (served by fetchly-server) to bypass browser restrictions."
            )
        })?;
        let status = resp.status();
        let url = resp.url().to_string();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        Ok(ResponseData {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            headers,
            body,
            time_ms: start.elapsed().as_secs_f64() * 1000.0,
            url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_auth_and_body() {
        let mut scope = VarScope::default();
        scope.environment.insert("base".into(), "api.test".into());
        scope.environment.insert("tok".into(), "abc".into());
        let req = Request {
            method: HttpMethod::Post,
            url: "{{base}}/items?q=a b".into(),
            body: Body::UrlEncoded(vec![KeyValue::new("name", "x y")]),
            ..Default::default()
        };
        let p = prepare(&req, &Auth::Bearer { token: "{{tok}}".into() }, &scope).unwrap();
        assert_eq!(p.url, "http://api.test/items?q=a%20b");
        assert_eq!(p.header("authorization"), Some("Bearer abc"));
        assert_eq!(p.body_text(), "name=x+y");
        assert_eq!(p.header("content-type"), Some("application/x-www-form-urlencoded"));
    }

    #[test]
    fn multipart() {
        let req = Request {
            method: HttpMethod::Post,
            url: "http://x".into(),
            body: Body::FormData(vec![FormField { enabled: true, key: "a".into(), value: "1".into(), ..Default::default() }]),
            ..Default::default()
        };
        let p = prepare(&req, &Auth::None, &VarScope::default()).unwrap();
        let ct = p.header("content-type").unwrap();
        assert!(ct.starts_with("multipart/form-data; boundary="));
        assert!(p.body_text().contains("name=\"a\"\r\n\r\n1\r\n"));
    }

    #[test]
    fn redirects_resolve() {
        assert_eq!(resolve_location("https://a.io/x/y?q=1", "/z"), "https://a.io/z");
        assert_eq!(resolve_location("https://a.io/x/y", "z"), "https://a.io/x/z");
        assert_eq!(resolve_location("https://a.io/x/y", "//b.io/p"), "https://b.io/p");
        assert_eq!(resolve_location("http://a.io:8080", "/p"), "http://a.io:8080/p");
        assert_eq!(resolve_location("https://a.io/x", "https://c.io/"), "https://c.io/");
    }

    #[test]
    fn cookie_parse() {
        let c = Cookie::parse("sid=xyz; Path=/; HttpOnly; Secure").unwrap();
        assert_eq!((c.name.as_str(), c.value.as_str(), c.http_only, c.secure), ("sid", "xyz", true, true));
    }
}
