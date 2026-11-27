//! Pre-request and test scripts, powered by the Rhai scripting language.
//!
//! Scripts get a `pm` object with a familiar API:
//! ```text
//! pm.environment.set("token", pm.response.json().access_token);
//! pm.test("status is 200", || pm.expect(pm.response.code).to_equal(200));
//! pm.request.headers.upsert("X-Trace", pm.variables.get("$guid"));
//! console.log("done");
//! ```

use crate::http::ResponseData;
use crate::model::KeyValue;
use crate::vars::VarScope;
use rhai::{Dynamic, Engine, EvalAltResult, FnPtr, NativeCallContext, Scope};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarTarget {
    Global,
    Collection,
    Environment,
    Local,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VarChange {
    pub target: VarTarget,
    pub key: String,
    /// `None` = unset
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub error: Option<String>,
}

/// The (unresolved) request as seen and editable by a script.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    /// Raw body text, if the body is raw.
    pub body: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ScriptOutcome {
    pub tests: Vec<TestResult>,
    pub logs: Vec<String>,
    pub error: Option<String>,
    pub changes: Vec<VarChange>,
    pub request: ScriptRequest,
    /// Name of the next request the collection runner should jump to (`pm.execution.setNextRequest`).
    pub next_request: Option<Option<String>>,
    pub skip_request: bool,
}

struct State {
    scope: VarScope,
    out: ScriptOutcome,
    response: Option<ResponseData>,
    json_cache: Option<Dynamic>,
}

type Shared = Rc<RefCell<State>>;

#[derive(Clone)]
struct Pm(Shared);
#[derive(Clone)]
struct Vars(Shared, Option<VarTarget>);
#[derive(Clone)]
struct Req(Shared);
#[derive(Clone)]
struct ReqHeaders(Shared);
#[derive(Clone)]
struct Resp(Shared);
#[derive(Clone)]
struct RespHeaders(Shared);
#[derive(Clone)]
struct Exec(Shared);
#[derive(Clone)]
struct Console(Shared);
#[derive(Clone)]
struct Expect {
    value: Dynamic,
    negate: bool,
}

type RResult<T> = Result<T, Box<EvalAltResult>>;

fn err<T>(msg: impl Into<String>) -> RResult<T> {
    Err(msg.into().into())
}

fn to_json(d: &Dynamic) -> serde_json::Value {
    rhai::serde::from_dynamic::<serde_json::Value>(d).unwrap_or(serde_json::Value::Null)
}

fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value::*;
    match (a, b) {
        (Number(x), Number(y)) => x.as_f64() == y.as_f64(),
        (Array(x), Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_eq(a, b)),
        (Object(x), Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w))),
        _ => a == b,
    }
}

fn show(d: &Dynamic) -> String {
    if d.is_string() {
        format!("'{}'", d)
    } else if d.is_map() || d.is_array() {
        to_json(d).to_string()
    } else {
        d.to_string()
    }
}

fn as_f64(d: &Dynamic) -> Option<f64> {
    d.as_float().ok().or_else(|| d.as_int().ok().map(|i| i as f64)).or_else(|| d.clone().into_string().ok()?.parse().ok())
}

impl Expect {
    fn check(&self, ok: bool, msg: String) -> RResult<()> {
        if ok != self.negate {
            Ok(())
        } else if self.negate {
            err(format!("expected {} not {}", show(&self.value), msg))
        } else {
            err(format!("expected {} {}", show(&self.value), msg))
        }
    }
}

fn register(engine: &mut Engine) {
    engine.register_type_with_name::<Pm>("Pm");
    engine.register_type_with_name::<Vars>("Variables");
    engine.register_type_with_name::<Req>("Request");
    engine.register_type_with_name::<ReqHeaders>("RequestHeaders");
    engine.register_type_with_name::<Resp>("Response");
    engine.register_type_with_name::<RespHeaders>("ResponseHeaders");
    engine.register_type_with_name::<Exec>("Execution");
    engine.register_type_with_name::<Console>("Console");
    engine.register_type_with_name::<Expect>("Expectation");

    // --- pm.* accessors
    engine.register_get("environment", |p: &mut Pm| Vars(p.0.clone(), Some(VarTarget::Environment)));
    engine.register_get("globals", |p: &mut Pm| Vars(p.0.clone(), Some(VarTarget::Global)));
    engine.register_get("collectionVariables", |p: &mut Pm| Vars(p.0.clone(), Some(VarTarget::Collection)));
    engine.register_get("variables", |p: &mut Pm| Vars(p.0.clone(), None));
    engine.register_get("request", |p: &mut Pm| Req(p.0.clone()));
    engine.register_get("response", |p: &mut Pm| Resp(p.0.clone()));
    engine.register_get("execution", |p: &mut Pm| Exec(p.0.clone()));
    engine.register_get("info", |p: &mut Pm| {
        let s = p.0.borrow();
        let mut m = rhai::Map::new();
        m.insert("eventName".into(), (if s.response.is_some() { "test" } else { "prerequest" }).into());
        Dynamic::from_map(m)
    });

    // --- variables
    fn map_for(s: &mut State, t: VarTarget) -> &mut std::collections::HashMap<String, String> {
        match t {
            VarTarget::Global => &mut s.scope.globals,
            VarTarget::Collection => &mut s.scope.collection,
            VarTarget::Environment => &mut s.scope.environment,
            VarTarget::Local => &mut s.scope.local,
        }
    }
    engine.register_fn("get", |v: &mut Vars, key: &str| -> Dynamic {
        let s = v.0.borrow();
        let val = match v.1 {
            None => s.scope.get(key),
            Some(VarTarget::Global) => s.scope.globals.get(key).cloned(),
            Some(VarTarget::Collection) => s.scope.collection.get(key).cloned(),
            Some(VarTarget::Environment) => s.scope.environment.get(key).cloned(),
            Some(VarTarget::Local) => s.scope.local.get(key).cloned(),
        };
        val.map(Dynamic::from).unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("has", |v: &mut Vars, key: &str| -> bool {
        let mut s = v.0.borrow_mut();
        match v.1 {
            None => s.scope.get(key).is_some(),
            Some(t) => map_for(&mut s, t).contains_key(key),
        }
    });
    engine.register_fn("set", |v: &mut Vars, key: &str, value: Dynamic| {
        let target = v.1.unwrap_or(VarTarget::Local);
        let value = if value.is_string() { value.to_string() } else if value.is_map() || value.is_array() { to_json(&value).to_string() } else { value.to_string() };
        let mut s = v.0.borrow_mut();
        map_for(&mut s, target).insert(key.to_string(), value.clone());
        s.out.changes.push(VarChange { target, key: key.into(), value: Some(value) });
    });
    engine.register_fn("unset", |v: &mut Vars, key: &str| {
        let target = v.1.unwrap_or(VarTarget::Local);
        let mut s = v.0.borrow_mut();
        map_for(&mut s, target).remove(key);
        s.out.changes.push(VarChange { target, key: key.into(), value: None });
    });
    engine.register_fn("clear", |v: &mut Vars| {
        let target = v.1.unwrap_or(VarTarget::Local);
        let mut s = v.0.borrow_mut();
        let keys: Vec<String> = map_for(&mut s, target).keys().cloned().collect();
        map_for(&mut s, target).clear();
        for k in keys {
            s.out.changes.push(VarChange { target, key: k, value: None });
        }
    });
    engine.register_fn("replaceIn", |v: &mut Vars, text: &str| -> String { v.0.borrow().scope.resolve(text) });
    engine.register_fn("toObject", |v: &mut Vars| -> rhai::Map {
        let s = v.0.borrow();
        let src = match v.1 {
            Some(VarTarget::Global) => s.scope.globals.clone(),
            Some(VarTarget::Collection) => s.scope.collection.clone(),
            Some(VarTarget::Environment) => s.scope.environment.clone(),
            Some(VarTarget::Local) | None => {
                let mut all = s.scope.globals.clone();
                all.extend(s.scope.collection.clone());
                all.extend(s.scope.environment.clone());
                all.extend(s.scope.local.clone());
                all
            }
        };
        src.into_iter().map(|(k, v)| (k.into(), Dynamic::from(v))).collect()
    });

    // --- request
    engine.register_get_set(
        "url",
        |r: &mut Req| r.0.borrow().out.request.url.clone(),
        |r: &mut Req, v: String| r.0.borrow_mut().out.request.url = v,
    );
    engine.register_get_set(
        "method",
        |r: &mut Req| r.0.borrow().out.request.method.clone(),
        |r: &mut Req, v: String| r.0.borrow_mut().out.request.method = v.to_uppercase(),
    );
    engine.register_get_set(
        "body",
        |r: &mut Req| r.0.borrow().out.request.body.clone().map(Dynamic::from).unwrap_or(Dynamic::UNIT),
        |r: &mut Req, v: Dynamic| r.0.borrow_mut().out.request.body = Some(if v.is_map() || v.is_array() { to_json(&v).to_string() } else { v.to_string() }),
    );
    engine.register_get("headers", |r: &mut Req| ReqHeaders(r.0.clone()));
    engine.register_fn("get", |h: &mut ReqHeaders, k: &str| -> Dynamic {
        h.0.borrow()
            .out
            .request
            .headers
            .iter()
            .find(|x| x.enabled && x.key.eq_ignore_ascii_case(k))
            .map(|x| Dynamic::from(x.value.clone()))
            .unwrap_or(Dynamic::UNIT)
    });
    engine.register_fn("has", |h: &mut ReqHeaders, k: &str| -> bool {
        h.0.borrow().out.request.headers.iter().any(|x| x.enabled && x.key.eq_ignore_ascii_case(k))
    });
    let upsert = |h: &mut ReqHeaders, k: &str, v: &str| {
        let mut s = h.0.borrow_mut();
        let hs = &mut s.out.request.headers;
        if let Some(x) = hs.iter_mut().find(|x| x.key.eq_ignore_ascii_case(k)) {
            x.value = v.to_string();
            x.enabled = true;
        } else {
            hs.push(KeyValue::new(k, v));
        }
    };
    engine.register_fn("upsert", upsert);
    engine.register_fn("add", |h: &mut ReqHeaders, k: &str, v: &str| h.0.borrow_mut().out.request.headers.push(KeyValue::new(k, v)));
    engine.register_fn("remove", |h: &mut ReqHeaders, k: &str| h.0.borrow_mut().out.request.headers.retain(|x| !x.key.eq_ignore_ascii_case(k)));

    // --- response
    fn with_resp<T>(r: &Resp, f: impl FnOnce(&ResponseData) -> T) -> RResult<T> {
        let s = r.0.borrow();
        match &s.response {
            Some(resp) => Ok(f(resp)),
            None => err("pm.response is only available in test scripts"),
        }
    }
    engine.register_get("code", |r: &mut Resp| with_resp(r, |x| x.status as i64));
    engine.register_get("status", |r: &mut Resp| with_resp(r, |x| x.status_text.clone()));
    engine.register_get("responseTime", |r: &mut Resp| with_resp(r, |x| x.time_ms.round() as i64));
    engine.register_get("responseSize", |r: &mut Resp| with_resp(r, |x| x.body.len() as i64));
    engine.register_fn("text", |r: &mut Resp| with_resp(r, |x| x.body_text()));
    engine.register_fn("json", |r: &mut Resp| -> RResult<Dynamic> {
        if let Some(cached) = r.0.borrow().json_cache.clone() {
            return Ok(cached);
        }
        let v: serde_json::Value = with_resp(r, |x| serde_json::from_slice(&x.body))?
            .map_err(|e| -> Box<EvalAltResult> { format!("Response body is not valid JSON: {e}").into() })?;
        let d = rhai::serde::to_dynamic(&v)?;
        r.0.borrow_mut().json_cache = Some(d.clone());
        Ok(d)
    });
    engine.register_get("headers", |r: &mut Resp| RespHeaders(r.0.clone()));
    engine.register_fn("get", |h: &mut RespHeaders, k: &str| -> RResult<Dynamic> {
        with_resp(&Resp(h.0.clone()), |x| x.header(k).map(|v| Dynamic::from(v.to_string())).unwrap_or(Dynamic::UNIT))
    });
    engine.register_fn("has", |h: &mut RespHeaders, k: &str| -> RResult<bool> { with_resp(&Resp(h.0.clone()), |x| x.header(k).is_some()) });
    engine.register_fn("toObject", |h: &mut RespHeaders| -> RResult<rhai::Map> {
        with_resp(&Resp(h.0.clone()), |x| x.headers.iter().map(|(k, v)| (k.as_str().into(), Dynamic::from(v.clone()))).collect())
    });

    // --- execution flow (collection runner)
    engine.register_fn("setNextRequest", |e: &mut Exec, name: &str| e.0.borrow_mut().out.next_request = Some(Some(name.to_string())));
    engine.register_fn("setNextRequest", |e: &mut Exec, _: ()| e.0.borrow_mut().out.next_request = Some(None));
    engine.register_fn("skipRequest", |e: &mut Exec| e.0.borrow_mut().out.skip_request = true);

    // --- tests
    engine.register_fn("test", |ctx: NativeCallContext, p: &mut Pm, name: &str, f: FnPtr| -> RResult<()> {
        let result = f.call_within_context::<Dynamic>(&ctx, ());
        let (passed, error) = match result {
            Ok(v) if v.as_bool() == Ok(false) => (false, Some("assertion returned false".to_string())),
            Ok(_) => (true, None),
            Err(e) => (false, Some(clean_error(&e))),
        };
        p.0.borrow_mut().out.tests.push(TestResult { name: name.into(), passed, error });
        Ok(())
    });
    engine.register_fn("test", |p: &mut Pm, name: &str, passed: bool| {
        p.0.borrow_mut().out.tests.push(TestResult { name: name.into(), passed, error: (!passed).then(|| "condition was false".into()) });
    });

    // --- expectations
    engine.register_fn("expect", |_: &mut Pm, v: Dynamic| Expect { value: v, negate: false });
    engine.register_fn("expect", |v: Dynamic| Expect { value: v, negate: false });
    engine.register_get("not", |e: &mut Expect| Expect { value: e.value.clone(), negate: !e.negate });
    engine.register_fn("to_equal", |e: &mut Expect, v: Dynamic| e.check(json_eq(&to_json(&e.value), &to_json(&v)), format!("to equal {}", show(&v))));
    engine.register_fn("to_eql", |e: &mut Expect, v: Dynamic| e.check(json_eq(&to_json(&e.value), &to_json(&v)), format!("to deeply equal {}", show(&v))));
    engine.register_fn("to_be_above", |e: &mut Expect, v: Dynamic| {
        e.check(matches!((as_f64(&e.value), as_f64(&v)), (Some(a), Some(b)) if a > b), format!("to be above {}", show(&v)))
    });
    engine.register_fn("to_be_below", |e: &mut Expect, v: Dynamic| {
        e.check(matches!((as_f64(&e.value), as_f64(&v)), (Some(a), Some(b)) if a < b), format!("to be below {}", show(&v)))
    });
    engine.register_fn("to_include", |e: &mut Expect, v: Dynamic| {
        let ok = if e.value.is_string() {
            e.value.to_string().contains(&v.to_string())
        } else if e.value.is_array() {
            let needle = to_json(&v);
            e.value.clone().into_array().unwrap_or_default().iter().any(|x| json_eq(&to_json(x), &needle))
        } else if e.value.is_map() {
            e.value.clone().try_cast::<rhai::Map>().is_some_and(|m| m.contains_key(v.to_string().as_str()))
        } else {
            false
        };
        e.check(ok, format!("to include {}", show(&v)))
    });
    engine.register_fn("to_have_property", |e: &mut Expect, k: &str| {
        let ok = e.value.clone().try_cast::<rhai::Map>().is_some_and(|m| m.contains_key(k));
        e.check(ok, format!("to have property '{k}'"))
    });
    engine.register_fn("to_be_true", |e: &mut Expect| e.check(e.value.as_bool() == Ok(true), "to be true".into()));
    engine.register_fn("to_be_false", |e: &mut Expect| e.check(e.value.as_bool() == Ok(false), "to be false".into()));
    engine.register_fn("to_exist", |e: &mut Expect| e.check(!e.value.is_unit(), "to exist".into()));
    engine.register_fn("to_be_a", |e: &mut Expect, ty: &str| {
        let actual = match e.value.type_name() {
            "i64" | "f64" => "number",
            "string" | "ImmutableString" | "char" => "string",
            "bool" => "boolean",
            "array" => "array",
            "map" => "object",
            "()" => "null",
            other => other,
        };
        e.check(actual == ty.to_lowercase(), format!("to be a {ty}"))
    });
    engine.register_fn("to_have_length", |e: &mut Expect, n: i64| {
        let len = if e.value.is_string() { Some(e.value.to_string().chars().count()) } else { e.value.clone().into_array().ok().map(|a| a.len()) };
        e.check(len == Some(n as usize), format!("to have length {n}"))
    });
    engine.register_fn("to_match", |e: &mut Expect, pat: &str| {
        // simple wildcard match: `*` matches any run of characters
        e.check(wildcard(pat, &e.value.to_string()), format!("to match '{pat}'"))
    });
    // response-specific helpers
    engine.register_fn("to_have_status", |r: &mut Resp, code: i64| -> RResult<()> {
        let actual = with_resp(r, |x| x.status as i64)?;
        if actual == code { Ok(()) } else { err(format!("expected response to have status code {code} but got {actual}")) }
    });
    engine.register_fn("to_have_header", |r: &mut Resp, name: &str| -> RResult<()> {
        if with_resp(r, |x| x.header(name).is_some())? { Ok(()) } else { err(format!("expected response to have header '{name}'")) }
    });

    // --- console
    engine.register_fn("log", |c: &mut Console, v: Dynamic| c.0.borrow_mut().out.logs.push(stringify(&v)));
    engine.register_fn("info", |c: &mut Console, v: Dynamic| c.0.borrow_mut().out.logs.push(stringify(&v)));
    engine.register_fn("warn", |c: &mut Console, v: Dynamic| c.0.borrow_mut().out.logs.push(format!("⚠ {}", stringify(&v))));
    engine.register_fn("error", |c: &mut Console, v: Dynamic| c.0.borrow_mut().out.logs.push(format!("✖ {}", stringify(&v))));

    // --- utilities
    engine.register_fn("to_json", |v: Dynamic| to_json(&v).to_string());
    engine.register_fn("parse_json", |s: &str| -> RResult<Dynamic> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
        rhai::serde::to_dynamic(&v)
    });
    engine.register_fn("base64_encode", |s: &str| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(s)
    });
    engine.register_fn("base64_decode", |s: &str| -> RResult<String> {
        use base64::Engine as _;
        let b = base64::engine::general_purpose::STANDARD.decode(s).map_err(|e| -> Box<EvalAltResult> { e.to_string().into() })?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    });
    engine.register_fn("timestamp", || chrono::Utc::now().timestamp_millis());
    engine.register_fn("uuid", || uuid::Uuid::new_v4().to_string());
}

fn stringify(v: &Dynamic) -> String {
    if v.is_map() || v.is_array() {
        serde_json::to_string_pretty(&to_json(v)).unwrap_or_default()
    } else {
        v.to_string()
    }
}

fn wildcard(pat: &str, s: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let mut rest = s;
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            if !rest.starts_with(p) {
                return false;
            }
            rest = &rest[p.len()..];
        } else if i == parts.len() - 1 {
            return rest.ends_with(p);
        } else if let Some(pos) = rest.find(p) {
            rest = &rest[pos + p.len()..];
        } else {
            return false;
        }
    }
    true
}

fn clean_error(e: &EvalAltResult) -> String {
    match e {
        EvalAltResult::ErrorRuntime(v, _) => v.to_string(),
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => clean_error(inner),
        other => other.to_string(),
    }
}

/// Run a script. `response` is `Some` for test scripts.
pub fn run_script(source: &str, scope: &VarScope, request: ScriptRequest, response: Option<&ResponseData>) -> ScriptOutcome {
    let state: Shared = Rc::new(RefCell::new(State {
        scope: scope.clone(),
        out: ScriptOutcome { request, ..Default::default() },
        response: response.cloned(),
        json_cache: None,
    }));
    if source.lines().all(|l| l.trim().is_empty() || l.trim_start().starts_with("//")) {
        return state.borrow().out.clone();
    }

    let mut engine = Engine::new();
    engine.set_max_operations(10_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(128, 64);
    register(&mut engine);
    {
        let st = state.clone();
        engine.on_print(move |s| st.borrow_mut().out.logs.push(s.to_string()));
        let st = state.clone();
        engine.on_debug(move |s, _, _| st.borrow_mut().out.logs.push(s.to_string()));
    }

    let mut rhai_scope = Scope::new();
    rhai_scope.push_constant("pm", Pm(state.clone()));
    rhai_scope.push_constant("console", Console(state.clone()));

    let result = engine.run_with_scope(&mut rhai_scope, source);
    drop(rhai_scope);
    drop(engine);
    let mut out = state.borrow().out.clone();
    if let Err(e) = result {
        out.error = Some(e.to_string());
    }
    out
}

/// Updated variable scope after applying a script's changes (so later scripts see them).
pub fn apply_changes(scope: &mut VarScope, changes: &[VarChange]) {
    for c in changes {
        let map = match c.target {
            VarTarget::Global => &mut scope.globals,
            VarTarget::Collection => &mut scope.collection,
            VarTarget::Environment => &mut scope.environment,
            VarTarget::Local => &mut scope.local,
        };
        match &c.value {
            Some(v) => {
                map.insert(c.key.clone(), v.clone());
            }
            None => {
                map.remove(&c.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(body: &str) -> ResponseData {
        ResponseData {
            status: 200,
            status_text: "OK".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.as_bytes().to_vec(),
            time_ms: 12.0,
            url: String::new(),
        }
    }

    #[test]
    fn tests_and_vars() {
        let script = r#"
            let data = pm.response.json();
            pm.test("status ok", || pm.expect(pm.response.code).to_equal(200));
            pm.test("has id", || { pm.expect(data).to_have_property("id"); });
            pm.test("name fails", || pm.expect(data.name).to_equal("bob"));
            pm.test("bool form", data.id == 7);
            pm.environment.set("user_id", data.id);
            console.log("id is " + data.id);
            pm.response.to_have_status(200);
        "#;
        let out = run_script(script, &VarScope::default(), ScriptRequest::default(), Some(&resp(r#"{"id":7,"name":"alice"}"#)));
        assert_eq!(out.error, None);
        assert_eq!(out.tests.len(), 4);
        assert!(out.tests[0].passed && out.tests[1].passed && out.tests[3].passed);
        assert!(!out.tests[2].passed);
        assert!(out.tests[2].error.as_ref().unwrap().contains("'alice'"));
        assert_eq!(out.changes[0], VarChange { target: VarTarget::Environment, key: "user_id".into(), value: Some("7".into()) });
        assert_eq!(out.logs, vec!["id is 7".to_string()]);
    }

    #[test]
    fn pre_request_edits() {
        let script = r#"
            pm.request.headers.upsert("X-Id", "abc");
            pm.request.url = pm.request.url + "?x=1";
            pm.variables.set("local", "1");
            print(pm.variables.get("local"));
        "#;
        let req = ScriptRequest { method: "GET".into(), url: "http://a".into(), headers: vec![], body: None };
        let out = run_script(script, &VarScope::default(), req, None);
        assert_eq!(out.error, None, "{:?}", out.error);
        assert_eq!(out.request.url, "http://a?x=1");
        assert_eq!(out.request.headers[0].value, "abc");
        assert_eq!(out.logs, vec!["1".to_string()]);
    }

    #[test]
    fn negation_and_types() {
        let script = r#"
            let d = pm.response.json();
            pm.test("not equal", || pm.expect(d.name).not.to_equal("bob"));
            pm.test("type", || pm.expect(d.id).to_be_a("number"));
            pm.test("negated fails", || pm.expect(d.id).not.to_equal(7));
            pm.test("includes", || pm.expect([201, 202]).to_include(201));
        "#;
        let out = run_script(script, &VarScope::default(), ScriptRequest::default(), Some(&resp(r#"{"id":7,"name":"alice"}"#)));
        assert_eq!(out.error, None, "{:?}", out.error);
        let passed: Vec<bool> = out.tests.iter().map(|t| t.passed).collect();
        assert_eq!(passed, vec![true, true, false, true]);
    }

    #[test]
    fn infinite_loop_is_bounded() {
        let out = run_script("loop {}", &VarScope::default(), ScriptRequest::default(), None);
        assert!(out.error.is_some());
    }
}
