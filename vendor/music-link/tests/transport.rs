use music_link::{Command, Snapshot, TransportState};

#[test]
fn transport_is_capability_and_generation_checked_and_seeks_are_bounded() {
    let mut snapshot = Snapshot {
        session: "s".into(),
        generation: 3,
        transport: Some(TransportState {
            paused: false,
            view_cycle: None,
        }),
        cycle: 10.0,
        cps: 0.5,
        ..Default::default()
    };
    let mut command:Command=serde_json::from_value(serde_json::json!({"request":"t","session":"s","generation":3,"lane":"","transport":{"action":"seek","cycle":5.0}})).unwrap();
    assert!(command.validate(&snapshot).is_ok());
    assert_eq!(snapshot.display_cycle(2.0), 11.0);
    snapshot.transport.as_mut().unwrap().paused = true;
    assert_eq!(snapshot.display_cycle(2.0), 10.0);
    snapshot.transport.as_mut().unwrap().view_cycle = Some(27.5);
    snapshot.cycle = 0.0;
    assert_eq!(
        snapshot.display_cycle(99.0),
        27.5,
        "stopped playback must not move the inspection scene to zero"
    );
    command.generation = 2;
    assert!(command.validate(&snapshot).is_err());
    command.generation = 3;
    command.solo = Some(true);
    assert!(command.validate(&snapshot).is_err());
    command.solo = None;
    command.transport = Some(music_link::TransportCommand::Seek { cycle: f64::NAN });
    assert!(command.validate(&snapshot).is_err());
    command.transport = Some(music_link::TransportCommand::Seek { cycle: -1.0 });
    assert!(command.validate(&snapshot).is_err());
    command.transport = Some(music_link::TransportCommand::Play);
    snapshot.transport = None;
    assert!(command.validate(&snapshot).is_err());
}
