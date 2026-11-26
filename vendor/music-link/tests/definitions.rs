use music_link::{Definition, Event, Lane, NamedPart, Snapshot};

fn hit(part: &str, pitch: Option<f64>, sound: &str, rest: bool) -> Event {
    serde_json::from_value(serde_json::json!({"id":1,"lane":"#0","part":part,"pitch":pitch,"original_pitch":pitch,"sound":sound,"rest":rest,"cycle":0.0,"duration":0.25,"gain":0.5})).unwrap()
}

#[test]
fn shared_constants_are_unique_and_never_take_lane_identities() {
    let mut snapshot = Snapshot {
        definitions: vec![
            Definition {
                name: "bpm".into(),
                value: Some(124.0),
                ..Default::default()
            },
            Definition {
                name: "harmony".into(),
                ..Default::default()
            },
            Definition {
                name: "rhythm".into(),
                ..Default::default()
            },
        ],
        lanes: ["#21", "#28"]
            .into_iter()
            .map(|id| Lane {
                id: id.into(),
                line: 7,
                parts: vec![
                    NamedPart {
                        name: "bpm".into(),
                        helper: true,
                        ..Default::default()
                    },
                    NamedPart {
                        name: "rhythm".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(
        snapshot
            .shared_definitions()
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        ["bpm", "harmony"]
    );
    let projected = snapshot.visual_lanes(&["#21".into(), "#28".into()], "");
    assert_eq!(projected.lanes.len(), 4);
    assert_eq!(projected.lanes[0].id, "#21");
    assert!(projected.lanes[0].label.contains("line 7"));
    assert!(!projected.lanes.iter().any(|l| l.source == "bpm"));
    for cycle in 0..200 {
        snapshot.cycle = cycle as f64;
        snapshot.update_definition_graphs();
    }
    assert_eq!(snapshot.definitions[0].history.len(), 128);
    snapshot.definitions[0].value = Some(135.0);
    snapshot.update_definition_graphs();
    assert_eq!(snapshot.definitions[0].history.last().unwrap()[1], 135.0);
    snapshot.cycle = 0.0;
    snapshot.update_definition_graphs();
    assert_eq!(snapshot.definitions[0].history.len(), 1);
}

#[test]
fn dictionaries_count_only_their_transitive_users_and_do_not_accumulate_duplicate_snapshots() {
    let mut snapshot = Snapshot {
        definitions: vec![
            Definition {
                name: "harmony".into(),
                ..Default::default()
            },
            Definition {
                name: "rhythm".into(),
                dependencies: vec!["harmony".into()],
                ..Default::default()
            },
            Definition {
                name: "guitar".into(),
                dependencies: vec!["rhythm".into()],
                ..Default::default()
            },
        ],
        events: vec![
            hit("guitar", Some(60.0), "", false),
            hit("drums", None, "bd", false),
            hit("rhythm", Some(64.0), "", false),
            hit("guitar", Some(65.0), "", true),
        ],
        ..Default::default()
    };
    snapshot.update_definition_graphs();
    let histogram = snapshot.definitions[0].histogram.clone();
    assert_eq!(histogram.len(), 2);
    assert_eq!(histogram["C4"], 1);
    assert_eq!(histogram["E4"], 1);
    snapshot.update_definition_graphs();
    assert_eq!(snapshot.definitions[0].histogram, histogram);
    snapshot.events.clear();
    snapshot.update_definition_graphs();
    assert!(snapshot.definitions[0].histogram.is_empty());
}
