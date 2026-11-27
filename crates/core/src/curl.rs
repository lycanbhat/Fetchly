//! Import requests from cURL command lines.

use crate::model::*;
use crate::url::params_from_url;

/// Parse a `curl ...` command into a [`Request`].
pub fn parse_curl(cmd: &str) -> Result<Request, String> {
    // Join line continuations (bash `\`, Windows cmd `^`, PowerShell backtick).
    let joined = cmd
        .replace("\\\r\n", " ")
        .replace("\\\n", " ")
        .replace("^\r\n", " ")
        .replace("^\n", " ")
        .replace("`\r\n", " ")
        .replace("`\n", " ");
    let args = shell_words::split(joined.trim()).map_err(|e| format!("Could not parse command: {e}"))?;
    let mut it = args.into_iter().peekable();
    match it.next() {
        Some(c) if c == "curl" || c.ends_with("/curl") || c.eq_ignore_ascii_case("curl.exe") => {}
        _ => return Err("Command must start with `curl`".into()),
    }

    let mut req = Request { auth: Auth::None, ..Default::default() };
    let mut method: Option<HttpMethod> = None;
    let mut data: Vec<String> = vec![];
    let mut data_urlencode: Vec<String> = vec![];
    let mut form: Vec<FormField> = vec![];
    let mut get_mode = false;
    let mut url: Option<String> = None;

    while let Some(arg) = it.next() {
        // Support --flag=value
        let (flag, inline_val) = if arg.starts_with("--") {
            match arg.split_once('=') {
                Some((f, v)) => (f.to_string(), Some(v.to_string())),
                None => (arg.clone(), None),
            }
        } else {
            (arg.clone(), None)
        };
        let value = |it: &mut std::iter::Peekable<std::vec::IntoIter<String>>| -> Result<String, String> {
            if let Some(v) = inline_val.clone() {
                return Ok(v);
            }
            it.next().ok_or_else(|| format!("Missing value for {flag}"))
        };
        match flag.as_str() {
            "-X" | "--request" => method = HttpMethod::parse(&value(&mut it)?),
            "-H" | "--header" => {
                let h = value(&mut it)?;
                if let Some((k, v)) = h.split_once(':') {
                    req.headers.push(KeyValue::new(k.trim(), v.trim()));
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" | "--json" => {
                let v = value(&mut it)?;
                if flag == "--json" {
                    req.headers.push(KeyValue::new("Content-Type", "application/json"));
                }
                data.push(v);
            }
            "--data-urlencode" => data_urlencode.push(value(&mut it)?),
            "-F" | "--form" | "--form-string" => {
                let v = value(&mut it)?;
                if let Some((k, val)) = v.split_once('=') {
                    let (kind, val) = match val.strip_prefix('@') {
                        Some(p) if flag != "--form-string" => (FormFieldKind::File, p.to_string()),
                        _ => (FormFieldKind::Text, val.to_string()),
                    };
                    form.push(FormField { enabled: true, key: k.into(), value: val, kind, content_type: String::new() });
                }
            }
            "-u" | "--user" => {
                let v = value(&mut it)?;
                let (u, p) = v.split_once(':').unwrap_or((&v, ""));
                req.auth = Auth::Basic { username: u.into(), password: p.into() };
            }
            "-A" | "--user-agent" => req.headers.push(KeyValue::new("User-Agent", value(&mut it)?)),
            "-e" | "--referer" => req.headers.push(KeyValue::new("Referer", value(&mut it)?)),
            "-b" | "--cookie" => req.headers.push(KeyValue::new("Cookie", value(&mut it)?)),
            "-G" | "--get" => get_mode = true,
            "-I" | "--head" => method = Some(HttpMethod::Head),
            "-k" | "--insecure" => req.settings.verify_ssl = false,
            "-L" | "--location" => req.settings.follow_redirects = true,
            "--url" => url = Some(value(&mut it)?),
            "-m" | "--max-time" => {
                let secs: f64 = value(&mut it)?.parse().unwrap_or(0.0);
                req.settings.timeout_ms = (secs * 1000.0) as u64;
            }
            "-o" | "--output" | "-w" | "--write-out" | "--connect-timeout" | "--retry" | "-x" | "--proxy" | "-c"
            | "--cookie-jar" => {
                let _ = value(&mut it)?; // flags with values we ignore
            }
            f if f.starts_with('-') => {} // boolean flags we ignore (-s, -v, --compressed, ...)
            _ => {
                if url.is_none() {
                    url = Some(arg);
                }
            }
        }
    }

    let url = url.ok_or("No URL found in cURL command")?;
    req.url = url;

    if get_mode && (!data.is_empty() || !data_urlencode.is_empty()) {
        let mut q: Vec<String> = data.drain(..).collect();
        q.extend(data_urlencode.drain(..));
        let sep = if req.url.contains('?') { '&' } else { '?' };
        req.url = format!("{}{}{}", req.url, sep, q.join("&"));
    }

    let content_type = req
        .headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default();

    if !form.is_empty() {
        req.body = Body::FormData(form);
        req.headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
    } else if !data.is_empty() || !data_urlencode.is_empty() {
        let joined = data.join("&");
        if !data_urlencode.is_empty() || content_type.contains("x-www-form-urlencoded") || (content_type.is_empty() && looks_like_form(&joined)) {
            let mut fields: Vec<KeyValue> = vec![];
            for part in joined.split('&').filter(|s| !s.is_empty()).map(String::from).chain(data_urlencode) {
                let (k, v) = part.split_once('=').unwrap_or((&part, ""));
                fields.push(KeyValue::new(crate::url::decode_component(k), crate::url::decode_component(v)));
            }
            req.body = Body::UrlEncoded(fields);
            req.headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
        } else {
            let language = if content_type.contains("json") || joined.trim_start().starts_with(['{', '[']) {
                RawLanguage::Json
            } else if content_type.contains("xml") {
                RawLanguage::Xml
            } else if content_type.contains("html") {
                RawLanguage::Html
            } else {
                RawLanguage::Text
            };
            req.body = Body::Raw { language, text: joined };
        }
    }

    req.method = method.unwrap_or(if matches!(req.body, Body::None) || get_mode { HttpMethod::Get } else { HttpMethod::Post });
    req.params = params_from_url(&req.url, &[]);
    req.name = derive_name(&req.url);
    Ok(req)
}

fn looks_like_form(s: &str) -> bool {
    !s.trim_start().starts_with(['{', '[', '<']) && s.contains('=') && !s.contains('\n')
}

/// A friendly default name from the URL path.
pub fn derive_name(url: &str) -> String {
    let no_scheme = url.split("://").nth(1).unwrap_or(url);
    let path = no_scheme.split(['?', '#']).next().unwrap_or("");
    let p = path.split_once('/').map(|(_, p)| p).unwrap_or("");
    if p.is_empty() {
        path.to_string()
    } else {
        format!("/{p}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_post() {
        let r = parse_curl(
            "curl -X POST 'https://api.example.com/v1/users?x=1' \\\n  -H 'Content-Type: application/json' \\\n  -H 'Authorization: Bearer t' \\\n  --data-raw '{\"name\":\"a\"}'",
        )
        .unwrap();
        assert_eq!(r.method, HttpMethod::Post);
        assert_eq!(r.url, "https://api.example.com/v1/users?x=1");
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.params.len(), 1);
        assert!(matches!(r.body, Body::Raw { language: RawLanguage::Json, .. }));
    }

    #[test]
    fn parses_form_and_user() {
        let r = parse_curl("curl https://x.io/up -u bob:pw -F 'file=@/tmp/a.txt' -F name=z").unwrap();
        assert_eq!(r.method, HttpMethod::Post);
        assert!(matches!(r.auth, Auth::Basic { .. }));
        match r.body {
            Body::FormData(f) => {
                assert_eq!(f[0].kind, FormFieldKind::File);
                assert_eq!(f[1].value, "z");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_urlencoded() {
        let r = parse_curl("curl https://x.io -d a=1 -d b=hello%20w").unwrap();
        match r.body {
            Body::UrlEncoded(f) => assert_eq!(f[1].value, "hello w"),
            _ => panic!("{:?}", r.body),
        }
    }
}
