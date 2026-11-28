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
    #[cfg(not(target_arch = "wasm32"))]
    pub studio_requests: std::collections::VecDeque<crate::studio_mcp::Request>,
    #[cfg(not(target_arch = "wasm32"))]
    pub studio_validator: Option<rustel_ui_adapter::ScoreValidator>,
    pub sessions: BTreeMap<String, Received<Snapshot>>,
    pub capabilities: BTreeMap<String, rustel_ui_adapter::Capabilities>,
    pub audio: BTreeMap<String, Received<AudioFrame>>,
    pub status: String,
    pub feedback: String,
    pub pending: BTreeMap<String, Instant>,
    pub messages: Vec<String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub control_replies: BTreeMap<String, std::sync::mpsc::SyncSender<music_link::Response>>,
}
impl State {
    pub fn log(&mut self, message: impl Into<String>) {
        let message = message.into();
        let message = message.chars().take(4096).collect::<String>();
        if self.messages.last() == Some(&message) {
            return;
        }
        self.messages.push(message);
        if self.messages.len() > 200 {
            self.messages.remove(0);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn reply(&mut self, response: &music_link::Response) {
        self.log(format!(
            "{}: {} {}",
            response.request,
            if response.accepted {
                "accepted"
            } else {
                "rejected"
            },
            response.message
        ));
        if let Some(sender) = self.control_replies.remove(&response.request) {
            let _ = sender.try_send(response.clone());
        }
    }
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
        #[cfg(not(target_arch = "wasm32"))]
        self.control_replies
            .retain(|request, _| self.pending.contains_key(request));
        if before != self.pending.len() {
            self.feedback = "Command acknowledgement timed out".into();
        }
    }
    pub fn receive_snapshot(&mut self, mut value: Snapshot) {
        if !valid_snapshot(&value)
            || (!self.sessions.contains_key(&value.session) && self.sessions.len() >= 32)
        {
            return;
        }
        if value.bindings.is_empty()
            && let Some(previous) = self.sessions.get(&value.session)
            && previous.value.generation == value.generation
            && previous.value.source == value.source
        {
            value.bindings = previous.value.bindings.clone();
            value.definitions = previous.value.definitions.clone();
            for lane in &mut value.lanes {
                if let Some(old) = previous.value.lanes.iter().find(|l| l.id == lane.id) {
                    lane.parts = old.parts.clone();
                }
            }
        }
        crate::provenance::enrich(&mut value);
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
