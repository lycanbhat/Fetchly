//! Syntax highlighting to escaped HTML (rendered via `dangerous_inner_html`).
//! All text is HTML-escaped here, so the output is safe to inject.

use fetchly_core::vars::VarScope;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// JSON / XML / plain data
    Data,
    /// Rhai / JavaScript-like scripts
    Script,
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn span(out: &mut String, class: &str, text: &str) {
    if class.is_empty() {
        out.push_str(&escape(text));
    } else {
        out.push_str("<span class=\"");
        out.push_str(class);
        out.push_str("\">");
        out.push_str(&escape(text));
        out.push_str("</span>");
    }
}

const SCRIPT_KEYWORDS: &[&str] = &[
    "let", "const", "fn", "if", "else", "for", "in", "while", "loop", "return", "break", "continue", "true", "false", "null", "throw", "try", "catch", "switch", "import", "export", "this", "pm", "console",
];

/// Highlight `text` as HTML.
pub fn to_html(text: &str, lang: Lang) -> String {
    let script = lang == Lang::Script;
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        if script && ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let end = text[i..].find('\n').map(|e| i + e).unwrap_or(bytes.len());
            span(&mut out, "tok-comment", &text[i..end]);
            i = end;
            continue;
        }
        if script && ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let end = text[i + 2..].find("*/").map(|e| i + 2 + e + 2).unwrap_or(bytes.len());
            span(&mut out, "tok-comment", &text[i..end]);
            i = end;
            continue;
        }
        if ch == b'{' && bytes.get(i + 1) == Some(&b'{') {
            if let Some(e) = text[i + 2..].find("}}") {
                let end = i + 2 + e + 2;
                span(&mut out, "tok-var", &text[i..end]);
                i = end;
                continue;
            }
        }
        if ch == b'"' || (script && (ch == b'\'' || ch == b'`')) {
            let quote = ch;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != quote && bytes[j] != b'\n' {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(bytes.len());
            let rest = text[end..].trim_start_matches([' ', '\t']);
            let class = if !script && rest.starts_with(':') { "tok-key" } else { "tok-str" };
            span(&mut out, class, &text[i..end]);
            i = end;
            continue;
        }
        if ch.is_ascii_digit() || (ch == b'-' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit()) && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())) {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.' || ((bytes[j] == b'+' || bytes[j] == b'-') && matches!(bytes[j - 1], b'e' | b'E'))) {
                j += 1;
            }
            span(&mut out, "tok-num", &text[i..j]);
            i = j;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == b'_' || ch == b'$' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$') {
                j += 1;
            }
            let word = &text[i..j];
            let kw = if script { SCRIPT_KEYWORDS.contains(&word) } else { matches!(word, "true" | "false" | "null") };
            span(&mut out, if kw { "tok-kw" } else { "" }, word);
            i = j;
            continue;
        }
        if ch == b'<' && !script {
            // XML/HTML tag
            let end = text[i..].find('>').map(|e| i + e + 1).unwrap_or(bytes.len());
            span(&mut out, "tok-tag", &text[i..end]);
            i = end;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && !text.is_char_boundary(j) {
            j += 1;
        }
        let class = if b"{}[],:;()".contains(&ch) { "tok-punct" } else { "" };
        span(&mut out, class, &text[i..j]);
        i = j;
    }
    // A trailing newline needs a character after it to occupy a line in a <pre>.
    if text.ends_with('\n') {
        out.push(' ');
    }
    out
}

/// Highlight `{{variables}}` in plain text: green when defined, red when not.
pub fn vars_html(text: &str, scope: &VarScope) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&escape(&rest[..start]));
        let after = &rest[start..];
        match after[2..].find("}}") {
            Some(end) => {
                let token = &after[..end + 4];
                let ok = scope.get(token[2..token.len() - 2].trim()).is_some();
                span(&mut out, if ok { "tok-var" } else { "tok-var-missing" }, token);
                rest = &after[end + 4..];
            }
            None => {
                out.push_str(&escape(after));
                rest = "";
            }
        }
    }
    out.push_str(&escape(rest));
    out
}

/// Wrap case-insensitive matches of `needle` in <mark>.
pub fn mark_matches(text: &str, needle: &str) -> (String, usize) {
    if needle.is_empty() {
        return (escape(text), 0);
    }
    let lower = text.to_lowercase();
    let n = needle.to_lowercase();
    if lower.len() != text.len() {
        // Non-ASCII case folding changed lengths; fall back to exact matching.
        return mark_exact(text, needle);
    }
    let mut out = String::new();
    let mut count = 0;
    let mut last = 0;
    for (pos, _) in lower.match_indices(&n) {
        out.push_str(&escape(&text[last..pos]));
        out.push_str("<mark>");
        out.push_str(&escape(&text[pos..pos + n.len()]));
        out.push_str("</mark>");
        last = pos + n.len();
        count += 1;
    }
    out.push_str(&escape(&text[last..]));
    (out, count)
}

fn mark_exact(text: &str, needle: &str) -> (String, usize) {
    let mut out = String::new();
    let mut count = 0;
    let mut last = 0;
    for (pos, m) in text.match_indices(needle) {
        out.push_str(&escape(&text[last..pos]));
        out.push_str("<mark>");
        out.push_str(&escape(m));
        out.push_str("</mark>");
        last = pos + m.len();
        count += 1;
    }
    out.push_str(&escape(&text[last..]));
    (out, count)
}

pub fn pretty_json(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

/// Small XML/HTML indenter for the Pretty view.
pub fn pretty_xml(text: &str) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    let mut rest = text.trim();
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix('<') {
            let end = stripped.find('>').map(|e| e + 2).unwrap_or(rest.len());
            let tag = &rest[..end];
            let closing = tag.starts_with("</");
            let self_closing = tag.ends_with("/>") || tag.starts_with("<?") || tag.starts_with("<!");
            if closing {
                depth = depth.saturating_sub(1);
            }
            out.push_str(&"  ".repeat(depth));
            out.push_str(tag);
            out.push('\n');
            if !closing && !self_closing {
                depth += 1;
            }
            rest = rest[end..].trim_start();
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            let t = rest[..end].trim();
            if !t.is_empty() {
                out.push_str(&"  ".repeat(depth));
                out.push_str(t);
                out.push('\n');
            }
            rest = &rest[end..];
        }
    }
    out
}

pub fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn human_time(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{} ms", ms.round())
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_highlights() {
        let h = to_html("{\"a\": \"<b>\", \"n\": -1.5e3, \"t\": true}", Lang::Data);
        assert!(h.contains("<span class=\"tok-key\">&quot;a&quot;</span>"));
        assert!(h.contains("&lt;b&gt;"));
        assert!(h.contains("<span class=\"tok-num\">-1.5e3</span>"));
        assert!(h.contains("<span class=\"tok-kw\">true</span>"));
        assert!(!h.contains("<b>"));
    }

    #[test]
    fn marks() {
        let (h, n) = mark_matches("Hello <World> hello", "hello");
        assert_eq!(n, 2);
        assert_eq!(h, "<mark>Hello</mark> &lt;World&gt; <mark>hello</mark>");
    }
}
