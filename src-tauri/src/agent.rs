//! Runs the meal-planning agent as one long-lived `claude -p` process per chat
//! (stream-json in and out), in the repo, so it picks up CLAUDE.md, .mcp.json and
//! .claude/settings.json as-is. Output streams to the UI as `agent-event`.
//!
//! A turn starts when a message is sent while idle and ends at the `result` event.
//! Messages sent mid-turn are steering: claude delivers them after the current tool
//! call, inside the same turn (`--replay-user-messages` echoes them on delivery).
//! Stop kills the process; the next message respawns it with `--resume`.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::{Ctx, telemetry};

/// How many times per user message we send validation errors back to the agent.
const AUTOFIX_ATTEMPTS: u8 = 2;

const UI_CONTEXT: &str = "The user is chatting with you through the Madplan desktop app. \
It renders plans/*.yaml (plan + shopping list + budget) and data/pantry.yaml live from disk, \
so keep chat replies short and point to the Plan / Shopping / Pantry tabs instead of repeating full tables. \
The user can send messages while you work; treat them as steering and adjust course. \
The app validates every YAML file you write and will send you any errors.";

#[derive(Default)]
pub struct Agent(Mutex<State>);

struct Proc {
    child: Child,
    stdin: ChildStdin,
    /// Which spawn this is, so a dying old process can't clobber a newer one.
    generation: u64,
}

#[derive(Default)]
struct State {
    proc: Option<Proc>,
    generation: u64,
    running: bool,
    stopping: bool,
    session_id: Option<String>,
    autofix_left: u8,
    /// Ties telemetry lines of one turn together.
    turn: u64,
    turn_started: u128,
    turn_events: u32,
}

impl State {
    fn begin_turn(&mut self, autofix: bool) {
        self.running = true;
        self.stopping = false;
        self.turn += 1;
        self.turn_started = telemetry::now_ms();
        self.turn_events = 0;
        if !autofix {
            self.autofix_left = AUTOFIX_ATTEMPTS;
        }
    }
}

impl Agent {
    pub fn running(&self) -> bool {
        self.0.lock().unwrap().running
    }

    pub fn stop(&self) {
        let mut s = self.0.lock().unwrap();
        s.autofix_left = 0;
        s.stopping = true;
        if let Some(p) = s.proc.as_mut() {
            let _ = p.child.kill();
        }
    }

    pub fn reset(&self) -> Result<(), String> {
        let mut s = self.0.lock().unwrap();
        if s.running {
            return Err("stop the agent before starting a new chat".into());
        }
        if let Some(mut p) = s.proc.take() {
            let _ = p.child.kill();
        }
        s.session_id = None;
        Ok(())
    }
}

fn emit(app: &AppHandle, ev: Value) {
    let _ = app.emit("agent-event", ev);
}

fn spawn(app: &AppHandle, s: &mut State) -> Result<(), String> {
    let ctx = app.state::<Ctx>();
    let mut cmd = Command::new("claude");
    cmd.current_dir(&ctx.root)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json"])
        .args(["--verbose", "--replay-user-messages"])
        .args(["--permission-mode", "acceptEdits", "--append-system-prompt", UI_CONTEXT])
        // Only the project's servers (nemlig), not whatever MCP servers the user has configured globally.
        .args(["--mcp-config", ".mcp.json", "--strict-mcp-config"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(id) = &s.session_id {
        cmd.args(["--resume", id]);
    }
    s.generation += 1;
    let generation = s.generation;
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    telemetry::info("agent.spawn", json!({"generation": generation, "session_id": s.session_id, "args": args}));
    let mut child = cmd.spawn().map_err(|e| {
        telemetry::error("agent.spawn_failed", json!({"generation": generation, "error": e.to_string()}));
        format!("couldn't start `claude`: {e}")
    })?;
    let (stdin, stdout, stderr) = (child.stdin.take().unwrap(), child.stdout.take().unwrap(), child.stderr.take().unwrap());
    s.proc = Some(Proc { child, stdin, generation });

    let app = app.clone();
    std::thread::spawn(move || {
        let err_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut buf);
            buf
        });
        for line in BufReader::new(stdout).lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    telemetry::error("agent.stdout_read_failed", json!({"generation": generation, "error": e.to_string()}));
                    break;
                }
            };
            match serde_json::from_str::<Value>(&line) {
                Ok(ev) => on_event(&app, ev),
                Err(_) => telemetry::error("agent.non_json_output", json!({"generation": generation, "line": line})),
            }
        }
        on_exit(&app, generation, err_reader.join().unwrap_or_default());
    });
    Ok(())
}

/// A tool call failed if its result says so; those sit inside `user` events, not at the top level.
fn is_failure(ev: &Value) -> bool {
    ev.get("is_error").and_then(Value::as_bool) == Some(true)
        || ev.pointer("/message/content").and_then(Value::as_array).is_some_and(|blocks| {
            blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result") && b.get("is_error") == Some(&Value::Bool(true)))
        })
}

fn on_event(app: &AppHandle, ev: Value) {
    let ctx = app.state::<Ctx>();
    let kind = ev.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
    let turn = {
        let mut s = ctx.agent.0.lock().unwrap();
        if let Some(id) = ev.get("session_id").and_then(Value::as_str) {
            s.session_id = Some(id.to_owned());
        }
        // A steering message that landed just after a `result` starts a turn of its own.
        if !s.running && (kind == "assistant" || kind == "user") {
            s.begin_turn(false);
            telemetry::info("agent.turn_start", json!({"turn": s.turn, "reason": "queued message", "session_id": s.session_id}));
            emit(app, json!({"type": "local", "kind": "busy"}));
        }
        s.turn_events += 1;
        s.turn
    };
    // Raw stream-json event: the full transcript, tool inputs/results included.
    let log = if is_failure(&ev) { telemetry::error } else { telemetry::info };
    log("agent.event", json!({"turn": turn, "event": ev}));
    let result = (kind == "result").then(|| ev.get("subtype").and_then(Value::as_str) == Some("success") && !is_failure(&ev));
    emit(app, ev);
    if let Some(success) = result {
        end_turn(app, success, "");
    }
}

fn on_exit(app: &AppHandle, generation: u64, stderr: String) {
    let ctx = app.state::<Ctx>();
    let (was_running, stopping) = {
        let mut s = ctx.agent.0.lock().unwrap();
        let current = s.proc.as_ref().is_some_and(|p| p.generation == generation);
        let status = if current { s.proc.take().and_then(|mut p| p.child.wait().ok()) } else { None };
        let failed = s.running && current && !s.stopping;
        let log = if failed { telemetry::error } else { telemetry::info };
        log("agent.process_exit", json!({
            "generation": generation,
            // None = killed by a signal (e.g. Stop) or replaced by a newer process.
            "exit_code": status.and_then(|st| st.code()),
            "stderr": stderr,
            "mid_turn": s.running && current,
        }));
        (s.running && current, s.stopping)
    };
    if was_running {
        let reason = if stopping { "Stopped." } else if stderr.trim().is_empty() { "The agent exited unexpectedly." } else { stderr.trim() };
        end_turn(app, false, reason);
    }
}

/// Turn is over: report how it ended, validate everything, and hand errors back to the agent.
fn end_turn(app: &AppHandle, success: bool, error: &str) {
    let ctx = app.state::<Ctx>();
    let (turn, retry) = {
        let mut s = ctx.agent.0.lock().unwrap();
        s.running = false;
        let log = if success { telemetry::info } else { telemetry::error };
        log("agent.turn_end", json!({
            "turn": s.turn,
            "success": success,
            "error": error,
            "duration_ms": telemetry::now_ms() - s.turn_started,
            "events": s.turn_events,
            "session_id": s.session_id,
        }));
        let retry = success && s.autofix_left > 0;
        if retry {
            s.autofix_left -= 1;
        }
        (s.turn, retry)
    };
    emit(app, json!({"type": "local", "kind": "done", "success": success, "stderr": error}));

    let reports = ctx.validator.validate_all();
    let invalid: Vec<_> = reports.iter().filter(|r| !r.ok()).collect();
    let log = if invalid.is_empty() { telemetry::info } else { telemetry::error };
    log("validation.after_turn", json!({"turn": turn, "checked": reports.len(), "invalid": invalid}));
    let _ = app.emit("files-changed", &reports);
    let failing: Vec<String> = invalid
        .iter()
        .map(|r| format!("{}:\n{}", r.path, r.errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")))
        .collect();
    if retry && !failing.is_empty() {
        let msg = format!(
            "The app's validator found errors in files you wrote. Fix them, then reply with one line saying what you changed.\n\n{}",
            failing.join("\n")
        );
        telemetry::info("agent.autofix", json!({"turn": turn, "attempts_left": ctx.agent.0.lock().unwrap().autofix_left}));
        if let Err(e) = send(app, msg, true) {
            telemetry::error("agent.autofix_failed", json!({"turn": turn, "error": e}));
        }
    }
}

/// Start a turn, or steer the running one.
pub fn send(app: &AppHandle, message: String, autofix: bool) -> Result<(), String> {
    let ctx = app.state::<Ctx>();
    let mut s = ctx.agent.0.lock().unwrap();
    let line = json!({"type": "user", "message": {"role": "user", "content": message}}).to_string() + "\n";
    let mut attempts = 0;
    loop {
        if s.proc.is_none() {
            spawn(app, &mut s)?;
        }
        let p = s.proc.as_mut().unwrap();
        match p.stdin.write_all(line.as_bytes()).and_then(|_| p.stdin.flush()) {
            Ok(()) => break,
            // The process died between turns (crash, killed): start a fresh one once.
            Err(e) if attempts == 0 => {
                telemetry::error("agent.stdin_write_failed", json!({"generation": p.generation, "error": e.to_string()}));
                let _ = p.child.kill();
                s.proc = None;
                attempts += 1;
            }
            Err(e) => return Err(format!("couldn't reach the agent: {e}")),
        }
    }
    if s.running {
        telemetry::info("agent.steer", json!({"turn": s.turn, "message": message}));
        emit(app, json!({"type": "local", "kind": "steer", "text": message}));
    } else {
        s.begin_turn(autofix);
        telemetry::info("agent.turn_start", json!({"turn": s.turn, "autofix": autofix, "session_id": s.session_id}));
        emit(app, json!({"type": "local", "kind": if autofix { "autofix" } else { "prompt" }, "text": message}));
    }
    Ok(())
}
