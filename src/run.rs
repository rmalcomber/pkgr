//! Foreground execution of the chosen task.

use std::env;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

/// Resolves a command name against PATH.
///
/// Rust's `Command` does not consult PATHEXT, so on Windows a package manager
/// installed as a `.cmd` shim — which npm, pnpm and yarn all are — would not be
/// found by name alone. Resolving here also means a missing tool produces a
/// clear message instead of an opaque OS error.
pub fn look_path(bin: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        if let Some(found) = look_in(&dir, bin) {
            return Some(found);
        }
    }
    None
}

/// Windows will only start a file whose extension is in PATHEXT.
///
/// An extensionless file of the same name is common and must be ignored: npm
/// ships a `#!/usr/bin/env bash` script called `npm` right next to `npm.cmd`,
/// and handing that to `CreateProcess` fails with "%1 is not a valid Win32
/// application" (os error 193). Only an explicit extension on `bin` itself is
/// taken as given.
#[cfg(windows)]
fn look_in(dir: &Path, bin: &str) -> Option<PathBuf> {
    let extensions = path_extensions();

    // "npm.cmd" was asked for by name, so use it as spelled.
    if has_executable_extension(bin, &extensions) {
        let exact = dir.join(bin);
        if exact.is_file() {
            return Some(exact);
        }
        return None;
    }

    extensions
        .iter()
        .map(|ext| dir.join(format!("{bin}{ext}")))
        .find(|candidate| candidate.is_file())
}

#[cfg(windows)]
fn path_extensions() -> Vec<String> {
    env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

/// PATHEXT entries are ASCII by definition, so an ASCII comparison is exact —
/// and it avoids linking std's Unicode case-mapping tables.
#[cfg(windows)]
fn has_executable_extension(bin: &str, extensions: &[String]) -> bool {
    let lower = bin.to_ascii_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext.as_str()))
}

/// On unix the name is used exactly as given; there are no implicit extensions.
///
/// The execute bit has to be checked, though. A shell skips a non-executable
/// file and keeps searching, so a stray `npm` earlier on PATH must not shadow
/// the real one — taking it would fail with EACCES instead.
#[cfg(unix)]
fn look_in(dir: &Path, bin: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let exact = dir.join(bin);
    let metadata = fs::metadata(&exact).ok()?;
    // Any of the three execute bits: which one applies depends on who is
    // running, and the kernel is the authority on that either way.
    if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
        return Some(exact);
    }
    None
}

/// Runs `bin` with `args` in `dir`, wired to the current terminal, and reports
/// its exit code.
///
/// Stdin, stdout and stderr are inherited rather than captured, so interactive
/// dev servers, progress bars and colour behave exactly as they would when the
/// command is run by hand.
pub fn run(dir: &Path, bin: &str, args: &[String]) -> Result<i32, String> {
    let path = look_path(bin).ok_or_else(|| format!("{bin} was not found on your PATH"))?;

    ignore_ctrl_c();

    let status = Command::new(path)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| format!("cannot start {bin}: {e}"))?;

    // A signal-terminated child reports no code; treat that as an interrupt.
    Ok(status.code().unwrap_or(130))
}

/// Stops Ctrl+C from killing pkgr while the task runs.
///
/// The console delivers Ctrl+C to every process in the group, so the task
/// already receives it. Ignoring it here means pkgr does not die first, and the
/// task gets to shut down and report its own exit code.
#[cfg(windows)]
fn ignore_ctrl_c() {
    unsafe extern "system" fn handler(_ctrl_type: u32) -> i32 {
        1 // handled, so the default terminate behaviour is skipped
    }
    unsafe {
        SetConsoleCtrlHandler(Some(handler), 1);
    }
}

/// The unix half of the same problem.
///
/// This installs a handler that does nothing rather than setting `SIG_IGN`,
/// and the difference matters: an ignored signal stays ignored across `exec`,
/// which would leave the task itself immune to Ctrl+C, whereas a handler is
/// reset to the default. So pkgr survives the interrupt, the task still
/// receives it through the foreground process group, and the task's own exit
/// code is what gets reported.
#[cfg(unix)]
fn ignore_ctrl_c() {
    extern "C" fn handler(_signal: libc::c_int) {}
    // sighandler_t is an integer-width slot, so the function item has to
    // become a pointer before it becomes an address.
    unsafe {
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds a throwaway directory under the OS temp dir.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "pkgr-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn installed(bin: &str) -> bool {
        look_path(bin).is_some()
    }

    #[test]
    fn finds_a_tool_that_exists() {
        // cargo is running this test, so it is certainly on PATH.
        assert!(look_path("cargo").is_some());
    }

    /// Drives the real npm. This is the only coverage of the part that actually
    /// broke in the field: resolving npm to its launchable shim, starting it,
    /// and reading back what it exited with. Skips when npm is absent.
    #[test]
    fn runs_real_npm_scripts() {
        if !installed("npm") || !installed("node") {
            eprintln!("skipping: npm or node is not installed");
            return;
        }

        let dir = temp_dir("npm-integration");
        // npm hands each body to a shell — cmd.exe on Windows, sh elsewhere —
        // so the bodies have to be spelled in the intersection of the two.
        // Parentheses are bare syntax to cmd.exe but a syntax error to sh, and
        // sh strips single quotes where cmd.exe passes them through, so the
        // scripts avoid both and the one body that needs them lives in a file.
        fs::write(
            dir.join("package.json"),
            r#"{
  "name": "pkgr-integration",
  "private": true,
  "scripts": {
    "ok": "node --eval process.exitCode=0",
    "fail": "node --eval process.exitCode=5",
    "cwd": "node cwd-check.js"
  }
}"#,
        )
        .unwrap();
        fs::write(
            dir.join("cwd-check.js"),
            "process.exitCode = require('fs').existsSync('marker') ? 0 : 1;\n",
        )
        .unwrap();
        // The cwd script only passes if cmd.current_dir actually took effect.
        fs::write(dir.join("marker"), "").unwrap();

        for (script, want) in [("ok", 0), ("fail", 5), ("cwd", 0)] {
            let code = run(
                &dir,
                "npm",
                &["run".into(), "--silent".into(), script.into()],
            )
            .unwrap_or_else(|e| panic!("npm run {script} failed to start: {e}"));
            assert_eq!(code, want, "npm run {script}");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn runs_real_deno_tasks() {
        if !installed("deno") {
            eprintln!("skipping: deno is not installed");
            return;
        }

        let dir = temp_dir("deno-integration");
        // The task bodies are quoted because deno task runs them through its
        // own shell, which would otherwise split on the parentheses.
        fs::write(
            dir.join("deno.json"),
            r#"{
  "tasks": {
    "ok": "deno eval 'Deno.exit(0)'",
    "fail": "deno eval 'Deno.exit(5)'"
  }
}"#,
        )
        .unwrap();

        for (task, want) in [("ok", 0), ("fail", 5)] {
            let code = run(
                &dir,
                "deno",
                &["task".into(), "--quiet".into(), task.into()],
            )
            .unwrap_or_else(|e| panic!("deno task {task} failed to start: {e}"));
            assert_eq!(code, want, "deno task {task}");
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// npm ships a POSIX shell script named `npm` alongside `npm.cmd`. Picking
    /// the extensionless one makes CreateProcess fail with os error 193, which
    /// is exactly what happened before look_in was split per platform.
    #[test]
    #[cfg(windows)]
    fn prefers_the_launchable_extension_over_a_shim_script() {
        use std::fs;

        let dir = env::temp_dir().join(format!(
            "pkgr-lookpath-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        // The same arrangement npm installs: a bash script with no extension,
        // and the real launchable shim next to it.
        fs::write(dir.join("faketool"), "#!/usr/bin/env bash\necho hi\n").unwrap();
        fs::write(dir.join("faketool.cmd"), "@echo off\r\necho hi\r\n").unwrap();

        let found = look_in(&dir, "faketool").expect("should resolve");
        assert_eq!(
            found.extension().and_then(|s| s.to_str()),
            Some("cmd"),
            "resolved to {} — an extensionless script cannot be started on Windows",
            found.display()
        );

        // An explicit extension is still honoured as spelled.
        let explicit = look_in(&dir, "faketool.cmd").expect("should resolve");
        assert_eq!(explicit, dir.join("faketool.cmd"));

        // A bare script with no launchable sibling is not a usable result.
        fs::write(dir.join("onlyscript"), "#!/bin/sh\n").unwrap();
        assert!(
            look_in(&dir, "onlyscript").is_none(),
            "an extensionless file alone must not be reported as runnable"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// A shell skips a non-executable file and keeps searching PATH. Taking it
    /// instead would shadow the real tool and fail with EACCES — the unix
    /// counterpart of picking npm's extensionless shim on Windows.
    #[test]
    #[cfg(unix)]
    fn skips_a_file_without_the_execute_bit() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("lookpath");

        // The same name twice: one not executable, one that is.
        fs::write(dir.join("faketool"), "#!/bin/sh\necho hi\n").unwrap();
        fs::set_permissions(dir.join("faketool"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            look_in(&dir, "faketool").is_none(),
            "a file without the execute bit is not runnable"
        );

        fs::set_permissions(dir.join("faketool"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            look_in(&dir, "faketool"),
            Some(dir.join("faketool")),
            "the same file is runnable once the execute bit is set"
        );

        // A directory named like the tool must not be mistaken for it, even
        // though directories carry execute bits of their own.
        fs::create_dir(dir.join("fakedir")).unwrap();
        assert!(look_in(&dir, "fakedir").is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_tool_is_none() {
        assert!(look_path("pkgr-definitely-not-a-real-binary").is_none());
    }

    #[test]
    fn missing_tool_reports_a_path_error() {
        let err = run(
            Path::new("."),
            "pkgr-definitely-not-a-real-binary",
            &["run".into()],
        )
        .unwrap_err();
        assert!(err.contains("not found on your PATH"), "got {err}");
    }
}
