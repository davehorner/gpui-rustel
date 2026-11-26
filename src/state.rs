use music_link::{AudioFrame, Snapshot};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use web_time::Instant;
#[derive(Clone)]
pub struct Received<T> {
    pub value: Arc<T>,
    pub at: Instant,
}
#[derive(Default)]
pub struct State {
    pub sessions: BTreeMap<String, Received<Snapshot>>,
    pub capabilities: BTreeMap<String, rustel_ui_adapter::Capabilities>,
    pub audio: BTreeMap<String, Received<AudioFrame>>,
    pub status: String,
    pub feedback: String,
    pub pending: BTreeMap<String, Instant>,
}
impl State {
    pub fn prune(&mut self) {
        self.sessions
            .retain(|_, v| v.at.elapsed() < Duration::from_secs(3));
        self.audio.retain(|session, v| {
            v.at.elapsed() < Duration::from_secs(1)
                && self
                    .sessions
                    .get(session)
                    .is_some_and(|s| s.value.generation == v.value.generation)
        });
        self.capabilities
            .retain(|id, _| self.sessions.contains_key(id));
        let before = self.pending.len();
        self.pending
            .retain(|_, deadline| *deadline > Instant::now());
        if before != self.pending.len() {
            self.feedback = "Command acknowledgement timed out".into();
        }
    }
    pub fn receive_snapshot(&mut self, value: Snapshot) {
        if !valid_snapshot(&value)
            || (!self.sessions.contains_key(&value.session) && self.sessions.len() >= 32)
        {
            return;
        }
        self.sessions.insert(
            value.session.clone(),
            Received {
                value: Arc::new(value),
                at: Instant::now(),
            },
        );
    }
    pub fn receive_audio(&mut self, value: AudioFrame) {
        if !value.valid()
            || self
                .sessions
                .get(&value.session)
                .is_none_or(|s| s.value.generation != value.generation)
        {
            return;
        }
        if self.audio.get(&value.session).is_some_and(|old| {
            old.value.generation == value.generation
                && old.value.stream_id == value.stream_id
                && old.value.epoch == value.epoch
                && old.value.sequence >= value.sequence
        }) {
            return;
        }
        self.audio.insert(
            value.session.clone(),
            Received {
                value: Arc::new(value),
                at: Instant::now(),
            },
        );
    }
}
pub fn valid_snapshot(s: &Snapshot) -> bool {
    s.version == 1
        && s.session.len() <= 128
        && !s.session.is_empty()
        && s.source.len() <= 512 * 1024
        && s.events.len() <= 2048
        && s.lanes.len() <= 64
        && s.cycle.is_finite()
        && s.cps.is_finite()
        && s.cps > 0.
        && s.cps <= 100.
        && s.events.iter().all(|e| {
            e.cycle.is_finite()
                && e.duration.is_finite()
                && e.duration >= 0.
                && e.pitch.is_none_or(f64::is_finite)
                && e.gain.is_finite()
        })
}
pub type Shared = Arc<Mutex<State>>;
