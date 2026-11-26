use music_link::{AudioFrame, Command, Event, Lane, Response, Snapshot, TransportCommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub backend: String,
    pub pause: bool,
    pub seek: bool,
    pub stereo: bool,
    pub named_parts: bool,
    pub control_maps: bool,
    pub demo: bool,
    pub restart: bool,
}
impl Default for Capabilities {
    fn default() -> Self {
        Self {
            backend: "upstream-stdio".into(),
            pause: false,
            seek: false,
            stereo: false,
            named_parts: false,
            control_maps: false,
            demo: true,
            restart: true,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Output {
    Snapshot(Box<Snapshot>),
    Audio(AudioFrame),
    Response(Response),
    Status(String),
}
pub struct Adapter {
    pub snapshot: Snapshot,
    pub revision: String,
    native_generation: u64,
    epoch: u64,
    pub dropped: u64,
    pending: BTreeMap<String, (Command, std::time::Instant)>,
    source: String,
}
fn number(v: &Value, k: &str) -> Result<f64, String> {
    v[k].as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("missing/invalid {k}"))
}
fn integer(v: &Value, k: &str) -> Result<u64, String> {
    v[k].as_u64().ok_or_else(|| format!("missing/invalid {k}"))
}
fn fraction(v: &Value) -> Result<f64, String> {
    let text = v.as_str().ok_or("missing rational time")?;
    let (n, d) = text.split_once('/').unwrap_or((text, "1"));
    let n = n.parse::<f64>().map_err(|_| "invalid numerator")?;
    let d = d.parse::<f64>().map_err(|_| "invalid denominator")?;
    let value = n / d;
    if !value.is_finite() {
        return Err("nonfinite rational time".into());
    }
    Ok(value)
}
impl Adapter {
    pub fn new(session: String, source: String, epoch: u64) -> Self {
        Self {
            snapshot: Snapshot {
                version: 1,
                session,
                cps: 0.5,
                ..Default::default()
            },
            revision: String::new(),
            native_generation: 0,
            epoch,
            dropped: 0,
            pending: BTreeMap::new(),
            source,
        }
    }
    pub fn set_source(&mut self, source: String) {
        self.source = source;
    }
    pub fn ingest(&mut self, line: &str) -> Result<Vec<Output>, String> {
        if line.len() > 2 * 1024 * 1024 {
            return Err("protocol line exceeds 2 MiB".into());
        }
        let value: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if let Some(layout) = value.get("ui_layout") {
            if integer(layout, "version")? != 2 {
                return Err("unsupported ui_layout version".into());
            }
            let generation = integer(layout, "generation")?;
            if generation > u32::MAX as u64 {
                return Err("native generation exceeds epoch range".into());
            }
            let revision = layout["source_revision"]
                .as_str()
                .ok_or("missing source revision")?;
            if revision.len() != 64 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid source revision".into());
            }
            let lanes = layout["lanes"].as_array().ok_or("missing lane catalogue")?;
            if lanes.len() > 64 {
                return Err("too many lanes".into());
            }
            let same_source = self.revision == revision;
            let transpose: BTreeMap<_, _> = self
                .snapshot
                .lanes
                .iter()
                .map(|l| (l.id.clone(), l.transpose))
                .collect();
            self.native_generation = generation;
            self.revision = revision.to_owned();
            self.snapshot.generation = (self.epoch << 32) | generation;
            self.snapshot.lanes = lanes
                .iter()
                .map(|l| {
                    Ok(Lane {
                        id: format!("#{}", integer(l, "id")?),
                        label: l["label"].as_str().unwrap_or("").to_owned(),
                        line: integer(l, "line")? as usize,
                        ..Default::default()
                    })
                })
                .collect::<Result<_, String>>()?;
            self.snapshot.events.clear();
            if same_source {
                for lane in &mut self.snapshot.lanes {
                    lane.transpose = transpose.get(&lane.id).copied().unwrap_or(0);
                }
            } else {
                self.snapshot.solo_lane = None;
            }
            self.snapshot.transport = Some(music_link::TransportState {
                paused: false,
                view_cycle: None,
            });
            self.snapshot.source =
                if format!("{:x}", Sha256::digest(self.source.as_bytes())) == revision {
                    self.source.clone()
                } else {
                    String::new()
                };
            self.snapshot.file_error = if self.snapshot.source.is_empty() && !self.source.is_empty()
            {
                Some("Source revision mismatch; source highlighting is disabled".into())
            } else {
                None
            };
            self.snapshot.bindings.clear();
            self.snapshot.definitions.clear();
            return Ok(vec![Output::Snapshot(Box::new(self.snapshot.clone()))]);
        }
        if let Some(batch) = value.get("ui_events") {
            if integer(batch, "version")? != 2 {
                return Err("unsupported ui_events version".into());
            }
            if integer(batch, "generation")? != self.native_generation
                || batch["source_revision"].as_str() != Some(self.revision.as_str())
            {
                return Ok(vec![]);
            }
            let cycle = number(batch, "cycle")?;
            let cps = number(batch, "cps")?;
            if cps <= 0. {
                return Err("invalid cps".into());
            }
            let events = batch["events"].as_array().ok_or("missing events")?;
            if events.len() > 256 {
                return Err("too many onsets".into());
            }
            self.snapshot.cycle = cycle;
            self.snapshot.cps = cps;
            self.dropped += batch["dropped"].as_u64().unwrap_or(0);
            for e in events {
                let Some(lane) = e["lane"].as_u64() else {
                    continue;
                };
                let lane = format!("#{lane}");
                if !self.snapshot.lanes.iter().any(|l| l.id == lane) {
                    continue;
                }
                let start = fraction(&e["whole_begin"])?;
                let duration = number(e, "duration_seconds")? * cps;
                if duration < 0. {
                    continue;
                }
                let pitch = e["frequency_hz"]
                    .as_f64()
                    .filter(|f| f.is_finite() && *f > 0.)
                    .map(|f| 69. + 12. * (f / 440.).log2());
                let context = e["context"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .take(32)
                            .filter_map(|r| {
                                Some((r[0].as_u64()? as usize, r[1].as_u64()? as usize))
                            })
                            .filter(|(a, b)| {
                                a < b
                                    && *b <= self.snapshot.source.len()
                                    && self.snapshot.source.is_char_boundary(*a)
                                    && self.snapshot.source.is_char_boundary(*b)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let event = Event {
                    id: integer(e, "onset_id")?,
                    lane,
                    cycle: start,
                    duration,
                    original_pitch: None,
                    pitch,
                    sound: e["sound"].as_str().unwrap_or("").to_owned(),
                    gain: e["gain"].as_f64().filter(|v| v.is_finite()).unwrap_or(1.),
                    context,
                    part: String::new(),
                    part_color: [0; 3],
                    block_label: e["label"]
                        .as_str()
                        .or(e["value"].as_str())
                        .unwrap_or("")
                        .chars()
                        .take(512)
                        .collect(),
                    rest: false,
                    controls: BTreeMap::new(),
                };
                if !self.snapshot.events.iter().any(|old| old.id == event.id) {
                    self.snapshot.events.push(event);
                }
            }
            self.snapshot
                .events
                .retain(|e| e.cycle + e.duration >= cycle - 2.);
            if self.snapshot.events.len() > 2048 {
                self.snapshot
                    .events
                    .drain(..self.snapshot.events.len() - 2048);
            }
            return Ok(vec![Output::Snapshot(Box::new(self.snapshot.clone()))]);
        }
        if let Some(a) = value.get("ui_audio") {
            if integer(a, "version")? != 1 || integer(a, "generation")? != self.native_generation {
                return Ok(vec![]);
            }
            let audio = AudioFrame {
                version: 1,
                session: self.snapshot.session.clone(),
                generation: self.snapshot.generation,
                sequence: integer(a, "sequence")?,
                stream_id: integer(a, "stream_id")?,
                epoch: integer(a, "epoch")?,
                end_frame: integer(a, "end_frame")?,
                sample_rate: integer(a, "sample_rate")? as u32,
                device_time: number(a, "device_time")?,
                scope: serde_json::from_value(a["scope"].clone()).map_err(|e| e.to_string())?,
                spectrum: serde_json::from_value(a["spectrum"].clone())
                    .map_err(|e| e.to_string())?,
                sides: vec![],
            };
            if !audio.valid() {
                return Err("invalid audio frame".into());
            }
            return Ok(vec![Output::Audio(audio)]);
        }
        if let Some(a) = value.get("ui_control_ack") {
            if integer(a, "version")? != 1 {
                return Err("unsupported acknowledgement version".into());
            }
            let request = a["request"].as_str().ok_or("missing request")?.to_owned();
            let Some((command, _)) = self.pending.remove(&request) else {
                return Ok(vec![]);
            };
            let accepted = a["accepted"].as_bool().ok_or("missing accepted")?;
            if accepted {
                if let Some(solo) = command.solo {
                    self.snapshot.solo_lane = solo.then_some(command.lane.clone());
                }
                if command.solo.is_none()
                    && command.transport.is_none()
                    && command.tempo.is_none()
                    && let Some(lane) = self
                        .snapshot
                        .lanes
                        .iter_mut()
                        .find(|l| l.id == command.lane)
                {
                    lane.transpose = command.semitones;
                }
                if let Some(tempo) = command.tempo {
                    self.snapshot.tempo = Some(tempo);
                }
            }
            return Ok(vec![
                Output::Response(Response {
                    request,
                    accepted,
                    message: a["message"].as_str().unwrap_or("").to_owned(),
                }),
                Output::Snapshot(Box::new(self.snapshot.clone())),
            ]);
        }
        Ok(vec![])
    }
    pub fn encode(&mut self, c: Command) -> Result<Value, String> {
        self.pending
            .retain(|_, (_, at)| at.elapsed() < std::time::Duration::from_secs(5));
        c.validate(&self.snapshot).map_err(str::to_owned)?;
        if self.pending.len() >= 64 {
            return Err("too many pending controls".into());
        }
        let mut body = json!({"version":1,"request":c.request});
        if c.source_update.is_some() {
            return Err("Source rewriting is not supported by the upstream adapter".into());
        }
        if c.demo.is_some() {
            return Err("Demo switching is not yet supported by the upstream adapter".into());
        }
        if let Some(t) = &c.transport {
            match t {
                TransportCommand::Play => {
                    body["kind"] = json!("transport");
                    body["action"] = json!("restart");
                }
                TransportCommand::Stop => {
                    body["kind"] = json!("transport");
                    body["action"] = json!("stop");
                }
                _ => return Err("Upstream exposes stop/restart, not pause or seek".into()),
            }
        } else if let Some(t) = &c.tempo {
            body["kind"] = json!("tempo");
            body["cps"] = json!(t.cps());
        } else {
            let lane = if c.solo == Some(false) {
                0
            } else {
                c.lane
                    .strip_prefix('#')
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or("invalid lane id")?
            };
            body["kind"] = json!(if c.solo.is_some() {
                "lane_solo"
            } else {
                "lane_transpose"
            });
            body["lane"] = if c.solo == Some(false) {
                Value::Null
            } else {
                json!(lane)
            };
            if c.solo.is_none() {
                body["semitones"] = json!(c.semitones);
            }
            body["generation"] = json!(self.native_generation);
            body["source_revision"] = json!(self.revision);
        }
        self.pending
            .insert(c.request.clone(), (c, std::time::Instant::now()));
        Ok(json!({"ui_control":body}))
    }
}
