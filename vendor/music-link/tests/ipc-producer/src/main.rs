//! IPC fixture: real shipping writer, deterministic stereo blocks instead of DSP.
use std::{io::Write, time::{Duration, Instant}};
mod scalar {
    pub struct Stems;
    impl Stems {
        pub fn orbit(&self, slot: usize, frames: usize) -> &[f32] {
            static FIRST: [f32;256] = { let mut a=[0.0;256]; let mut i=0; while i<128 {a[i*2]=0.125;a[i*2+1]=-0.25;i+=1;} a };
            static SECOND: [f32;256] = { let mut a=[0.0;256]; let mut i=0; while i<128 {a[i*2]=0.25;a[i*2+1]=0.5;i+=1;} a };
            static SILENT: [f32;256] = [0.0;256];
            &(match slot { 0=>&FIRST,1=>&SECOND,_=>&SILENT })[..frames*2]
        }
    }
}
#[allow(dead_code)]
#[path = "../../../../vendor/rustel/files/crates/audio/src/lane_audio.rs"]
mod lane_audio;

fn main() {
    let map=lane_audio::LaneAudioMap::create().expect("native mapping");
    println!("{}",map.name()); std::io::stdout().flush().unwrap();
    let start=Instant::now();let mut muted=false;let mut restored=false;
    while start.elapsed()<Duration::from_secs(4) {
        let mut main=[0.0;256];for frame in main.chunks_exact_mut(2) {frame[0]=0.375;frame[1]=0.25;}
        map.write(&scalar::Stems,48000,128,&mut main);
        if main[254]<0.251 {muted=true;}
        if muted && main[254]>0.374 {restored=true;}
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("muted={muted} restored={restored}");
}
