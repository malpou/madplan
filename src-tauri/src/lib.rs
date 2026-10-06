mod agent;
mod pantry;
mod telemetry;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use validator::{Report, Validator};

pub struct Ctx {
    pub root: PathBuf,
    pub validator: Validator,
    pub agent: agent::Agent,
}

#[derive(Serialize)]
struct PlanInfo {
    file: String,
    week: Option<String>,
    status: Option<String>,
}

#[derive(Serialize)]
struct Snapshot {
    plans: Vec<PlanInfo>,
    profile: Value,
    pantry: Value,
    /// Shared enums (units, categories, …) for the editors.
    defs: Value,
    validation: Vec<Report>,
    agent_running: bool,
}

/// The repo the app works on: $MADPLAN_ROOT, else the directory above src-tauri.
fn repo_root() -> PathBuf {
    std::env::var_os("MADPLAN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf())
}

fn load_or_null(path: &Path) -> Value {
    validator::load(path).unwrap_or(Value::Null)
}

#[tauri::command]
fn snapshot(ctx: State<Ctx>) -> Snapshot {
    let mut files: Vec<PathBuf> = std::fs::read_dir(ctx.root.join("plans"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "yaml"))
        .collect();
    files.sort();
    files.reverse(); // newest week first
    let plans = files
        .iter()
        .map(|p| {
            let doc = load_or_null(p);
            let field = |k: &str| doc.get(k).and_then(Value::as_str).map(str::to_owned);
            PlanInfo { file: p.file_name().unwrap().to_string_lossy().into(), week: field("week"), status: field("status") }
        })
        .collect();
    Snapshot {
        plans,
        profile: load_or_null(&ctx.root.join("data/profile.yaml")),
        pantry: load_or_null(&ctx.root.join("data/pantry.yaml")),
        defs: load_or_null(&ctx.root.join("schemas/defs.schema.yaml"))["$defs"].take(),
        validation: ctx.validator.validate_all(),
        agent_running: ctx.agent.running(),
    }
}

#[tauri::command]
fn read_plan(ctx: State<Ctx>, file: String) -> Result<Value, String> {
    // Only plain file names inside plans/ — the webview must not read arbitrary paths.
    if file.contains(['/', '\\']) || file.starts_with('.') || !file.ends_with(".yaml") {
        telemetry::error("read_plan.rejected", json!({"file": file}));
        return Err(format!("invalid plan file: {file}"));
    }
    validator::load(&ctx.root.join("plans").join(&file))
        .inspect_err(|e| telemetry::error("read_plan.failed", json!({"file": file, "error": e})))
}

/// Validate first; only write the file when it's valid. Returns the report either way.
#[tauri::command]
fn save_pantry(ctx: State<Ctx>, pantry: Value) -> Result<Report, String> {
    if ctx.agent.running() {
        telemetry::info("pantry.save_blocked", json!({"reason": "agent running"}));
        return Err("The agent is working — save again when it's done so you don't overwrite each other.".into());
    }
    let path = ctx.root.join("data/pantry.yaml");
    let report = ctx.validator.validate_value(&path, &pantry);
    if report.ok() {
        std::fs::write(&path, pantry::to_yaml(&pantry)).map_err(|e| {
            telemetry::error("pantry.write_failed", json!({"error": e.to_string()}));
            e.to_string()
        })?;
    }
    telemetry::info("pantry.save", json!({"written": report.ok(), "errors": report.errors}));
    Ok(report)
}

#[tauri::command]
fn agent_send(app: AppHandle, message: String) -> Result<(), String> {
    agent::send(&app, message, false)
}

#[tauri::command]
fn agent_stop(ctx: State<Ctx>) {
    telemetry::info("agent.stop_requested", Value::Null);
    ctx.agent.stop();
}

#[tauri::command]
fn agent_new_session(ctx: State<Ctx>) -> Result<(), String> {
    telemetry::info("agent.new_session", Value::Null);
    ctx.agent.reset()
}

/// Frontend errors and notable UI events, so webview failures land in the same log.
#[tauri::command]
fn log_client(level: String, kind: String, data: Value) {
    let kind = format!("ui.{kind}");
    if level == "error" { telemetry::error(&kind, data) } else { telemetry::info(&kind, data) }
}

/// Watch data/ and plans/; validate changed YAML files in the background and tell the UI.
fn watch(app: AppHandle, root: PathBuf) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let ev = match res {
            Ok(ev) => ev,
            Err(e) => return telemetry::error("watch.error", json!({"error": e.to_string()})),
        };
        // Reads raise Access events too; reacting to those loops forever since validating reads the file.
        if ev.kind.is_access() || ev.kind.is_other() {
            return;
        }
        for p in ev.paths.into_iter().filter(|p| p.extension().is_some_and(|e| e == "yaml")) {
            let _ = tx.send(p);
        }
    })?;
    for dir in ["data", "plans"] {
        let dir = root.join(dir);
        std::fs::create_dir_all(&dir).ok();
        watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    }
    std::thread::spawn(move || {
        let _watcher = watcher; // keep alive for the life of the thread
        while let Ok(first) = rx.recv() {
            // Debounce: editors and the agent often write a file in several steps.
            std::thread::sleep(Duration::from_millis(300));
            let changed: BTreeSet<PathBuf> = std::iter::once(first).chain(rx.try_iter()).collect();
            let ctx = app.state::<Ctx>();
            let reports: Vec<Report> =
                changed.iter().filter(|p| p.exists()).map(|p| ctx.validator.validate_file(p)).collect();
            let level = if reports.iter().all(Report::ok) { telemetry::info } else { telemetry::error };
            level("validation.files_changed", json!({"reports": reports}));
            let _ = app.emit("files-changed", reports);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let root = repo_root();
    telemetry::init(&root);
    telemetry::info("app.start", json!({
        "version": env!("CARGO_PKG_VERSION"),
        "root": root,
        "pid": std::process::id(),
        "os": std::env::consts::OS,
        "wsl": std::env::var_os("WSL_DISTRO_NAME").is_some(),
    }));
    let validator = Validator::new(&root).unwrap_or_else(|e| panic!("loading schemas from {}: {e}", root.display()));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Ctx { root: root.clone(), validator, agent: agent::Agent::default() })
        .setup(move |app| {
            watch(app.handle().clone(), root)
                .inspect_err(|e| telemetry::error("watch.setup_failed", json!({"error": e.to_string()})))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![snapshot, read_plan, save_pantry, agent_send, agent_stop, agent_new_session, log_client])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            telemetry::error("app.run_failed", json!({"error": e.to_string()}));
            panic!("error while running tauri application: {e}")
        });
    telemetry::info("app.exit", Value::Null);
}
