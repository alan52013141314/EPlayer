use crate::audio::{AudioEngine, Command, Event, Repeat};
use anyhow::{bail, Result};
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

// Opt-in local acceptance diagnostics. No writer thread or file is created in
// normal use, and the bounded queue never makes the UI wait for disk I/O.
pub struct GuiReport {
    sender: Option<crossbeam_channel::Sender<serde_json::Value>>,
    worker: Option<std::thread::JoinHandle<()>>,
    started: Instant,
    last_sample: Instant,
}

impl GuiReport {
    pub fn from_env() -> Result<Option<Self>> {
        let Some(path) = std::env::var_os("EPLAYER_GUI_REPORT") else {
            return Ok(None);
        };
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let (sender, receiver) = crossbeam_channel::bounded::<serde_json::Value>(8);
        let worker = std::thread::Builder::new()
            .name("gui-report".into())
            .spawn(move || {
                use std::io::Write;
                let mut file = std::io::BufWriter::new(file);
                for record in receiver {
                    if writeln!(file, "{record}")
                        .and_then(|_| file.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Some(Self {
            sender: Some(sender),
            worker: Some(worker),
            started: Instant::now(),
            last_sample: Instant::now(),
        }))
    }

    pub fn sample(&mut self, snapshot: impl FnOnce() -> serde_json::Value) {
        if self.last_sample.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_sample = Instant::now();
        let mut record = snapshot();
        record["elapsed_ms"] = serde_json::json!(self.started.elapsed().as_millis());
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(record);
        }
    }
}

impl Drop for GuiReport {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn wait_for(engine: &AudioEngine, condition: impl Fn() -> bool) -> Result<()> {
    let start = Instant::now();
    while !condition() {
        while let Ok(event) = engine.events.try_recv() {
            if let Event::Error(error) = event {
                bail!(error);
            }
        }
        if start.elapsed() > Duration::from_secs(8) {
            bail!("Audio control test timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

pub fn order_test(path: PathBuf) -> Result<()> {
    let engine = AudioEngine::new()?;
    engine.set_volume(0.0);
    let queue = vec![path.clone(), path.clone(), path];
    for (name, repeat, shuffle, start, expected) in [
        ("sequential", Repeat::Off, false, 0, vec![0, 1, 2]),
        ("repeat_all", Repeat::All, false, 2, vec![2, 0, 1]),
        ("repeat_one", Repeat::One, false, 1, vec![1, 1, 1]),
        ("shuffle", Repeat::All, true, 0, vec![0; 8]),
    ] {
        let mut token = engine.state.token.load(Ordering::Acquire);
        engine.send(Command::Mode(repeat, shuffle));
        engine.send(Command::Queue(queue.clone(), start));
        let mut observed = Vec::new();
        for _ in 0..expected.len() {
            wait_for(&engine, || {
                engine.state.token.load(Ordering::Acquire) != token
            })?;
            token = engine.state.token.load(Ordering::Acquire);
            observed.push(engine.state.index.load(Ordering::Relaxed));
        }
        if shuffle {
            anyhow::ensure!(
                observed.iter().all(|&i| i < queue.len()),
                "Invalid shuffle index"
            );
            anyhow::ensure!(
                observed.windows(2).all(|pair| pair[0] != pair[1]),
                "Shuffle immediately repeated a track"
            );
        } else {
            anyhow::ensure!(observed == expected, "{name}: {observed:?} != {expected:?}");
        }
        if repeat == Repeat::Off {
            wait_for(&engine, || engine.state.ended.load(Ordering::Acquire))?;
            anyhow::ensure!(
                !engine.state.playing.load(Ordering::Relaxed),
                "Sequential playback did not stop"
            );
        }
        println!(
            "{}",
            serde_json::json!({"mode":name,"audible_indices":observed,"status":"passed"})
        );
    }
    let underruns = engine.state.underruns.load(Ordering::Relaxed);
    anyhow::ensure!(underruns == 0, "Order test underruns: {underruns}");
    println!(
        "{}",
        serde_json::json!({"order_test":"passed","underruns":underruns})
    );
    Ok(())
}

pub fn seek_stress(path: PathBuf) -> Result<()> {
    let engine = AudioEngine::new()?;
    engine.set_volume(0.0);
    engine.send(Command::Queue(vec![path], 0));
    engine.send(Command::Mode(Repeat::One, false));
    wait_for(&engine, || engine.position() > 0.2)?;
    for i in 0..300 {
        engine.send(Command::Seek(if i % 2 == 0 { 0.3 } else { 1.5 }));
        std::thread::sleep(Duration::from_millis(37));
        while let Ok(event) = engine.events.try_recv() {
            if let Event::Error(error) = event {
                bail!(error);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(200));
    let underruns = engine.state.underruns.load(Ordering::Relaxed);
    println!(
        "{}",
        serde_json::json!({"seek_stress":300,"underruns":underruns,"frames":engine.state.output_frames.load(Ordering::Relaxed)})
    );
    anyhow::ensure!(underruns == 0, "Underrun during repeated seek");
    Ok(())
}

pub fn control_test(path: PathBuf) -> Result<()> {
    let engine = AudioEngine::new()?;
    engine.set_volume(0.0);
    engine.send(Command::Queue(vec![path.clone(), path.clone()], 0));
    wait_for(&engine, || engine.position() > 0.25)?;
    engine.state.playing.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(100));
    let paused = engine.position();
    std::thread::sleep(Duration::from_millis(150));
    if (engine.position() - paused).abs() > 0.001 {
        bail!("Pause advanced the playback clock");
    }
    engine.send(Command::Seek(1.5));
    std::thread::sleep(Duration::from_millis(100));
    anyhow::ensure!(
        (engine.position() - 1.5).abs() < 0.01,
        "Paused seek clock did not update: {}",
        engine.position()
    );
    anyhow::ensure!(
        !engine.state.playing.load(Ordering::Relaxed),
        "Seeking resumed paused playback"
    );
    engine.toggle();
    wait_for(&engine, || engine.position() >= 1.5)?;
    if engine.position() > 1.8 {
        bail!("Seek resumed at incorrect position");
    }
    engine.send(Command::Next);
    wait_for(&engine, || {
        engine.state.index.load(Ordering::Relaxed) == 1 && engine.position() < 0.5
    })?;
    engine.send(Command::Previous);
    wait_for(&engine, || {
        engine.state.index.load(Ordering::Relaxed) == 0 && engine.position() < 0.5
    })?;
    engine.send(Command::Mode(Repeat::One, false));
    engine.send(Command::Seek(2.5));
    wait_for(&engine, || engine.position() > 2.5)?;
    wait_for(&engine, || engine.position() < 0.4)?;
    if engine.state.index.load(Ordering::Relaxed) != 0 {
        bail!("Repeat one changed track");
    }
    // Edit the active queue while paused, then empty it and start again.
    engine.state.playing.store(false, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(50));
    let retained_position = engine.position();
    let retained_token = engine.state.token.load(Ordering::Relaxed);
    engine.send(Command::UpdateQueue(vec![path], None));
    std::thread::sleep(Duration::from_millis(150));
    anyhow::ensure!(
        !engine.state.playing.load(Ordering::Relaxed),
        "Queue edit resumed paused playback"
    );
    engine.toggle();
    wait_for(&engine, || engine.position() > retained_position + 0.05)?;
    anyhow::ensure!(
        engine.state.token.load(Ordering::Relaxed) == retained_token,
        "Queue edit restarted the current track"
    );
    anyhow::ensure!(
        (engine.position() - retained_position).abs() < 0.3,
        "Queue edit lost playback position"
    );
    engine.send(Command::UpdateQueue(Vec::new(), None));
    wait_for(&engine, || engine.state.ended.load(Ordering::Relaxed))?;
    println!(
        "{}",
        serde_json::json!({"control_test":"passed","checks":["pause clock","seek while paused","next","previous","repeat one","edit queue while paused","preserve position after edit","empty queue stops"],"underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    Ok(())
}

pub fn spectrum_test(path: PathBuf) -> Result<()> {
    let engine = AudioEngine::new()?;
    engine.set_volume(0.03);
    engine.send(Command::Queue(vec![path], 0));
    engine.send(Command::Mode(Repeat::One, false));
    engine
        .spectrum
        .shared
        .enabled
        .store(true, Ordering::Release);
    wait_for(&engine, || {
        engine.spectrum.shared.analyses.load(Ordering::Relaxed) >= 8
    })?;
    let enabled_count = engine.spectrum.shared.analyses.load(Ordering::Relaxed);
    let peak = engine
        .spectrum
        .shared
        .bands
        .lock()
        .unwrap()
        .iter()
        .copied()
        .fold(0.0f32, f32::max);
    if peak <= 0.0 {
        bail!("Spectrum was empty during playback");
    }
    engine
        .spectrum
        .shared
        .enabled
        .store(false, Ordering::Release);
    std::thread::sleep(Duration::from_millis(100));
    let disabled_count = engine.spectrum.shared.analyses.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(300));
    if engine.spectrum.shared.analyses.load(Ordering::Relaxed) != disabled_count {
        bail!("FFT continued while disabled");
    }
    if engine
        .spectrum
        .shared
        .bands
        .lock()
        .unwrap()
        .iter()
        .any(|&value| value != 0.0)
    {
        bail!("Spectrum did not clear after disable");
    }
    println!(
        "{}",
        serde_json::json!({"spectrum_test":"passed","enabled_analyses":enabled_count,"disabled_count":disabled_count,"peak":peak,"underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    Ok(())
}

pub fn soak(seconds: u64, root: PathBuf, report: PathBuf) -> Result<()> {
    use crate::library::{Library, Request, Response};
    use std::io::Write;
    let mut paths = std::fs::read_dir(&root)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| crate::library::is_audio(p))
        .collect::<Vec<_>>();
    paths.sort();
    anyhow::ensure!(!paths.is_empty(), "No soak fixtures");
    let mut log = std::fs::File::create(report)?;
    let library = Library::start(
        root.join(format!("soak-{}.sqlite", std::process::id())),
        eframe::egui::Context::default(),
    );
    let engine = AudioEngine::new()?;
    engine.set_volume(0.0);
    engine.send(Command::Queue(paths.clone(), 0));
    engine.send(Command::Mode(Repeat::All, false));
    let start = Instant::now();
    let mut last_phase = u64::MAX;
    let mut queue_edits = 0;
    let mut last_log = u64::MAX;
    let mut loaded = 0;
    let mut scans = 0;
    let mut baseline_memory = None;
    let mut peak_memory = 0;
    let mut last_output = 0;
    let mut last_underruns = 0;
    let mut last_loaded_path = PathBuf::new();
    while start.elapsed().as_secs() < seconds {
        while let Ok(event) = engine.events.try_recv() {
            match event {
                Event::Error(e) => bail!(e),
                Event::Loaded { info, .. } => {
                    loaded += 1;
                    last_loaded_path = info.path;
                }
            }
        }
        while let Ok(response) = library.responses.try_recv() {
            match response {
                Response::Error(e) => bail!(e),
                Response::ScanProgress {
                    done: true, errors, ..
                } => {
                    anyhow::ensure!(errors == 0, "Scan errors during soak");
                    scans += 1;
                }
                _ => (),
            }
        }
        let elapsed = start.elapsed().as_secs();
        let underruns = engine.state.underruns.load(Ordering::Relaxed);
        if underruns != last_underruns {
            let record = serde_json::json!({"event":"underrun_observed","elapsed_ms":start.elapsed().as_millis(),"underruns":underruns,"while_opening":engine.state.underruns_while_opening.load(Ordering::Relaxed),"decoder_opening":engine.state.decoder_opening.load(Ordering::Relaxed),"max_open_micros":engine.state.max_open_micros.load(Ordering::Relaxed),"max_read_micros":engine.state.max_read_micros.load(Ordering::Relaxed),"while_reading":engine.state.underruns_while_reading.load(Ordering::Relaxed),"decoder_reading":engine.state.decoder_reading.load(Ordering::Relaxed),"audible_index":engine.state.index.load(Ordering::Relaxed),"position":engine.position(),"last_loaded_path":last_loaded_path});
            writeln!(log, "{record}")?;
            log.flush()?;
            println!("{record}");
            last_underruns = underruns;
        }
        let phase = elapsed / 30;
        if phase != last_phase {
            let mut edited = paths.clone();
            if phase.is_multiple_of(2) {
                edited.push(paths[0].clone());
            }
            engine.send(Command::UpdateQueue(edited, None));
            queue_edits += 1;
            engine
                .spectrum
                .shared
                .enabled
                .store(phase.is_multiple_of(2), Ordering::Release);
            let mut eq = crate::dsp::EqSettings {
                enabled: phase.is_multiple_of(3),
                preamp: -6.0,
                ..Default::default()
            };
            eq.gains[5] = 3.0;
            engine.eq.set(eq);
            if phase > 0 && phase.is_multiple_of(3) {
                engine.send(Command::Seek(0.5));
            }
            if phase > 0 && phase.is_multiple_of(5) {
                engine.send(Command::Next);
            }
            if phase.is_multiple_of(4) {
                library.send(Request::Scan(root.clone()));
            }
            last_phase = phase;
        }
        if elapsed / 60 != last_log {
            let memory = process_memory();
            peak_memory = peak_memory.max(memory);
            if elapsed >= 60 && baseline_memory.is_none() {
                baseline_memory = Some(memory);
            }
            let output = engine.state.output_frames.load(Ordering::Relaxed);
            if elapsed >= 60 {
                anyhow::ensure!(output > last_output, "Playback stopped during soak");
            }
            last_output = output;
            let record = serde_json::json!({"elapsed_seconds":elapsed,"output_frames":output,"underruns":engine.state.underruns.load(Ordering::Relaxed),"tracks_loaded":loaded,"scans":scans,"fft_analyses":engine.spectrum.shared.analyses.load(Ordering::Relaxed),"working_set_bytes":memory,"max_open_micros":engine.state.max_open_micros.load(Ordering::Relaxed),"max_read_micros":engine.state.max_read_micros.load(Ordering::Relaxed)});
            writeln!(log, "{record}")?;
            log.flush()?;
            println!("{record}");
            last_log = elapsed / 60;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let underruns = engine.state.underruns.load(Ordering::Relaxed);
    let growth = process_memory().saturating_sub(baseline_memory.unwrap_or(0));
    let passed = underruns == 0 && growth < 64 * 1024 * 1024;
    let record = serde_json::json!({"status":if passed{"passed"}else{"failed"},"duration_seconds":start.elapsed().as_secs(),"underruns":underruns,"memory_growth_after_warmup":growth,"peak_working_set":peak_memory,"tracks_loaded":loaded,"scans":scans,"queue_edits":queue_edits,"scope":"audio+decode+queue edits+EQ+FFT+scan; native window behavior verified separately"});
    writeln!(log, "{record}")?;
    log.sync_all()?;
    println!("{record}");
    anyhow::ensure!(passed, "Soak verification failed");
    Ok(())
}

fn process_memory() -> u64 {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // SAFETY: caller-owned structure and current-process pseudo-handle.
    unsafe {
        let mut info: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        info.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut info, info.cb) != 0 {
            info.WorkingSetSize as u64
        } else {
            0
        }
    }
}

pub fn queue_edit_test(paths: Vec<PathBuf>) -> Result<()> {
    anyhow::ensure!(
        paths.len() == 3,
        "Supply three distinct long audio fixtures"
    );
    let (a, b, c) = (paths[0].clone(), paths[1].clone(), paths[2].clone());
    let engine = AudioEngine::new()?;
    engine.set_volume(0.0);
    engine.send(Command::Queue(vec![a.clone(), b.clone()], 1));
    wait_for(&engine, || engine.position() > 0.25)?;
    let epoch = engine.state.epoch.load(Ordering::Acquire);
    let token = engine.state.token.load(Ordering::Relaxed);
    for i in 0..50 {
        engine.send(Command::UpdateQueue(
            if i % 2 == 0 {
                vec![b.clone(), c.clone(), a.clone()]
            } else {
                vec![a.clone(), b.clone()]
            },
            None,
        ));
        std::thread::sleep(Duration::from_millis(30));
        anyhow::ensure!(
            engine.state.epoch.load(Ordering::Acquire) == epoch,
            "Queue edit discarded active audio buffer at edit {i}"
        );
        anyhow::ensure!(
            engine.state.token.load(Ordering::Relaxed) == token,
            "Queue edit reopened current decoder at edit {i}"
        );
        anyhow::ensure!(
            engine.state.index.load(Ordering::Relaxed) == if i % 2 == 0 { 0 } else { 1 },
            "Buffered frame restored obsolete row index"
        );
    }
    anyhow::ensure!(engine.position() > 1.5, "Playback clock failed to advance");
    println!(
        "{}",
        serde_json::json!({"phase":"edits","underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    engine.send(Command::UpdateQueue(vec![a.clone(), c.clone()], None));
    wait_for(&engine, || {
        engine.state.token.load(Ordering::Relaxed) > token
    })?;
    anyhow::ensure!(
        engine.state.index.load(Ordering::Relaxed) == 1,
        "Deleting current track did not switch to next"
    );
    println!(
        "{}",
        serde_json::json!({"phase":"remove_current","underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    let next_token = engine.state.token.load(Ordering::Relaxed);
    engine.send(Command::UpdateQueue(vec![a.clone()], None));
    wait_for(&engine, || {
        engine.state.token.load(Ordering::Relaxed) > next_token
    })?;
    anyhow::ensure!(
        engine.state.index.load(Ordering::Relaxed) == 0,
        "Deleting last row did not wrap to remaining track"
    );
    println!(
        "{}",
        serde_json::json!({"phase":"remove_last","underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    engine.send(Command::UpdateQueue(vec![], None));
    wait_for(&engine, || engine.state.ended.load(Ordering::Relaxed))?;
    println!(
        "{}",
        serde_json::json!({"phase":"clear","underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    engine.send(Command::Queue(vec![a.clone(), a.clone(), c.clone()], 1));
    wait_for(&engine, || {
        engine.position() > 0.1 && engine.state.index.load(Ordering::Relaxed) == 1
    })?;
    let duplicate_token = engine.state.token.load(Ordering::Relaxed);
    let duplicate_epoch = engine.state.epoch.load(Ordering::Acquire);
    engine.send(Command::UpdateQueue(
        vec![a.clone(), c.clone()],
        Some(crate::audio::QueueEdit::Remove(0)),
    ));
    std::thread::sleep(Duration::from_millis(60));
    anyhow::ensure!(
        engine.state.epoch.load(Ordering::Acquire) == duplicate_epoch
            && engine.state.token.load(Ordering::Relaxed) == duplicate_token
            && engine.state.index.load(Ordering::Relaxed) == 0,
        "Removing another duplicate interrupted current occurrence"
    );
    engine.send(Command::UpdateQueue(
        vec![c.clone()],
        Some(crate::audio::QueueEdit::Remove(0)),
    ));
    wait_for(&engine, || {
        engine.state.token.load(Ordering::Relaxed) > duplicate_token
    })?;
    // Exercise removal of an already-prefetched next track near the boundary.
    let duration = crate::media::MediaReader::open(&a)?.info().duration;
    engine.send(Command::Queue(vec![a.clone(), b, c.clone()], 0));
    wait_for(&engine, || {
        engine.position() > 0.1 && engine.position() < 1.0
    })?;
    engine.send(Command::Seek(duration - 0.3));
    wait_for(&engine, || engine.position() > duration - 0.28)?;
    let boundary_epoch = engine.state.epoch.load(Ordering::Acquire);
    let boundary_token = engine.state.token.load(Ordering::Relaxed);
    engine.send(Command::UpdateQueue(vec![a, c], None));
    wait_for(&engine, || {
        engine.state.token.load(Ordering::Relaxed) > boundary_token
    })?;
    println!(
        "{}",
        serde_json::json!({"phase":"boundary","underruns":engine.state.underruns.load(Ordering::Relaxed)})
    );
    anyhow::ensure!(
        engine.state.epoch.load(Ordering::Acquire) == boundary_epoch,
        "Prefetch removal discarded current tail"
    );
    anyhow::ensure!(
        engine.state.index.load(Ordering::Relaxed) == 1,
        "Prefetch replacement has wrong index"
    );
    // Removing the prefetched last row must still honor repeat-all at the tail.
    let tail = paths[0].clone();
    engine.send(Command::Queue(
        vec![paths[0].clone(), paths[1].clone(), paths[2].clone(), tail],
        2,
    ));
    engine.send(Command::Mode(Repeat::All, false));
    wait_for(&engine, || {
        engine.state.index.load(Ordering::Relaxed) == 2
            && engine.position() > 0.1
            && engine.position() < 1.0
    })?;
    let duration = crate::media::MediaReader::open(&paths[2])?.info().duration;
    engine.send(Command::Seek(duration - 0.3));
    wait_for(&engine, || engine.position() > duration - 0.28)?;
    let token = engine.state.token.load(Ordering::Relaxed);
    engine.send(Command::UpdateQueue(
        paths.clone(),
        Some(crate::audio::QueueEdit::Remove(3)),
    ));
    wait_for(&engine, || {
        engine.state.token.load(Ordering::Relaxed) > token
            && engine.state.index.load(Ordering::Relaxed) == 0
    })?;
    anyhow::ensure!(
        engine.state.playing.load(Ordering::Relaxed),
        "Repeat-all stopped after removing prefetched tail"
    );
    anyhow::ensure!(
        engine.state.underruns.load(Ordering::Relaxed) == 0,
        "Queue edits caused underruns"
    );
    println!(
        "{}",
        serde_json::json!({"queue_edit_test":"passed","edits":50,"checks":["retain decoder and samples","row remapping","delete current","delete last","empty queue","prefetched deletion", "duplicate row identity", "repeat-all after prefetched tail deletion"],"underruns":0})
    );
    Ok(())
}
