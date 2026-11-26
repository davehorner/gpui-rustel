use music_link::Lane;

#[test]
fn statement_selection_preserves_original_multiline_bytes_and_stops_at_its_end() {
    let document = "// café\r\nconst bass = note(\"c3\")\r\n  $:stack(\r\n    bass, s(\"hh*8\")\r\n  ).gain(0.3) // outside the lane\r\n$: bass\r\n";
    let lane = Lane {
        line: 3,
        source: "$: stack(\n    bass, s(\"hh*8\")\n  ).gain(0.3)".into(),
        ..Default::default()
    };
    let (from, to) = lane.source_range(document).unwrap();
    assert_eq!(
        &document[from..to],
        "$:stack(\r\n    bass, s(\"hh*8\")\r\n  ).gain(0.3)"
    );
    assert!(from <= document.find("$:stack").unwrap());
    assert!(document.find("outside").unwrap() >= to);
    let next = Lane {
        line: 6,
        source: "$: bass".into(),
        ..Default::default()
    };
    let range = next.source_range(document).unwrap();
    assert_eq!(&document[range.0..range.1], "$: bass");
    assert!(range.0 > to);
}

#[test]
fn mismatched_or_unavailable_source_cannot_select_an_unrelated_statement() {
    let document = "$: s(\"bd\")\n$: s(\"bd\")\n";
    let mut lane = Lane {
        line: 2,
        source: "$: s(\"bd\")".into(),
        ..Default::default()
    };
    assert_eq!(lane.source_range(document), Some((11, 21)));
    lane.source = "$: s(\"sd\")".into();
    assert_eq!(lane.source_range(document), None);
    lane.source.clear();
    assert_eq!(lane.source_range(document), None);
}
