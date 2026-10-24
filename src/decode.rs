use anyhow::{anyhow, Context, Result};
use std::{fs::File, path::Path};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::{Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::{MetadataOptions, StandardTagKey},
    probe::Hint,
    units::Time,
};

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TrackInfo {
    pub path: std::path::PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: usize,
    #[serde(skip)]
    pub cover: Option<Vec<u8>>,
}

pub struct AudioReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    pub info: TrackInfo,
    buffer: Option<SampleBuffer<f32>>,
    skip_frames: u64,
}

impl AudioReader {
    pub fn open(path: &Path) -> Result<Self> {
        let (format, info) = open_format(path)?;
        let track = format.default_track().ok_or_else(|| anyhow!("沒有音軌"))?;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        Ok(Self {
            track_id: track.id,
            format,
            decoder,
            info,
            buffer: None,
            skip_frames: 0,
        })
    }

    pub fn metadata(path: &Path) -> Result<TrackInfo> {
        open_format(path).map(|(_, info)| info)
    }

    pub fn seek(&mut self, seconds: f64) -> Result<()> {
        let seconds = seconds.max(0.0);
        let seeked = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::new(seconds.floor() as u64, seconds.fract()),
                track_id: Some(self.track_id),
            },
        )?;
        self.decoder.reset();
        // Timestamp units can differ from PCM frames (notably MP4).
        let track = self
            .format
            .tracks()
            .iter()
            .find(|x| x.id == self.track_id)
            .unwrap();
        self.skip_frames = track
            .codec_params
            .time_base
            .map(|base| {
                let t = base.calc_time(seeked.required_ts.saturating_sub(seeked.actual_ts));
                ((t.seconds as f64 + t.frac) * self.info.sample_rate as f64).round() as u64
            })
            .unwrap_or(0);
        Ok(())
    }

    pub fn next_packet(&mut self) -> Result<Option<Vec<f32>>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = self.decoder.decode(&packet)?;
            let spec = *decoded.spec();
            self.info.sample_rate = spec.rate;
            self.info.channels = spec.channels.count();
            // Packets may have different capacities. Allocate only on format/capacity changes.
            let required = decoded.capacity() * self.info.channels;
            if self.buffer.as_ref().is_none_or(|b| b.capacity() < required) {
                self.buffer = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
            }
            let buffer = self.buffer.as_mut().unwrap();
            buffer.copy_interleaved_ref(decoded);
            let samples = buffer.samples();
            let skip = self
                .skip_frames
                .min((samples.len() / self.info.channels) as u64) as usize;
            self.skip_frames -= skip as u64;
            let result = samples[skip * self.info.channels..].to_vec();
            if !result.is_empty() {
                return Ok(Some(result));
            }
        }
    }
}

fn read_metadata(metadata: &symphonia::core::meta::MetadataRevision, info: &mut TrackInfo) {
    for tag in metadata.tags() {
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => {
                info.title = tag.value.to_string().trim_end_matches('\0').to_owned()
            }
            Some(StandardTagKey::Artist) => {
                info.artist = tag.value.to_string().trim_end_matches('\0').to_owned()
            }
            Some(StandardTagKey::Album) => {
                info.album = tag.value.to_string().trim_end_matches('\0').to_owned()
            }
            _ => (),
        }
    }
    if let Some(visual) = metadata
        .visuals()
        .iter()
        .find(|v| v.data.len() <= 16 * 1024 * 1024)
    {
        info.cover = Some(visual.data.to_vec());
    }
}

fn open_format(path: &Path) -> Result<(Box<dyn FormatReader>, TrackInfo)> {
    let stream = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .with_context(|| format!("無法辨識音訊：{}", path.display()))?;
    let mut info = TrackInfo {
        path: path.to_owned(),
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    if let Some(metadata) = probed.metadata.get().as_ref().and_then(|x| x.current()) {
        read_metadata(metadata, &mut info);
    }
    if let Some(metadata) = probed.format.metadata().current() {
        read_metadata(metadata, &mut info);
    }
    let track = probed
        .format
        .default_track()
        .ok_or_else(|| anyhow!("沒有音軌"))?;
    info.sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    info.channels = track.codec_params.channels.map(|x| x.count()).unwrap_or(2);
    if let (Some(n), Some(base)) = (track.codec_params.n_frames, track.codec_params.time_base) {
        let time = base.calc_time(n);
        info.duration = time.seconds as f64 + time.frac;
    }
    Ok((probed.format, info))
}
