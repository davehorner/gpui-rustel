use music_link::{Command, TransportCommand};
use rustel_ui_adapter::{Adapter, Config, Hosted, Output};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn command(session: &str, generation: u64, request: &str) -> Command {
    Command {
        session: session.into(),
        generation,
        request: request.into(),
        lane: "#0".into(),
        demo: None,
        tempo: None,
        source_update: None,
        transport: None,
        semitones: 0,
        solo: None,
    }
}
fn layout(source: &str, generation: u64) -> String {
    json!({"ui_layout":{"version":2,"generation":generation,"source_revision":format!("{:x}",Sha256::digest(source.as_bytes())),"lanes":[{"id":0,"label":"bass","line":1}]}}).to_string()
}
#[test]
fn source_generation_and_control_semantics() {
    let source = "bass: note(\"c3\").s(\"sine\")";
    let mut a = Adapter::new("test".into(), source.into(), 1);
    a.ingest(&layout(source, 1)).unwrap();
    assert_eq!(a.snapshot.source, source);
    let mut c = command("test", a.snapshot.generation, "pause");
    c.transport = Some(TransportCommand::Pause);
    assert!(a.encode(c).unwrap_err().contains("not pause"));
    let mut c = command("test", a.snapshot.generation, "solo");
    c.solo = Some(true);
    let value = a.encode(c).unwrap();
    assert_eq!(value["ui_control"]["generation"], 1);
    assert_eq!(value["ui_control"]["kind"], "lane_solo");
    a.ingest(
        &json!({"ui_control_ack":{"version":1,"request":"solo","accepted":true,"generation":2}})
            .to_string(),
    )
    .unwrap();
    a.ingest(&layout(source, 2)).unwrap();
    assert_eq!(a.snapshot.solo_lane.as_deref(), Some("#0"));
    let mut clear = command("test", a.snapshot.generation, "clear-solo");
    clear.lane.clear();
    clear.solo = Some(false);
    assert!(a.encode(clear).unwrap()["ui_control"]["lane"].is_null());
    let mut source_edit = command("test", a.snapshot.generation, "edit");
    source_edit.source_update = Some("changed".into());
    assert!(
        a.encode(source_edit)
            .unwrap_err()
            .contains("Source rewriting")
    );
    assert!(a.encode(command("test", 1, "stale")).is_err());
    a.ingest(&layout("different source", 3)).unwrap();
    assert!(a.snapshot.source.is_empty());
    assert!(a.snapshot.file_error.is_some());
}
#[test]
fn real_accepted_voice_fields_are_not_invented_provenance() {
    let mut a = Adapter::new("test".into(), "écho".into(), 1);
    a.ingest(&layout("écho", 1)).unwrap();
    let value = json!({"ui_events":{"version":2,"generation":1,"source_revision":a.revision,"cycle":0.,"cps":0.5,"events":[{"onset_id":0,"whole_begin":"0/1","duration_seconds":0.5,"frequency_hz":440.,"gain":0.2,"lane":0,"sound":"sine","context":[[0,2],[1,3]]}]}});
    let output = a.ingest(&value.to_string()).unwrap();
    assert!(matches!(output.first(), Some(Output::Snapshot(_))));
    let e = &a.snapshot.events[0];
    assert_eq!(e.pitch, Some(69.));
    assert_eq!(e.original_pitch, None);
    assert!(e.part.is_empty());
    assert_eq!(e.context, vec![(0, 2)]);
    assert_eq!(e.duration, 0.25);
    let mut stale = value;
    stale["ui_events"]["generation"] = json!(9);
    assert!(a.ingest(&stale.to_string()).unwrap().is_empty());
}
#[test]
#[ignore = "requires RUSTEL_UPSTREAM_EXE pointing at a device-audio,midi upstream build"]
fn owned_upstream_process_streams_and_acknowledges() {
    let executable =
        PathBuf::from(std::env::var_os("RUSTEL_UPSTREAM_EXE").expect("RUSTEL_UPSTREAM_EXE"));
    let root = std::env::temp_dir().join(format!("rustel-ui-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let score = root.join("test.strudel");
    std::fs::write(
        &score,
        "bass: note(\"c3 e3 g3 b3\").s(\"sine\").gain(0.1).scope()\n",
    )
    .unwrap();
    let hosted = Hosted::spawn(Config {
        executable,
        score,
        silent: true,
        demo_dir: None,
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut snapshot = None;
    let mut audio = false;
    while Instant::now() < deadline
        && (!audio
            || snapshot
                .as_ref()
                .is_none_or(|s: &music_link::Snapshot| s.events.is_empty()))
    {
        if let Ok(output) = hosted.events.recv_timeout(Duration::from_millis(100)) {
            match output {
                Output::Snapshot(s) => snapshot = Some(*s),
                Output::Audio(a) => {
                    assert!(a.valid());
                    assert!(a.sides.is_empty());
                    audio = true;
                }
                Output::Status(s) => eprintln!("{s}"),
                _ => {}
            }
        }
    }
    let snapshot = snapshot.unwrap();
    assert!(!snapshot.events.is_empty());
    assert!(audio);
    let mut c = command(&snapshot.session, snapshot.generation, "transpose");
    c.semitones = 7;
    hosted.send(c).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut accepted = false;
    while Instant::now() < deadline {
        if let Ok(Output::Response(r)) = hosted.events.recv_timeout(Duration::from_millis(100))
            && r.request == "transpose"
        {
            assert!(r.accepted, "{}", r.message);
            accepted = true;
            break;
        }
    }
    assert!(accepted);
    drop(hosted);
}

#[test]
#[ignore = "requires RUSTEL_UPSTREAM_EXE; switches real watched scores"]
fn demo_switch_is_acknowledged_and_does_not_rewrite_scores() {
    let executable = std::env::var_os("RUSTEL_UPSTREAM_EXE")
        .expect("RUSTEL_UPSTREAM_EXE")
        .into();
    let root = std::env::temp_dir().join(format!("rustel-demo-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let first = "bass: note(\"c3 e3\").s(\"sine\").gain(0.1).scope()\n";
    let second = "lead: note(\"c5 g5\").s(\"triangle\").gain(0.1).scope()\n";
    let score = root.join("first.strudel");
    std::fs::write(&score, first).unwrap();
    std::fs::write(root.join("second.strudel"), second).unwrap();
    let hosted = Hosted::spawn(Config {
        executable,
        score: score.clone(),
        silent: true,
        demo_dir: Some(root.clone()),
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let snapshot = loop {
        assert!(Instant::now() < deadline);
        if let Ok(Output::Snapshot(s)) = hosted.events.recv_timeout(Duration::from_millis(100))
            && !s.events.is_empty()
        {
            break s;
        }
    };
    assert!(snapshot.demos.contains(&"second.strudel".to_owned()));
    let mut c = command(&snapshot.session, snapshot.generation, "demo");
    c.demo = Some("second.strudel".into());
    c.lane.clear();
    hosted.send(c).unwrap();
    let mut accepted = false;
    let mut changed = false;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && !(accepted && changed) {
        match hosted.events.recv_timeout(Duration::from_millis(100)) {
            Ok(Output::Response(r)) if r.request == "demo" => {
                assert!(r.accepted, "{}", r.message);
                accepted = true;
            }
            Ok(Output::Snapshot(s)) if s.demo == "second.strudel" && !s.events.is_empty() => {
                assert!(s.generation > snapshot.generation);
                assert_eq!(s.source, second);
                changed = true;
            }
            _ => {}
        }
    }
    assert!(accepted && changed);
    assert_eq!(std::fs::read_to_string(&score).unwrap(), first);
    assert_eq!(
        std::fs::read_to_string(root.join("second.strudel")).unwrap(),
        second
    );
    let mut stale = command(&snapshot.session, snapshot.generation, "old-demo");
    stale.demo = Some("first.strudel".into());
    hosted.send(stale).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut rejected = false;
    while Instant::now() < deadline {
        if let Ok(Output::Response(r)) = hosted.events.recv_timeout(Duration::from_millis(100))
            && r.request == "old-demo"
        {
            assert!(!r.accepted);
            rejected = true;
            break;
        }
    }
    assert!(rejected);
    drop(hosted);
    std::fs::remove_dir_all(root).unwrap();
}
