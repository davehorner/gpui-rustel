mod javascript;
pub use javascript::JavaScriptEngine;
// Rustel engines hosted in a browser worker, with no CLI or device backend.
use rustel_engine::{
    audio::{AudioBackend, ScalarBackend},
    core::controls::ControlSpec,
    mini,
    scheduler::{Scheduler, TickStatus, Transport, VirtualClock},
    voice,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Lane {
    label: String,
    notes: String,
    sound: String,
    gain: f64,
    pan: f64,
    #[serde(default)]
    transpose: i32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Score {
    bpm: f64,
    top: u32,
    bottom: u32,
    lanes: Vec<Lane>,
    #[serde(default)]
    solo: Option<usize>,
}
struct Track {
    lane: Lane,
    scheduler: Scheduler,
}
#[wasm_bindgen]
pub struct BrowserEngine {
    tracks: Vec<Track>,
    backend: ScalarBackend,
    frame: u64,
    sample_rate: u32,
    cps: f64,
    solo: Option<usize>,
    events: Vec<Value>,
    id: u64,
}
#[wasm_bindgen]
impl BrowserEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, sample_rate: u32) -> Result<BrowserEngine, String> {
        if source.len() > 64 * 1024 || !(8000..=96000).contains(&sample_rate) {
            return Err("Invalid score size or sample rate".into());
        }
        let score: Score = serde_json::from_str(source).map_err(|e| e.to_string())?;
        if !score.bpm.is_finite()
            || !(20.0..=300.0).contains(&score.bpm)
            || !(1..=64).contains(&score.top)
            || !score.bottom.is_power_of_two()
            || score.bottom > 64
            || score.lanes.is_empty()
            || score.lanes.len() > 16
            || score.solo.is_some_and(|s| s >= score.lanes.len())
        {
            return Err("Invalid tempo, meter or lane count".into());
        }
        let cps = score.bpm * score.bottom as f64 / (240. * score.top as f64);
        let mut tracks = vec![];
        for lane in score.lanes {
            if lane.notes.len() > 4096
                || lane.label.len() > 128
                || !lane.gain.is_finite()
                || !(0.0..=0.3).contains(&lane.gain)
                || !lane.pan.is_finite()
                || !(-1.0..=1.0).contains(&lane.pan)
                || !(-48..=48).contains(&lane.transpose)
                || !matches!(
                    lane.sound.as_str(),
                    "sine" | "triangle" | "sawtooth" | "square"
                )
            {
                return Err("Browser mode requires bounded mini-notation synth lanes".into());
            }
            let pattern = ControlSpec::new(["note"])
                .pattern(&mini::mini(&lane.notes).map_err(|e| e.to_string())?);
            let mut scheduler = Scheduler::new(Arc::new(Transport::default()), cps, 0.25);
            scheduler.set_pattern(pattern, 0.);
            tracks.push(Track { lane, scheduler });
        }
        let mut backend = ScalarBackend::new();
        backend.init(sample_rate)?;
        Ok(Self {
            tracks,
            backend,
            frame: 0,
            sample_rate,
            cps,
            solo: score.solo,
            events: vec![],
            id: 0,
        })
    }
    pub fn render(&mut self, frames: usize) -> Result<Vec<f32>, String> {
        if !(128..=8192).contains(&frames) {
            return Err("Render block must contain 128..8192 frames".into());
        }
        let now = self.frame as f64 / self.sample_rate as f64;
        let through = (self.frame + frames as u64) as f64 / self.sample_rate as f64;
        let clock = VirtualClock::new(now);
        self.events.clear();
        let mut onsets = vec![];
        for (index, track) in self.tracks.iter_mut().enumerate() {
            if matches!(
                track.scheduler.tick(&clock),
                TickStatus::Refused | TickStatus::QueueFull
            ) {
                return Err("Mini pattern exceeds the browser scheduling budget".into());
            }
            for event in track.scheduler.drain_through(&clock, through) {
                if self.solo.is_some_and(|solo| solo != index) {
                    continue;
                }
                let mut value: Value = serde_json::from_str(
                    &event
                        .value
                        .json_stringify()
                        .ok_or("Invalid note controls")?,
                )
                .map_err(|e| e.to_string())?;
                value["s"] = json!(track.lane.sound);
                value["gain"] = json!(track.lane.gain);
                value["pan"] = json!(track.lane.pan);
                let mut onset = voice::resolve_voice_with_samples(
                    &value,
                    self.id,
                    event.duration.to_f64() / self.cps,
                    event.target_time,
                    self.sample_rate,
                    self.cps,
                    &voice::BundledOnly,
                )?;
                onset.freq_hz *= 2_f32.powf(track.lane.transpose as f32 / 12.);
                self.events.push(json!({"id":self.id,"lane":format!("#{index}"),"cycle":event.whole_begin.to_f64(),"duration":event.duration.to_f64(),"pitch":69.+12.*(onset.freq_hz/440.).log2(),"original_pitch":null,"sound":track.lane.sound,"gain":onset.gain,"block_label":value["note"].as_str().unwrap_or("")}));
                self.id += 1;
                if onsets.len() >= 4096 {
                    return Err("Too many onsets in one browser render chunk".into());
                }
                onsets.push(onset);
            }
        }
        onsets.sort_by_key(|e| e.onset_frame);
        for onset in onsets {
            self.backend.prepare_note(&onset)?;
            self.backend.note(onset);
        }
        let mut pcm = vec![0.; frames * 2];
        for start in (0..frames).step_by(128) {
            let end = (start + 128).min(frames);
            self.backend
                .process_block(&mut pcm[start * 2..end * 2], end - start);
        }
        if !pcm.iter().all(|v| v.is_finite()) {
            return Err("DSP returned nonfinite PCM".into());
        }
        self.frame += frames as u64;
        Ok(pcm)
    }
    pub fn events_json(&self) -> String {
        serde_json::to_string(&self.events).expect("finite event metadata")
    }
    pub fn frame(&self) -> f64 {
        self.frame as f64
    }
    pub fn cps(&self) -> f64 {
        self.cps
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn score(transpose: i32) -> String {
        json!({"bpm":120,"top":4,"bottom":4,"lanes":[{"label":"bass","notes":"c4 e4 g4 c5","sound":"sine","gain":0.1,"pan":-0.3,"transpose":transpose}]}).to_string()
    }
    #[test]
    fn streaming_engine_renders_finite_stereo_and_exact_voice_metadata() {
        let mut engine = BrowserEngine::new(&score(12), 48000).unwrap();
        let mut peak = 0_f32;
        let mut events = vec![];
        for _ in 0..24 {
            let pcm = engine.render(4096).unwrap();
            assert_eq!(pcm.len(), 8192);
            assert!(pcm.iter().all(|v| v.is_finite()));
            peak = pcm.iter().fold(peak, |p, v| p.max(v.abs()));
            events.extend(engine.events.clone());
        }
        assert!(peak > 0.001);
        assert!(events.len() >= 4);
        assert!((events[0]["pitch"].as_f64().unwrap() - 72.).abs() < 0.01);
        assert_eq!(engine.frame(), 98304.);
    }
    #[test]
    fn invalid_scores_fail_before_an_audio_engine_is_installed() {
        assert!(BrowserEngine::new("{}", 48000).is_err());
        let mut s: Value = serde_json::from_str(&score(0)).unwrap();
        s["lanes"][0]["sound"] = json!("gm_piano");
        assert!(BrowserEngine::new(&s.to_string(), 48000).is_err());
    }
}

// Resolve QuickJS's sole browser OS-tail clock import through wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn __rquickjs_host_now_us() -> f64 {
    js_sys::Date::now() * 1000.
}
