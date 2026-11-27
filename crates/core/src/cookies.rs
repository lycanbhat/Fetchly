//! A browsable, editable cookie jar (RFC 6265 subset) shared by every request.
//!
//! On desktop it is plugged into reqwest so cookies set during redirects are kept too.
//! When requests go through the Fetchly proxy, cookies are attached and captured manually.

use serde::{Deserialize, Serialize};
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    /// Lowercase, without a leading dot.
    pub domain: String,
    /// `true` = only sent to exactly `domain` (no `Domain` attribute was given).
    #[serde(default)]
    pub host_only: bool,
    pub path: String,
    /// Unix seconds. `None` = session cookie.
    #[serde(default)]
    pub expires: Option<i64>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub same_site: String,
}

impl StoredCookie {
    pub fn new(domain: &str, name: &str, value: &str) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            domain: normalize_domain(domain),
            host_only: false,
            path: "/".into(),
            expires: None,
            secure: false,
            http_only: false,
            same_site: String::new(),
        }
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expires.is_some_and(|e| e <= now)
    }

    /// `Set-Cookie`-style representation, as shown in the cookie manager.
    pub fn to_header_string(&self) -> String {
        let mut s = format!("{}={}; Path={}", self.name, self.value, self.path);
        if !self.host_only {
            s.push_str(&format!("; Domain={}", self.domain));
        }
        if let Some(e) = self.expires.and_then(|e| chrono::DateTime::from_timestamp(e, 0)) {
            s.push_str(&format!("; Expires={}", e.format("%a, %d %b %Y %H:%M:%S GMT")));
        }
        if self.secure {
            s.push_str("; Secure");
        }
        if self.http_only {
            s.push_str("; HttpOnly");
        }
        if !self.same_site.is_empty() {
            s.push_str(&format!("; SameSite={}", self.same_site));
        }
        s
    }

    fn domain_matches(&self, host: &str) -> bool {
        if self.host_only {
            host == self.domain
        } else {
            host == self.domain || host.ends_with(&format!(".{}", self.domain))
        }
    }

    fn path_matches(&self, req_path: &str) -> bool {
        if req_path == self.path {
            return true;
        }
        req_path.starts_with(&self.path) && (self.path.ends_with('/') || req_path[self.path.len()..].starts_with('/'))
    }
}

pub fn normalize_domain(d: &str) -> String {
    d.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Split a URL into (scheme, host, path) without pulling in a URL parser.
pub fn url_parts(url: &str) -> Option<(String, String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = if host_port.starts_with('[') {
        host_port.split(']').next().map(|h| format!("{h}]")).unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    let path_q = &rest[end..];
    let path = path_q.split(['?', '#']).next().unwrap_or("");
    let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
    Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase(), path))
}

fn default_path(req_path: &str) -> String {
    match req_path.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => req_path[..i].to_string(),
    }
}

fn parse_expires(v: &str) -> Option<i64> {
    let v = v.trim();
    chrono::DateTime::parse_from_rfc2822(v)
        .ok()
        .or_else(|| chrono::DateTime::parse_from_rfc2822(&v.replace('-', " ")).ok())
        .map(|d| d.timestamp())
        .or_else(|| chrono::NaiveDateTime::parse_from_str(v, "%a, %d-%b-%Y %H:%M:%S GMT").ok().map(|d| d.and_utc().timestamp()))
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CookieJar {
    pub cookies: Vec<StoredCookie>,
}

impl CookieJar {
    /// Parse one `Set-Cookie` header received from `url` and store it.
    pub fn store(&mut self, url: &str, header: &str) {
        let Some((_, host, req_path)) = url_parts(url) else { return };
        let mut parts = header.split(';');
        let Some((name, value)) = parts.next().and_then(|p| p.split_once('=')) else { return };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let mut c = StoredCookie {
            name: name.into(),
            value: value.trim().trim_matches('"').into(),
            domain: host.clone(),
            host_only: true,
            path: default_path(&req_path),
            expires: None,
            secure: false,
            http_only: false,
            same_site: String::new(),
        };
        let mut max_age: Option<i64> = None;
        for attr in parts {
            let (k, v) = attr.split_once('=').unwrap_or((attr, ""));
            match k.trim().to_ascii_lowercase().as_str() {
                "domain" => {
                    let d = normalize_domain(v);
                    // Reject cookies for unrelated domains.
                    if !d.is_empty() && (host == d || host.ends_with(&format!(".{d}"))) {
                        c.domain = d;
                        c.host_only = false;
                    } else if !d.is_empty() {
                        return;
                    }
                }
                "path" if v.trim().starts_with('/') => c.path = v.trim().into(),
                "expires" => c.expires = parse_expires(v),
                "max-age" => max_age = v.trim().parse().ok(),
                "secure" => c.secure = true,
                "httponly" => c.http_only = true,
                "samesite" => c.same_site = v.trim().into(),
                _ => {}
            }
        }
        if let Some(m) = max_age {
            c.expires = Some(now() + m);
        }
        self.upsert(c);
    }

    /// Insert or replace (same name + domain + path). Expired cookies delete their match.
    pub fn upsert(&mut self, c: StoredCookie) {
        self.cookies.retain(|x| !(x.name == c.name && x.domain == c.domain && x.path == c.path));
        if !c.is_expired(now()) {
            self.cookies.push(c);
        }
    }

    pub fn remove(&mut self, domain: &str, name: &str, path: &str) {
        self.cookies.retain(|x| !(x.domain == domain && x.name == name && x.path == path));
    }

    pub fn clear_domain(&mut self, domain: &str) {
        self.cookies.retain(|x| x.domain != domain);
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn purge_expired(&mut self) {
        let n = now();
        self.cookies.retain(|c| !c.is_expired(n));
    }

    /// Cookies that would be sent to `url`, most specific path first.
    pub fn matching(&self, url: &str) -> Vec<&StoredCookie> {
        let Some((scheme, host, path)) = url_parts(url) else { return vec![] };
        let n = now();
        let mut v: Vec<&StoredCookie> = self
            .cookies
            .iter()
            .filter(|c| !c.is_expired(n) && c.domain_matches(&host) && c.path_matches(&path) && (!c.secure || scheme == "https" || host == "localhost"))
            .collect();
        v.sort_by(|a, b| b.path.len().cmp(&a.path.len()));
        v
    }

    /// Value for a `Cookie` request header, if any cookie applies.
    pub fn header_for(&self, url: &str) -> Option<String> {
        let m = self.matching(url);
        if m.is_empty() {
            None
        } else {
            Some(m.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; "))
        }
    }

    /// Sorted list of domains that have cookies.
    pub fn domains(&self) -> Vec<String> {
        let mut d: Vec<String> = self.cookies.iter().map(|c| c.domain.clone()).collect();
        d.sort();
        d.dedup();
        d
    }

    pub fn for_domain(&self, domain: &str) -> Vec<&StoredCookie> {
        self.cookies.iter().filter(|c| c.domain == domain).collect()
    }
}

/// The process-wide jar used by the app.
pub fn global() -> &'static RwLock<CookieJar> {
    static JAR: OnceLock<RwLock<CookieJar>> = OnceLock::new();
    JAR.get_or_init(|| RwLock::new(CookieJar::default()))
}

/// Adapter so reqwest uses the global jar (desktop).
#[cfg(not(target_arch = "wasm32"))]
pub struct GlobalJarProvider;

#[cfg(not(target_arch = "wasm32"))]
impl reqwest::cookie::CookieStore for GlobalJarProvider {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>, url: &reqwest::Url) {
        let mut jar = global().write().unwrap();
        for h in cookie_headers {
            if let Ok(s) = h.to_str() {
                jar.store(url.as_str(), s);
            }
        }
    }

    fn cookies(&self, url: &reqwest::Url) -> Option<reqwest::header::HeaderValue> {
        global().read().unwrap().header_for(url.as_str()).and_then(|h| reqwest::header::HeaderValue::from_str(&h).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_match() {
        let mut j = CookieJar::default();
        j.store("https://api.example.com/v1/login", "sid=abc; Path=/; HttpOnly; Secure");
        j.store("https://api.example.com/v1/login", "pref=dark; Domain=.example.com; Path=/; Max-Age=3600");
        j.store("https://api.example.com/v1/login", "local=1"); // default path /v1
        j.store("https://api.example.com/", "evil=1; Domain=other.com"); // rejected
        assert_eq!(j.cookies.len(), 3);
        assert_eq!(j.header_for("https://api.example.com/v1/users").as_deref(), Some("local=1; sid=abc; pref=dark"));
        // secure cookie not sent over http, host-only cookie not sent to subdomain siblings
        assert_eq!(j.header_for("http://www.example.com/").as_deref(), Some("pref=dark"));
        assert_eq!(j.domains(), vec!["api.example.com".to_string(), "example.com".to_string()]);
    }

    #[test]
    fn expiry_deletes() {
        let mut j = CookieJar::default();
        j.store("https://x.io/", "a=1");
        j.store("https://x.io/", "a=; Expires=Thu, 01 Jan 1970 00:00:00 GMT");
        assert!(j.cookies.is_empty());
        j.store("https://x.io/", "b=2; Expires=Wed, 21 Oct 2099 07:28:00 GMT");
        assert!(j.cookies[0].expires.is_some());
        assert!(j.cookies[0].to_header_string().contains("Expires=Wed, 21 Oct 2099"));
    }

    #[test]
    fn url_parsing() {
        assert_eq!(url_parts("http://user@localhost:8080/a/b?x=1"), Some(("http".into(), "localhost".into(), "/a/b".into())));
        assert_eq!(url_parts("https://x.io"), Some(("https".into(), "x.io".into(), "/".into())));
    }
}
