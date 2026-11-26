//! Local broker plus a tiny HTTP adapter for JUCE's existing HTTP client.
use hearsay::{Broker, Client, ClientSettings, Route};
use music_link::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let broker = Broker::start("127.0.0.1:9612").await?;
    broker.start_websocket_listener("127.0.0.1:9613").await?;
    let client = Client::new("music-http", ClientSettings::default());
    client.connect("127.0.0.1:9612").await?;
    client
        .subscribe(&[SNAPSHOT_TOPIC, RESPONSE_TOPIC, AUDIO_TOPIC])
        .await?;
    let sessions = Arc::new(Mutex::new(HashMap::<String, (Snapshot, Instant)>::new()));
    let responses = Arc::new(Mutex::new(HashMap::<String, Response>::new()));
    let audio = Arc::new(Mutex::new(HashMap::<String, (AudioFrame, Instant)>::new()));
    let audio_store = audio.clone();
    let receiver = client.clone();
    let stored = sessions.clone();
    let replies = responses.clone();
    tokio::spawn(async move {
        while let Some(message) = receiver.next_message().await {
            if message.topic() == SNAPSHOT_TOPIC {
                if let Ok(snapshot) = message.read_json::<Snapshot>() {
                    let mut sessions = stored.lock().await;
                    sessions.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(3));
                    if sessions.len() < 64 || sessions.contains_key(&snapshot.session) {
                        sessions.insert(snapshot.session.clone(), (snapshot, Instant::now()));
                    }
                }
            } else if message.topic() == AUDIO_TOPIC {
                if let Ok(frame) = message.read_json::<AudioFrame>() {
                    if !frame.valid() {
                        continue;
                    }
                    let mut latest = audio_store.lock().await;
                    latest.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(1));
                    if latest
                        .get(&frame.session)
                        .is_some_and(|(old, _)| old.sequence >= frame.sequence)
                    {
                        continue;
                    }
                    if latest.len() < 64 || latest.contains_key(&frame.session) {
                        latest.insert(frame.session.clone(), (frame, Instant::now()));
                    }
                }
            } else if let Ok(response) = message.read_json::<Response>() {
                let mut replies = replies.lock().await;
                if replies.len() > 256 {
                    replies.clear();
                }
                replies.insert(response.request.clone(), response);
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:9614").await?;
    println!("Music hub: TCP 9612, viewer WebSocket 9613, Bespoke HTTP 9614");
    loop {
        let (mut socket, _) = listener.accept().await?;
        let sessions = sessions.clone();
        let responses = responses.clone();
        let client = client.clone();
        let audio = audio.clone();
        tokio::spawn(async move {
            let operation = async {
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut buf = [0; 2048];
                    let count = socket.read(&mut buf).await?;
                    if count == 0 {
                        return Ok::<(), std::io::Error>(());
                    }
                    bytes.extend_from_slice(&buf[..count]);
                    if bytes.len() > 16_384 {
                        return Ok(());
                    }
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let header = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                let length = header
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if length > 4096 {
                    return Ok(());
                }
                while bytes.len() < header_end + length {
                    let mut buf = [0; 2048];
                    let count = socket.read(&mut buf).await?;
                    if count == 0 {
                        return Ok(());
                    }
                    bytes.extend_from_slice(&buf[..count]);
                }
                let (status, body) = if header.starts_with("GET /sessions ") {
                    let mut sessions = sessions.lock().await;
                    sessions.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(3));
                    let mut snapshots: Vec<_> =
                        sessions.values().map(|(snapshot, _)| snapshot).collect();
                    snapshots.sort_by(|a, b| a.session.cmp(&b.session));
                    (
                        "200 OK",
                        serde_json::to_string(&snapshots).unwrap_or("[]".into()),
                    )
                } else if header.starts_with("GET /analysis ") {
                    let mut latest = audio.lock().await;
                    latest.retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(1));
                    let frames: Vec<_> = latest.values().map(|(frame, _)| frame).collect();
                    (
                        "200 OK",
                        serde_json::to_string(&frames).unwrap_or("[]".into()),
                    )
                } else if header.starts_with("POST /transpose ")
                    || header.starts_with("POST /solo ")
                    || header.starts_with("POST /control ")
                {
                    match serde_json::from_slice::<Command>(&bytes[header_end..header_end + length])
                    {
                        Ok(command) => {
                            let valid = sessions.lock().await.get(&command.session).is_some_and(
                                |(s, seen)| {
                                    seen.elapsed() < Duration::from_secs(3)
                                        && command.validate(s).is_ok()
                                },
                            );
                            if !valid {
                                (
                                    "409 Conflict",
                                    "{\"error\":\"stale or invalid lane\"}".into(),
                                )
                            } else {
                                let _ = client
                                    .publish_json(COMMAND_TOPIC, &command, Route::Global)
                                    .await;
                                let deadline = Instant::now() + Duration::from_millis(1200);
                                let response = loop {
                                    if let Some(response) =
                                        responses.lock().await.remove(&command.request)
                                    {
                                        break Some(response);
                                    }
                                    if Instant::now() >= deadline {
                                        break None;
                                    }
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                };
                                match response {
                                    Some(response) => (
                                        "200 OK",
                                        serde_json::to_string(&response).unwrap_or_default(),
                                    ),
                                    None => (
                                        "504 Gateway Timeout",
                                        "{\"error\":\"Rustel did not acknowledge command\"}".into(),
                                    ),
                                }
                            }
                        }
                        Err(_) => ("400 Bad Request", "{}".into()),
                    }
                } else {
                    ("404 Not Found", "{}".into())
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await?;
                Ok(())
            };
            let _ = tokio::time::timeout(Duration::from_secs(3), operation).await;
        });
    }
}
