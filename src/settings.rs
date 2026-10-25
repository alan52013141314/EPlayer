use crate::audio::Repeat;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
    pub mini: bool,
    pub compact: bool,
    pub volume: f32,
    pub repeat: Repeat,
    pub shuffle: bool,
    pub playlist: i64,
    pub roots: Vec<PathBuf>,
    pub lyrics: bool,
    pub mini_window: WindowSettings,
    pub lyrics_window: WindowSettings,
    pub eq: crate::dsp::EqSettings,
    pub spectrum: bool,
    pub library_double_click_enqueue: bool,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub position: Option<[f32; 2]>,
    pub size: [f32; 2],
    pub opacity: f32,
    pub topmost: bool,
    #[serde(skip)]
    pub observed_position: Option<[f32; 2]>,
    #[serde(skip)]
    pub observed_size: Option<[f32; 2]>,
    #[serde(skip)]
    pub resize_anchor: Option<([f32; 2], [f32; 2])>,
    #[serde(skip)]
    pub position_initialized: bool,
    #[serde(skip)]
    pub pending_drag: Option<([f32; 2], [f32; 2])>,
}
impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            position: None,
            size: [440.0, 260.0],
            opacity: 0.94,
            topmost: true,
            observed_position: None,
            observed_size: None,
            resize_anchor: None,
            position_initialized: false,
            pending_drag: None,
        }
    }
}
impl WindowSettings {
    pub fn observe_geometry(&mut self, position: Option<[f32; 2]>, size: Option<[f32; 2]>) {
        if let Some(actual) = position {
            if self.observed_position != Some(actual) {
                self.position = Some(actual);
            }
            self.observed_position = Some(actual);
        }
        if let Some(actual) = size {
            if self.observed_size != Some(actual) {
                self.size = actual;
            }
            self.observed_size = Some(actual);
        }
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            dark: true,
            mini: false,
            compact: false,
            volume: 0.7,
            repeat: Repeat::Off,
            shuffle: false,
            playlist: 1,
            roots: Vec::new(),
            lyrics: false,
            mini_window: WindowSettings::default(),
            lyrics_window: WindowSettings {
                size: [600.0, 130.0],
                ..Default::default()
            },
            eq: Default::default(),
            spectrum: false,
            library_double_click_enqueue: false,
        }
    }
}

pub fn data_dir() -> Result<PathBuf> {
    let dir = std::env::var_os("EPLAYER_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or(
            std::env::current_exe()?
                .parent()
                .context("缺少程式目錄")?
                .join("data"),
        );
    std::fs::create_dir_all(&dir).context("無法建立 data 資料夾，請將 EPlayer 搬到可寫入的位置")?;
    Ok(dir)
}
pub fn load(dir: &Path) -> Result<Settings> {
    let path = dir.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let mut settings: Settings = serde_json::from_slice(&std::fs::read(path)?)?;
    if !settings.volume.is_finite() {
        settings.volume = 0.7;
    }
    settings.volume = settings.volume.clamp(0.0, 1.0);
    Ok(settings)
}
pub fn save(dir: &Path, settings: &Settings) -> Result<()> {
    let temp = dir.join("settings.tmp");
    let file = std::fs::File::create(&temp)?;
    serde_json::to_writer_pretty(&file, settings)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(temp, dir.join("settings.json"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_settings_default_to_direct_library_playback() {
        let legacy: Settings = serde_json::from_str("{\"dark\":false}").unwrap();
        assert!(!legacy.library_double_click_enqueue);
    }
    #[test]
    fn delayed_os_geometry_does_not_undo_preset_or_position_reset() {
        let mut window = WindowSettings::default();
        window.observe_geometry(Some([10.0, 20.0]), Some([380.0, 105.0]));
        window.size = [440.0, 300.0];
        window.position = Some([80.0, 80.0]);
        // winit can report the old bounds during the frame issuing the request.
        window.observe_geometry(Some([10.0, 20.0]), Some([380.0, 105.0]));
        assert_eq!(window.size, [440.0, 300.0]);
        assert_eq!(window.position, Some([80.0, 80.0]));
        window.observe_geometry(Some([80.0, 80.0]), Some([440.0, 300.0]));
        // Subsequent user moves/resizes must still be accepted.
        window.observe_geometry(Some([120.0, 90.0]), Some([420.0, 280.0]));
        assert_eq!(window.size, [420.0, 280.0]);
        assert_eq!(window.position, Some([120.0, 90.0]));
    }
    #[test]
    fn settings_replace_and_restart() {
        let dir = std::env::temp_dir().join(format!("eplayer-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut settings = Settings::default();
        save(&dir, &settings).unwrap();
        settings.dark = false;
        settings.library_double_click_enqueue = true;
        settings.volume = 0.23;
        settings.roots.push(PathBuf::from("C:/音樂"));
        save(&dir, &settings).unwrap();
        let restored = load(&dir).unwrap();
        assert!(!restored.dark);
        assert!(restored.library_double_click_enqueue);
        assert_eq!(restored.volume, 0.23);
        assert_eq!(restored.roots, settings.roots);
        std::fs::remove_file(dir.join("settings.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
