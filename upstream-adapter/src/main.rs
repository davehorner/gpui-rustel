use hearsay::{Client, ClientSettings, Route};
use music_link::{Command, Response};
use rustel_ui_adapter::{Capabilities, Config, Hosted, Output};
use std::{path::PathBuf, time::Duration};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut executable = None;
    let mut score = None;
    let mut demo_dir = None;
    let mut silent = false;
    let mut listen = false;
    let mut websocket = "127.0.0.1:19613".to_owned();
    let mut hub = "127.0.0.1:9612".to_owned();
    for argument in std::env::args().skip(1) {
        if let Some(v) = argument.strip_prefix("--rustel=") {
            executable = Some(PathBuf::from(v));
        } else if let Some(v) = argument.strip_prefix("--score=") {
            score = Some(PathBuf::from(v));
        } else if let Some(v) = argument.strip_prefix("--demos=") {
            demo_dir = Some(PathBuf::from(v));
        } else if let Some(v) = argument.strip_prefix("--listen=") {
            listen = true;
            hub = v.to_owned();
        } else if let Some(v) = argument.strip_prefix("--websocket=") {
            websocket = v.to_owned();
        } else if let Some(v) = argument.strip_prefix("--hub=") {
            hub = v.to_owned();
        } else if argument == "--silent" {
            silent = true;
        } else {
            return Err(format!("Unknown option {argument}; use --rustel=EXE --score=FILE [--silent] [--hub=HOST:PORT]").into());
        }
    }
    let _broker = if listen {
        let broker = hearsay::Broker::start(&hub).await?;
        broker.start_websocket_listener(&websocket).await?;
        eprintln!("Music host listening: TCP {hub}, WebSocket ws://{websocket}");
        Some(broker)
    } else {
        None
    };
    let hosted = Hosted::spawn(Config {
        executable: executable.ok_or("--rustel is required")?,
        score: score.ok_or("--score is required")?,
        silent,
        demo_dir,
    })?;
    let client = Client::new(
        "upstream-rustel-adapter",
        ClientSettings {
            max_connection_attempts: None,
            ..Default::default()
        },
    );
    tokio::select! {result=client.connect(&hub)=>result?, _=tokio::signal::ctrl_c()=>return Ok(())};
    client.subscribe(&[music_link::COMMAND_TOPIC]).await?;
    let mut session = String::new();
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    loop {
        tokio::select! {
            _=tokio::signal::ctrl_c()=>break,
            _=tick.tick()=>for event in hosted.events.try_iter().take(64) {
                match event {
                    Output::Snapshot(s)=>{session=s.session.clone();let mut value=serde_json::to_value(s)?;value["capabilities"]=serde_json::to_value(Capabilities::default())?;client.publish_json(music_link::SNAPSHOT_TOPIC,&value,Route::Global).await?;},
                    Output::Audio(a)=>client.publish_json(music_link::AUDIO_TOPIC,&a,Route::Global).await?,
                    Output::Response(r)=>{eprintln!("Control {} accepted={} {}",r.request,r.accepted,r.message);client.publish_json(music_link::RESPONSE_TOPIC,&r,Route::Global).await?;},
                    Output::Status(s)=>eprintln!("{s}"),
                }
            },
            message=client.next_message()=>{
                let Some(message)=message else{break};
                if let Ok(c)=message.read_json::<Command>() && c.session==session {
                    let request=c.request.clone();if let Err(error)=hosted.send(c){client.publish_json(music_link::RESPONSE_TOPIC,&Response {request,accepted:false,message:error},Route::Global).await?;}
                }
            }
        }
    }
    Ok(())
}
