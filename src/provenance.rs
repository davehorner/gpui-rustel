//! Source identities are recovered from the exact evaluated document, never lane order.
use music_link::{Binding, Definition, NamedPart, Snapshot};
pub fn bindings(source: &str) -> Vec<Binding> {
    crate::source_identity::bindings(source)
        .into_iter()
        .map(|b| Binding {
            name: b.name,
            color: b.color,
            from: b.from,
            to: b.to,
            definition_from: b.definition_from,
            definition_to: b.definition_to,
        })
        .collect()
}

pub fn enrich(snapshot: &mut Snapshot) {
    if snapshot.bindings.is_empty() {
        snapshot.bindings = bindings(&snapshot.source);
    }
    if snapshot.definitions.is_empty() {
        for binding in &snapshot.bindings {
            if snapshot.definitions.iter().any(|d| d.name == binding.name) {
                continue;
            }
            snapshot.definitions.push(Definition {
                name: binding.name.clone(),
                color: binding.color,
                ..Default::default()
            });
        }
    }
    for event in &mut snapshot.events {
        if event.part.is_empty() {
            let identity = event
                .context
                .iter()
                // Runtime contexts run from the emitting transformations back to
                // their shared inputs. Preserve that order: a short chord helper
                // must not steal notes from the instrument derived from it.
                .find_map(|(from, to)| {
                    snapshot
                        .bindings
                        .iter()
                        .filter(|b| {
                            *from >= b.definition_from && *to <= b.definition_to && from < to
                        })
                        .min_by_key(|b| b.definition_to - b.definition_from)
                });
            if let Some(binding) = identity {
                event.part = binding.name.clone();
                event.part_color = binding.color;
            }
        }
        if !event.part.is_empty()
            && let Some(lane) = snapshot.lanes.iter_mut().find(|l| l.id == event.lane)
            && !lane.parts.iter().any(|p| p.name == event.part)
        {
            lane.parts.push(NamedPart {
                name: event.part.clone(),
                color: event.part_color,
                helper: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emitting_instruments_keep_shared_chords_in_separate_lanes() {
        let source = "const harmony = chord(\"Am9\"); const keys = harmony.s(\"piano\"); const pad = harmony.s(\"strings\"); $: stack(keys, pad);";
        let chord = source.find("Am9").unwrap();
        let events = ["piano", "strings"].map(|sound| {
            let start = source.find(sound).unwrap();
            music_link::Event {
                lane: "#0".into(),
                sound: sound.into(),
                context: vec![(start, start + sound.len()), (chord, chord + 3)],
                ..crate::model::test_event()
            }
        });
        let mut snapshot = Snapshot {
            source: source.into(),
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            events: events.into(),
            ..Default::default()
        };
        enrich(&mut snapshot);
        assert_eq!(snapshot.events[0].part, "keys");
        assert_eq!(snapshot.events[1].part, "pad");
        assert_eq!(
            snapshot.events[0].part_color,
            music_link::named_color("keys")
        );
        let projected = snapshot.visual_lanes(&["#0".into()], "");
        assert_ne!(projected.events[0].lane, projected.events[1].lane);
        assert!(
            !projected
                .lanes
                .iter()
                .any(|l| l.parts.iter().any(|p| p.name == "harmony"))
        );
        assert_eq!(projected.events.len(), 2);
    }
    #[test]
    fn shadowed_parameters_are_not_colored_as_score_constants() {
        let source = "const bass = note(\"c3\"); const f = bass => bass; $: bass;";
        assert_eq!(
            bindings(source).iter().filter(|b| b.name == "bass").count(),
            2
        );
    }
    #[test]
    fn exact_utf8_context_colors_named_parts_and_uses() {
        let source = "// const fake = 0\nconst mélodie = note(\"c3\");\n$: mélodie.slow(2);";
        let start = source.find("c3").unwrap();
        let mut s = Snapshot {
            source: source.into(),
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            events: vec![music_link::Event {
                lane: "#0".into(),
                context: vec![(start, start + 2)],
                ..crate::model::test_event()
            }],
            ..Default::default()
        };
        enrich(&mut s);
        assert_eq!(s.definitions.len(), 1);
        assert_eq!(s.bindings.len(), 2);
        assert_eq!(s.events[0].part, "mélodie");
        assert_eq!(s.events[0].part_color, music_link::named_color("mélodie"));
        assert_eq!(s.lanes[0].parts[0].name, "mélodie");
        assert!(
            s.bindings
                .iter()
                .all(|b| &source[b.from..b.to] == "mélodie")
        );
    }
}
