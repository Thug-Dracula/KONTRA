//! `sounding_zones <instrument.nki> [key] [velocity]`: which zones a note
//! actually starts, named by their sample, with the range each one answered.
//!
//! `kontra-silence` counts sounding zones; this names them, which is what a
//! question about *layer selection* needs. The render loop is silence's: a
//! quarter second of load silence, the note on the engine's own clock, a
//! 500 ms hold and a 500 ms tail.
//!
//! ```sh
//! cargo run --release -p kontra-host --example sounding_zones -- \
//!   "…/Evolution Steel Strings.nki" 60 100
//! ```

use std::sync::Arc;
use std::time::Duration;

use kontra_engine::{Engine, Event, HostNote, HostPattern, SampleTime, Timed};
use kontra_stream::{Store, StoreConfig};

/// The engine's rate, as every probe and the oracle use.
const RATE: u32 = 48_000;
/// Frames per `process` call.
const BLOCK: usize = 128;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: sounding_zones <file> [key] [velocity]")?;
    let key: u8 = args.next().and_then(|k| k.parse().ok()).unwrap_or(60);
    let velocity: u8 = args.next().and_then(|v| v.parse().ok()).unwrap_or(100);

    let mut store = Store::new(StoreConfig::DEFAULT)?;
    let ir = kontra_host::instrument::read(std::path::Path::new(&path))?;
    // The plan numbers zones flat; keep the IR's own coordinates to name them.
    let mut names = Vec::new();
    for (g, group) in ir.groups.iter().enumerate() {
        for (m, zone) in group.zones.iter().enumerate() {
            names.push((
                g,
                m,
                zone.sample,
                zone.keys.clone(),
                zone.velocities.clone(),
                zone.root,
            ));
        }
    }
    let samples: Vec<String> = ir.samples.iter().map(|s| s.name.clone()).collect();
    let loaded = kontra_host::instrument::build(ir, RATE, &mut store);
    let plan = Arc::new(loaded.plan);
    println!(
        "zones prepared: {} of {} translated",
        plan.zones.len(),
        names.len()
    );

    let stream = store.client();
    let mut engine = Engine::new_traced(Arc::clone(&plan), stream.clone(), RATE, 1 << 20);
    engine.set_offline_wait(true);
    let mut silent = vec![[0.0f32; 2]; BLOCK];
    for _ in 0..(RATE as usize / 4 / BLOCK) {
        engine.process(&mut silent, &[]);
    }
    let start = engine.now().0;
    let hold = u64::from(RATE) / 2;
    let stop = hold + u64::from(RATE) / 2;
    let on = Event::NoteOn {
        note: HostNote {
            port: 0,
            channel: 0,
            key,
            id: -1,
            clap: false,
        },
        velocity: f64::from(velocity) / 127.0,
        tune: 0.0,
    };
    let off = Event::NoteOff(HostPattern {
        port: 0,
        channel: 0,
        key: i32::from(key),
        id: -1,
        clap: false,
    });

    let mut started: Vec<usize> = Vec::new();
    let mut peak = 0.0f64;
    let mut audio = vec![[0.0f32; 2]; BLOCK];
    let mut at = 0u64;
    while at < stop {
        let frames = usize::try_from((stop - at).min(BLOCK as u64))?;
        let until = at + frames as u64;
        let mut timed: Vec<Timed<Event>> = Vec::new();
        if at == 0 {
            timed.push(Timed {
                at: SampleTime(start),
                body: on,
            });
        }
        if hold >= at && hold < until {
            timed.push(Timed {
                at: SampleTime(start + hold),
                body: off,
            });
        }
        stream.settle(Duration::from_secs(5));
        let Some(block) = audio.get_mut(..frames) else {
            break;
        };
        engine.process(block, &timed);
        for sample in block.iter() {
            peak = peak.max(f64::from(sample[0].abs()).max(f64::from(sample[1].abs())));
        }
        for event in engine.trace_events() {
            if let kontra_engine::TraceEvent::Voice {
                zone,
                event: kontra_engine::VoiceEvent::Start,
                ..
            } = event
            {
                started.push(*zone);
            }
        }
        engine.clear_trace();
        at = until;
    }

    println!("peak {peak:.6}");
    for &z in &started {
        let Some(zone) = plan.zones.get(z) else {
            println!("zone {z}: not in the plan");
            continue;
        };
        let Some((g, m, sample, keys, velocities, root)) = names
            .iter()
            .find(|(g, m, ..)| *g == zone.group as usize && *m == zone.member as usize)
        else {
            println!(
                "zone {z}: no IR zone for group {} member {}",
                zone.group, zone.member
            );
            continue;
        };
        println!(
            "zone {z}: group {g} member {m} keys {keys:?} velocities {velocities:?} \
             root {root} sample {}",
            samples.get(*sample).map_or("?", String::as_str)
        );
    }
    Ok(())
}
