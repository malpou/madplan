//! Usage: cargo run -q -p validator                         (validate everything)
//!        cargo run -q -p validator -- plans/2026-W42.yaml
//! Run from the repo root.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = std::env::current_dir().expect("cwd");
    let v = match validator::Validator::new(&root) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let args: Vec<PathBuf> = std::env::args().skip(1).map(|p| root.join(p)).collect();
    let paths = if args.is_empty() { v.all_files() } else { args };
    let mut ok = true;
    for p in &paths {
        let r = v.validate_file(p);
        println!("{} {}", if r.ok() { "✓" } else { "✗" }, r.path);
        for e in &r.errors {
            println!("   - {e}");
        }
        ok &= r.ok();
    }
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
