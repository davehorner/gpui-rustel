use music_link::{Snapshot, session_files::SessionFiles};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "music session files {} {}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(path.join("strudel/nested")).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        assert_eq!(
            self.0.parent().unwrap(),
            std::env::temp_dir().canonicalize().unwrap()
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn demos_are_discovered_dynamically_and_cannot_escape_the_catalog() {
    let root = Workspace::new();
    fs::write(root.0.join("strudel/one.strudel"), "$: s(\"bd\")").unwrap();
    let files = SessionFiles::new(root.0.clone(), None, String::new());
    assert_eq!(files.demos().unwrap(), ["one.strudel"]);
    fs::write(root.0.join("strudel/nested/two.strudel"), "$: s(\"hh\")").unwrap();
    assert_eq!(
        files.demos().unwrap(),
        ["nested/two.strudel", "one.strudel"]
    );
    assert_eq!(
        files.load_demo("nested/two.strudel").unwrap(),
        "$: s(\"hh\")"
    );
    assert!(files.load_demo("../one.strudel").is_err());
}

#[test]
fn session_files_are_independent_and_never_overwrite_templates_or_newer_edits() {
    let root = Workspace::new();
    let template = root.0.join("strudel/one.strudel");
    fs::write(&template, "$: s(\"bd\")").unwrap();
    let mut first = SessionFiles::new(root.0.clone(), Some(template.clone()), "one.strudel".into());
    let mut a = Snapshot {
        session: "first".into(),
        source: "$: s(\"bd\")".into(),
        ..Default::default()
    };
    first.sync(&mut a).unwrap();
    assert_ne!(PathBuf::from(&a.session_file), template);
    let mut second = SessionFiles::new(root.0.clone(), None, String::new());
    let mut b = Snapshot {
        session: "second".into(),
        source: a.source.clone(),
        ..Default::default()
    };
    second.sync(&mut b).unwrap();
    assert_ne!(a.session_file, b.session_file);
    a.source = "const bpm = 135; $: s(\"hh\")".into();
    first.sync(&mut a).unwrap();
    assert_eq!(fs::read_to_string(&a.session_file).unwrap(), a.source);
    assert_eq!(fs::read_to_string(&b.session_file).unwrap(), "$: s(\"bd\")");
    assert_eq!(fs::read_to_string(&template).unwrap(), "$: s(\"bd\")");
    fs::write(&a.session_file, "$: s(\"newer edit\")").unwrap();
    a.source = "const bpm = 140; $: s(\"hh\")".into();
    assert!(first.sync(&mut a).is_err());
    assert_eq!(
        fs::read_to_string(&a.session_file).unwrap(),
        "$: s(\"newer edit\")"
    );
    a.source = "$: s(\"newer edit\")".into();
    first.sync(&mut a).unwrap();
}
