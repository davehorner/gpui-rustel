//! File work stays on Link's background worker, never the audio/producer turn.
use crate::Snapshot;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub struct SessionFiles {
    root: PathBuf,
    path: Option<PathBuf>,
    last_source: Option<String>,
    last_disk: Option<String>,
    configured: Option<PathBuf>,
    initial_demo: String,
}
impl SessionFiles {
    pub fn new(root: PathBuf, configured: Option<PathBuf>, initial_demo: String) -> Self {
        Self {
            root,
            path: None,
            last_source: None,
            last_disk: None,
            configured,
            initial_demo,
        }
    }
    pub fn environment() -> Self {
        Self::new(
            std::env::var_os("MUSIC_LINK_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
            std::env::var_os("RUSTEL_SESSION_FILE").map(PathBuf::from),
            std::env::var("RUSTEL_DEMO_NAME").unwrap_or_default(),
        )
    }
    pub fn demos(&self) -> io::Result<Vec<String>> {
        fn visit(base: &Path, dir: &Path, result: &mut Vec<String>) -> io::Result<()> {
            if !dir.exists() {
                return Ok(());
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    visit(base, &entry.path(), result)?;
                } else if kind.is_file() {
                    result.push(
                        entry
                            .path()
                            .strip_prefix(base)
                            .map_err(io::Error::other)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
            Ok(())
        }
        let base = self.root.join("strudel");
        let mut result = Vec::new();
        visit(&base, &base, &mut result)?;
        result.sort();
        Ok(result)
    }
    pub fn load_demo(&self, name: &str) -> io::Result<String> {
        if !self.demos()?.iter().any(|n| n == name) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown demo"));
        }
        let base = self.root.join("strudel").canonicalize()?;
        let path = base.join(name).canonicalize()?;
        if !path.starts_with(&base) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "demo escapes catalog",
            ));
        }
        read_score(&path)
    }
    pub fn sync(&mut self, snapshot: &mut Snapshot) -> io::Result<()> {
        snapshot.demos = self.demos()?;
        if snapshot.demo.is_empty() {
            snapshot.demo = self.initial_demo.clone();
        }
        if snapshot.source.is_empty() {
            return Ok(());
        }
        if self.path.is_none() {
            let dir = self.root.join("sessions");
            fs::create_dir_all(&dir)?;
            let dir = dir.canonicalize()?;
            let configured = self.configured.take().filter(|p| {
                p.parent()
                    .and_then(|p| p.canonicalize().ok())
                    .is_some_and(|p| p == dir)
            });
            let mut path = configured
                .unwrap_or_else(|| dir.join(format!("{}.strudel", safe_name(&snapshot.session))));
            let owner = path.with_extension("owner");
            if fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&owner)
                .and_then(|mut f| std::io::Write::write_all(&mut f, snapshot.session.as_bytes()))
                .is_err()
            {
                path = dir.join(format!("{}.strudel", safe_name(&snapshot.session)));
                if path.exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "session file already owned",
                    ));
                }
            }
            self.path = Some(path);
        }
        let path = self.path.as_ref().unwrap();
        snapshot.session_file = path.to_string_lossy().into_owned();
        if self.last_source.as_deref() != Some(&snapshot.source) {
            if let Some(written) = &self.last_disk {
                let disk = read_score(path)?;
                if &disk != written && disk != snapshot.source {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "session file has newer edits; waiting for the watched score",
                    ));
                }
            }
            atomic_write(path, &snapshot.source)?;
            self.last_source = Some(snapshot.source.clone());
            self.last_disk = Some(snapshot.source.clone());
        }
        Ok(())
    }
}
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect()
}
fn read_score(path: &Path) -> io::Result<String> {
    if fs::metadata(path)?.len() > 1_000_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "score exceeds 1 MB",
        ));
    }
    fs::read_to_string(path)
}
fn atomic_write(path: &Path, source: &str) -> io::Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, source)?;
    fs::rename(&temporary, path)
}
