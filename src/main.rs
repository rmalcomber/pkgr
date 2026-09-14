//! pkgr: pick a script from package.json or a task from deno.json and run it.

mod json;
mod manifest;
mod run;
mod ui;

use std::env;
use std::fs;
use std::process::exit;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 130 is the conventional code for a command interrupted at the terminal.
const EXIT_USAGE: i32 = 1;
const EXIT_CANCELLED: i32 = 130;

fn main() {
    exit(pkgr());
}

fn pkgr() -> i32 {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "-version" || a == "--version") {
        println!("pkgr {VERSION}");
        return 0;
    }
    if args
        .iter()
        .any(|a| a == "-h" || a == "--help" || a == "-help")
    {
        usage();
        return 0;
    }
    if args.len() > 1 {
        eprintln!("pkgr: expected at most one path, got {}", args.len());
        usage();
        return EXIT_USAGE;
    }

    let manifest = match manifest::resolve(args.first().map(String::as_str).unwrap_or("")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("pkgr: {e}");
            return EXIT_USAGE;
        }
    };

    let source = match fs::read_to_string(&manifest.path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("pkgr: cannot read {}: {e}", manifest.path.display());
            return EXIT_USAGE;
        }
    };

    let parsed = match json::parse(&source, manifest.task_key()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("pkgr: {}: {e}", manifest.path.display());
            return EXIT_USAGE;
        }
    };

    let title = format!("Select a task  ({})", display_path(&manifest));

    let index = match ui::select(&title, &parsed.tasks) {
        Ok(i) => i,
        Err(ui::Error::Cancelled) => return EXIT_CANCELLED,
        Err(ui::Error::NotInteractive) => {
            // Nothing can be picked, but showing what is on offer beats
            // leaving the caller with only an error.
            eprintln!("pkgr: pkgr needs an interactive terminal to choose a task");
            list_tasks(&manifest, &parsed);
            return EXIT_USAGE;
        }
        Err(ui::Error::Io(e)) => {
            eprintln!("pkgr: {e}");
            return EXIT_USAGE;
        }
    };

    let (bin, cmd_args) = manifest::command(
        &manifest,
        &parsed.package_manager,
        &parsed.tasks[index].name,
    );

    // Echo the resolved command so it is obvious what ran, and so the line can
    // be copied straight back into the shell.
    eprintln!("\n> {} {}\n", bin, cmd_args.join(" "));

    match run::run(&manifest.dir, &bin, &cmd_args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("pkgr: {e}");
            EXIT_USAGE
        }
    }
}

/// Shortens the manifest path to a relative one when it sits under the current
/// directory.
fn display_path(m: &manifest::Manifest) -> String {
    if let Ok(wd) = env::current_dir() {
        if let Ok(rel) = m.path.strip_prefix(&wd) {
            return rel.display().to_string();
        }
    }
    m.path.display().to_string()
}

/// Prints the tasks and the command that would run each one, for when the
/// picker cannot be shown.
fn list_tasks(m: &manifest::Manifest, parsed: &json::Manifest) {
    eprintln!("\nTasks in {}:", m.path.display());
    for task in &parsed.tasks {
        let (bin, args) = manifest::command(m, &parsed.package_manager, &task.name);
        eprintln!("  {:<16} {} {}", task.name, bin, args.join(" "));
    }
}

fn usage() {
    eprint!(
        "pkgr - pick and run a script from package.json or deno.json

Usage:
  pkgr [path]

  path   a directory holding package.json, deno.json or deno.jsonc,
         or one of those files directly. Defaults to the current directory.

Flags:
  -version   print the version and exit
  -h         show this help

Examples:
  pkgr
  pkgr .
  pkgr ./packages/api
  pkgr ./packages/api/package.json
"
    );
}
