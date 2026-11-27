//! Operations that span several pieces of state: sending, importing, running collections.

use crate::highlight::human_time;
use crate::store::*;
use dioxus::prelude::*;
use fetchly_core::http::execute;
use fetchly_core::model::*;
use fetchly_core::pipeline;
use fetchly_core::postman::{self, Imported};
use fetchly_core::script::TestResult;
use std::rc::Rc;

fn log_lines(lines: &[String]) {
    for l in lines {
        log(ConsoleKind::Log, l.clone(), None);
    }
}

/// Send the request in `tab_id` (pre-request scripts → HTTP → tests).
pub fn send(tab_id: String) {
    if PENDING.read().contains_key(&tab_id) {
        return;
    }
    let Some(tab) = TABS.read().iter().find(|t| t.id == tab_id).cloned() else { return };
    let cid = tab.origin.as_ref().map(|o| o.collection_id.clone());
    let plan = pipeline::plan(&WS.read(), &tab.request, cid.as_deref());
    let pre = pipeline::pre_request(&plan);
    pipeline::apply_to_workspace(&mut WS.write(), cid.as_deref(), &pre.changes);
    log_lines(&pre.logs);
    if let Some(err) = &pre.error {
        log(ConsoleKind::Error, err.clone(), None);
        RESULTS.write().insert(tab_id, Rc::new(ResultView::error(err.clone(), pre.prepared.clone())));
        return;
    }
    if pre.skip {
        RESULTS.write().insert(tab_id, Rc::new(ResultView::error("Request skipped by pre-request script (pm.execution.skipRequest)".into(), None)));
        return;
    }
    let prepared = pre.prepared.clone().expect("prepared");
    let transport = transport();
    let id = tab_id.clone();
    let task = spawn(async move {
        let result = execute(prepared.clone(), transport).await;
        PENDING.write().remove(&id);
        let (view, status, time) = match result {
            Ok(resp) => {
                let post = pipeline::post_response(&plan, &pre, &resp);
                pipeline::apply_to_workspace(&mut WS.write(), cid.as_deref(), &post.changes);
                let details = format!(
                    "Request headers\n{}\n\nRequest body\n{}\n\nResponse headers\n{}",
                    prepared.headers.iter().map(|(k, v)| format!("  {k}: {v}")).collect::<Vec<_>>().join("\n"),
                    if prepared.body.is_empty() { "  (empty)".to_string() } else { prepared.body_text().chars().take(4000).collect() },
                    resp.headers.iter().map(|(k, v)| format!("  {k}: {v}")).collect::<Vec<_>>().join("\n"),
                );
                log(ConsoleKind::Request, format!("{} {} → {} {} · {}", prepared.method, prepared.url, resp.status, resp.status_text, human_time(resp.time_ms)), Some(details));
                log_lines(&post.logs);
                if let Some(e) = &post.error {
                    log(ConsoleKind::Error, e.clone(), None);
                }
                let (status, time) = (Some(resp.status), Some(resp.time_ms));
                let mut v = ResultView::from_response(resp, prepared);
                v.tests = post.tests;
                v.script_error = post.error;
                (v, status, time)
            }
            Err(e) => {
                log(ConsoleKind::Error, format!("{} {} → {e}", prepared.method, prepared.url), None);
                (ResultView::error(e, Some(prepared)), None, None)
            }
        };
        if SETTINGS.read().save_history {
            WS.write().push_history(HistoryEntry { id: new_id(), timestamp: chrono::Utc::now(), request: tab.request.clone(), status, time_ms: time });
        }
        RESULTS.write().insert(id, Rc::new(view));
    });
    PENDING.write().insert(tab_id, task);
}

pub fn cancel(tab_id: &str) {
    if let Some(task) = PENDING.write().remove(tab_id) {
        task.cancel();
        RESULTS.write().insert(tab_id.to_string(), Rc::new(ResultView::error("Request cancelled".into(), None)));
    }
}

// ------------------------------------------------------------------ import / export

pub fn import_text(text: &str) -> Result<String, String> {
    match postman::import_any(text)? {
        Imported::Collection(c) => {
            let msg = format!("Imported “{}” ({} requests)", c.name, c.all_requests().len());
            UI.write().expanded.insert(c.id.clone());
            WS.write().collections.push(c);
            UI.write().sidebar_mode = SidebarMode::Collections;
            Ok(msg)
        }
        Imported::Environment(e) => {
            let msg = format!("Imported environment “{}”", e.name);
            WS.write().environments.push(e);
            UI.write().sidebar_mode = SidebarMode::Environments;
            Ok(msg)
        }
        Imported::Workspace(w) => {
            let n = w.collections.len();
            let mut ws = WS.write();
            ws.collections.extend(w.collections);
            ws.environments.extend(w.environments);
            for g in w.globals {
                set_var(&mut ws.globals, &g.key, &g.value);
            }
            Ok(format!("Imported workspace backup ({n} collections)"))
        }
        Imported::Request(r) => {
            let name = r.name.clone();
            push_tab(Tab::new(r, None));
            Ok(format!("Imported cURL request {name}"))
        }
    }
}

pub fn export_file(name: &str, content: &str) {
    match crate::platform::save_file(name, content.as_bytes()) {
        Ok(msg) => toast(msg),
        Err(e) if e == "Cancelled" => {}
        Err(e) => toast_error(format!("Export failed: {e}")),
    }
}

pub fn export_collection(cid: &str) {
    let Some(c) = WS.read().collection(cid).cloned() else { return };
    let name = format!("{}.postman_collection.json", c.name.replace(['/', '\\', ' '], "_"));
    export_file(&name, &postman::export_collection(&c));
}

// ------------------------------------------------------------------ collection runner

#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub iteration: usize,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub status: Option<u16>,
    pub time_ms: Option<f64>,
    pub error: Option<String>,
    pub tests: Vec<TestResult>,
}

impl RunResult {
    pub fn ok(&self) -> bool {
        self.error.is_none() && self.tests.iter().all(|t| t.passed)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RunnerState {
    pub collection_id: Option<String>,
    pub folder_id: Option<String>,
    pub iterations: usize,
    pub delay_ms: u64,
    pub stop_on_failure: bool,
    pub running: bool,
    pub progress: (usize, usize, usize, usize), // iteration, iterations, index, total
    pub results: Vec<RunResult>,
    pub started: f64,
    pub finished: Option<f64>,
    pub generation: u64,
}

impl Default for RunnerState {
    fn default() -> Self {
        Self {
            collection_id: None,
            folder_id: None,
            iterations: 1,
            delay_ms: 0,
            stop_on_failure: false,
            running: false,
            progress: (0, 0, 0, 0),
            results: vec![],
            started: 0.0,
            finished: None,
            generation: 0,
        }
    }
}

pub static RUNNER: GlobalSignal<RunnerState> = Signal::global(RunnerState::default);

/// Request ids in run order for the configured collection/folder.
pub fn runner_queue(ws: &Workspace, cid: Option<&str>, folder: Option<&str>) -> Vec<Request> {
    let Some(c) = cid.and_then(|id| ws.collection(id)) else { return vec![] };
    match folder.and_then(|f| c.folder(f)) {
        Some(f) => {
            let tmp = Collection { items: f.items.clone(), ..Collection::new("") };
            tmp.all_requests().into_iter().cloned().collect()
        }
        None => c.all_requests().into_iter().cloned().collect(),
    }
}

pub fn start_run() {
    let (cid, folder, iterations, delay, stop_on_failure) = {
        let r = RUNNER.read();
        (r.collection_id.clone(), r.folder_id.clone(), r.iterations.max(1), r.delay_ms, r.stop_on_failure)
    };
    let queue: Vec<String> = runner_queue(&WS.read(), cid.as_deref(), folder.as_deref()).iter().map(|r| r.id.clone()).collect();
    if queue.is_empty() {
        return;
    }
    let generation = {
        let mut r = RUNNER.write();
        r.generation += 1;
        r.running = true;
        r.results.clear();
        r.started = crate::platform::now_secs();
        r.finished = None;
        r.progress = (1, iterations, 1, queue.len());
        r.generation
    };
    spawn(async move {
        let still_running = move || RUNNER.read().running && RUNNER.read().generation == generation;
        let mut iteration = 0;
        let mut index = 0;
        'outer: while iteration < iterations {
            while index < queue.len() {
                if !still_running() {
                    break 'outer;
                }
                RUNNER.write().progress = (iteration + 1, iterations, index + 1, queue.len());
                let rid = queue[index].clone();
                // Re-read the request every time so variable changes from earlier requests apply.
                let req = cid.as_deref().and_then(|c| WS.read().collection(c).and_then(|c| c.find_request(&rid)).cloned());
                let Some(req) = req else {
                    index += 1;
                    continue;
                };
                let plan = pipeline::plan(&WS.read(), &req, cid.as_deref());
                let pre = pipeline::pre_request(&plan);
                pipeline::apply_to_workspace(&mut WS.write(), cid.as_deref(), &pre.changes);
                log_lines(&pre.logs);
                let mut rr = RunResult { iteration: iteration + 1, name: req.name.clone(), method: req.method, url: req.url.clone(), status: None, time_ms: None, error: None, tests: vec![] };
                let mut next = pre.next_request.clone();
                if pre.error.is_some() || pre.skip {
                    rr.error = Some(pre.error.clone().unwrap_or_else(|| "Skipped".into()));
                } else {
                    let prepared = pre.prepared.clone().unwrap();
                    rr.url = prepared.url.clone();
                    match execute(prepared.clone(), transport()).await {
                        Ok(resp) => {
                            if !still_running() {
                                break 'outer;
                            }
                            let post = pipeline::post_response(&plan, &pre, &resp);
                            pipeline::apply_to_workspace(&mut WS.write(), cid.as_deref(), &post.changes);
                            log_lines(&post.logs);
                            log(ConsoleKind::Request, format!("[runner] {} {} → {}", prepared.method, prepared.url, resp.status), None);
                            rr.status = Some(resp.status);
                            rr.time_ms = Some(resp.time_ms);
                            rr.tests = post.tests;
                            rr.error = post.error;
                            next = post.next_request;
                        }
                        Err(e) => rr.error = Some(e),
                    }
                }
                let failed = !rr.ok();
                RUNNER.write().results.push(rr);
                if failed && stop_on_failure {
                    break 'outer;
                }
                match next {
                    Some(None) => break 'outer,
                    Some(Some(name)) => {
                        let reqs = runner_queue(&WS.read(), cid.as_deref(), folder.as_deref());
                        index = reqs.iter().position(|r| r.name == name).unwrap_or(index + 1);
                    }
                    None => index += 1,
                }
                if delay > 0 {
                    crate::platform::sleep_ms(delay).await;
                }
            }
            index = 0;
            iteration += 1;
        }
        let mut r = RUNNER.write();
        if r.generation == generation {
            r.running = false;
            r.finished = Some(crate::platform::now_secs());
        }
    });
}

pub fn stop_run() {
    let mut r = RUNNER.write();
    r.running = false;
    r.finished = Some(crate::platform::now_secs());
}

pub fn run_results_json(r: &RunnerState) -> String {
    let v: Vec<serde_json::Value> = r
        .results
        .iter()
        .map(|x| {
            serde_json::json!({
                "iteration": x.iteration, "name": x.name, "method": x.method.as_str(), "url": x.url,
                "status": x.status, "time_ms": x.time_ms, "error": x.error,
                "tests": x.tests.iter().map(|t| serde_json::json!({"name": t.name, "passed": t.passed, "error": t.error})).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "results": v })).unwrap_or_default()
}
