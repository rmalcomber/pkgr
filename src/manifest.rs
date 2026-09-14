//! Locating a manifest and deciding which tool runs its tasks.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Npm,
    Deno,
}

pub struct Manifest {
    pub path: PathBuf,
    pub dir: PathBuf,
    pub kind: Kind,
}

/// Probed in order when the argument names a directory.
const CANDIDATES: [&str; 3] = ["package.json", "deno.json", "deno.jsonc"];

#[derive(Debug, PartialEq)]
pub enum Error {
    NotFound(PathBuf),
    Unsupported(String),
    Io { path: PathBuf, msg: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound(dir) => write!(
                f,
                "{}: no package.json, deno.json or deno.jsonc found",
                dir.display()
            ),
            Error::Unsupported(name) => write!(
                f,
                "{name} is not a supported manifest (want package.json, deno.json or deno.jsonc)"
            ),
            Error::Io { path, msg } => write!(f, "cannot read {}: {msg}", path.display()),
        }
    }
}

impl Manifest {
    pub fn task_key(&self) -> &'static str {
        match self.kind {
            Kind::Npm => "scripts",
            Kind::Deno => "tasks",
        }
    }
}

/// Turns a command-line argument into a located manifest. The argument may be
/// empty (the current directory), a directory, or a manifest file itself.
pub fn resolve(arg: &str) -> Result<Manifest, Error> {
    let start = if arg.is_empty() {
        env::current_dir().map_err(|e| Error::Io {
            path: PathBuf::from("."),
            msg: e.to_string(),
        })?
    } else {
        PathBuf::from(arg)
    };

    // `absolute` rather than `canonicalize`: it never produces Windows'
    // `\\?\` extended-length prefix, and it leaves symlinks unresolved, so tasks
    // run in the directory as typed. It does not touch the filesystem, so a
    // missing path is caught explicitly to keep the "cannot read" error.
    let io_error = |e: std::io::Error| Error::Io {
        path: start.clone(),
        msg: e.to_string(),
    };
    let start = std::path::absolute(&start).map_err(io_error)?;
    std::fs::metadata(&start).map_err(io_error)?;

    let path = if start.is_dir() {
        CANDIDATES
            .iter()
            .map(|name| start.join(name))
            .find(|p| p.is_file())
            .ok_or_else(|| Error::NotFound(start.clone()))?
    } else {
        start
    };

    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let kind = kind_of(base)?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    Ok(Manifest { path, dir, kind })
}

/// Unrecognised file names are rejected rather than assumed to be Deno.
fn kind_of(base: &str) -> Result<Kind, Error> {
    match base {
        "package.json" => Ok(Kind::Npm),
        "deno.json" | "deno.jsonc" => Ok(Kind::Deno),
        other => Err(Error::Unsupported(other.to_string())),
    }
}

/// Which tool runs this manifest's tasks. Deno manifests always use deno; for
/// package.json the `packageManager` field wins, then a lockfile, then npm.
pub fn package_manager(m: &Manifest, field: &str) -> String {
    if m.kind == Kind::Deno {
        return "deno".into();
    }

    // Corepack format, e.g. "pnpm@9.1.0". An unrecognised name falls through to
    // lockfile detection rather than producing a command that cannot run.
    let name = field
        .split('@')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(name.as_str(), "npm" | "pnpm" | "yarn" | "bun") {
        return name;
    }

    const LOCKFILES: [(&str, &str); 5] = [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
    ];

    for (file, pm) in LOCKFILES {
        if m.dir.join(file).is_file() {
            return pm.into();
        }
    }

    "npm".into()
}

/// The argv that runs `task`, matching what the user would type by hand.
pub fn command(m: &Manifest, field: &str, task: &str) -> (String, Vec<String>) {
    let pm = package_manager(m, field);
    let verb = if pm == "deno" { "task" } else { "run" };
    (pm, vec![verb.to_string(), task.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Builds a throwaway directory under the OS temp dir.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut p = env::temp_dir();
            p.push(format!(
                "pkgr-test-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }

        fn file(&self, name: &str, contents: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::write(&p, contents).unwrap();
            p
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn manifest_in(dir: &Path, kind: Kind) -> Manifest {
        Manifest {
            path: dir.join("package.json"),
            dir: dir.to_path_buf(),
            kind,
        }
    }

    #[test]
    fn resolves_package_json_in_a_directory() {
        let d = TempDir::new("npm");
        d.file("package.json", "{}");

        let m = resolve(d.path().to_str().unwrap()).unwrap();
        assert_eq!(m.kind, Kind::Npm);
        assert_eq!(m.path.file_name().unwrap(), "package.json");
        assert_eq!(m.task_key(), "scripts");
    }

    #[test]
    fn prefers_package_json_over_deno_json() {
        let d = TempDir::new("both");
        d.file("package.json", "{}");
        d.file("deno.json", "{}");

        let m = resolve(d.path().to_str().unwrap()).unwrap();
        assert_eq!(m.path.file_name().unwrap(), "package.json");
    }

    #[test]
    fn resolves_deno_jsonc() {
        let d = TempDir::new("jsonc");
        d.file("deno.jsonc", "{}");

        let m = resolve(d.path().to_str().unwrap()).unwrap();
        assert_eq!(m.kind, Kind::Deno);
        assert_eq!(m.task_key(), "tasks");
    }

    #[test]
    fn resolves_an_explicit_file() {
        let d = TempDir::new("explicit");
        let p = d.file("deno.json", "{}");

        let m = resolve(p.to_str().unwrap()).unwrap();
        assert_eq!(m.kind, Kind::Deno);
        assert_eq!(m.dir, d.path());
    }

    /// Paths get printed in errors and in the picker title, so the extended
    /// prefix must not leak into them.
    #[test]
    #[cfg(windows)]
    fn resolved_paths_have_no_extended_prefix() {
        let d = TempDir::new("prefix");
        d.file("package.json", "{}");

        let m = resolve(d.path().to_str().unwrap()).unwrap();
        assert!(
            !m.path.to_str().unwrap().starts_with(r"\\?\"),
            "path leaked an extended prefix: {}",
            m.path.display()
        );
        assert!(m.path.is_file(), "stripped path must still resolve");
    }

    #[test]
    fn empty_directory_is_not_found() {
        let d = TempDir::new("empty");
        assert!(matches!(
            resolve(d.path().to_str().unwrap()),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn missing_path_is_an_io_error() {
        let d = TempDir::new("missing");
        let p = d.path().join("nope");
        assert!(matches!(
            resolve(p.to_str().unwrap()),
            Err(Error::Io { .. })
        ));
    }

    #[test]
    fn unsupported_file_name_is_rejected() {
        let d = TempDir::new("unsupported");
        let p = d.file("tsconfig.json", "{}");
        assert!(matches!(
            resolve(p.to_str().unwrap()),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn package_manager_field_wins() {
        let d = TempDir::new("field");
        d.file("package-lock.json", "{}"); // would say npm
        let m = manifest_in(d.path(), Kind::Npm);
        assert_eq!(package_manager(&m, "pnpm@9.1.0"), "pnpm");
    }

    #[test]
    fn junk_package_manager_field_falls_through_to_lockfile() {
        let d = TempDir::new("junk");
        d.file("yarn.lock", "");
        let m = manifest_in(d.path(), Kind::Npm);
        assert_eq!(package_manager(&m, "notreal@1.0.0"), "yarn");
    }

    #[test]
    fn lockfiles_are_detected() {
        for (file, want) in [
            ("pnpm-lock.yaml", "pnpm"),
            ("yarn.lock", "yarn"),
            ("bun.lock", "bun"),
            ("bun.lockb", "bun"),
            ("package-lock.json", "npm"),
        ] {
            let d = TempDir::new("lock");
            d.file(file, "");
            let m = manifest_in(d.path(), Kind::Npm);
            assert_eq!(package_manager(&m, ""), want, "for {file}");
        }
    }

    #[test]
    fn npm_is_the_fallback() {
        let d = TempDir::new("bare");
        let m = manifest_in(d.path(), Kind::Npm);
        assert_eq!(package_manager(&m, ""), "npm");
    }

    #[test]
    fn deno_ignores_lockfiles() {
        let d = TempDir::new("deno");
        d.file("package-lock.json", "{}");
        let m = manifest_in(d.path(), Kind::Deno);
        assert_eq!(package_manager(&m, "pnpm@9"), "deno");
    }

    #[test]
    fn commands_match_what_you_would_type() {
        let d = TempDir::new("cmd");
        let npm = manifest_in(d.path(), Kind::Npm);
        assert_eq!(
            command(&npm, "", "build"),
            (
                "npm".to_string(),
                vec!["run".to_string(), "build".to_string()]
            )
        );

        let deno = manifest_in(d.path(), Kind::Deno);
        assert_eq!(
            command(&deno, "", "dev"),
            (
                "deno".to_string(),
                vec!["task".to_string(), "dev".to_string()]
            )
        );
    }
}
