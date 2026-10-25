use crate::{decode::TrackInfo, media::MediaReader};
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};
use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

#[derive(Clone, Copy)]
pub enum QueueEdit {
    Remove(usize),
    Swap(usize, usize),
}

pub enum Command {
    Queue(Vec<PathBuf>, usize),
    UpdateQueue(Vec<PathBuf>, Option<QueueEdit>),
    Seek(f64),
    Restart,
    Next,
    Previous,
    Mode(Repeat, bool),
    Shutdown,
}

pub enum Event {
    Loaded {
        token: u64,
        index: usize,
        info: TrackInfo,
    },
    Error(String),
}

#[derive(Default)]
pub struct PlaybackState {
    pub playing: AtomicBool,
    pub epoch: AtomicU64,
    pub token: AtomicU64,
    pub index: AtomicUsize,
    pub position_frames: AtomicU64,
    pub volume: AtomicU32,
    pub underruns: AtomicU64,
    pub underruns_while_opening: AtomicU64,
    pub decoder_opening: AtomicBool,
    pub max_open_micros: AtomicU64,
    pub decoder_reading: AtomicBool,
    pub max_read_micros: AtomicU64,
    pub underruns_while_reading: AtomicU64,
    pub output_frames: AtomicU64,
    pub finished: AtomicBool,
    pub ended: AtomicBool,
    pub ready: AtomicBool,
}

fn next_queue_index(index: usize, len: usize, repeat: Repeat, shuffle: bool) -> Option<usize> {
    if len == 0 {
        None
    } else if repeat == Repeat::One {
        Some(index.min(len - 1))
    } else if shuffle && len > 1 {
        Some((index + rand::random_range(1..len)) % len)
    } else if index + 1 < len {
        Some(index + 1)
    } else if repeat == Repeat::All {
        Some(0)
    } else {
        None
    }
}

struct TrackSlot {
    path: PathBuf,
    index: AtomicUsize,
    active: AtomicBool,
}

// Metadata is shared by buffered frames, so an edit can remap their row index
// without discarding samples or seeking the decoder. The callback never locks.
fn reconcile_slots(old: &[Arc<TrackSlot>], paths: &[PathBuf]) -> Vec<Arc<TrackSlot>> {
    let mut available: std::collections::HashMap<
        &std::path::Path,
        std::collections::VecDeque<&Arc<TrackSlot>>,
    > = std::collections::HashMap::new();
    for slot in old {
        available
            .entry(slot.path.as_path())
            .or_default()
            .push_back(slot);
    }
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let slot = available
                .get_mut(path.as_path())
                .and_then(|slots| slots.pop_front())
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(TrackSlot {
                        path: path.clone(),
                        index: AtomicUsize::new(index),
                        active: AtomicBool::new(true),
                    })
                });
            slot.index.store(index, Ordering::Relaxed);
            slot
        })
        .collect()
}

#[derive(Clone)]
struct Frame {
    samples: [f32; 2],
    epoch: u64,
    token: u64,
    slot: Arc<TrackSlot>,
    position: u64,
}

pub struct AudioEngine {
    pub commands: Sender<Command>,
    pub events: Receiver<Event>,
    pub state: Arc<PlaybackState>,
    pub sample_rate: u32,
    pub device_name: String,
    _stream: cpal::Stream,
    worker: Option<thread::JoinHandle<()>>,
    pub eq: Arc<crate::dsp::EqControls>,
    pub spectrum: crate::spectrum::Spectrum,
}

impl AudioEngine {
    pub fn new() -> Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("沒有可用音訊輸出裝置"))?;
        let config = device.default_output_config()?;
        let rate = config.sample_rate().0;
        let state = Arc::new(PlaybackState::default());
        state.volume.store(0.7f32.to_bits(), Ordering::Relaxed);
        let (producer, consumer) = HeapRb::<Frame>::new(rate as usize / 2).split();
        let (commands, rx) = crossbeam_channel::unbounded();
        let (tx, events) = crossbeam_channel::unbounded();
        let eq = Arc::new(crate::dsp::EqControls::default());
        let (spectrum, spectrum_output) = crate::spectrum::Spectrum::new(rate);
        let pipeline = OutputPipeline {
            consumer,
            state: state.clone(),
            errors: tx.clone(),
            eq: eq.clone(),
            spectrum: spectrum.shared.clone(),
            spectrum_output,
        };
        let stream_config = config.clone().into();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => make_stream::<f32>(&device, &stream_config, pipeline)?,
            cpal::SampleFormat::I16 => make_stream::<i16>(&device, &stream_config, pipeline)?,
            cpal::SampleFormat::U16 => make_stream::<u16>(&device, &stream_config, pipeline)?,
            cpal::SampleFormat::I32 => make_stream::<i32>(&device, &stream_config, pipeline)?,
            cpal::SampleFormat::F64 => make_stream::<f64>(&device, &stream_config, pipeline)?,
            x => return Err(anyhow!("不支援的輸出樣本格式：{x:?}")),
        };
        stream.play()?;
        let worker_state = state.clone();
        let worker = thread::Builder::new()
            .name("audio-decode".into())
            .spawn(move || {
                decode_worker(producer, worker_state, rx, tx, rate);
            })?;
        Ok(Self {
            commands,
            events,
            state,
            sample_rate: rate,
            device_name: device.name()?,
            _stream: stream,
            worker: Some(worker),
            eq,
            spectrum,
        })
    }
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
    pub fn toggle(&self) {
        self.state.playing.fetch_xor(true, Ordering::Relaxed);
    }
    pub fn set_volume(&self, value: f32) {
        self.state
            .volume
            .store(value.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    pub fn position(&self) -> f64 {
        self.state.position_frames.load(Ordering::Relaxed) as f64 / self.sample_rate as f64
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.state.playing.store(false, Ordering::Relaxed);
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct OutputPipeline {
    consumer: ringbuf::HeapCons<Frame>,
    state: Arc<PlaybackState>,
    errors: Sender<Event>,
    eq: Arc<crate::dsp::EqControls>,
    spectrum: Arc<crate::spectrum::Shared>,
    spectrum_output: ringbuf::HeapProd<f32>,
}
fn make_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    pipeline: OutputPipeline,
) -> Result<cpal::Stream> {
    let OutputPipeline {
        mut consumer,
        state,
        errors,
        eq,
        spectrum,
        mut spectrum_output,
    } = pipeline;
    let channels = config.channels as usize;
    let mut equalizer = crate::dsp::Equalizer::new(config.sample_rate.0);
    Ok(device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            let epoch = state.epoch.load(Ordering::Acquire);
            let playing = state.playing.load(Ordering::Relaxed);
            let ready = state.ready.load(Ordering::Acquire);
            let volume = f32::from_bits(state.volume.load(Ordering::Relaxed));
            equalizer.prepare(&eq, output.len() / channels);
            let analyze = spectrum.enabled.load(Ordering::Acquire);
            let mut missing = false;
            let mut emitted = 0;
            for out in output.chunks_mut(channels) {
                // Drain only older generations, even while paused/buffering.
                // A seek may publish a newer generation during this callback;
                // leave those frames for the next callback instead of losing them.
                while consumer
                    .try_peek()
                    .is_some_and(|f| f.epoch < epoch || !f.slot.active.load(Ordering::Acquire))
                {
                    consumer.try_pop();
                }
                let frame =
                    if playing && ready && consumer.try_peek().is_some_and(|f| f.epoch == epoch) {
                        consumer.try_pop()
                    } else {
                        None
                    };
                let stereo = if let Some(frame) = &frame {
                    state.token.store(frame.token, Ordering::Relaxed);
                    state
                        .index
                        .store(frame.slot.index.load(Ordering::Relaxed), Ordering::Relaxed);
                    state
                        .position_frames
                        .store(frame.position + 1, Ordering::Relaxed);
                    emitted += 1;
                    frame.samples
                } else {
                    if playing && ready {
                        missing = true;
                    }
                    [0.0; 2]
                };
                let stereo = equalizer.process(stereo);
                if analyze && frame.is_some() {
                    let _ = spectrum_output.try_push((stereo[0] + stereo[1]) * 0.5 * volume);
                }
                for (channel, sample) in out.iter_mut().enumerate() {
                    let value = if channels == 1 {
                        (stereo[0] + stereo[1]) * 0.5
                    } else if channel < 2 {
                        stereo[channel]
                    } else {
                        0.0
                    };
                    *sample = T::from_sample((value * volume).clamp(-1.0, 1.0));
                }
            }
            state.output_frames.fetch_add(emitted, Ordering::Relaxed);
            if missing && state.epoch.load(Ordering::Acquire) == epoch {
                if state.finished.load(Ordering::Acquire) {
                    state.playing.store(false, Ordering::Relaxed);
                    state.ended.store(true, Ordering::Relaxed);
                } else {
                    state.underruns.fetch_add(1, Ordering::Relaxed);
                    if state.decoder_opening.load(Ordering::Relaxed) {
                        state
                            .underruns_while_opening
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    if state.decoder_reading.load(Ordering::Relaxed) {
                        state
                            .underruns_while_reading
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        },
        move |error| {
            let _ = errors.send(Event::Error(format!("音訊裝置錯誤：{error}")));
        },
        None,
    )?)
}

// Holds only the current decoded packet and two interpolation frames.
struct Resampled {
    diagnostics: Option<Arc<PlaybackState>>,
    reader: MediaReader,
    packet: Vec<f32>,
    cursor: usize,
    a: Option<[f32; 2]>,
    b: Option<[f32; 2]>,
    phase: u64,
    step: u64,
    output_rate: u64,
}
impl Resampled {
    fn new(reader: MediaReader, rate: u32) -> Self {
        let step = reader.info().sample_rate as u64;
        Self {
            diagnostics: None,
            reader,
            packet: Vec::new(),
            cursor: 0,
            a: None,
            b: None,
            phase: 0,
            step,
            output_rate: rate as u64,
        }
    }
    fn input(&mut self) -> Result<Option<[f32; 2]>> {
        if self.cursor >= self.packet.len() {
            let read_start = self.diagnostics.as_ref().map(|state| {
                state.decoder_reading.store(true, Ordering::Relaxed);
                std::time::Instant::now()
            });
            let packet = self.reader.next_packet();
            if let (Some(state), Some(start)) = (&self.diagnostics, read_start) {
                state
                    .max_read_micros
                    .fetch_max(start.elapsed().as_micros() as u64, Ordering::Relaxed);
                state.decoder_reading.store(false, Ordering::Relaxed);
            }
            match packet? {
                Some(packet) => {
                    self.packet = packet;
                    self.cursor = 0;
                }
                None => return Ok(None),
            }
        }
        let channels = self.reader.info().channels;
        let slice = &self.packet[self.cursor..self.cursor + channels];
        let value = if channels == 1 {
            [slice[0]; 2]
        } else {
            [slice[0], slice[1]]
        };
        self.cursor += channels;
        Ok(Some(value))
    }
    fn next(&mut self) -> Result<Option<[f32; 2]>> {
        if self.a.is_none() {
            self.a = self.input()?;
            self.b = self.input()?;
        }
        let Some(a) = self.a else { return Ok(None) };
        let b = self.b.unwrap_or(a);
        let result = [
            a[0] + (b[0] - a[0]) * (self.phase as f32 / self.output_rate as f32),
            a[1] + (b[1] - a[1]) * (self.phase as f32 / self.output_rate as f32),
        ];
        self.phase += self.step;
        while self.phase >= self.output_rate {
            self.phase -= self.output_rate;
            self.a = self.b;
            self.b = if self.a.is_some() {
                self.input()?
            } else {
                None
            };
        }
        Ok(Some(result))
    }
}

pub fn render_probe(output: PathBuf, paths: Vec<PathBuf>) -> Result<()> {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(output)?);
    let mut reports = Vec::new();
    for path in paths {
        let mut source = Resampled::new(MediaReader::open(&path)?, 48000);
        let mut frames = 0u64;
        while let Some(frame) = source.next()? {
            for sample in frame {
                file.write_all(&sample.to_le_bytes())?;
            }
            frames += 1;
        }
        reports.push(serde_json::json!({"path":path,"frames":frames}));
    }
    file.flush()?;
    println!(
        "{}",
        serde_json::json!({"sample_rate":48000,"tracks":reports})
    );
    Ok(())
}

fn decode_worker(
    mut output: ringbuf::HeapProd<Frame>,
    state: Arc<PlaybackState>,
    commands: Receiver<Command>,
    events: Sender<Event>,
    rate: u32,
) {
    let mut queue: Vec<PathBuf> = Vec::new();
    let mut slots: Vec<Arc<TrackSlot>> = Vec::new();
    // Keep removed metadata alive until the worker can reclaim it; the real-time
    // callback must not be responsible for freeing the last Arc/path allocation.
    let mut retired: Vec<Arc<TrackSlot>> = Vec::new();
    let mut index = 0;
    let mut reader: Option<Resampled> = None;
    let mut token = 0;
    let mut position = 0;
    let mut epoch = 0;
    let mut repeat = Repeat::Off;
    let mut shuffle = false;
    let mut load: Option<f64> = None;
    let mut pending = None;
    let mut buffered_since_reset = 0usize;
    loop {
        while let Some(command) = pending.take().or_else(|| commands.try_recv().ok()) {
            let mut reset = false;
            match command {
                Command::Shutdown => return,
                Command::UpdateQueue(paths, edit) => {
                    let old_index = state.index.load(Ordering::Relaxed);
                    let audible = slots.get(old_index).cloned();
                    let decoding = slots.get(index).cloned();
                    let mut candidates = slots.clone();
                    match edit {
                        Some(QueueEdit::Remove(row)) if row < candidates.len() => {
                            candidates.remove(row);
                        }
                        Some(QueueEdit::Swap(a, b))
                            if a < candidates.len() && b < candidates.len() =>
                        {
                            candidates.swap(a, b)
                        }
                        _ => (),
                    }
                    let replacement = reconcile_slots(&candidates, &paths);
                    let retained = audible.as_ref().and_then(|current| {
                        replacement.iter().position(|s| Arc::ptr_eq(s, current))
                    });
                    let decoder_retained = decoding.as_ref().and_then(|current| {
                        replacement.iter().position(|s| Arc::ptr_eq(s, current))
                    });
                    let retained_slots: std::collections::HashSet<_> =
                        replacement.iter().map(Arc::as_ptr).collect();
                    for slot in slots.drain(..) {
                        if !retained_slots.contains(&Arc::as_ptr(&slot)) {
                            // A reset invalidates the entire old epoch atomically.
                            // Do not empty its frames before reset is published,
                            // especially while closing an external decoder.
                            if retained.is_some() {
                                slot.active.store(false, Ordering::Release);
                            }
                            retired.push(slot);
                        }
                    }
                    slots = replacement;
                    queue = paths;
                    if let Some(current) = retained {
                        state.index.store(current, Ordering::Relaxed);
                        if let Some(decoder_index) = decoder_retained {
                            index = decoder_index;
                            if reader.is_none()
                                && load.is_none()
                                && state.finished.load(Ordering::Acquire)
                                && !state.ended.load(Ordering::Relaxed)
                            {
                                if let Some(next) =
                                    next_queue_index(index, queue.len(), repeat, shuffle)
                                {
                                    index = next;
                                    load = Some(0.0);
                                    state.finished.store(false, Ordering::Release);
                                }
                            }
                        } else {
                            // Only the prefetched next track was deleted. Buffered
                            // current-track audio stays valid while its replacement loads.
                            reader = None;
                            index = current;
                            load = None;
                            if let Some(next) =
                                next_queue_index(current, queue.len(), repeat, shuffle)
                            {
                                index = next;
                                load = Some(0.0);
                                state.finished.store(false, Ordering::Release);
                            } else {
                                state.finished.store(true, Ordering::Release);
                            }
                        }
                    } else {
                        // User requested immediate next-track playback when removing
                        // the current row. Empty queues stop through the normal reset.
                        index = if queue.is_empty() {
                            0
                        } else {
                            old_index % queue.len()
                        };
                        load = Some(0.0);
                        reset = true;
                    }
                }
                Command::Queue(paths, start) => {
                    retired.append(&mut slots);
                    slots = reconcile_slots(&[], &paths);
                    queue = paths;
                    index = start.min(queue.len().saturating_sub(1));
                    load = Some(0.0);
                    reset = true;
                    state.playing.store(true, Ordering::Relaxed);
                }
                Command::Restart => {
                    index = state
                        .index
                        .load(Ordering::Relaxed)
                        .min(queue.len().saturating_sub(1));
                    load = Some(0.0);
                    reset = true;
                    state.playing.store(true, Ordering::Relaxed);
                }
                Command::Seek(seconds) => {
                    index = state
                        .index
                        .load(Ordering::Relaxed)
                        .min(queue.len().saturating_sub(1));
                    load = Some(seconds);
                    reset = true;
                }
                Command::Next => {
                    if !queue.is_empty() {
                        index = (state.index.load(Ordering::Relaxed) + 1) % queue.len();
                        load = Some(0.0);
                        reset = true;
                    }
                }
                Command::Previous => {
                    if !queue.is_empty() {
                        index =
                            (state.index.load(Ordering::Relaxed) + queue.len() - 1) % queue.len();
                        load = Some(0.0);
                        reset = true;
                    }
                }
                Command::Mode(r, s) => {
                    repeat = r;
                    shuffle = s;
                    if reader.is_none()
                        && load.is_none()
                        && state.finished.load(Ordering::Acquire)
                        && !state.ended.load(Ordering::Relaxed)
                        && !queue.is_empty()
                    {
                        let next = if repeat == Repeat::One {
                            Some(index)
                        } else if shuffle && queue.len() > 1 {
                            Some((index + rand::random_range(1..queue.len())) % queue.len())
                        } else if repeat == Repeat::All {
                            Some(0)
                        } else {
                            None
                        };
                        if let Some(next) = next {
                            index = next;
                            load = Some(0.0);
                            state.finished.store(false, Ordering::Release);
                        }
                    }
                }
            }
            retired.retain(|slot| Arc::strong_count(slot) > 1);
            if reset {
                state.ready.store(false, Ordering::Release);
                epoch = state.epoch.fetch_add(1, Ordering::AcqRel) + 1;
                reader = None;
                buffered_since_reset = 0;
                state.finished.store(false, Ordering::Release);
                state.ended.store(false, Ordering::Relaxed);
                // Paused playback does not consume frames, so publish the seek
                // position now rather than waiting for the output callback.
                state.position_frames.store(
                    (load.unwrap_or(0.0) * rate as f64) as u64,
                    Ordering::Relaxed,
                );
                state.index.store(index, Ordering::Relaxed);
            }
        }
        if let Some(seconds) = load.take() {
            if let Some(path) = queue.get(index) {
                state.decoder_opening.store(true, Ordering::Relaxed);
                let open_start = std::time::Instant::now();
                let opened = MediaReader::open(path).and_then(|mut r| {
                    if seconds > 0.0 {
                        r.seek(seconds)?;
                    }
                    Ok(r)
                });
                state
                    .max_open_micros
                    .fetch_max(open_start.elapsed().as_micros() as u64, Ordering::Relaxed);
                state.decoder_opening.store(false, Ordering::Relaxed);
                match opened {
                    Ok(r) => {
                        token += 1;
                        let _ = events.send(Event::Loaded {
                            token,
                            index,
                            info: r.info().clone(),
                        });
                        let mut source = Resampled::new(r, rate);
                        source.diagnostics = Some(state.clone());
                        reader = Some(source);
                        position = (seconds * rate as f64) as u64;
                    }
                    Err(e) => {
                        let _ = events.send(Event::Error(format!("{}：{e:#}", path.display())));
                        state.playing.store(false, Ordering::Relaxed);
                        state.finished.store(true, Ordering::Release);
                    }
                }
            } else {
                state.playing.store(false, Ordering::Relaxed);
                state.finished.store(true, Ordering::Release);
                state.ended.store(true, Ordering::Relaxed);
            }
        }
        if output.is_full() || reader.is_none() {
            pending = commands.recv_timeout(Duration::from_millis(10)).ok();
            continue;
        }
        match reader.as_mut().unwrap().next() {
            Ok(Some(samples)) => {
                let _ = output.try_push(Frame {
                    samples,
                    epoch,
                    token,
                    slot: slots[index].clone(),
                    position,
                });
                position += 1;
                buffered_since_reset += 1;
                // Start only after initial buffering, to avoid counting startup as underrun.
                if buffered_since_reset >= (rate / 10) as usize {
                    state.ready.store(true, Ordering::Release);
                }
            }
            Ok(None) => {
                reader = None;
                state.ready.store(true, Ordering::Relaxed);
                let next = next_queue_index(index, queue.len(), repeat, shuffle);
                if let Some(next) = next {
                    index = next;
                    load = Some(0.0);
                } else {
                    state.finished.store(true, Ordering::Release);
                }
            }
            Err(e) => {
                let _ = events.send(Event::Error(format!("音訊解碼錯誤：{e:#}")));
                reader = None;
                state.finished.store(true, Ordering::Release);
            }
        }
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    #[test]
    fn replacement_prefetch_uses_same_repeat_rules_as_normal_eof() {
        assert_eq!(next_queue_index(2, 3, Repeat::All, false), Some(0));
        assert_eq!(next_queue_index(2, 3, Repeat::One, false), Some(2));
        assert_eq!(next_queue_index(2, 3, Repeat::Off, false), None);
        assert_eq!(next_queue_index(0, 0, Repeat::All, false), None);
    }
    #[test]
    fn retained_buffer_metadata_tracks_edits_and_duplicate_occurrences() {
        let a = PathBuf::from("a.wav");
        let b = PathBuf::from("b.wav");
        let old = reconcile_slots(&[], &[a.clone(), b.clone(), a.clone()]);
        let buffered = old[1].clone();
        let edited = reconcile_slots(&old, &[b, a.clone(), a]);
        assert!(Arc::ptr_eq(&buffered, &edited[0]));
        assert_eq!(buffered.index.load(Ordering::Relaxed), 0);
        assert!(Arc::ptr_eq(&old[0], &edited[1]));
        assert!(Arc::ptr_eq(&old[2], &edited[2]));
    }
}
