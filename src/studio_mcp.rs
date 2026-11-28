//! Bounded requests serviced by GPUI's existing UI turn. MCP never touches an Entity.
use crate::bridge::Shared;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub struct Request {
    pub name: String,
    pub args: Value,
    pub reply: Reply,
}
pub struct Reply {
    tx: mpsc::SyncSender<Result<Value, String>>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Reply {
    pub fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && Instant::now() < self.deadline
    }
    pub fn send(self, result: Result<Value, String>) {
        let _ = self.tx.try_send(result);
    }
}
pub fn tools() -> Vec<Value> {
    let guard =
        json!({"draft_id":{"type":"string"},"expected_revision":{"type":"integer","minimum":0}});
    let mut result = Vec::new();
    for (name, description, mut fields, required) in [
        (
            "get_studio_draft",
            "Read the actual native Studio draft, revision, dirty state and selected scene. Initializes the editor if necessary; does not open it or play audio.",
            json!({}),
            vec![],
        ),
        (
            "set_studio_draft",
            "Replace the revision-checked Studio draft as one undoable edit. Does not evaluate or change playback.",
            guard.clone(),
            vec!["draft_id", "expected_revision", "source"],
        ),
        (
            "validate_studio_draft",
            "Validate the captured revision using the current owned Rustel validator, without installing a score or starting playback. Results apply only to the returned revision.",
            guard.clone(),
            vec!["draft_id", "expected_revision"],
        ),
        (
            "execute_studio_draft",
            "Execute the exact revision-checked draft through the existing owned runtime. Requires the current music session/generation and waits for a correlated runtime score acknowledgement.",
            guard,
            vec!["draft_id", "expected_revision", "session", "generation"],
        ),
    ] {
        if name == "set_studio_draft" {
            fields["source"] = json!({"type":"string","maxLength":524288});
        }
        if name == "execute_studio_draft" {
            fields["session"] = json!({"type":"string"});
            fields["generation"] = json!({"type":"integer","minimum":0});
        }
        result.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":fields,"required":required,"additionalProperties":false}}));
    }
    result
}
fn check(name: &str, args: &Value) -> Result<(), String> {
    let all = tools();
    let spec = all
        .iter()
        .find(|s| s["name"] == name)
        .ok_or("Unknown Studio tool")?;
    let object = args.as_object().ok_or("Arguments must be an object")?;
    for key in object.keys() {
        if spec["inputSchema"]["properties"].get(key).is_none() {
            return Err(format!("Unknown argument: {key}"));
        }
    }
    for key in spec["inputSchema"]["required"].as_array().unwrap() {
        let k = key.as_str().unwrap();
        if object.get(k).is_none_or(Value::is_null) {
            return Err(format!("Missing argument: {k}"));
        }
    }
    for field in ["draft_id", "session", "source"] {
        if let Some(v) = object.get(field) {
            let text = v
                .as_str()
                .ok_or_else(|| format!("{field} must be a string"))?;
            if field == "source" && text.len() > 512 * 1024 {
                return Err("Draft exceeds 512 KiB UTF-8".into());
            }
        }
    }
    for field in ["expected_revision", "generation"] {
        if let Some(v) = object.get(field) {
            v.as_u64()
                .ok_or_else(|| format!("{field} must be a nonnegative integer"))?;
        }
    }
    Ok(())
}
pub fn call(shared: &Shared, name: &str, args: Value) -> Result<Value, String> {
    check(name, &args)?;
    let (tx, rx) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut state = shared.lock().map_err(|_| "Studio state unavailable")?;
        if state.studio_requests.len() >= 16 {
            return Err("Studio request queue is full".into());
        }
        state.studio_requests.push_back(Request {
            name: name.into(),
            args,
            reply: Reply {
                tx,
                cancelled: cancelled.clone(),
                deadline: Instant::now() + Duration::from_secs(15),
            },
        });
    }
    let captured = match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(value) => value?,
        Err(_) => {
            cancelled.store(true, Ordering::Release);
            return Err("Studio request timed out; outcome unknown if execution already began. Read the draft and music state before retrying.".into());
        }
    };
    if name != "validate_studio_draft" {
        return Ok(captured);
    }
    let validator = shared
        .lock()
        .map_err(|_| "Music state unavailable")?
        .studio_validator
        .clone()
        .ok_or("Validation requires an owned runtime ready for validation")?;
    let report = validator.validate(
        captured["source"]
            .as_str()
            .ok_or("Missing captured draft")?,
    )?;
    Ok(
        json!({"draft_id":captured["draft_id"],"revision":captured["revision"],"scene":captured["scene"],"valid":report["valid"],"diagnostics":report["diagnostics"],"message":report["message"],"boundary":"runtime_validation_only","playback_changed":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_mutations_require_a_revision_and_bound_utf8_bytes() {
        assert!(check("set_studio_draft", &json!({"source":"x"})).is_err());
        assert!(
            check(
                "set_studio_draft",
                &json!({"draft_id":"x","expected_revision":1,"source":"🦊".repeat(131073)})
            )
            .is_err()
        );
        assert!(check("execute_studio_draft",&json!({"draft_id":"x","expected_revision":1,"session":"test","generation":2,"play":true})).is_err());
    }
    #[test]
    fn expired_ui_requests_are_not_active() {
        let (tx, _) = mpsc::sync_channel(1);
        let reply = Reply {
            tx,
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(!reply.active());
    }
}
