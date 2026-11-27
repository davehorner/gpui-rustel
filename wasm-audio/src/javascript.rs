//! Full Rustel score evaluation and host-installed PCM sample banks.
use rustel_engine::{
    audio::{AudioBackend, DecodedSample, SampleId, ScalarBackend},
    fraction::Fraction,
    jsruntime::{JsRuntime, Slot},
    scheduler::{Scheduler, TickStatus, Transport, VirtualClock},
    transpiler::{self, TranspileOptions},
    voice::{self, SampleLookup, SampleResolution},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use wasm_bindgen::prelude::*;
#[derive(Clone, Deserialize)]
struct Entry {
    id: u32,
    variant: usize,
    variant_count: usize,
    root: f64,
    duration: f64,
    #[serde(default)]
    font: bool,
    #[serde(default)]
    low: f64,
    #[serde(default = "high")]
    high: f64,
    #[serde(default)]
    loop_start: Option<f64>,
    #[serde(default)]
    loop_end: Option<f64>,
}
fn high() -> f64 {
    127.
}
#[derive(Default)]
struct Library {
    banks: BTreeMap<String, Vec<Entry>>,
}
impl SampleLookup for Library {
    fn note_range(&self, name: &str, n: f64) -> Option<(f64, f64)> {
        let entries = self.banks.get(&name.to_lowercase())?;
        let first = entries.first()?;
        if !first.font {
            return None;
        }
        let variant = (n.round() as i64).rem_euclid(first.variant_count as i64) as usize;
        let selected: Vec<_> = entries.iter().filter(|e| e.variant == variant).collect();
        Some((
            selected.iter().map(|e| e.low).reduce(f64::min)?,
            selected.iter().map(|e| e.high).reduce(f64::max)?,
        ))
    }
    fn resolve(&self, name: &str, n: f64, midi: f64) -> SampleResolution {
        let Some(entries) = self.banks.get(&name.to_lowercase()) else {
            return voice::BundledOnly.resolve(name, n, midi);
        };
        let Some(first) = entries.first() else {
            return SampleResolution::Loading;
        };
        let mut selected: Vec<_> = entries.iter().collect();
        if first.font {
            let variant = (n.round() as i64).rem_euclid(first.variant_count as i64) as usize;
            selected.retain(|e| e.variant == variant);
        } else {
            let root = entries
                .iter()
                .min_by(|a, b| (a.root - midi).abs().total_cmp(&(b.root - midi).abs()))
                .map(|e| e.root)
                .unwrap_or(36.);
            selected.retain(|e| e.root == root);
            if let Some(e) = selected.first() {
                let variant = (n.round() as i64).rem_euclid(e.variant_count as i64) as usize;
                selected.retain(|e| e.variant == variant);
            }
        }
        let Some(entry) = selected
            .into_iter()
            .find(|e| !e.font || (midi >= e.low && midi <= e.high))
        else {
            return if first.font {
                SampleResolution::Unknown
            } else {
                SampleResolution::Loading
            };
        };
        SampleResolution::Found {
            id: SampleId(entry.id),
            transpose: midi - entry.root,
            duration_secs: entry.duration,
            loop_secs: entry.loop_start.zip(entry.loop_end),
            envelope_peak: if entry.font { 0.3 } else { 1. },
            soundfont: entry.font,
        }
    }
}
#[derive(Default, Deserialize)]
struct Overrides {
    #[serde(default)]
    cps: Option<f64>,
    #[serde(default)]
    solo: Option<u32>,
    #[serde(default)]
    transpose: BTreeMap<u32, i16>,
}
#[wasm_bindgen]
pub struct JavaScriptEngine {
    capture_visuals: bool,
    visual_mask: u64,
    orbit_gains: [f32; rustel_engine::audio::MAX_ORBITS],
    visual_history: BTreeMap<u8, Vec<f32>>,
    scheduler: Scheduler,
    backend: ScalarBackend,
    library: Library,
    frame: u64,
    sample_rate: u32,
    cps: f64,
    events: Vec<Value>,
    traces: BTreeMap<u64, rustel_engine::scheduler::ScheduleTraceEvent>,
    metadata: Value,
    missing: Vec<Value>,
    sample_bytes: usize,
    next_sample: u32,
    runtime: JsRuntime,
}
#[wasm_bindgen]
impl JavaScriptEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, sample_rate: u32, overrides: &str) -> Result<Self, String> {
        if source.len() > 512 * 1024 || !(8000..=96000).contains(&sample_rate) {
            return Err("Invalid JavaScript source size or sample rate".into());
        }
        let overrides: Overrides = serde_json::from_str(overrides).map_err(|e| e.to_string())?;
        let runtime = JsRuntime::new().map_err(|e| e.to_string())?;
        runtime.set_lane_tagging(true);
        runtime.set_native_tier(false);
        let cancel = AtomicBool::new(false);
        let lanes = transpiler::on_caller_stack(|| transpiler::lane_statements(source))
            .ok_or("Could not inspect score lanes")?;
        if lanes.len() > 64 {
            return Err("Too many score lanes".into());
        }
        let options = TranspileOptions {
            lane_numbers: (0..lanes.len() as u64).collect(),
            default_widget_methods: true,
            emit_mini_locations: true,
            ..Default::default()
        };
        let (_, effects) = transpiler::on_caller_stack(|| {
            runtime.evaluate_score_with_effects_cancellable(
                source,
                &options,
                // Bounded worker execution: mobile WASM needs more than the native 1.5s budget.
                Duration::from_secs(10),
                &cancel,
            )
        })
        .map_err(|e| e.to_string())?;
        if !effects.csound.is_empty() || !effects.plugins.is_empty() {
            return Err("This browser host cannot load native audio plugins or Csound".into());
        }
        let cps = overrides.cps.or(effects.cps).unwrap_or(0.5);
        if !cps.is_finite() || !(0.001..=100.).contains(&cps) {
            return Err("Invalid score tempo".into());
        }
        runtime.set_cps(cps);
        let pattern = runtime
            .active_pattern()
            .ok_or("The JavaScript score produced no pattern")?;
        let mut scheduler = Scheduler::new(Arc::new(Transport::default()), cps, 0.25);
        scheduler.set_trace_enabled(true);
        scheduler.set_pattern(pattern, 0.);
        scheduler.lane_controls_mut().set_solo(overrides.solo);
        for (lane, semitones) in overrides.transpose {
            if !(-48..=48).contains(&semitones) {
                return Err("Invalid transpose".into());
            }
            scheduler.lane_controls_mut().set_transpose(lane, semitones);
        }
        let metadata = json!({"cps":cps,"source":source,"samples":effects.samples,"preload":effects.preload,"lanes":lanes.iter().enumerate().map(|(i,l)|json!({"id":format!("#{i}"),"label":if l.label.is_empty(){format!("Lane {}",i+1)}else{l.label.clone()},"line":l.line,"source":if l.label.is_empty(){l.body.clone()}else{format!("{}: {}",l.label,l.body)},"transpose":scheduler.lane_controls().transpose_of(i as u32)})).collect::<Vec<_>>()});
        let mut backend = ScalarBackend::new();
        backend.init(sample_rate)?;
        Ok(Self {
            scheduler,
            backend,
            capture_visuals: false,
            visual_mask: 0,
            orbit_gains: [1.0; rustel_engine::audio::MAX_ORBITS],
            visual_history: BTreeMap::new(),
            library: Library::default(),
            frame: 0,
            sample_rate,
            cps,
            events: vec![],
            traces: BTreeMap::new(),
            metadata,
            missing: vec![],
            sample_bytes: 0,
            next_sample: 2,
            runtime,
        })
    }
    pub fn metadata_json(&self) -> String {
        self.metadata.to_string()
    }
    /// Query the real graph for assets before committing playback. No voice substitutions.
    pub fn probe_json(&self, begin: f64, end: f64) -> Result<String, String> {
        if !begin.is_finite() || !end.is_finite() || end <= begin || end - begin > 8. {
            return Err("Invalid sample probe range".into());
        }
        self.runtime.set_cycle_time(begin);
        let haps = self
            .runtime
            .query_cancellable(
                Slot::Active,
                0,
                Fraction::from_f64(begin).ok_or("Invalid probe begin")?,
                Fraction::from_f64(end).ok_or("Invalid probe end")?,
                Duration::from_millis(750),
                &AtomicBool::new(false),
            )
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for hap in haps.into_iter().take(8192) {
            let value: Value =
                serde_json::from_str(&hap.value.json_stringify().ok_or("Invalid score value")?)
                    .map_err(|e| e.to_string())?;
            out.push(value);
        }
        serde_json::to_string(&out).map_err(|e| e.to_string())
    }
    pub fn needs_sample(&self, json: &str) -> Result<bool, String> {
        let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        match voice::resolve_voice_with_samples_detailed(
            &value,
            0,
            0.1,
            0.,
            0.,
            self.sample_rate,
            self.cps,
            &self.library,
        ) {
            Ok(_) | Err(voice::VoiceError::NoteOutsideFont(_)) => Ok(false),
            Err(voice::VoiceError::UnknownSound(_) | voice::VoiceError::SampleLoading(_)) => {
                Ok(true)
            }
            Err(error) => Err(error.to_string()),
        }
    }
    pub fn install_pcm(
        &mut self,
        sample_rate: u32,
        channels: u16,
        pcm: Vec<f32>,
    ) -> Result<u32, String> {
        let bytes = pcm.len().checked_mul(4).ok_or("Sample size overflow")?;
        if self.sample_bytes + bytes > 256 * 1024 * 1024
            || bytes > 32 * 1024 * 1024
            || !pcm.iter().all(|v| v.is_finite())
        {
            return Err("Sample PCM exceeds the browser budget or is nonfinite".into());
        }
        let sample = DecodedSample::from_parts(sample_rate, channels, pcm)?;
        let id = self.next_sample;
        self.backend
            .install_sample(SampleId(id), Box::new(sample))
            .map_err(|_| "Sample bank is full")?;
        self.next_sample += 1;
        self.sample_bytes += bytes;
        Ok(id)
    }
    pub fn register_bank(&mut self, name: &str, json: &str) -> Result<(), String> {
        if name.is_empty()
            || name.len() > 128
            || json.len() > 1024 * 1024
            || self.library.banks.len() >= 128
        {
            return Err("Invalid sample bank registration".into());
        }
        let entries: Vec<Entry> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if entries.is_empty()
            || entries.len() > 4096
            || entries.iter().any(|e| {
                e.id < 2
                    || e.id >= self.next_sample
                    || e.variant_count == 0
                    || !e.root.is_finite()
                    || !e.duration.is_finite()
                    || e.duration <= 0.
            })
        {
            return Err("Invalid installed sample entries".into());
        }
        self.library.banks.insert(name.to_lowercase(), entries);
        Ok(())
    }
    pub fn render(&mut self, frames: usize) -> Result<Vec<f32>, String> {
        if !(128..=8192).contains(&frames) {
            return Err("Invalid render block".into());
        }
        let now = self.frame as f64 / self.sample_rate as f64;
        let through = (self.frame + frames as u64) as f64 / self.sample_rate as f64;
        self.runtime.set_cycle_time(now * self.cps);
        self.runtime.set_cps(self.cps);
        let status = self
            .runtime
            .with_active_scope_cancellable(
                Duration::from_millis(200),
                &AtomicBool::new(false),
                || self.scheduler.tick(&VirtualClock::new(now)),
            )
            .map_err(|e| e.to_string())?;
        if matches!(status, TickStatus::Refused | TickStatus::QueueFull) {
            return Err("JavaScript pattern exceeded the browser scheduling budget".into());
        }
        for trace in self.scheduler.take_trace_events() {
            self.traces.insert(trace.onset_id, trace);
        }
        self.events.clear();
        self.missing.clear();
        let mut onsets = vec![];
        for event in self
            .scheduler
            .drain_through(&VirtualClock::new(now), through)
        {
            let trace = self.traces.remove(&event.onset_id);
            let controls: Value = serde_json::from_str(
                &event
                    .value
                    .json_stringify()
                    .ok_or("Invalid event controls")?,
            )
            .map_err(|e| e.to_string())?;
            let mut onset = match voice::resolve_hap_value(
                &event.value,
                event.onset_id,
                event.duration.to_f64() / self.cps,
                event.target_time,
                self.sample_rate,
                self.cps,
                &self.library,
            ) {
                Ok(onset) => onset,
                Err(voice::VoiceError::SampleLoading(_) | voice::VoiceError::UnknownSound(_)) => {
                    self.missing.push(controls);
                    continue;
                }
                Err(voice::VoiceError::NoteOutsideFont(_)) => continue,
                Err(error) => return Err(error.to_string()),
            };
            onset.ui_visuals = trace.as_ref().map(|t| t.ui_visuals).unwrap_or(0);
            self.visual_mask |= onset.ui_visuals;
            let sound = controls["s"]
                .as_str()
                .or(controls["sound"].as_str())
                .unwrap_or("sine");
            let lane = trace.as_ref().and_then(|t| t.lane).unwrap_or(0);
            let pitch = trace
                .as_ref()
                .and_then(|t| t.frequency_hz)
                .or((onset.sample.is_none()).then_some(onset.freq_hz))
                .filter(|v| *v > 0.)
                .map(|f| 69. + 12. * (f as f64 / 440.).log2());
            self.events.push(json!({"id":event.onset_id,"ui_visuals":onset.ui_visuals.to_string(),"lane":format!("#{lane}"),"cycle":event.whole_begin.to_f64(),"duration":event.duration.to_f64(),"pitch":pitch,"original_pitch":null,"sound":sound,"gain":onset.gain,"context":trace.as_ref().map(|t|t.context.clone()).unwrap_or_default(),"block_label":trace.as_ref().and_then(|t|t.label.clone()).unwrap_or_default()}));
            onsets.push(onset);
            if onsets.len() > 4096 {
                return Err("Too many browser onsets".into());
            }
        }
        onsets.sort_by_key(|e| e.onset_frame);
        for onset in onsets {
            self.backend.prepare_note(&onset)?;
            self.backend.note(onset);
        }
        if self.capture_visuals {
            self.backend.set_ui_visual_capture_mask(self.visual_mask);
        }
        let mut pcm = vec![0.; frames * 2];
        for start in (0..frames).step_by(128) {
            let end = (start + 128).min(frames);
            self.backend
                .process_block(&mut pcm[start * 2..end * 2], end - start);
            if self.capture_visuals {
                for slot in 0..64 {
                    if self.visual_mask & (1_u64 << slot) == 0 {
                        continue;
                    }
                    let samples = self.backend.ui_visual_mix(slot, end - start);
                    if samples.is_empty() {
                        continue;
                    }
                    let history = self.visual_history.entry(slot as u8).or_default();
                    history.extend(samples.chunks_exact(2).map(|s| (s[0] + s[1]) * 0.5));
                    if history.len() > 4096 {
                        history.drain(..history.len() - 4096);
                    }
                }
            }
        }
        if !pcm.iter().all(|s| s.is_finite()) {
            return Err("Nonfinite browser DSP output".into());
        }
        self.frame += frames as u64;
        Ok(pcm)
    }
    pub fn set_slider(&mut self, id: &str, value: f64) -> Result<bool, String> {
        self.runtime.set_slider_value(id, value)
    }
    pub fn set_orbit_gain(&mut self, orbit: usize, gain: f32) -> Result<(), String> {
        if orbit >= self.orbit_gains.len() || !gain.is_finite() || !(0.0..=4.0).contains(&gain) {
            return Err("Invalid orbit gain".into());
        }
        self.orbit_gains[orbit] = gain;
        self.backend.set_orbit_gains(&self.orbit_gains);
        Ok(())
    }
    pub fn set_visual_capture(&mut self, enabled: bool) {
        self.capture_visuals = enabled;
    }
    pub fn visual_audio_json(&self) -> String {
        serde_json::to_string(
            &self
                .visual_history
                .iter()
                .map(|(slot, samples)| json!({"slot":slot,"samples":samples}))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into())
    }
    pub fn events_json(&self) -> String {
        serde_json::to_string(&self.events).expect("finite onsets")
    }
    pub fn missing_json(&self) -> String {
        serde_json::to_string(&self.missing).expect("event JSON")
    }
    pub fn frame(&self) -> f64 {
        self.frame as f64
    }
    pub fn cps(&self) -> f64 {
        self.cps
    }
}
