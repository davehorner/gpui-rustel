//! Music tools share the viewer endpoint and the normal acknowledged command bridge.
use crate::bridge::{Shared, command};
use music_link::{Command, TempoSettings, TransportCommand};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::Duration;

fn schema(name: &str, description: &str, fields: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":fields,"required":required,"additionalProperties":false}})
}
fn tools() -> Vec<Value> {
    let session = json!({"session":{"type":"string"}});
    let mut result = vec![
        schema(
            "get_music_state",
            "Read live sessions, generation, tempo, transport, lanes, demos and backend capabilities.",
            session.clone(),
            &[],
        ),
        schema(
            "get_music_messages",
            "Read the latest bounded runtime messages and control results.",
            json!({}),
            &[],
        ),
        schema(
            "get_music_score",
            "Read the exact evaluated score and its generation.",
            session,
            &[],
        ),
    ];
    for (name, description, extra, required) in [
        (
            "set_music_transport",
            "Play/restart, pause, stop or seek audio. Backend capabilities apply.",
            json!({"action":{"enum":["play","pause","stop","seek"]},"cycle":{"type":"number","minimum":0,"maximum":1000000}}),
            vec!["action"],
        ),
        (
            "set_music_tempo",
            "Set BPM and time signature through Rustel's normal control interface.",
            json!({"bpm":{"type":"number","minimum":1,"maximum":1000},"top":{"type":"integer","minimum":1,"maximum":64},"bottom":{"type":"integer","enum":[1,2,4,8,16,32,64]}}),
            vec!["bpm", "top", "bottom"],
        ),
        (
            "set_music_lane_transpose",
            "Set a real emitting lane's transpose in semitones.",
            json!({"lane":{"type":"string"},"semitones":{"type":"integer","minimum":-48,"maximum":48}}),
            vec!["lane", "semitones"],
        ),
        (
            "set_music_lane_solo",
            "Enable or disable lane audition.",
            json!({"lane":{"type":"string"},"enabled":{"type":"boolean"}}),
            vec!["lane", "enabled"],
        ),
        (
            "execute_music_score",
            "Validate and execute an edited score in an owned runtime; original score files are preserved.",
            json!({"source":{"type":"string","maxLength":524288}}),
            vec!["source"],
        ),
        (
            "select_music_demo",
            "Load an existing demo from the runtime catalogue.",
            json!({"demo":{"type":"string"}}),
            vec!["demo"],
        ),
    ] {
        let mut fields = extra.as_object().unwrap().clone();
        fields.insert("session".into(), json!({"type":"string"}));
        fields.insert("generation".into(), json!({"type":"integer","minimum":0}));
        let mut required = required;
        required.extend(["session", "generation"]);
        result.push(schema(name, description, Value::Object(fields), &required));
    }
    result
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args[key]
        .as_str()
        .ok_or_else(|| format!("{key} must be a string"))
}
fn validate(name: &str, args: &Value) -> Result<(), String> {
    let catalogue = tools();
    let spec = catalogue
        .iter()
        .find(|t| t["name"] == name)
        .ok_or("Unknown music tool")?;
    let object = args.as_object().ok_or("Arguments must be an object")?;
    for key in object.keys() {
        if spec["inputSchema"]["properties"].get(key).is_none() {
            return Err(format!("Unknown argument: {key}"));
        }
    }
    for key in spec["inputSchema"]["required"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        if !object.contains_key(key) || object[key].is_null() {
            return Err(format!("Missing argument: {key}"));
        }
    }
    if object.contains_key("session") {
        text(args, "session")?;
    }
    Ok(())
}
pub fn extension(
    shared: Shared,
    commands: mpsc::SyncSender<Command>,
    hosted: bool,
) -> nightshade_viewer_core::McpExtension {
    let serial = AtomicU64::new(0);
    nightshade_viewer_core::McpExtension {
        tools: tools(),
        call: Arc::new(move |name, args| {
            validate(name, &args)?;
            let mut state = shared.lock().map_err(|_| "Music state unavailable")?;
            if name == "get_music_messages" {
                return Ok(
                    json!({"messages":state.messages,"status":state.status,"feedback":state.feedback}),
                );
            }
            if name == "get_music_state"
                && args.get("session").is_none()
                && state.sessions.len() != 1
            {
                return Ok(
                    json!({"sessions":state.sessions.keys().collect::<Vec<_>>(),"status":state.status,"message":state.feedback}),
                );
            }
            let session = if let Some(id) = args.get("session") {
                id.as_str().ok_or("Invalid session")?.to_owned()
            } else if state.sessions.len() == 1 {
                state.sessions.keys().next().unwrap().clone()
            } else {
                return Err("Supply a session from get_music_state; no unique live session".into());
            };
            let snapshot = state
                .sessions
                .get(&session)
                .ok_or("Unknown music session")?
                .clone();
            let s = &snapshot.value;
            if name == "get_music_score" {
                return Ok(
                    json!({"session":s.session,"generation":s.generation,"source":s.source,"fresh":snapshot.at.elapsed()<Duration::from_secs(3)}),
                );
            }
            if name == "get_music_state" {
                return Ok(
                    json!({"sessions":state.sessions.keys().collect::<Vec<_>>(),"session":s.session,"generation":s.generation,"cycle":s.display_cycle(snapshot.at.elapsed().as_secs_f64()),"tempo":s.tempo,"transport":s.transport,"demo":s.demo,"demos":s.demos,"lanes":s.lanes,"capabilities":state.capabilities.get(&session),"status":state.status,"message":state.feedback,"fresh":snapshot.at.elapsed()<Duration::from_secs(3)}),
                );
            }
            if args["generation"].as_u64() != Some(s.generation)
                || snapshot.at.elapsed() >= Duration::from_secs(3)
            {
                return Err("Stale session/generation; read get_music_state again".into());
            }
            let caps = state.capabilities.get(&session);
            let mut c = command();
            c.session = session;
            c.generation = s.generation;
            c.request = format!(
                "music-mcp-{}-{}",
                std::process::id(),
                serial.fetch_add(1, Ordering::Relaxed)
            );
            match name {
                "set_music_transport" => {
                    let action = text(&args, "action")?;
                    if action != "seek" && args.get("cycle").is_some() {
                        return Err("cycle is only valid for seek".into());
                    }
                    c.transport = Some(match action {
                        "play" => TransportCommand::Play,
                        "stop" => TransportCommand::Stop,
                        "pause" if caps.is_some_and(|c| c.pause) => TransportCommand::Pause,
                        "seek" if caps.is_some_and(|c| c.seek) => TransportCommand::Seek {
                            cycle: args["cycle"]
                                .as_f64()
                                .ok_or("seek requires numeric cycle")?,
                        },
                        "pause" | "seek" => {
                            return Err(
                                "This backend does not support pause/seek; use stop/play".into()
                            );
                        }
                        _ => return Err("Unknown transport action".into()),
                    });
                }
                "set_music_tempo" => {
                    c.tempo = Some(TempoSettings {
                        bpm: args["bpm"].as_f64().ok_or("Invalid BPM")?,
                        top: args["top"]
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or("Invalid numerator")?,
                        bottom: args["bottom"]
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or("Invalid denominator")?,
                    })
                }
                "set_music_lane_transpose" => {
                    c.lane = text(&args, "lane")?.into();
                    c.semitones = args["semitones"]
                        .as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("Invalid semitones")?;
                }
                "set_music_lane_solo" => {
                    c.lane = text(&args, "lane")?.into();
                    c.solo = Some(args["enabled"].as_bool().ok_or("enabled must be boolean")?);
                }
                "execute_music_score" => {
                    if !hosted {
                        return Err("Score execution requires an owned runtime".into());
                    }
                    let source = text(&args, "source")?;
                    if source.len() > 512 * 1024 {
                        return Err("Score exceeds 512 KiB".into());
                    }
                    c.source_update = Some(source.into());
                }
                "select_music_demo" => {
                    let demo = text(&args, "demo")?;
                    if !s.demos.iter().any(|d| d == demo) {
                        return Err("Unknown demo".into());
                    }
                    c.demo = Some(demo.into());
                }
                _ => return Err("Unknown music tool".into()),
            }
            c.validate(s).map_err(str::to_owned)?;
            if state.pending.len() >= 64 {
                return Err("Music command queue is full".into());
            }
            let score_change = c.source_update.is_some() || c.demo.is_some();
            let request = c.request.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            state.pending.insert(
                request.clone(),
                web_time::Instant::now() + Duration::from_secs(10),
            );
            state.control_replies.insert(request.clone(), tx);
            if commands.try_send(c).is_err() {
                state.pending.remove(&request);
                state.control_replies.remove(&request);
                return Err("Music command queue unavailable".into());
            }
            drop(state);
            let response = rx.recv_timeout(Duration::from_secs(10));
            if let Ok(mut state) = shared.lock() {
                state.control_replies.remove(&request);
            }
            match response {
                Ok(r) if r.accepted => Ok(
                    json!({"request":request,"acknowledged":true,"message":r.message,"boundary":if score_change {"runtime_score_snapshot"} else {"runtime_control_acknowledgement"}}),
                ),
                Ok(r) => Err(r.message),
                Err(_) => Err(format!(
                    "No runtime acknowledgement for {request}; outcome unknown. Query music state before retrying."
                )),
            }
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Shared, mpsc::SyncSender<Command>, mpsc::Receiver<Command>) {
        let shared = Arc::new(std::sync::Mutex::new(crate::state::State::default()));
        shared.lock().unwrap().sessions.insert(
            "test".into(),
            crate::state::Received {
                value: Arc::new(music_link::Snapshot {
                    version: 1,
                    session: "test".into(),
                    generation: 2,
                    transport: Some(music_link::TransportState {
                        paused: false,
                        view_cycle: None,
                    }),
                    ..Default::default()
                }),
                at: web_time::Instant::now(),
            },
        );
        let (tx, rx) = mpsc::sync_channel(4);
        (shared, tx, rx)
    }
    #[test]
    fn stale_and_unsupported_controls_never_enqueue() {
        let (shared, tx, rx) = fixture();
        let ext = extension(shared, tx, true);
        assert!(
            (ext.call)(
                "set_music_transport",
                json!({"session":"test","generation":1,"action":"stop"})
            )
            .is_err()
        );
        assert!(
            (ext.call)(
                "set_music_transport",
                json!({"session":"test","generation":2,"action":"pause"})
            )
            .is_err()
        );
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn controls_return_their_own_runtime_acknowledgement() {
        let (shared, tx, rx) = fixture();
        let state = shared.clone();
        let ext = extension(shared, tx, true);
        let worker = std::thread::spawn(move || {
            let c = rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(matches!(c.transport, Some(TransportCommand::Stop)));
            state.lock().unwrap().reply(&music_link::Response {
                request: c.request,
                accepted: true,
                message: "stopped".into(),
            });
        });
        let answer = (ext.call)(
            "set_music_transport",
            json!({"session":"test","generation":2,"action":"stop"}),
        )
        .unwrap();
        assert_eq!(answer["acknowledged"], true);
        assert_eq!(answer["message"], "stopped");
        worker.join().unwrap();
    }
    #[test]
    fn strict_arguments() {
        assert!(
            validate(
                "set_music_tempo",
                &json!({"session":"s","generation":1,"bpm":120,"top":4,"bottom":4,"extra":true})
            )
            .is_err()
        );
        assert!(
            validate(
                "set_music_tempo",
                &json!({"session":"s","generation":null,"bpm":120,"top":4,"bottom":4})
            )
            .is_err()
        );
    }
}
