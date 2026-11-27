//! The full send pipeline shared by the editor and the collection runner:
//! inherit auth & scripts → pre-request scripts → resolve & prepare → (send) → test scripts.

use crate::http::{effective_auth, prepare, PreparedRequest, ResponseData};
use crate::model::*;
use crate::script::{apply_changes, run_script, ScriptOutcome, ScriptRequest, TestResult, VarChange, VarTarget};
use crate::vars::{to_map, VarScope};

/// Everything needed to execute one request in context.
#[derive(Clone, Debug)]
pub struct Plan {
    pub request: Request,
    pub collection_id: Option<String>,
    pub auth: Auth,
    /// Outermost first: collection, folders, request.
    pub pre_scripts: Vec<String>,
    pub test_scripts: Vec<String>,
    pub scope: VarScope,
}

pub fn build_scope(ws: &Workspace, collection_id: Option<&str>) -> VarScope {
    VarScope {
        globals: to_map(&ws.globals),
        collection: collection_id.and_then(|id| ws.collection(id)).map(|c| to_map(&c.variables)).unwrap_or_default(),
        environment: ws.active_env().map(|e| to_map(&e.variables)).unwrap_or_default(),
        local: Default::default(),
    }
}

pub fn plan(ws: &Workspace, request: &Request, collection_id: Option<&str>) -> Plan {
    let collection = collection_id.and_then(|id| ws.collection(id));
    let folders = collection.map(|c| c.ancestors_of(&request.id)).unwrap_or_default();
    let mut pre = vec![];
    let mut test = vec![];
    if let Some(c) = collection {
        pre.push(c.pre_request_script.clone());
        test.push(c.test_script.clone());
    }
    for f in &folders {
        pre.push(f.pre_request_script.clone());
        test.push(f.test_script.clone());
    }
    pre.push(request.pre_request_script.clone());
    test.push(request.test_script.clone());
    Plan {
        auth: effective_auth(request, &folders, collection),
        request: request.clone(),
        collection_id: collection_id.map(String::from),
        pre_scripts: pre.into_iter().filter(|s| !s.trim().is_empty()).collect(),
        test_scripts: test.into_iter().filter(|s| !s.trim().is_empty()).collect(),
        scope: build_scope(ws, collection_id),
    }
}

#[derive(Clone, Debug, Default)]
pub struct PreOutcome {
    pub prepared: Option<PreparedRequest>,
    pub error: Option<String>,
    pub logs: Vec<String>,
    pub changes: Vec<VarChange>,
    /// Scope after pre-request scripts ran (used by test scripts).
    pub scope: VarScope,
    pub request: Request,
    pub skip: bool,
    pub next_request: Option<Option<String>>,
}

fn script_request(r: &Request) -> ScriptRequest {
    ScriptRequest {
        method: r.method.as_str().into(),
        url: r.url.clone(),
        headers: r.headers.clone(),
        body: match &r.body {
            Body::Raw { text, .. } => Some(text.clone()),
            _ => None,
        },
    }
}

pub fn pre_request(plan: &Plan) -> PreOutcome {
    let mut scope = plan.scope.clone();
    let mut request = plan.request.clone();
    let mut out = PreOutcome::default();
    for src in &plan.pre_scripts {
        let r: ScriptOutcome = run_script(src, &scope, script_request(&request), None);
        apply_changes(&mut scope, &r.changes);
        out.logs.extend(r.logs);
        out.changes.extend(r.changes);
        if r.next_request.is_some() {
            out.next_request = r.next_request;
        }
        // apply request edits
        request.url = r.request.url;
        request.method = HttpMethod::parse(&r.request.method).unwrap_or(request.method);
        request.headers = r.request.headers;
        if let (Some(new_body), Body::Raw { text, .. }) = (r.request.body, &mut request.body) {
            *text = new_body;
        }
        if let Some(e) = r.error {
            out.error = Some(format!("Pre-request script error: {e}"));
            break;
        }
        if r.skip_request {
            out.skip = true;
            break;
        }
    }
    if out.error.is_none() && !out.skip {
        match prepare(&request, &plan.auth, &scope) {
            Ok(p) => out.prepared = Some(p),
            Err(e) => out.error = Some(e),
        }
    }
    out.scope = scope;
    out.request = request;
    out
}

#[derive(Clone, Debug, Default)]
pub struct PostOutcome {
    pub tests: Vec<TestResult>,
    pub logs: Vec<String>,
    pub changes: Vec<VarChange>,
    pub error: Option<String>,
    pub next_request: Option<Option<String>>,
}

pub fn post_response(plan: &Plan, pre: &PreOutcome, response: &ResponseData) -> PostOutcome {
    let mut scope = pre.scope.clone();
    let mut out = PostOutcome { next_request: pre.next_request.clone(), ..Default::default() };
    for src in &plan.test_scripts {
        let r = run_script(src, &scope, script_request(&pre.request), Some(response));
        apply_changes(&mut scope, &r.changes);
        out.tests.extend(r.tests);
        out.logs.extend(r.logs);
        out.changes.extend(r.changes);
        if r.next_request.is_some() {
            out.next_request = r.next_request;
        }
        if let Some(e) = r.error {
            out.error = Some(format!("Test script error: {e}"));
        }
    }
    out
}

/// Persist script variable changes into the workspace (local changes are discarded).
pub fn apply_to_workspace(ws: &mut Workspace, collection_id: Option<&str>, changes: &[VarChange]) {
    for c in changes {
        let list = match c.target {
            VarTarget::Global => Some(&mut ws.globals),
            VarTarget::Environment => ws.active_env_mut().map(|e| &mut e.variables),
            VarTarget::Collection => collection_id.and_then(|id| ws.collection_mut(id)).map(|c| &mut c.variables),
            VarTarget::Local => None,
        };
        if let Some(list) = list {
            match &c.value {
                Some(v) => set_var(list, &c.key, v),
                None => unset_var(list, &c.key),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inheritance_and_scripts() {
        let mut ws = Workspace::default();
        let mut c = Collection::new("C");
        c.auth = Auth::Bearer { token: "{{tok}}".into() };
        c.pre_request_script = r#"pm.collectionVariables.set("tok", "secret");"#.into();
        let mut folder = Folder::new("F");
        let req = Request { url: "http://example.com/{{path}}".into(), pre_request_script: r#"pm.variables.set("path", "p");"#.into(), test_script: r#"pm.environment.set("x", "1");"#.into(), ..Default::default() };
        let rid = req.id.clone();
        folder.items.push(Item::Request(req));
        c.items.push(Item::Folder(folder));
        let cid = c.id.clone();
        ws.collections.push(c);
        let mut env = Environment::new("E");
        env.id = "e".into();
        ws.environments.push(env);
        ws.active_environment = Some("e".into());

        let r = ws.collection(&cid).unwrap().find_request(&rid).unwrap().clone();
        let p = plan(&ws, &r, Some(&cid));
        let pre = pre_request(&p);
        assert_eq!(pre.error, None);
        let prepared = pre.prepared.clone().unwrap();
        assert_eq!(prepared.url, "http://example.com/p");
        assert_eq!(prepared.header("authorization"), Some("Bearer secret"));

        let post = post_response(&p, &pre, &ResponseData { status: 200, ..Default::default() });
        apply_to_workspace(&mut ws, Some(&cid), &pre.changes);
        apply_to_workspace(&mut ws, Some(&cid), &post.changes);
        assert_eq!(ws.active_env().unwrap().variables[0].value, "1");
        assert_eq!(ws.collection(&cid).unwrap().variables[0].value, "secret");
    }
}
