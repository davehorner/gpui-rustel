use crate::bridge::{Received, Shared};
use music_link::{AudioFrame, Snapshot};
use std::sync::{Arc, Mutex};

pub const MODES: [&str; 9] = [
    "Stage (3D)",
    "Piano roll",
    "Wordfall",
    "Spiral",
    "Pitch wheel",
    "Scope (master)",
    "Spectrum (master)",
    "Vectorscope (master)",
    "Control curves",
];
pub const COLORS: [u32; 8] = [
    0x1ff2d1, 0xff596b, 0xa36bff, 0xffba33, 0x33a6ff, 0xff59cc, 0x9eff47, 0xff8a38,
];
#[derive(Clone)]
pub struct Visual {
    pub session: String,
    pub lane: String,
    pub expanded: Vec<String>,
    pub mode: usize,
    pub foxy_slot: Option<u8>,
    pub foxy_active: bool,
    pub embedded: bool,
    pub embedded_visible: bool,
    pub rests: bool,
    pub orbit: bool,
    pub camera_reframe: u64,
    #[cfg(not(target_arch = "wasm32"))]
    pub agent_port: Option<u16>,
    #[cfg(not(target_arch = "wasm32"))]
    pub music_mcp: Option<nightshade_viewer_core::McpExtension>,
    pub fox_opacity: f32,
    pub gain: f32,
    pub zoom: f32,
    pub pan: [f32; 2],
    parts_score: Option<(String, u64)>,
    seen_parts: std::collections::BTreeSet<String>,
}
impl Default for Visual {
    fn default() -> Self {
        Self {
            session: String::new(),
            lane: String::new(),
            expanded: vec![],
            mode: 0,
            foxy_slot: None,
            foxy_active: false,
            embedded: false,
            embedded_visible: true,
            rests: false,
            orbit: false,
            camera_reframe: 0,
            #[cfg(not(target_arch = "wasm32"))]
            agent_port: None,
            #[cfg(not(target_arch = "wasm32"))]
            music_mcp: None,
            fox_opacity: 0.65,
            gain: 4.,
            zoom: 1.,
            pan: [0.; 2],
            parts_score: None,
            seen_parts: Default::default(),
        }
    }
}
impl Visual {
    pub fn sync_score(&mut self, s: &Snapshot) -> bool {
        let key = (s.session.clone(), s.generation);
        let changed = self.parts_score.as_ref() != Some(&key);
        if changed {
            self.parts_score = Some(key);
            self.seen_parts.clear();
            self.expanded.clear();
            self.lane.clear();
        }
        for lane in &s.lanes {
            if lane.parts.iter().filter(|p| !p.helper).count() > 1
                && self.seen_parts.insert(lane.id.clone())
            {
                self.expanded.push(lane.id.clone());
            }
        }
        changed
    }
}
pub type Settings = Arc<Mutex<Visual>>;
pub struct Frame {
    pub capabilities: Option<rustel_ui_adapter::Capabilities>,
    pub snapshot: Option<Received<Snapshot>>,
    pub audio: Option<Received<AudioFrame>>,
    pub visual: Visual,
}
impl Frame {
    pub fn read(shared: &Shared, settings: &Settings) -> Self {
        let visual = settings.lock().unwrap().clone();
        let state = shared.lock().unwrap();
        let snapshot = state
            .sessions
            .get(&visual.session)
            .filter(|s| s.at.elapsed().as_secs_f64() < 3.)
            .cloned();
        let audio = state
            .audio
            .get(&visual.session)
            .filter(|a| {
                a.at.elapsed().as_secs_f64() < 1.
                    && snapshot
                        .as_ref()
                        .is_some_and(|s| s.value.generation == a.value.generation)
            })
            .cloned();
        Self {
            capabilities: state.capabilities.get(&visual.session).cloned(),
            snapshot,
            audio,
            visual,
        }
    }
    pub fn projected(&self) -> Option<(Snapshot, f64)> {
        let s = self.snapshot.as_ref()?;
        let cycle = s.value.display_cycle(s.at.elapsed().as_secs_f64());
        let mut snapshot = std::borrow::Cow::Borrowed(s.value.as_ref());
        if let Some(slot) = self.visual.foxy_slot {
            let snapshot = snapshot.to_mut();
            snapshot
                .events
                .retain(|e| e.ui_visuals & (1_u64 << slot) != 0);
            let lanes: std::collections::HashSet<_> =
                snapshot.events.iter().map(|e| e.lane.clone()).collect();
            snapshot.lanes.retain(|l| lanes.contains(&l.id));
        }
        let expanded = if self.visual.foxy_active {
            snapshot
                .lanes
                .iter()
                .filter(|l| l.parts.iter().filter(|p| !p.helper).count() > 1)
                .map(|l| l.id.clone())
                .collect()
        } else {
            self.visual.expanded.clone()
        };
        Some((
            snapshot.with_rests(self.visual.rests).visual_lanes(
                &expanded,
                if self.visual.foxy_active {
                    ""
                } else {
                    &self.visual.lane
                },
            ),
            cycle,
        ))
    }
}
pub fn event_color(event: &music_link::Event, lane: usize) -> u32 {
    if !event.part.is_empty() {
        let [r, g, b] = event.part_color;
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    } else {
        COLORS[lane % COLORS.len()]
    }
}
pub fn spiral(age: f64, pitch: f64) -> (f32, f32) {
    let radius = 0.12 + (age + 1.5) * 0.39 + (pitch - 36.) / 48. * 0.08;
    let angle = age * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    ((angle.cos() * radius) as f32, (angle.sin() * radius) as f32)
}
pub fn wheel(pitch: f64) -> (f32, f32) {
    let angle = pitch.rem_euclid(12.) / 12. * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
    let radius = 0.3 + 0.65 * (pitch - 36.) / 48.;
    ((angle.cos() * radius) as f32, (angle.sin() * radius) as f32)
}

pub fn active_ranges(s: &Snapshot, cycle: f64, lane: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<_> = s
        .events
        .iter()
        .filter(|e| {
            (lane.is_empty() || e.lane == lane) && e.cycle <= cycle && e.cycle + e.duration > cycle
        })
        .flat_map(|e| e.context.iter())
        .filter(|(a, b)| {
            a < b
                && *b <= s.source.len()
                && s.source.is_char_boundary(*a)
                && s.source.is_char_boundary(*b)
        })
        .map(|(a, b)| *a..*b)
        .collect();
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<std::ops::Range<usize>> = vec![];
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_foxy_filters_tags_without_changing_the_running_score() {
        let mut tagged = test_event();
        tagged.lane = "#0".into();
        tagged.ui_visuals = 1_u64 << 63;
        let mut sibling = test_event();
        sibling.lane = "#1".into();
        let source = Arc::new(Snapshot {
            lanes: vec![
                music_link::Lane {
                    id: "#0".into(),
                    ..Default::default()
                },
                music_link::Lane {
                    id: "#1".into(),
                    ..Default::default()
                },
            ],
            events: vec![tagged.clone(), sibling],
            ..Default::default()
        });
        let mut frame = Frame {
            capabilities: None,
            audio: None,
            snapshot: Some(Received {
                value: source.clone(),
                at: web_time::Instant::now(),
            }),
            visual: Visual {
                foxy_slot: Some(63),
                foxy_active: true,
                lane: "#1".into(),
                ..Default::default()
            },
        };
        let (filtered, _) = frame.projected().unwrap();
        assert_eq!(filtered.events.len(), 1);
        assert_eq!(filtered.lanes[0].id, "#0");
        assert_eq!(source.events.len(), 2);
        frame.visual.foxy_slot = None;
        assert_eq!(frame.projected().unwrap().0.events.len(), 2);
        let mut browser = serde_json::to_value(&tagged).unwrap();
        browser["ui_visuals"] = serde_json::json!((1_u64 << 63).to_string());
        assert_eq!(
            serde_json::from_value::<music_link::Event>(browser)
                .unwrap()
                .ui_visuals,
            1_u64 << 63
        );
    }
    #[test]
    fn new_scores_expand_named_parts_but_keep_manual_collapses() {
        let mut s = Snapshot {
            session: "test".into(),
            generation: 1,
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                parts: vec![
                    music_link::NamedPart {
                        name: "bass".into(),
                        helper: false,
                        ..Default::default()
                    },
                    music_link::NamedPart {
                        name: "lead".into(),
                        helper: false,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut v = Visual::default();
        assert!(v.sync_score(&s));
        assert_eq!(v.expanded, vec!["#0"]);
        v.expanded.clear();
        assert!(!v.sync_score(&s));
        assert!(v.expanded.is_empty());
        s.generation += 1;
        assert!(v.sync_score(&s));
        assert_eq!(v.expanded, vec!["#0"]);
    }
    #[test]
    fn single_instrument_lanes_do_not_spawn_redundant_character_lanes() {
        let mut visual = Visual::default();
        let snapshot = Snapshot {
            session: "band".into(),
            generation: 1,
            lanes: (0..8)
                .map(|index| music_link::Lane {
                    id: format!("#{index}"),
                    parts: vec![music_link::NamedPart {
                        name: format!("part{index}"),
                        helper: false,
                        ..Default::default()
                    }],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        visual.sync_score(&snapshot);
        assert!(visual.expanded.is_empty());
        assert_eq!(snapshot.visual_lanes(&visual.expanded, "").lanes.len(), 8);
    }
    #[test]
    fn source_highlights_preserve_utf8_and_merge_overlapping_active_contexts() {
        let mut s = Snapshot {
            source: "écho melody".into(),
            ..Default::default()
        };
        let event = music_link::Event {
            id: 1,
            lane: "#0".into(),
            cycle: 1.,
            duration: 1.,
            context: vec![(0, 2), (1, 3), (0, 5), (200, 300)],
            pitch: Some(60.),
            original_pitch: Some(60.),
            sound: "sine".into(),
            gain: 1.,
            part: String::new(),
            part_color: [0; 3],
            block_label: String::new(),
            rest: false,
            ui_visuals: 0,
            controls: Default::default(),
        };
        s.events.push(event);
        assert_eq!(active_ranges(&s, 1.5, "#0"), vec![0..5]);
        assert!(active_ranges(&s, 3., "#0").is_empty());
        assert!(active_ranges(&s, 1.5, "#1").is_empty());
    }
    #[test]
    fn radial_views_keep_the_original_music_geometry() {
        let a = wheel(60.);
        let b = wheel(72.);
        assert!(a.0.abs() < 0.0001 && b.0.abs() < 0.0001);
        assert!(b.1 < a.1);
        let a = spiral(0., 36.);
        let b = spiral(0.25, 36.);
        assert!(a.0.abs() < 0.0001 && a.1 < 0.);
        assert!(b.0 > 0. && b.1.abs() < 0.0001);
    }
}

pub fn rgb_color([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}
pub fn rgb_channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 255) as f32 / 255.,
        ((color >> 8) & 255) as f32 / 255.,
        (color & 255) as f32 / 255.,
    ]
}
pub fn source_highlights(
    s: &Snapshot,
    cycle: f64,
    lane: &str,
) -> Vec<(std::ops::Range<usize>, gpui_kit::HighlightStyle)> {
    let active = active_ranges(s, cycle, lane);
    let mut edges = vec![0, s.source.len()];
    for b in &s.bindings {
        if b.from < b.to
            && b.to <= s.source.len()
            && s.source.is_char_boundary(b.from)
            && s.source.is_char_boundary(b.to)
        {
            edges.extend([b.from, b.to]);
        }
    }
    for r in &active {
        edges.extend([r.start, r.end]);
    }
    edges.sort_unstable();
    edges.dedup();
    edges
        .windows(2)
        .filter_map(|w| {
            let color = s
                .bindings
                .iter()
                .find(|b| b.from <= w[0] && b.to >= w[1])
                .map(|b| gpui_kit::rgb(rgb_color(b.color)).into());
            let background_color = active
                .iter()
                .any(|r| r.start <= w[0] && r.end >= w[1])
                .then(|| gpui_kit::rgb(0x23574f).into());
            (color.is_some() || background_color.is_some()).then_some((
                w[0]..w[1],
                gpui_kit::HighlightStyle {
                    color,
                    background_color,
                    ..Default::default()
                },
            ))
        })
        .collect()
}

#[cfg(test)]
pub fn test_event() -> music_link::Event {
    music_link::Event {
        id: 0,
        lane: String::new(),
        cycle: 0.,
        duration: 0.,
        original_pitch: None,
        pitch: None,
        sound: String::new(),
        gain: 1.,
        block_label: String::new(),
        rest: false,
        ui_visuals: 0,
        part: String::new(),
        part_color: [0; 3],
        context: vec![],
        controls: Default::default(),
    }
}
