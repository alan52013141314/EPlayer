use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

// File dialogs and library scans can return different Windows path forms for
// the same file (notably the extended-length \\?\ prefix).
pub fn source_key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_owned())
}

#[derive(Clone, Debug, Default)]
pub struct Lyrics {
    pub lines: Vec<Line>,
}
#[derive(Clone, Debug)]
pub struct Line {
    pub seconds: f64,
    pub text: String,
}
impl Lyrics {
    pub fn parse(text: &str) -> Self {
        let mut lines = Vec::new();
        let mut offset = 0.0;
        for raw in text.lines() {
            let mut text = raw.trim_start_matches('\u{feff}').trim();
            let mut times = Vec::new();
            while let Some(rest) = text.strip_prefix('[') {
                let Some(end) = rest.find(']') else { break };
                let tag = &rest[..end];
                text = &rest[end + 1..];
                if let Some(value) = tag.strip_prefix("offset:") {
                    if let Ok(ms) = value.trim().parse::<f64>() {
                        if ms.is_finite() {
                            offset = ms / 1000.0;
                        }
                    }
                } else if let Some((minutes, seconds)) = tag.split_once(':') {
                    if let (Ok(m), Ok(s)) = (minutes.parse::<u32>(), seconds.parse::<f64>()) {
                        if s.is_finite() && (0.0..60.0).contains(&s) {
                            times.push(m as f64 * 60.0 + s);
                        }
                    }
                }
            }
            for seconds in times {
                lines.push(Line {
                    seconds,
                    text: text.trim().to_owned(),
                });
            }
        }
        for line in &mut lines {
            line.seconds -= offset;
        }
        lines.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
        Self { lines }
    }
    pub fn load(path: &Path) -> Result<Self> {
        if std::fs::metadata(path)?.len() > 4 * 1024 * 1024 {
            bail!("LRC 檔案過大");
        }
        let bytes = std::fs::read(path)?;
        let text = if bytes.starts_with(&[0xff, 0xfe]) {
            encoding_rs::UTF_16LE.decode(&bytes[2..]).0.into_owned()
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            encoding_rs::UTF_16BE.decode(&bytes[2..]).0.into_owned()
        } else if let Ok(text) = std::str::from_utf8(&bytes) {
            text.to_owned()
        } else {
            let (big5, _, bad) = encoding_rs::BIG5.decode(&bytes);
            if bad {
                encoding_rs::GBK.decode(&bytes).0.into_owned()
            } else {
                big5.into_owned()
            }
        };
        Ok(Self::parse(&text))
    }
    pub fn current_index(&self, seconds: f64) -> Option<usize> {
        self.lines
            .partition_point(|line| line.seconds <= seconds)
            .checked_sub(1)
    }
    pub fn current(&self, seconds: f64) -> &str {
        self.current_index(seconds)
            .map(|i| self.lines[i].text.as_str())
            .unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_dialog_and_library_paths_share_lyric_identity() {
        let path =
            std::env::temp_dir().join(format!("eplayer-lyric-identity-{}.wav", std::process::id()));
        std::fs::write(&path, []).unwrap();
        let canonical = path.canonicalize().unwrap();
        let mut selections = std::collections::BTreeMap::new();
        selections.insert(source_key(&canonical), PathBuf::from("selected.lrc"));
        assert_eq!(
            selections.get(&source_key(&path)),
            Some(&PathBuf::from("selected.lrc"))
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn multiple_times_and_late_offset() {
        let l = Lyrics::parse("[ar:歌手]\n[00:02.50][00:01.20]你好\n[00:03.00]\n[offset:+200]");
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.current(0.9), "");
        assert_eq!(l.current(1.0), "你好");
        assert_eq!(l.current(2.8), "");
    }
    #[test]
    fn seeking_is_independent_of_ui_history() {
        let l = Lyrics::parse("[00:00]A\n[00:01.00]B\n[00:02.0]C");
        assert_eq!(l.current(2.5), "C");
        assert_eq!(l.current(0.5), "A");
        assert_eq!(l.current(1.5), "B");
    }
    #[test]
    fn malformed_timestamps_do_not_poison_sort() {
        let l = Lyrics::parse("[00:NaN]bad\n[00:90]bad\n[xx:01]bad\n[00:01.25]ok");
        assert_eq!(l.lines.len(), 1);
        assert_eq!(l.current(2.0), "ok");
    }
    #[test]
    fn file_encodings_preserve_chinese_text_and_seek_times() {
        let text = "[00:00.50]本機歌詞\n[00:02.00]播放測試";
        let mut utf16le = vec![0xff, 0xfe];
        let mut utf16be = vec![0xfe, 0xff];
        for unit in text.encode_utf16() {
            utf16le.extend(unit.to_le_bytes());
            utf16be.extend(unit.to_be_bytes());
        }
        let (big5, _, errors) = encoding_rs::BIG5.encode(text);
        assert!(!errors);
        let path =
            std::env::temp_dir().join(format!("eplayer-lrc-encoding-{}.lrc", std::process::id()));
        for bytes in [
            text.as_bytes().to_vec(),
            utf16le,
            utf16be,
            big5.into_owned(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            let lyrics = Lyrics::load(&path).unwrap();
            assert_eq!(lyrics.current(2.5), "播放測試");
            assert_eq!(lyrics.current(0.0), "");
            assert_eq!(lyrics.current(1.0), "本機歌詞");
        }
        std::fs::remove_file(path).unwrap();
    }
}
