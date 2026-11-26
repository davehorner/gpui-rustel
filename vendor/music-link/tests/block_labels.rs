use music_link::{Event, Snapshot};

fn hit(sound: &str, cycle: f64, duration: f64) -> Event {
    serde_json::from_value(serde_json::json!({
        "id": 1, "lane": "#0", "part": "pulse", "cycle": cycle,
        "duration": duration, "pitch": null, "original_pitch": null,
        "sound": sound, "gain": 0.5
    }))
    .unwrap()
}

fn span(source: &str, token: &str) -> (usize, usize) {
    let from = source.find(token).unwrap();
    (from, from + token.len())
}

#[test]
fn drum_labels_use_resolved_names_including_old_snapshots() {
    assert_eq!(hit("bd", 0.0, 0.1).face_label(), "bd");
    assert_eq!(hit("sd", 0.5, 0.1).face_label(), "sd");
    let mut event = hit("hh", 0.0, 0.125);
    let source = "// s(\"wrong\")\nconst guira = s(\"hh*8\").gain(\"0.23 0.1\")";
    event.context = vec![span(source, "hh"), span(source, "8")];
    event.controls.insert("gain".into(), 0.23);
    assert_eq!(event.describe(source), "hh*8.gain(0.23)");
    event.controls.insert("gain".into(), 0.1);
    assert_eq!(event.describe(source), "hh*8.gain(0.1)");
}

#[test]
fn guitar_faces_keep_instrument_pitch_and_selected_structure_leaf() {
    let source = "note(\"c4\").s(\"gm_acoustic_guitar_nylon\").struct(\"~ x ~ xx\")";
    let mut event = hit("gm_acoustic_guitar_nylon", 0.25, 0.25);
    event.pitch = Some(60.0);
    event.context = vec![
        span(source, "c4"),
        span(source, "gm_acoustic_guitar_nylon"),
        span(source, "x ~"),
    ];
    event.context[2].1 = event.context[2].0 + 1;
    assert_eq!(
        event.describe(source),
        "gm_acoustic_guitar_nylon / C4 / struct x"
    );
    event.context[2] = span(source, "xx");
    assert_eq!(
        event.describe(source),
        "gm_acoustic_guitar_nylon / C4 / struct xx"
    );
    event.context = vec![(usize::MAX, usize::MAX), (0, source.len() + 1)];
    assert_eq!(event.describe(source), "gm_acoustic_guitar_nylon / C4");
}

#[test]
fn rests_merge_chords_and_overlaps_and_do_not_invent_edge_silence() {
    let mut snapshot = Snapshot::default();
    snapshot.events = vec![
        hit("bd", 0.0, 0.25),
        hit("sd", 0.5, 0.25),
        hit("hh", 0.0, 0.125),
    ];
    let mut other = hit("rim", 0.25, 0.25);
    other.part = "another_part".into();
    snapshot.events.push(other);
    snapshot.rests = snapshot.inferred_rests();
    assert_eq!(snapshot.rests.len(), 1);
    let rest = &snapshot.rests[0];
    assert_eq!((rest.cycle, rest.duration), (0.25, 0.25));
    assert!(rest.rest && rest.context.is_empty() && rest.controls.is_empty());
    assert_eq!(rest.face_label(), "~");
    assert_eq!(snapshot.with_rests(false).events.len(), 4);
    assert_eq!(snapshot.with_rests(true).events.len(), 5);
    snapshot.events.push(hit("cp", 0.2, 0.4));
    assert!(
        snapshot.inferred_rests().is_empty(),
        "newly accepted events close old gaps"
    );
}
