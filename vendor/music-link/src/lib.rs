//! Shared music messages. Network I/O never runs on a Session or audio thread.
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_TOPIC: &str = "music/snapshot";
pub const COMMAND_TOPIC: &str = "music/command";
pub const RESPONSE_TOPIC: &str = "music/response";
pub const AUDIO_TOPIC: &str = "music/audio";

/// Stable UTF-8 name hash → bright hue. No dependency on declaration/lane order.
/// RGB travels with the event so every client uses exactly the same color.
pub fn named_color(name: &str) -> [u8; 3] {
    let hash = name.bytes().fold(2166136261_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
    let hue = hash % 1536;
    let ramp = (hue % 256) as u8;
    let rgb = match hue / 256 {
        0 => [255, ramp, 0], 1 => [255 - ramp, 255, 0],
        2 => [0, 255, ramp], 3 => [0, 255 - ramp, 255],
        4 => [ramp, 0, 255], _ => [255, 0, 255 - ramp],
    };
    rgb.map(|v| 64 + (u16::from(v) * 180 / 255) as u8)
}

#[cfg(test)]
mod named_part_tests {
    use super::*;

    #[test]
    fn block_glyphs_distinguish_notes_from_unpitched_hits() {
        assert_eq!(note_glyph(Some(60.0)), "C4");
        assert_eq!(note_glyph(Some(61.0)), "C#4");
        assert_eq!(note_glyph(Some(69.0)), "A4");
        assert_eq!(note_glyph(Some(-1.0)), "B-2");
        assert_eq!(note_glyph(Some(60.25)), "C4+25c");
        assert_eq!(note_glyph(None), "x");
        assert_eq!(note_glyph(Some(f64::NAN)), "x");
    }

    #[test]
    fn old_binding_messages_default_to_compatibility_definition_ranges() {
        let binding: Binding = serde_json::from_str(r#"{"name":"bass","color":[1,2,3],"from":6,"to":10}"#).unwrap();
        assert_eq!((binding.definition_from, binding.definition_to), (0,0));
    }

    #[test]
    fn virtual_source_target_keeps_parent_and_unicode_part_identity() {
        let id=virtual_id("#4","mélodie");
        assert_eq!(virtual_parent(&id),Some(("#4","mélodie")));
        assert_eq!(virtual_parent("#4"),None);
        assert_eq!(virtual_parent("~99:short"),None);
        assert_eq!(virtual_parent("~1:é"),None);
    }

    #[test]
    fn names_have_stable_bounded_colors_and_old_events_still_decode() {
        assert_eq!(named_color("bass"), named_color("bass"));
        assert_ne!(named_color("bass"), named_color("rhythm"));
        assert_ne!(named_color("écho"), named_color("echo"));
        for channel in named_color("any_user_name") { assert!((64..=244).contains(&channel)); }
        let event: Event = serde_json::from_str(r##"{
            "id":1,"lane":"#0","cycle":0,"duration":1,"original_pitch":60,
            "pitch":60,"sound":"sine","gain":0.5
        }"##).unwrap();
        assert!(event.part.is_empty());
    }

    #[test]
    fn expanding_one_parent_scopes_shared_parts_and_never_duplicates_events() {
        let part = NamedPart { name: "moon".into(), color: named_color("moon"), helper: false };
        let lane = |id: &str| Lane { id: id.into(), source: "$: arrangement".into(), parts: vec![part.clone()], ..Default::default() };
        let event = |lane: &str, part: &str, id| Event { lane: lane.into(), part: part.into(), id,
            block_label: String::new(), rest: false,
            part_color: named_color(part), cycle: 0.0, duration: 1.0, original_pitch: None,
            pitch: None, sound: "bd".into(), gain: 1.0, context: vec![], controls: Default::default() };
        let source = Snapshot { lanes: vec![lane("#1"), lane("#2")],
            events: vec![event("#1", "moon", 1), event("#2", "moon", 2), event("#1", "", 3)], ..Default::default() };
        let view = source.visual_lanes(&["#1".into()], "");
        assert_eq!(view.lanes.len(), 3);
        assert!(view.lanes[0].label.contains("arrangement"));
        assert_eq!(view.events.len(), source.events.len());
        assert_eq!(view.events[0].lane, view.lanes[1].id);
        assert_eq!(view.events[1].lane, "#2");
        assert_eq!(view.events[2].lane, "#1");
        let both = source.visual_lanes(&["#1".into(), "#2".into()], "");
        assert_ne!(both.events[0].lane, both.events[1].lane);
        let selected = source.visual_lanes(&["#1".into()], "#1");
        assert_eq!(selected.lanes.len(), 2);
        assert_eq!(selected.events.len(), 2);
        assert_eq!(source.lanes.len(), 2, "projection leaves control targets untouched");
    }

    #[test]
    fn core_expression_keeps_sample_colons_and_multiline_expression() {
        let lane = |source: &str| Lane { source: source.into(), ..Default::default() };
        assert_eq!(lane("s(\"bd:1\")").display_name(), "s(\"bd:1\")");
        assert_eq!(lane("$: \n arrangement.slow(2);").display_name(), "arrangement.slow(2)");
        assert_eq!(lane("$: stack(\r\n  bass,\n  lead\n)\n  .slow(2);").display_name(), "stack( bass, lead ) .slow(2)");
        assert_eq!(lane("$: s(\"bd  hh\")\n .gain(0.4)").display_name(), "s(\"bd  hh\") .gain(0.4)");
        let long = format!("$: stack({})", "bass, ".repeat(40));
        assert_eq!(lane(&long).display_name(), long[3..].to_owned());
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioFrame {
    pub version: u32,
    pub session: String,
    pub generation: u64,
    pub sequence: u64,
    pub stream_id: u64,
    pub epoch: u64,
    pub end_frame: u64,
    pub sample_rate: u32,
    pub device_time: f64,
    pub scope: Vec<f32>,
    pub spectrum: Vec<f32>,
    pub sides: Vec<[f32; 2]>,
}

impl AudioFrame {
    pub fn valid(&self) -> bool {
        self.version == 1
            && self.session.len() <= 128
            && self.sample_rate > 0
            && self.device_time.is_finite()
            && self.scope.len() == 512
            && self.spectrum.len() == 512
            && self.sides.len() <= 4096
            && self
                .scope
                .iter()
                .chain(self.spectrum.iter())
                .all(|v| v.is_finite())
            && self.sides.iter().flatten().all(|v| v.is_finite())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lane {
    pub id: String,
    pub label: String,
    /// Complete score statement for this lane, including its label and chain.
    #[serde(default)]
    pub source: String,
    pub line: usize,
    pub transpose: i32,
    #[serde(default)]
    pub parts: Vec<NamedPart>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NamedPart {
    pub name: String,
    pub color: [u8; 3],
    /// A source definition with no independently emitted events.
    #[serde(default)]
    pub helper: bool,
}

impl Lane {
    /// Match the bridge's normalized statement to the original UTF-8 document.
    /// Only whitespace may differ (indentation, label spacing and CRLF).
    pub fn source_range(&self, document: &str) -> Option<(usize, usize)> {
        if self.line == 0 || self.source.trim().is_empty() { return None; }
        let start = document.split_inclusive('\n').take(self.line-1).map(str::len).sum::<usize>();
        let written = document.get(start..)?;
        let mut original = written.char_indices().peekable();
        let mut from = None;
        let mut to = start;
        for expected in self.source.chars().filter(|c| !c.is_whitespace()) {
            while original.peek().is_some_and(|(_, c)| c.is_whitespace()) { original.next(); }
            let (at, actual) = original.next()?;
            if actual != expected { return None; }
            from.get_or_insert(start+at);
            to = start+at+actual.len_utf8();
        }
        Some((from?, to))
    }

    pub fn display_name(&self) -> String {
        let source = self.source.trim();
        let body = source.split_once(':').filter(|(label, _)| !label.is_empty() && label.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
            .map_or(source, |(_, body)| body.trim());
        let expression = body.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" ");
        let expression = expression.trim_end_matches(';');
        if expression.is_empty() { self.label.clone() } else { expression.to_owned() }
    }
}

fn virtual_id(parent: &str, part: &str) -> String {
    format!("~{}:{parent}{part}", parent.len())
}

pub fn virtual_parent(id: &str) -> Option<(&str, &str)> {
    let (length, tail) = id.strip_prefix('~')?.split_once(':')?;
    let length = length.parse::<usize>().ok()?;
    Some((tail.get(..length)?, tail.get(length..)?))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Producer-resolved sound, mini token and controls for block faces.
    #[serde(default)]
    pub block_label: String,
    /// Visual-only silence marker, never sent to an audio renderer.
    #[serde(default)]
    pub rest: bool,
    /// Top-level const pattern binding, retained through composition.
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub part_color: [u8; 3],
    #[serde(default)]
    pub context: Vec<(usize, usize)>,
    #[serde(default)]
    pub controls: std::collections::BTreeMap<String, f64>,
    pub id: u64,
    pub lane: String,
    pub cycle: f64,
    pub duration: f64,
    pub original_pitch: Option<f64>,
    pub pitch: Option<f64>,
    pub sound: String,
    pub gain: f64,
}

impl Event {
    pub fn face_label(&self) -> String {
        if self.rest { return "~".into(); }
        if !self.block_label.is_empty() { return self.block_label.clone(); }
        self.describe("")
    }

    /// Resolve labels from accepted controls and the exact authored mini leaf.
    /// Names are not guessed from const names or a fixed instrument list.
    pub fn describe(&self, source: &str) -> String {
        if self.rest { return "~".into(); }
        let mut sound = self.sound.clone();
        let mut structure = String::new();
        for &(from, to) in &self.context {
            let Some((role, token)) = source_token(source, from, to) else { continue; };
            // Mini context also includes operator arguments (the 8 in hh*8).
            // Only the leaf matching the resolved sound may name that sound.
            if (role == "s" || role == "sound") && !self.sound.is_empty()
                && token.strip_prefix(&self.sound).is_some_and(|suffix| suffix.chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_')) {
                sound = token;
            }
            else if role == "struct" && structure.is_empty() { structure = token; }
        }
        let mut text = if self.pitch.is_some() {
            let note = note_glyph(self.pitch);
            if sound.is_empty() { note } else { format!("{sound} / {note}") }
        } else if sound.is_empty() { "x".into() } else { sound };
        if !structure.is_empty() { text.push_str(&format!(" / struct {structure}")); }
        if let Some(gain) = self.controls.get("gain").filter(|v| v.is_finite()) {
            let gain = format!("{gain:.3}").trim_end_matches('0').trim_end_matches('.').to_owned();
            text.push_str(&format!(".gain({gain})"));
        }
        text
    }
}

// Locate the literal containing a context range, skipping comments and escaped
// quotes. Include its attached mini postfix (hh*8), not an unrelated next leaf.
fn source_token(source: &str, from: usize, to: usize) -> Option<(String, String)> {
    if from >= to || to > source.len() || !source.is_char_boundary(from) || !source.is_char_boundary(to) { return None; }
    let bytes = source.as_bytes();
    let mut at = 0;
    while at < source.len() {
        if source[at..].starts_with("//") { at += source[at..].find('\n').unwrap_or(source.len()-at); continue; }
        if source[at..].starts_with("/*") { at += source[at+2..].find("*/").map_or(source.len()-at, |n| n+4); continue; }
        let quote = bytes[at];
        if !matches!(quote, b'\'' | b'"' | b'`') { at += source[at..].chars().next()?.len_utf8(); continue; }
        let begin = at;
        at += 1;
        while at < source.len() && bytes[at] != quote {
            if bytes[at] == b'\\' { at += 1; }
            if at < source.len() { at += source[at..].chars().next()?.len_utf8(); }
        }
        let end = at;
        if at < source.len() { at += 1; }
        if from <= begin || to > end { continue; }
        if source[begin+1..from].trim_end().chars().last().is_some_and(|c| matches!(c, '*' | '!' | '/' | '@')) { return None; }
        let prefix = source[..begin].trim_end().strip_suffix('(')?.trim_end();
        let role = prefix.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>().chars().rev().collect::<String>();
        let mut finish = to;
        if matches!(bytes.get(finish), Some(b'*' | b'!' | b'/' | b'@')) {
            finish += 1;
            while finish < end && (bytes[finish].is_ascii_digit() || bytes[finish] == b'.') { finish += 1; }
        }
        let token = source[from..finish].replace("\\\r\n", "").replace("\\\n", "").split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((role, token));
    }
    None
}

/// Scientific pitch name for timeline glyphs. Unpitched samples are hits (`x`),
/// never the arbitrary height the renderer uses to position a drum block.
pub fn note_glyph(pitch: Option<f64>) -> String {
    let Some(pitch) = pitch.filter(|p| p.is_finite() && (-256.0..=256.0).contains(p)) else { return "x".into(); };
    let midi = pitch.round() as i32;
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let name = format!("{}{}", names[midi.rem_euclid(12) as usize], midi.div_euclid(12)-1);
    let cents = ((pitch - f64::from(midi)) * 100.0).round() as i32;
    if cents == 0 { name } else { format!("{name}{cents:+}c") }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LaneAudioStream {
    pub mapping: String,
    /// Stable slots are never reassigned during a session, including retired lanes.
    pub lanes: std::collections::BTreeMap<String, u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    pub color: [u8; 3],
    pub from: usize,
    pub to: usize,
    #[serde(default)]
    pub definition_from: usize,
    #[serde(default)]
    pub definition_to: usize,
}

/// One session-wide definition, independent of lane expansion and numbering.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub color: [u8; 3],
    pub value: Option<f64>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// [cycle, value], bounded to 128 observations.
    #[serde(default)]
    pub history: Vec<[f64; 2]>,
    /// Pitch (or sound for unpitched events) counts in the current event window.
    #[serde(default)]
    pub histogram: std::collections::BTreeMap<String, u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub definitions: Vec<Definition>,
    #[serde(default)]
    pub demos: Vec<String>,
    #[serde(default)]
    pub demo: String,
    #[serde(default)]
    pub session_file: String,
    #[serde(default)]
    pub file_error: Option<String>,
    #[serde(default)]
    pub tempo: Option<TempoSettings>,
    #[serde(default)]
    pub transport: Option<TransportState>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_audio: Option<LaneAudioStream>,
    #[serde(default)]
    pub solo_lane: Option<String>,
    /// Exact evaluated document; context ranges are UTF-8 byte offsets here.
    #[serde(default)]
    pub source: String,
    pub version: u32,
    pub session: String,
    pub generation: u64,
    pub cycle: f64,
    pub cps: f64,
    pub lanes: Vec<Lane>,
    pub events: Vec<Event>,
    /// Internal gaps between accepted events, grouped by lane and named part.
    /// These carry no audio or source context and are opt-in in each viewer.
    #[serde(default)]
    pub rests: Vec<Event>,
}

impl Snapshot {
    /// Helpers are shared once; independently emitting parts stay under their parents.
    pub fn shared_definitions(&self) -> Vec<&Definition> {
        self.definitions.iter().filter(|d| d.value.is_some() || !self.lanes.iter().any(|l|l.parts.iter().any(|p|!p.helper && p.name==d.name))).collect()
    }

    pub fn update_definition_graphs(&mut self) {
        let dependencies: std::collections::BTreeMap<_,_> = self.definitions.iter().map(|d|(d.name.clone(),d.dependencies.clone())).collect();
        for definition in &mut self.definitions {
            if definition.value.is_none() && self.lanes.iter().any(|l|l.parts.iter().any(|p|!p.helper && p.name==definition.name)) {continue;}
            if let Some(value) = definition.value {
                if definition.history.last().is_some_and(|p|p[0]>self.cycle) { definition.history.clear(); }
                if definition.history.last().is_none_or(|p|p[1]!=value || self.cycle-p[0]>=0.25) {
                    definition.history.push([self.cycle,value]);
                    if definition.history.len()>128 {definition.history.remove(0);}
                }
            } else {
                let mut users = std::collections::BTreeSet::from([definition.name.clone()]);
                // Transitive reverse dependencies; cycles reach a fixed point.
                for _ in 0..dependencies.len() {
                    let before=users.len();
                    for (name,refs) in &dependencies {if refs.iter().any(|r|users.contains(r)) {users.insert(name.clone());}}
                    if users.len()==before {break;}
                }
                definition.histogram.clear();
                for event in self.events.iter().filter(|e|!e.rest && users.contains(&e.part)) {
                    let label=if event.pitch.is_some(){note_glyph(event.pitch)}else if event.sound.is_empty(){"x".into()}else{event.sound.clone()};
                    if definition.histogram.len()<64 || definition.histogram.contains_key(&label) { *definition.histogram.entry(label).or_default()+=1; }
                }
            }
        }
    }
    pub fn display_cycle(&self, age: f64) -> f64 {
        if let Some(transport)=self.transport.as_ref().filter(|t|t.paused) {
            transport.view_cycle.unwrap_or(self.cycle)
        } else {self.cycle+age.max(0.0).min(3.0)*self.cps}
    }
    pub fn inferred_rests(&self) -> Vec<Event> {
        let mut groups = std::collections::BTreeMap::<(&str, &str), Vec<&Event>>::new();
        for event in &self.events {
            if !event.rest && event.cycle.is_finite() && event.duration.is_finite() && event.duration > 0.0 {
                groups.entry((&event.lane, &event.part)).or_default().push(event);
            }
        }
        let mut rests = Vec::new();
        for events in groups.values_mut() {
            events.sort_by(|a,b| a.cycle.total_cmp(&b.cycle));
            let mut end = events[0].cycle + events[0].duration;
            for next in events.iter().skip(1) {
                if next.cycle - end > 1e-7 && rests.len() < 512 {
                    let mut rest = (*next).clone();
                    rest.id = 0;
                    rest.cycle = end;
                    rest.duration = next.cycle - end;
                    rest.rest = true;
                    rest.block_label = "~".into();
                    rest.pitch = None;
                    rest.original_pitch = None;
                    rest.sound.clear();
                    rest.gain = 0.0;
                    rest.context.clear();
                    rest.controls.clear();
                    rests.push(rest);
                }
                end = end.max(next.cycle + next.duration);
            }
        }
        rests
    }

    /// Add optional visual silence before projecting virtual parts.
    pub fn with_rests(&self, show: bool) -> Self {
        let mut result = self.clone();
        if show { result.events.extend(self.rests.iter().cloned()); }
        result.rests.clear();
        result
    }

    /// A visual-only projection. Commands always use the original snapshot IDs.
    /// Parent rows hold unnamed events when expanded; each named child appears
    /// exactly once. Length-prefixed IDs scope identical names to their parents.
    pub fn visual_lanes(&self, expanded: &[String], selected: &str) -> Self {
        let mut output = self.clone();
        output.lanes.clear();
        let mut destinations = std::collections::BTreeMap::new();
        for lane in self.lanes.iter().filter(|l| selected.is_empty() || l.id == selected).take(64) {
            let mut core = lane.clone();
            core.parts.retain(|p|!p.helper);
            let open = expanded.contains(&lane.id) && !core.parts.is_empty();
            core.label = format!("{} line {} / {}", if core.parts.is_empty() { "·" } else if open { "−" } else { "+" }, lane.line, lane.display_name());
            output.lanes.push(core);
            if open {
                for part in lane.parts.iter().filter(|p|!p.helper).take(64) {
                    if output.lanes.len() >= 256 { break; }
                    let id = virtual_id(&lane.id, &part.name);
                    destinations.insert((lane.id.clone(), part.name.clone()), id.clone());
                    output.lanes.push(Lane { id, label: format!("  ↳ {}{}", part.name, if part.helper {" (const)"} else {""}), source: part.name.clone(), parts: vec![part.clone()], ..Default::default() });
                }
            }
        }
        output.events.retain_mut(|event| {
            if !selected.is_empty() && event.lane != selected { return false; }
            if let Some(id) = destinations.get(&(event.lane.clone(), event.part.clone())) { event.lane = id.clone(); }
            true
        });
        output
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    #[serde(default)]
    pub demo: Option<String>,
    #[serde(default)]
    pub tempo: Option<TempoSettings>,
    /// Filled only by the producer's file worker, never accepted over the wire.
    #[serde(skip)]
    pub source_update: Option<String>,
    #[serde(default)]
    pub transport: Option<TransportCommand>,
    pub request: String,
    pub session: String,
    pub generation: u64,
    pub lane: String,
    #[serde(default)]
    pub semitones: i32,
    /// None is a transpose command; Some enables/disables lane audition.
    #[serde(default)]
    pub solo: Option<bool>,
}

impl Command {
    pub fn validate(&self, snapshot: &Snapshot) -> Result<(), &'static str> {
        if self.session != snapshot.session || self.generation != snapshot.generation {
            return Err("stale session or score generation");
        }
        let source_control=self.demo.is_some() || self.tempo.is_some() || self.source_update.is_some();
        if usize::from(self.demo.is_some())+usize::from(self.tempo.is_some())+usize::from(self.transport.is_some())>1 {return Err("mixed control actions");}
        if source_control && (self.solo.is_some() || self.semitones!=0) {return Err("mixed source and lane command");}
        if let Some(demo)=&self.demo {if demo.is_empty() || demo.len()>512 {return Err("invalid demo name");}}
        if let Some(tempo)=&self.tempo {tempo.validate()?;}
        if let Some(transport) = &self.transport {
            if snapshot.transport.is_none() {return Err("transport unavailable");}
            if self.solo.is_some() || self.semitones != 0 {return Err("mixed transport and lane command");}
            if let TransportCommand::Seek {cycle} = transport {if !cycle.is_finite() || !(0.0..=1_000_000.0).contains(cycle) {return Err("invalid seek cycle");}}
        }
        if !source_control && self.transport.is_none() && self.solo != Some(false) && !snapshot.lanes.iter().any(|lane| lane.id == self.lane) {
            return Err("unknown lane");
        }
        if !(-48..=48).contains(&self.semitones) || self.request.len() > 128 {
            return Err("invalid transpose or request id");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TempoSettings {pub bpm:f64,pub top:u32,pub bottom:u32}
impl TempoSettings {
    pub fn validate(&self)->Result<(), &'static str> {
        if !self.bpm.is_finite() || !(1.0..=1000.0).contains(&self.bpm) || !(1..=64).contains(&self.top)
            || !self.bottom.is_power_of_two() || self.bottom>64 {return Err("invalid tempo or time signature");}
        Ok(())
    }
    pub fn cps(&self)->f64 {self.bpm*f64::from(self.bottom)/(240.0*f64::from(self.top))}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportState {
    pub paused: bool,
    /// Frozen inspection position; Stop can reset playback to zero without
    /// moving the retained blocks out of the viewer's visible time window.
    #[serde(default)]
    pub view_cycle: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag="action", rename_all="snake_case")]
pub enum TransportCommand { Play, Pause, Stop, Seek {cycle:f64} }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub request: String,
    pub accepted: bool,
    pub message: String,
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub mod session_files;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn solo_commands_are_distinct_from_transpose_and_generation_checked() {
        let snapshot = Snapshot {
            session: "s".into(),
            generation: 2,
            lanes: vec![Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut command: Command = serde_json::from_value(
            serde_json::json!({"request":"r","session":"s","generation":2,"lane":"#0","solo":true}),
        )
        .unwrap();
        assert_eq!(command.solo, Some(true));
        assert!(command.validate(&snapshot).is_ok());
        command.generation = 1;
        assert!(command.validate(&snapshot).is_err());
        command.generation = 2;
        command.lane = "#gone".into();
        assert!(command.validate(&snapshot).is_err());
        command.solo = Some(false);
        assert!(command.validate(&snapshot).is_ok());
        let legacy:Command=serde_json::from_value(serde_json::json!({"request":"r","session":"s","generation":2,"lane":"#0","semitones":7})).unwrap();
        assert!(legacy.solo.is_none());
        assert_eq!(legacy.semitones, 7);
    }
    #[test]
    fn audio_frames_are_bounded_and_reject_nonfinite_data() {
        let mut frame = AudioFrame {
            version: 1,
            sample_rate: 48_000,
            session: "test".into(),
            scope: vec![0.0; 512],
            spectrum: vec![-120.0; 512],
            sides: vec![[0.1, -0.1]; 1024],
            ..Default::default()
        };
        assert!(frame.valid());
        frame.scope[0] = f32::NAN;
        assert!(!frame.valid());
        frame.scope[0] = 0.0;
        frame.sides.resize(4097, [0.0, 0.0]);
        assert!(!frame.valid());
    }
    #[test]
    fn commands_cannot_address_a_replaced_score_or_missing_lane() {
        let snapshot = Snapshot {
            session: "a".into(),
            generation: 3,
            lanes: vec![Lane {
                id: "#7".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut command = Command {
            demo: None, tempo: None, source_update: None,
            transport: None,
            request: "r".into(),
            session: "a".into(),
            generation: 3,
            lane: "#7".into(),
            semitones: 7,
            solo: None,
        };
        assert!(command.validate(&snapshot).is_ok());
        command.generation = 2;
        assert!(command.validate(&snapshot).is_err());
        command.generation = 3;
        command.lane = "#8".into();
        assert!(command.validate(&snapshot).is_err());
    }
}
