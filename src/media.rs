//! Local-only decoder adapter. External tools never receive URLs.
use crate::decode::{AudioReader, TrackInfo};
use anyhow::{anyhow, Context, Result};
use std::os::windows::process::CommandExt;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
};

pub enum MediaReader {
    Native(AudioReader),
    External(ExternalReader),
}

impl MediaReader {
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_file() {
            return Err(anyhow!("找不到本機檔案：{}", path.display()));
        }
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        // FFmpeg handles MP4 edit lists and AAC encoder delay. Native remains the
        // primary decoder for MP3, FLAC, PCM and Vorbis.
        if matches!(ext.as_str(), "aac" | "m4a" | "mp4") {
            return Ok(Self::External(ExternalReader::open(path, 0.0)?));
        }
        match AudioReader::open(path) {
            Ok(reader) => Ok(Self::Native(reader)),
            Err(native_error) => ExternalReader::open(path, 0.0)
                .map(Self::External)
                .with_context(|| format!("原生解碼失敗：{native_error:#}")),
        }
    }
    pub fn info(&self) -> &TrackInfo {
        match self {
            Self::Native(r) => &r.info,
            Self::External(r) => &r.info,
        }
    }
    pub fn next_packet(&mut self) -> Result<Option<Vec<f32>>> {
        match self {
            Self::Native(r) => r.next_packet(),
            Self::External(r) => r.next_packet(),
        }
    }
    pub fn seek(&mut self, seconds: f64) -> Result<()> {
        match self {
            Self::Native(r) => r.seek(seconds),
            Self::External(r) => {
                *r = ExternalReader::open(&r.path, seconds)?;
                Ok(())
            }
        }
    }
}

pub fn tool_path(name: &str) -> Result<PathBuf> {
    let root = std::env::var_os("EPLAYER_MEDIA_TOOLS")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.parent().unwrap().join("tools"));
    let path = root.join(format!("{name}.exe"));
    if !path.is_file() {
        return Err(anyhow!("缺少隨附工具：{}", path.display()));
    }
    Ok(path)
}

fn local_command(name: &str) -> Result<Command> {
    let mut command = Command::new(tool_path(name)?);
    command.creation_flags(0x08000000).stdin(Stdio::null());
    Ok(command)
}

pub struct ExternalReader {
    path: PathBuf,
    child: Child,
    output: ChildStdout,
    pub info: TrackInfo,
    ended: bool,
}

impl ExternalReader {
    fn open(path: &Path, seconds: f64) -> Result<Self> {
        let output = local_command("ffprobe")?
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-select_streams",
                "a:0",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(path)
            .output()?;
        if !output.status.success() {
            return Err(anyhow!(
                "無法讀取音訊資訊：{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let stream = json["streams"]
            .as_array()
            .and_then(|x| x.first())
            .ok_or_else(|| anyhow!("沒有音軌"))?;
        let tag = |key: &str| {
            [&json["format"]["tags"], &stream["tags"]]
                .into_iter()
                .filter_map(serde_json::Value::as_object)
                .flat_map(|tags| tags.iter())
                .find_map(|(name, value)| {
                    if name.eq_ignore_ascii_case(key) {
                        value.as_str().filter(|value| !value.is_empty())
                    } else {
                        None
                    }
                })
        };
        let mut info = AudioReader::metadata(path).unwrap_or_default();
        info.path = path.to_owned();
        info.title = tag("title")
            .map(str::to_owned)
            .filter(|x| !x.is_empty())
            .unwrap_or_else(|| {
                if info.title.is_empty() {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                } else {
                    info.title.clone()
                }
            });
        info.artist = tag("artist").unwrap_or(&info.artist).to_owned();
        info.album = tag("album").unwrap_or(&info.album).to_owned();
        info.duration = stream["duration"]
            .as_str()
            .or(json["format"]["duration"].as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        info.sample_rate = 48000;
        info.channels = 2;
        let mut command = local_command("ffmpeg")?;
        command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-i",
            ])
            .arg(path);
        // Output-side seek preserves decoder priming/edit lists. Input-side -ss 0
        // was experimentally found to remove an extra AAC frame in FFmpeg 9.
        if seconds > 0.0 {
            command.args(["-ss", &format!("{seconds:.6}")]);
        }
        let mut child = command
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let output = child.stdout.take().context("無法建立解碼管線")?;
        Ok(Self {
            path: path.to_owned(),
            child,
            output,
            info,
            ended: false,
        })
    }
    fn next_packet(&mut self) -> Result<Option<Vec<f32>>> {
        if self.ended {
            return Ok(None);
        }
        let mut bytes = vec![0u8; 4096 * 8];
        let mut count = 0;
        while count < bytes.len() {
            match self.output.read(&mut bytes[count..]) {
                Ok(0) => {
                    self.ended = true;
                    break;
                }
                Ok(n) => count += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        if self.ended && !self.child.wait()?.success() {
            return Err(anyhow!("外部音訊解碼失敗"));
        }
        if count == 0 {
            return Ok(None);
        }
        if count % 8 != 0 {
            return Err(anyhow!("不完整的音訊樣本"));
        }
        Ok(Some(
            bytes[..count]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect(),
        ))
    }
}

impl Drop for ExternalReader {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub fn probe(path: &Path) -> Result<serde_json::Value> {
    let mut reader = MediaReader::open(path)?;
    let mut count = 0u64;
    let mut peak = 0.0f32;
    while let Some(packet) = reader.next_packet()? {
        count += packet.len() as u64;
        for sample in packet {
            if !sample.is_finite() {
                return Err(anyhow!("Non-finite decoded sample"));
            }
            peak = peak.max(sample.abs());
        }
    }
    Ok(
        serde_json::json!({"path":path,"info":reader.info(),"decoded_frames":count / reader.info().channels as u64,"peak":peak,"cover_bytes":reader.info().cover.as_ref().map_or(0, Vec::len)}),
    )
}
