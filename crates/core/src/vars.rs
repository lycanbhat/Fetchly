//! `{{variable}}` resolution with scoped precedence and dynamic variables.

use crate::model::KeyValue;
use std::collections::HashMap;

/// Variable scopes, from lowest to highest precedence:
/// globals < collection < environment < local (script-set).
#[derive(Clone, Debug, Default)]
pub struct VarScope {
    pub globals: HashMap<String, String>,
    pub collection: HashMap<String, String>,
    pub environment: HashMap<String, String>,
    pub local: HashMap<String, String>,
}

pub fn to_map(vars: &[KeyValue]) -> HashMap<String, String> {
    vars.iter()
        .filter(|v| v.enabled && !v.key.is_empty())
        .map(|v| (v.key.clone(), v.value.clone()))
        .collect()
}

impl VarScope {
    pub fn get(&self, key: &str) -> Option<String> {
        self.local
            .get(key)
            .or_else(|| self.environment.get(key))
            .or_else(|| self.collection.get(key))
            .or_else(|| self.globals.get(key))
            .cloned()
            .or_else(|| dynamic_variable(key))
    }

    /// Where a variable would be resolved from (for UI hints).
    pub fn source_of(&self, key: &str) -> Option<&'static str> {
        if self.local.contains_key(key) {
            Some("Local")
        } else if self.environment.contains_key(key) {
            Some("Environment")
        } else if self.collection.contains_key(key) {
            Some("Collection")
        } else if self.globals.contains_key(key) {
            Some("Global")
        } else if key.starts_with('$') && dynamic_variable(key).is_some() {
            Some("Dynamic")
        } else {
            None
        }
    }

    /// Replace every `{{name}}` occurrence. Unknown variables are left untouched.
    /// Resolution is applied repeatedly (up to a depth limit) so variables may reference others.
    pub fn resolve(&self, input: &str) -> String {
        let mut current = input.to_string();
        for _ in 0..8 {
            let next = self.resolve_once(&current);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    fn resolve_once(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            if let Some(end) = after.find("}}") {
                let name = after[..end].trim();
                match self.get(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push_str("{{");
                        out.push_str(&after[..end]);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            } else {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
        out.push_str(rest);
        out
    }

    /// Names of variables referenced in `input` that cannot be resolved.
    pub fn unresolved(&self, input: &str) -> Vec<String> {
        referenced_variables(input).into_iter().filter(|n| self.get(n).is_none()).collect()
    }
}

/// All `{{name}}` names referenced in a string.
pub fn referenced_variables(input: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            out.push(after[..end].trim().to_string());
            rest = &after[end + 2..];
        } else {
            break;
        }
    }
    out
}

fn random_u64() -> u64 {
    let b = uuid::Uuid::new_v4().into_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[9], b[10]])
}

const FIRST_NAMES: &[&str] = &["Alex", "Sam", "Jordan", "Taylor", "Morgan", "Casey", "Riley", "Jamie", "Avery", "Quinn"];
const LAST_NAMES: &[&str] = &["Smith", "Patel", "Garcia", "Kim", "Nguyen", "Müller", "Rossi", "Silva", "Sato", "Okafor"];
const WORDS: &[&str] = &["alpha", "bravo", "cobalt", "delta", "ember", "falcon", "granite", "harbor", "iris", "juniper"];

fn pick<'a>(list: &[&'a str]) -> &'a str {
    list[(random_u64() % list.len() as u64) as usize]
}

/// Built-in dynamic variables such as `{{$guid}}` and `{{$timestamp}}`.
pub fn dynamic_variable(name: &str) -> Option<String> {
    let now = chrono::Utc::now();
    Some(match name {
        "$guid" | "$randomUUID" => uuid::Uuid::new_v4().to_string(),
        "$timestamp" => now.timestamp().to_string(),
        "$timestampMs" => now.timestamp_millis().to_string(),
        "$isoTimestamp" => now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "$randomInt" => (random_u64() % 1001).to_string(),
        "$randomBoolean" => (random_u64() % 2 == 0).to_string(),
        "$randomFirstName" => pick(FIRST_NAMES).to_string(),
        "$randomLastName" => pick(LAST_NAMES).to_string(),
        "$randomFullName" => format!("{} {}", pick(FIRST_NAMES), pick(LAST_NAMES)),
        "$randomWord" => pick(WORDS).to_string(),
        "$randomEmail" => format!("{}.{}@example.com", pick(FIRST_NAMES).to_lowercase(), random_u64() % 10000),
        "$randomColor" => format!("#{:06x}", random_u64() % 0x1000000),
        "$randomIP" => {
            let r = random_u64().to_le_bytes();
            format!("{}.{}.{}.{}", r[0], r[1], r[2], r[3])
        }
        _ => return None,
    })
}

pub const DYNAMIC_VARIABLES: &[&str] = &[
    "$guid",
    "$randomUUID",
    "$timestamp",
    "$timestampMs",
    "$isoTimestamp",
    "$randomInt",
    "$randomBoolean",
    "$randomFirstName",
    "$randomLastName",
    "$randomFullName",
    "$randomWord",
    "$randomEmail",
    "$randomColor",
    "$randomIP",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_with_precedence() {
        let mut s = VarScope::default();
        s.globals.insert("host".into(), "global".into());
        s.environment.insert("host".into(), "env".into());
        s.collection.insert("path".into(), "users/{{id}}".into());
        s.environment.insert("id".into(), "42".into());
        assert_eq!(s.resolve("https://{{host}}/{{path}}?x={{missing}}"), "https://env/users/42?x={{missing}}");
        assert_eq!(s.unresolved("{{host}} {{nope}}"), vec!["nope".to_string()]);
    }

    #[test]
    fn dynamic() {
        let s = VarScope::default();
        assert_eq!(s.resolve("{{$guid}}").len(), 36);
        assert!(s.resolve("{{$timestamp}}").parse::<i64>().is_ok());
    }
}
