use crate::{Adapter, Output};
use music_link::{Command, Response};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
#[derive(Clone, Debug)]
pub struct Config {
    pub executable: PathBuf,
    pub score: PathBuf,
    pub silent: bool,
    pub demo_dir: Option<PathBuf>,
}
fn catalogue(config: &Config) -> std::collections::BTreeMap<String, PathBuf> {
    let mut demos = std::collections::BTreeMap::new();
    let label = config
        .score
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    demos.insert(label, config.score.clone());
    if let Some(directory) = &config.demo_dir
        && let Ok(entries) = std::fs::read_dir(directory)
    {
        for entry in entries.flatten().take(256) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "strudel")
                && path.is_file()
                && let Some(name) = path.file_name().and_then(|s| s.to_str())
            {
                if demos.values().any(|p| {
                    p == &path
                        || p.canonicalize()
                            .ok()
                            .zip(path.canonicalize().ok())
                            .is_some_and(|(a, b)| a == b)
                }) {
                    continue;
                }
                let name = if demos.contains_key(name) {
                    format!("Demos/{name}")
                } else {
                    name.to_owned()
                };
                demos.insert(name, path);
                if demos.len() >= 64 {
                    break;
                }
            }
        }
    }
    demos
}
fn source(path: &std::path::Path) -> io::Result<String> {
    let mut value = String::new();
    std::fs::File::open(path)?
        .take(512 * 1024 + 1)
        .read_to_string(&mut value)?;
    if value.len() > 512 * 1024 {
        return Err(io::Error::other("Score exceeds 512 KiB"));
    }
    Ok(value)
}
struct EditorFiles(Vec<PathBuf>);
impl Drop for EditorFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
fn editor_score(config: &Config, text: &str, files: &mut EditorFiles) -> Result<PathBuf, String> {
    use std::sync::atomic::AtomicU64;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    if text.len() > 512 * 1024 {
        return Err("Score exceeds 512 KiB".into());
    }
    let directory = config.score.parent().unwrap_or(std::path::Path::new("."));
    let path = directory.join(format!(
        ".rustel-editor-{}-{}.js",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    files.0.push(path.clone());
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    drop(file);
    let log = path.with_extension("validation.log");
    let output = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    files.0.push(log.clone());
    let mut command = std::process::Command::new(&config.executable);
    command
        .arg("validate")
        .arg(&path)
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| e.to_string())?)
        .stderr(output);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let began = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                let message = source(&log).unwrap_or_default();
                return Err(format!(
                    "Score rejected: {}",
                    message.chars().take(4096).collect::<String>()
                ));
            }
            break;
        }
        if began.elapsed() > Duration::from_secs(8) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Score validation timed out; previous score kept".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(path)
}
fn adapter(
    config: &Config,
    session: &str,
    epoch: u64,
    demos: &std::collections::BTreeMap<String, PathBuf>,
) -> Adapter {
    let mut adapter = Adapter::new(
        session.to_owned(),
        source(&config.score).unwrap_or_default(),
        epoch,
    );
    adapter.snapshot.demos = demos.keys().cloned().collect();
    adapter.snapshot.demo = demos
        .iter()
        .find(|(_, p)| **p == config.score)
        .map(|(n, _)| n.clone())
        .unwrap_or_default();
    adapter
}
enum Request {
    Control(Box<Command>),
    Restart,
}
struct Process {
    child: Child,
    records: mpsc::Receiver<String>,
    writer: mpsc::SyncSender<serde_json::Value>,
    eof: bool,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut bytes = vec![];
    let count = reader
        .take(2 * 1024 * 1024 + 1)
        .read_until(b'\n', &mut bytes)?;
    if count == 0 {
        return Ok(None);
    }
    if count > 2 * 1024 * 1024 {
        return Err(io::Error::other("Rustel protocol line exceeds 2 MiB"));
    }
    String::from_utf8(bytes).map(Some).map_err(io::Error::other)
}
fn start(config: &Config) -> io::Result<Process> {
    let mut command = std::process::Command::new(&config.executable);
    command
        .arg("play")
        .arg(&config.score)
        .args(["--watch", "--ui-events", "--no-update-check"]);
    if config.silent {
        command.args(["--output", "silent"]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (tx, records) = mpsc::sync_channel(64);
    let (writer, rx) = mpsc::sync_channel::<serde_json::Value>(64);
    let errors = tx.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_line(&mut reader) {
                Ok(Some(line)) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tx.send(serde_json::json!({"adapter_error":e.to_string()}).to_string());
                    break;
                }
            }
        }
    });
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        while let Ok(Some(line)) = read_line(&mut reader) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                && (v.get("live_error").is_some() || v.get("error").is_some() || v.get("log").is_some())
                && errors
                    .send(serde_json::json!({"adapter_error":v.get("log").or_else(||v.get("live_error")).and_then(|v|v.get("message")).and_then(|v|v.as_str()).map(str::to_owned).unwrap_or_else(||v.to_string())}).to_string())
                    .is_err()
            {
                break;
            }
        }
    });
    thread::spawn(move || {
        for value in rx {
            if writeln!(stdin, "{value}")
                .and_then(|_| stdin.flush())
                .is_err()
            {
                break;
            }
        }
    });
    Ok(Process {
        child,
        records,
        writer,
        eof: false,
    })
}
pub struct Hosted {
    requests: mpsc::SyncSender<Request>,
    pub events: mpsc::Receiver<Output>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Hosted {
    pub fn spawn(config: Config) -> io::Result<Self> {
        let process = start(&config)?;
        let (requests, rx) = mpsc::sync_channel(64);
        let (tx, events) = mpsc::sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = thread::Builder::new()
            .name("rustel-supervisor".into())
            .spawn(move || {
                let session = format!(
                    "upstream-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );
                let mut config=config;
                let mut editor_files=EditorFiles(Vec::new());
                let demos=catalogue(&config);
                let mut epoch=1;
                let mut adapter=adapter(&config,&session,epoch,&demos);
                let mut pending_demo=None::<String>;
                let mut last_stopped=std::time::Instant::now();
                let mut process = Some(process);
                while !stopped.load(Ordering::Acquire) {
                    for request in rx.try_iter().take(64) {
                        match request {
                            Request::Restart => {
                                if let Some(request)=pending_demo.take() {let _=tx.try_send(Output::Response(Response {request,accepted:false,message:"Score load superseded by restart".into()}));}
                                drop(process.take());
                                epoch += 1;
                                adapter = self::adapter(&config,&session,epoch,&demos);
                                match start(&config) {
                                    Ok(p) => process = Some(p),
                                    Err(e) => {
                                        let _ = tx.try_send(Output::Status(e.to_string()));
                                    }
                                }
                            }
                            Request::Control(c) => {
                                let request = c.request.clone();
                                if matches!(c.transport,Some(music_link::TransportCommand::Play)) {
                                    let result=c.validate(&adapter.snapshot).map_err(str::to_owned).and_then(|()| {
                                        let child=start(&config).map_err(|e|e.to_string())?;
                                        process=Some(child);epoch+=1;adapter=self::adapter(&config,&session,epoch,&demos);
                                        pending_demo=Some(request.clone());Ok(())
                                    });
                                    if let Err(message)=result {let _=tx.try_send(Output::Response(Response {request,accepted:false,message}));}
                                    continue;
                                }
                                if let Some(text)=&c.source_update {
                                    let result=c.validate(&adapter.snapshot).map_err(str::to_owned).and_then(|()| {
                                        let score=editor_score(&config,text,&mut editor_files)?;
                                        let mut next=config.clone();next.score=score;
                                        let child=start(&next).map_err(|e|e.to_string())?;
                                        drop(process.replace(child));config=next;epoch+=1;
                                        adapter=self::adapter(&config,&session,epoch,&demos);adapter.snapshot.demo="Editor".into();
                                        if let Some(previous)=pending_demo.replace(request.clone()) {let _=tx.try_send(Output::Response(Response {request:previous,accepted:false,message:"Score load superseded".into()}));}
                                        Ok(())
                                    });
                                    if let Err(message)=result {let _=tx.try_send(Output::Response(Response {request,accepted:false,message}));}
                                    continue;
                                }
                                if let Some(name)=&c.demo {
                                    let result=c.validate(&adapter.snapshot).map_err(str::to_owned).and_then(|()| {
                                        let score=demos.get(name).ok_or("Unknown demo".to_owned())?;
                                        source(score).map_err(|e|e.to_string())?;
                                        let mut next=config.clone();next.score=score.clone();
                                        let child=start(&next).map_err(|e|e.to_string())?;
                                        drop(process.replace(child));
                                        config=next;epoch+=1;
                                        adapter=self::adapter(&config,&session,epoch,&demos);
                                        if let Some(request)=pending_demo.replace(request.clone()) {
                                            let _=tx.try_send(Output::Response(Response {request,accepted:false,message:"Demo request superseded".into()}));
                                        }
                                        Ok(())
                                    });
                                    if let Err(message)=result {let _=tx.try_send(Output::Response(Response {request,accepted:false,message}));}
                                    continue;
                                }
                                let result = adapter.encode(*c).and_then(|v| {
                                    process
                                        .as_ref()
                                        .ok_or("Runtime stopped; restart it first".to_owned())?
                                        .writer
                                        .try_send(v)
                                        .map_err(|_| "Runtime control queue unavailable".into())
                                });
                                if let Err(message) = result {
                                    let _ = tx.try_send(Output::Response(Response {
                                        request,
                                        accepted: false,
                                        message,
                                    }));
                                }
                            }
                        }
                    }
                    if let Some(p) = &mut process {
                        for _ in 0..64 {
                            let record = match p.records.try_recv() {
                                Ok(record) => record,
                                Err(mpsc::TryRecvError::Empty) => break,
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    p.eof = true;
                                    break;
                                }
                            };
                            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&record)
                                && let Some(error) = value.get("adapter_error")
                            {
                                let _ = tx.try_send(Output::Status(error.as_str().map(str::to_owned).unwrap_or_else(||error.to_string())));
                                continue;
                            }
                            if let Ok(value)=serde_json::from_str::<serde_json::Value>(&record) {
                                if let Some(message)=value.get("live_error").and_then(|v|v.get("message")).and_then(|v|v.as_str()) {
                                    let _=tx.try_send(Output::Status(message.chars().take(4096).collect()));
                                }
                            }
                            if record.contains("\"ui_layout\"") {
                                adapter.set_source(
                                    source(&config.score).unwrap_or_default(),
                                );
                            }
                            match adapter.ingest(&record) {
                                Ok(values) => {
                                    for value in values {
                                        if matches!(&value,Output::Snapshot(s) if s.generation>0 && !s.source.is_empty())
                                            && let Some(request)=pending_demo.take() {
                                            let _=tx.try_send(Output::Response(Response {request,accepted:true,message:String::new()}));
                                        }
                                        let _ = tx.try_send(value);
                                    }
                                }
                                Err(error) => {
                                    let _ = tx.try_send(Output::Status(format!(
                                        "Protocol rejected: {error}"
                                    )));
                                }
                            }
                        }
                        if let Ok(Some(exit)) = p.child.try_wait()
                            && p.eof
                        {
                            let _ = tx.try_send(Output::Status(format!(
                                "Rustel stopped ({exit}); restart runtime to play again"
                            )));
                            drop(process.take());
                            adapter.snapshot.events.clear();
                            adapter.snapshot.transport=Some(music_link::TransportState {paused:true,view_cycle:Some(adapter.snapshot.cycle)});
                            let _=tx.try_send(Output::Snapshot(Box::new(adapter.snapshot.clone())));
                            last_stopped=std::time::Instant::now();
                        }
                    }
                    if process.is_none() && last_stopped.elapsed()>=Duration::from_millis(500) {
                        let _=tx.try_send(Output::Snapshot(Box::new(adapter.snapshot.clone())));
                        last_stopped=std::time::Instant::now();
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                drop(process);
            })?;
        Ok(Self {
            requests,
            events,
            stop,
            thread: Some(thread),
        })
    }
    pub fn send(&self, command: Command) -> Result<(), String> {
        self.requests
            .try_send(Request::Control(Box::new(command)))
            .map_err(|_| "Runtime command queue unavailable".into())
    }
    pub fn restart(&self) -> Result<(), String> {
        self.requests
            .try_send(Request::Restart)
            .map_err(|_| "Runtime command queue unavailable".into())
    }
}
impl Drop for Hosted {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
