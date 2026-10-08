//! Runs every `big_space` example sequentially, waiting for each window to close before
//! launching the next. Useful for a manual visual sweep after touching rendering, gizmos,
//! UI, or a bevy version bump.
//!
//! The list of examples is discovered at runtime by scanning `examples/*.rs`, so new
//! examples are picked up automatically.
//!
//! Run with: `cargo run --bin run_examples`

use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn discover_examples() -> Vec<String> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("examples");
    let entries = fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read examples dir {}: {err}", dir.display()));
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "rs")
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    names.sort();
    names
}

fn main() -> ExitCode {
    let examples = discover_examples();
    if examples.is_empty() {
        eprintln!("no examples found");
        return ExitCode::FAILURE;
    }
    let cargo = env!("CARGO");
    for (i, example) in examples.iter().enumerate() {
        println!("\n=== [{}/{}] {example} ===", i + 1, examples.len());
        let status = match Command::new(cargo)
            .args(["run", "--release", "--example", example, "--all-features"])
            .status()
        {
            Ok(status) => status,
            Err(err) => {
                eprintln!("failed to invoke cargo for example `{example}`: {err}");
                return ExitCode::FAILURE;
            }
        };
        if !status.success() {
            eprintln!("example `{example}` exited with {status}; aborting remaining examples");
            return ExitCode::FAILURE;
        }
    }
    println!("\nAll {} examples completed.", examples.len());
    ExitCode::SUCCESS
}
