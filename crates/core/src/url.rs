//! Keep the URL bar and the query-params table in sync.

use crate::model::KeyValue;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

/// Characters to encode inside a query component (keeps `{{ }}` readable).
const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'&').add(b'=').add(b'+').add(b'<').add(b'>');

pub fn encode_component(s: &str) -> String {
    utf8_percent_encode(s, QUERY).to_string()
}

pub fn decode_component(s: &str) -> String {
    let s = s.replace('+', " ");
    percent_decode_str(&s).decode_utf8_lossy().into_owned()
}

/// Split `url` into (base-without-query, query pairs, fragment).
pub fn split_url(url: &str) -> (String, Vec<(String, String)>, Option<String>) {
    let (no_frag, frag) = match url.find('#') {
        Some(i) => (&url[..i], Some(url[i + 1..].to_string())),
        None => (url, None),
    };
    match no_frag.find('?') {
        None => (no_frag.to_string(), vec![], frag),
        Some(i) => {
            let base = no_frag[..i].to_string();
            let q = &no_frag[i + 1..];
            let pairs = q
                .split('&')
                .filter(|p| !p.is_empty())
                .map(|p| match p.find('=') {
                    Some(eq) => (p[..eq].to_string(), p[eq + 1..].to_string()),
                    None => (p.to_string(), String::new()),
                })
                .collect();
            (base, pairs, frag)
        }
    }
}

/// Called after the URL text changed: rebuild the params table, keeping disabled rows.
pub fn params_from_url(url: &str, old: &[KeyValue]) -> Vec<KeyValue> {
    let (_, pairs, _) = split_url(url);
    let mut out: Vec<KeyValue> = Vec::new();
    let mut old_enabled = old.iter().filter(|p| p.enabled);
    for (k, v) in pairs {
        // Preserve descriptions positionally where keys match.
        let desc = old_enabled.next().filter(|o| o.key == k).map(|o| o.description.clone()).unwrap_or_default();
        out.push(KeyValue { enabled: true, key: k, value: v, description: desc });
    }
    out.extend(old.iter().filter(|p| !p.enabled && !p.is_blank()).cloned());
    out
}

/// Called after the params table changed: rebuild the URL's query string.
/// Values are written as typed (raw), mirroring what the user sees.
pub fn url_from_params(url: &str, params: &[KeyValue]) -> String {
    let (base, _, frag) = split_url(url);
    let q: Vec<String> = params
        .iter()
        .filter(|p| p.enabled && !(p.key.is_empty() && p.value.is_empty()))
        .map(|p| if p.value.is_empty() { p.key.clone() } else { format!("{}={}", p.key, p.value) })
        .collect();
    let mut out = base;
    if !q.is_empty() {
        out.push('?');
        out.push_str(&q.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(&f);
    }
    out
}

/// Ensure a scheme is present (defaults to http:// like most API clients).
pub fn normalize_url(url: &str) -> String {
    let t = url.trim();
    if t.contains("://") {
        t.to_string()
    } else {
        format!("http://{t}")
    }
}

/// Encode query values of an already-resolved URL so it is valid on the wire,
/// without double-encoding existing `%XX` sequences.
pub fn encode_query_for_wire(url: &str) -> String {
    let (base, pairs, frag) = split_url(url);
    if pairs.is_empty() {
        return url.to_string();
    }
    let q: Vec<String> = pairs
        .into_iter()
        .map(|(k, v)| {
            let k = encode_component(&decode_component(&k));
            if v.is_empty() {
                k
            } else {
                format!("{}={}", k, encode_component(&decode_component(&v)))
            }
        })
        .collect();
    let mut out = format!("{base}?{}", q.join("&"));
    if let Some(f) = frag {
        out.push('#');
        out.push_str(&f);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let url = "https://api.example.com/users?page=2&q={{term}}#top";
        let params = params_from_url(url, &[]);
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].value, "{{term}}");
        assert_eq!(url_from_params(url, &params), url);
    }

    #[test]
    fn disabled_params_preserved() {
        let mut params = params_from_url("http://x/?a=1&b=2", &[]);
        params[1].enabled = false;
        let url = url_from_params("http://x/?a=1&b=2", &params);
        assert_eq!(url, "http://x/?a=1");
        let again = params_from_url(&url, &params);
        assert_eq!(again.len(), 2);
        assert!(!again[1].enabled);
    }

    #[test]
    fn wire_encoding() {
        assert_eq!(encode_query_for_wire("http://x/?q=hello world&a=%20"), "http://x/?q=hello%20world&a=%20");
    }
}
