use crate::widgets::{button, enabled_button};
use crate::{
    audio::{AudioEngine, Command, Event, Repeat},
    decode::TrackInfo,
};
use crate::{
    library::{Library, Playlist, Request, Response, Track},
    settings::{self, Settings},
};
use eframe::egui::{self, Color32, RichText};
use std::{collections::BTreeMap, path::PathBuf, sync::atomic::Ordering, time::Duration};

pub struct PlayerApp {
    engine: Option<AudioEngine>,
    queue: Vec<PathBuf>,
    loaded: BTreeMap<u64, TrackInfo>,
    current: TrackInfo,
    current_token: u64,
    error: Option<String>,
    dark: bool,
    mini: bool,
    compact: bool,
    volume: f32,
    repeat: Repeat,
    shuffle: bool,
    dialog_tx: crossbeam_channel::Sender<DialogResult>,
    dialog_rx: crossbeam_channel::Receiver<DialogResult>,
    startup_paths: Vec<PathBuf>,
    playlist_loaded: bool,
    data_dir: Option<PathBuf>,
    library: Option<Library>,
    playlists: Vec<Playlist>,
    playlist_id: i64,
    playback_playlist: Option<i64>,
    playlist_name: String,
    library_tab: bool,
    library_double_click_enqueue: bool,
    search: String,
    query_generation: u64,
    library_page: Vec<Track>,
    library_total: usize,
    page_offset: usize,
    scan_status: String,
    roots: Vec<PathBuf>,
    last_saved: String,
    last_save_check: std::time::Instant,
    cover: Option<egui::TextureHandle>,
    brand: egui::TextureHandle,
    art_tx: crossbeam_channel::Sender<(u64, egui::ColorImage)>,
    art_rx: crossbeam_channel::Receiver<(u64, egui::ColorImage)>,
    lyrics: crate::lyrics::Lyrics,
    manual_lyrics: BTreeMap<PathBuf, PathBuf>,
    lyrics_tx: crossbeam_channel::Sender<(u64, Result<crate::lyrics::Lyrics, String>)>,
    lyrics_rx: crossbeam_channel::Receiver<(u64, Result<crate::lyrics::Lyrics, String>)>,
    show_lyrics: bool,
    mini_window: settings::WindowSettings,
    lyrics_window: settings::WindowSettings,
    mini_passthrough: bool,
    lyrics_passthrough: bool,
    desktop: Option<crate::desktop::Desktop>,
    overlay_settings: bool,
    main_hidden: bool,
    eq: crate::dsp::EqSettings,
    eq_open: bool,
    show_spectrum: bool,
    seek_preview: Option<f64>,
    gui_report: Option<crate::diagnostics::GuiReport>,
}

enum DialogResult {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
    Lyrics(PathBuf, PathBuf),
}

impl PlayerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        gui_smoke: bool,
        startup_paths: Vec<PathBuf>,
    ) -> Self {
        crate::theme::apply(&cc.egui_ctx, true);
        let mut fonts = egui::FontDefinitions::default();
        for path in [
            "C:/Windows/Fonts/msjh.ttc",
            "C:/Windows/Fonts/msyh.ttc",
            "C:/Windows/Fonts/simhei.ttf",
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
                fonts
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .unwrap()
                    .push("cjk".into());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 9.0);
        style.spacing.button_padding = egui::vec2(12.0, 7.0);
        style.visuals.selection.bg_fill = Color32::from_rgb(83, 53, 123);
        cc.egui_ctx.set_style(style);
        let (engine, mut error) = match AudioEngine::new() {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        let data_dir = match settings::data_dir() {
            Ok(d) => Some(d),
            Err(e) => {
                error = Some(format!("{e:#}"));
                None
            }
        };
        let saved = data_dir
            .as_ref()
            .map(|d| settings::load(d))
            .transpose()
            .unwrap_or_else(|e| {
                error = Some(format!("設定讀取失敗：{e:#}"));
                None
            })
            .unwrap_or_default();
        let library = data_dir
            .as_ref()
            .map(|d| Library::start(d.join("library.sqlite"), cc.egui_ctx.clone()));
        if let Some(l) = &library {
            l.send(Request::ListPlaylists);
            l.send(Request::LoadPlaylist(saved.playlist));
        }
        if let Some(e) = &engine {
            e.set_volume(saved.volume);
            e.eq.set(saved.eq);
        }
        let (dialog_tx, dialog_rx) = crossbeam_channel::unbounded();
        let (art_tx, art_rx) = crossbeam_channel::unbounded();
        let (lyrics_tx, lyrics_rx) = crossbeam_channel::unbounded();
        let desktop = match crate::desktop::Desktop::new(cc.egui_ctx.clone()) {
            Ok(d) => {
                if !d.warnings.is_empty() {
                    error = Some(d.warnings.join("\n"));
                }
                Some(d)
            }
            Err(e) => {
                error = Some(format!("桌面整合：{e:#}"));
                None
            }
        };
        let playlist_loaded = library.is_none();
        Self {
            engine,
            queue: Vec::new(),
            loaded: BTreeMap::new(),
            current: TrackInfo::default(),
            current_token: 0,
            error,
            dark: saved.dark,
            mini: gui_smoke || saved.mini,
            compact: saved.compact,
            volume: saved.volume,
            repeat: saved.repeat,
            shuffle: saved.shuffle,
            dialog_tx,
            dialog_rx,
            startup_paths,
            playlist_loaded,
            data_dir,
            library,
            playlists: Vec::new(),
            playlist_id: saved.playlist,
            playback_playlist: None,
            playlist_name: String::new(),
            library_tab: false,
            library_double_click_enqueue: saved.library_double_click_enqueue,
            search: String::new(),
            query_generation: 0,
            library_page: Vec::new(),
            library_total: 0,
            page_offset: 0,
            scan_status: String::new(),
            roots: saved.roots,
            last_saved: String::new(),
            last_save_check: std::time::Instant::now(),
            brand: {
                let icon = crate::theme::icon();
                cc.egui_ctx.load_texture(
                    "brand",
                    egui::ColorImage::from_rgba_unmultiplied([128, 128], &icon.rgba),
                    egui::TextureOptions::LINEAR,
                )
            },
            cover: None,
            art_tx,
            art_rx,
            lyrics: Default::default(),
            manual_lyrics: BTreeMap::new(),
            lyrics_tx,
            lyrics_rx,
            show_lyrics: gui_smoke || saved.lyrics,
            mini_window: saved.mini_window,
            lyrics_window: saved.lyrics_window,
            mini_passthrough: false,
            lyrics_passthrough: false,
            desktop,
            overlay_settings: false,
            main_hidden: false,
            eq: saved.eq,
            eq_open: false,
            show_spectrum: saved.spectrum,
            seek_preview: None,
            gui_report: crate::diagnostics::GuiReport::from_env().unwrap_or_else(|error| {
                eprintln!("GUI diagnostics: {error:#}");
                None
            }),
        }
    }

    fn poll(&mut self) {
        while let Ok(result) = self.dialog_rx.try_recv() {
            match result {
                DialogResult::Files(paths) => {
                    if !paths.is_empty() {
                        let start = self.queue.len();
                        self.queue.extend(paths);
                        self.save_queue();
                        self.play_index(start);
                    }
                }
                DialogResult::Folder(path) => {
                    if !self.roots.contains(&path) {
                        self.roots.push(path.clone());
                    }
                    self.scan_status = "掃描中…".into();
                    if let Some(l) = &self.library {
                        l.send(Request::Scan(path));
                    }
                }
                DialogResult::Lyrics(track, path) => {
                    let key = crate::lyrics::source_key(&track);
                    self.manual_lyrics.insert(key.clone(), path.clone());
                    if key == crate::lyrics::source_key(&self.current.path) {
                        self.load_lyrics(self.current_token, path, true);
                    }
                }
            }
        }
        let responses = self
            .library
            .as_ref()
            .map(|l| l.responses.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for response in responses {
            match response {
                Response::Page {
                    tracks,
                    total,
                    generation,
                } => {
                    if generation == self.query_generation {
                        self.library_page = tracks;
                        self.library_total = total;
                    }
                }
                Response::Playlists(lists) => {
                    self.playlists = lists;
                    if !self.playlists.iter().any(|p| p.id == self.playlist_id) {
                        self.playlist_id = 1;
                        if let Some(l) = &self.library {
                            l.send(Request::LoadPlaylist(1));
                        }
                    }
                }
                Response::Playlist { id, paths } => {
                    if id == self.playlist_id {
                        self.queue = paths;
                        self.playlist_loaded = true;
                    }
                }
                Response::ScanProgress {
                    processed,
                    errors,
                    done,
                } => {
                    self.scan_status = format!(
                        "{}：{} 首，{} 個錯誤",
                        if done { "掃描完成" } else { "掃描中" },
                        processed,
                        errors
                    );
                    if done {
                        self.query_library();
                    }
                }
                Response::Error(e) => self.error = Some(e),
                Response::Saved => (),
            }
        }
        if self.playlist_loaded && !self.startup_paths.is_empty() {
            let start = self.queue.len();
            self.queue.append(&mut self.startup_paths);
            self.save_queue();
            self.play_index(start);
        }
        if let Some(engine) = &self.engine {
            while let Ok(event) = engine.events.try_recv() {
                match event {
                    Event::Error(e) => self.error = Some(e),
                    Event::Loaded { token, info, .. } => {
                        self.loaded.insert(token, info);
                    }
                }
            }
            let token = engine.state.token.load(Ordering::Relaxed);
            if token != self.current_token {
                if let Some(info) = self.loaded.remove(&token) {
                    self.current = info;
                    self.current_token = token;
                    self.seek_preview = None;
                    self.cover = None;
                    self.lyrics = Default::default();
                    let lyric_path = self
                        .manual_lyrics
                        .get(&crate::lyrics::source_key(&self.current.path))
                        .cloned()
                        .unwrap_or_else(|| self.current.path.with_extension("lrc"));
                    let tx = self.lyrics_tx.clone();
                    std::thread::spawn(move || {
                        if lyric_path.is_file() {
                            let _ = tx.send((
                                token,
                                crate::lyrics::Lyrics::load(&lyric_path)
                                    .map_err(|e| format!("{e:#}")),
                            ));
                        }
                    });
                    if let Some(bytes) = self.current.cover.take() {
                        let tx = self.art_tx.clone();
                        std::thread::spawn(move || {
                            let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes));
                            if let Ok(format_reader) = reader.with_guessed_format() {
                                reader = format_reader;
                                let mut limits = image::Limits::default();
                                limits.max_image_width = Some(8192);
                                limits.max_image_height = Some(8192);
                                limits.max_alloc = Some(128 * 1024 * 1024);
                                reader.limits(limits);
                                if let Ok(image) = reader.decode() {
                                    let image = image.thumbnail(256, 256).to_rgba8();
                                    let size = [image.width() as usize, image.height() as usize];
                                    let _ = tx.send((
                                        token,
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            size,
                                            image.as_raw(),
                                        ),
                                    ));
                                }
                            }
                        });
                    }
                }
                self.loaded.retain(|&key, _| key > token);
            }
        }
    }

    fn save_queue(&self) {
        self.save_queue_edit(None);
    }
    fn save_queue_edit(&self, edit: Option<crate::audio::QueueEdit>) {
        if let Some(l) = &self.library {
            l.send(Request::SavePlaylist(self.playlist_id, self.queue.clone()));
        }
        if self.playback_playlist == Some(self.playlist_id) {
            if let Some(e) = &self.engine {
                e.send(Command::UpdateQueue(self.queue.clone(), edit));
            }
        }
    }
    fn query_library(&mut self) {
        self.query_generation += 1;
        if let Some(l) = &self.library {
            l.send(Request::Query {
                text: self.search.clone(),
                offset: self.page_offset,
                generation: self.query_generation,
            });
        }
    }
    fn persist_settings(&mut self) {
        let saved = Settings {
            dark: self.dark,
            mini: self.mini,
            compact: self.compact,
            volume: self.volume,
            repeat: self.repeat,
            shuffle: self.shuffle,
            playlist: self.playlist_id,
            roots: self.roots.clone(),
            lyrics: self.show_lyrics,
            mini_window: self.mini_window,
            lyrics_window: self.lyrics_window,
            eq: self.eq,
            spectrum: self.show_spectrum,
            library_double_click_enqueue: self.library_double_click_enqueue,
        };
        if let Ok(serialized) = serde_json::to_string(&saved) {
            if serialized != self.last_saved {
                if let Some(dir) = &self.data_dir {
                    match settings::save(dir, &saved) {
                        Ok(()) => self.last_saved = serialized,
                        Err(e) => self.error = Some(format!("無法保存設定：{e:#}")),
                    }
                }
            }
        }
    }

    fn play_index(&mut self, index: usize) {
        self.playback_playlist = Some(self.playlist_id);
        if let Some(engine) = &self.engine {
            engine.send(Command::Queue(self.queue.clone(), index));
            engine.send(Command::Mode(self.repeat, self.shuffle));
        }
    }

    fn open_files(&self, ctx: &egui::Context) {
        let tx = self.dialog_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter("音樂", &["mp3", "flac", "wav", "aac", "m4a", "ogg", "opus"])
                .pick_files()
            {
                let _ = tx.send(DialogResult::Files(paths));
                ctx.request_repaint();
            }
        });
    }

    fn pick_folder(&self, ctx: &egui::Context) {
        let tx = self.dialog_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                let _ = tx.send(DialogResult::Folder(path));
                ctx.request_repaint();
            }
        });
    }

    fn collection_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal_wrapped(|ui| {
            if ui.selectable_label(!self.library_tab, "播放清單").clicked() {
                self.library_tab = false;
            }
            if ui.selectable_label(self.library_tab, "音樂庫").clicked() {
                self.library_tab = true;
                self.query_library();
            }
            if button(ui, "+", "掃描資料夾").clicked() {
                self.pick_folder(ctx);
            }
            if button(ui, "↻", "重新掃描").clicked() {
                if let Some(l) = &self.library {
                    for root in &self.roots {
                        l.send(Request::Scan(root.clone()));
                    }
                }
            }
        });
        if !self.scan_status.is_empty() {
            ui.small(&self.scan_status);
        }
        if self.library_tab {
            ui.horizontal_wrapped(|ui| {
                ui.label("搜尋");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.search)
                            .desired_width((ui.available_width() - 50.0).max(80.0)),
                    )
                    .changed()
                {
                    self.page_offset = 0;
                    self.query_library();
                }
                ui.label(format!("{} 首", self.library_total));
            });
            ui.horizontal_wrapped(|ui| {
                if enabled_button(ui, self.page_offset > 0, "prev", "上一頁").clicked() {
                    self.page_offset = self.page_offset.saturating_sub(100);
                    self.query_library();
                }
                ui.label(format!(
                    "{}–{}",
                    if self.library_total == 0 {
                        0
                    } else {
                        self.page_offset + 1
                    },
                    (self.page_offset + 100).min(self.library_total)
                ));
                if enabled_button(
                    ui,
                    self.page_offset + 100 < self.library_total,
                    "next",
                    "下一頁",
                )
                .clicked()
                {
                    self.page_offset += 100;
                    self.query_library();
                }
            });
            let mut add = None;
            let mut play = None;
            egui::ScrollArea::vertical().show_rows(
                ui,
                48.0,
                self.library_page.len(),
                |ui, range| {
                    for i in range {
                        let track = &self.library_page[i];
                        ui.horizontal_wrapped(|ui| {
                            if button(ui, "+", "加入目前清單").clicked() {
                                add = Some(track.path.clone());
                            }
                            ui.vertical(|ui| {
                                if ui
                                    .add(
                                        egui::Label::new(&track.title)
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                    )
                                    .double_clicked()
                                {
                                    play = Some(track.path.clone());
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "{} · {} · {}",
                                            track.artist,
                                            track.album,
                                            time_label(track.duration)
                                        ))
                                        .small(),
                                    )
                                    .truncate(),
                                );
                            });
                        });
                    }
                },
            );
            if let Some(path) = add {
                self.queue.push(path);
                self.save_queue();
            }
            if let Some(path) = play {
                if self.library_double_click_enqueue {
                    let i = self.queue.len();
                    self.queue.push(path);
                    self.save_queue();
                    self.play_index(i);
                } else {
                    self.playback_playlist = None;
                    if let Some(engine) = &self.engine {
                        engine.send(Command::Queue(vec![path], 0));
                        engine.send(Command::Mode(self.repeat, self.shuffle));
                    }
                }
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                let before = self.playlist_id;
                egui::ComboBox::from_id_salt("playlists")
                    .selected_text(
                        self.playlists
                            .iter()
                            .find(|p| p.id == self.playlist_id)
                            .map(|p| p.name.as_str())
                            .unwrap_or("我的音樂"),
                    )
                    .show_ui(ui, |ui| {
                        for p in &self.playlists {
                            ui.selectable_value(&mut self.playlist_id, p.id, &p.name);
                        }
                    });
                if before != self.playlist_id {
                    if let Some(l) = &self.library {
                        l.send(Request::LoadPlaylist(self.playlist_id));
                    }
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.playlist_name)
                        .hint_text("清單名稱")
                        .desired_width(110.0),
                );
            });
            ui.horizontal(|ui| {
                if button(ui, "+", "新增清單").clicked() {
                    if let Some(l) = &self.library {
                        l.send(Request::CreatePlaylist(self.playlist_name.clone()));
                    }
                }
                if button(ui, "✎", "清單改名").clicked() {
                    if let Some(l) = &self.library {
                        l.send(Request::RenamePlaylist(
                            self.playlist_id,
                            self.playlist_name.clone(),
                        ));
                    }
                }
                if enabled_button(ui, self.playlist_id != 1, "−", "刪除清單").clicked() {
                    if let Some(l) = &self.library {
                        l.send(Request::DeletePlaylist(self.playlist_id));
                    }
                }
                if button(ui, "存", "儲存清單").clicked() {
                    self.save_queue();
                }
            });
            if self.queue.is_empty() {
                ui.add_space(20.0);
                ui.label("將本機音樂拖曳至此，或選擇「加入音樂」");
            }
            let current = self
                .engine
                .as_ref()
                .filter(|_| self.playback_playlist == Some(self.playlist_id))
                .map(|e| e.state.index.load(Ordering::Relaxed));
            let mut edit = None;
            egui::ScrollArea::vertical().show_rows(ui, 30.0, self.queue.len(), |ui, range| {
                for i in range {
                    let name = self.queue[i]
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    let response = ui
                        .add(
                            egui::Button::new(format!("{:02}    {}", i + 1, name))
                                .selected(current == Some(i))
                                .frame(false)
                                .truncate(),
                        )
                        .on_hover_text(name.as_ref());
                    if response.double_clicked() {
                        self.play_index(i);
                    }
                    response.context_menu(|ui| {
                        if ui.button("播放").clicked() {
                            self.play_index(i);
                            ui.close_menu();
                        }
                        if ui.add_enabled(i > 0, egui::Button::new("上移")).clicked() {
                            edit = Some((i, -1));
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(i + 1 < self.queue.len(), egui::Button::new("下移"))
                            .clicked()
                        {
                            edit = Some((i, 1));
                            ui.close_menu();
                        }
                        if ui.button("從清單移除").clicked() {
                            edit = Some((i, 0));
                            ui.close_menu();
                        }
                    });
                }
            });
            if let Some((i, action)) = edit {
                match action {
                    -1 => self.queue.swap(i, i - 1),
                    1 => self.queue.swap(i, i + 1),
                    _ => {
                        self.queue.remove(i);
                    }
                }
                self.save_queue_edit(Some(match action {
                    -1 => crate::audio::QueueEdit::Swap(i, i - 1),
                    1 => crate::audio::QueueEdit::Swap(i, i + 1),
                    _ => crate::audio::QueueEdit::Remove(i),
                }));
            }
        }
    }

    fn transport(&mut self, ui: &mut egui::Ui, full: bool) {
        ui.horizontal(|ui| {
            self.transport_buttons(ui);
            if full {
                ui.add_space(10.0);
                ui.label("音量");
                if ui
                    .add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    if let Some(e) = &self.engine {
                        e.set_volume(self.volume);
                    }
                }
            }
        });
        if full {
            let mut position = self
                .seek_preview
                .unwrap_or_else(|| self.engine.as_ref().map(|e| e.position()).unwrap_or(0.0));
            ui.horizontal(|ui| {
                ui.monospace(time_label(position));
                ui.spacing_mut().slider_width = (ui.available_width() - 80.0).max(80.0);
                let response = ui.add_sized(
                    [ui.available_width() - 60.0, 20.0],
                    egui::Slider::new(&mut position, 0.0..=self.current.duration.max(0.01))
                        .show_value(false),
                );
                if response.dragged() {
                    self.seek_preview = Some(position);
                }
                if response.drag_stopped() || response.changed() && !response.dragged() {
                    if let Some(e) = &self.engine {
                        e.send(Command::Seek(position));
                    }
                    self.seek_preview = None;
                }
                ui.monospace(time_label(self.current.duration));
            });
        }
    }

    fn transport_buttons(&mut self, ui: &mut egui::Ui) {
        if button(ui, "prev", "上一首").clicked() {
            if let Some(e) = &self.engine {
                e.send(Command::Previous);
            }
        }
        let playing = self
            .engine
            .as_ref()
            .is_some_and(|e| e.state.playing.load(Ordering::Relaxed));
        if button(
            ui,
            if playing { "pause" } else { "play" },
            if playing { "暫停" } else { "播放" },
        )
        .clicked()
        {
            self.toggle_playback();
        }
        if button(ui, "next", "下一首").clicked() {
            if let Some(e) = &self.engine {
                e.send(Command::Next);
            }
        }
    }

    fn set_mini(&mut self, ctx: &egui::Context, mini: bool) {
        self.mini = mini;
        self.main_hidden = mini;
        self.mini_passthrough = false;
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Visible(!mini),
        );
        if !mini {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Minimized(false),
            );
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
        }
    }

    fn title(&self) -> &str {
        if self.current.title.is_empty() {
            "未選取任何音樂"
        } else {
            &self.current.title
        }
    }

    fn load_lyrics(&self, token: u64, path: PathBuf, report_missing: bool) {
        let tx = self.lyrics_tx.clone();
        std::thread::spawn(move || {
            if report_missing || path.is_file() {
                let _ = tx.send((
                    token,
                    crate::lyrics::Lyrics::load(&path).map_err(|e| format!("{e:#}")),
                ));
            }
        });
    }
    fn lyric_text(&self) -> &str {
        if self.lyrics.lines.is_empty() {
            "尚未載入本機 LRC 歌詞"
        } else {
            self.lyrics
                .current(self.engine.as_ref().map(|e| e.position()).unwrap_or(0.0))
        }
    }
    fn toggle_playback(&mut self) {
        if let Some(e) = &self.engine {
            if e.state.token.load(Ordering::Relaxed) == 0 {
                self.play_index(e.state.index.load(Ordering::Relaxed));
            } else if e.state.ended.load(Ordering::Relaxed) {
                e.send(Command::Restart);
            } else {
                e.toggle();
            }
        }
    }
    fn desktop_events(&mut self, ctx: &egui::Context) {
        use crate::desktop::Action;
        let events = self
            .desktop
            .as_ref()
            .map(|d| d.events.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for event in events {
            match event {
                Action::Restore => {
                    self.mini_passthrough = false;
                    self.lyrics_passthrough = false;
                    self.mini = false;
                    self.main_hidden = false;
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::ROOT,
                        egui::ViewportCommand::Visible(true),
                    );
                    ctx.send_viewport_cmd_to(
                        egui::ViewportId::ROOT,
                        egui::ViewportCommand::Minimized(false),
                    );
                    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                }
                Action::Toggle => self.toggle_playback(),
                Action::Next => {
                    if let Some(e) = &self.engine {
                        e.send(Command::Next);
                    }
                }
                Action::Previous => {
                    if let Some(e) = &self.engine {
                        e.send(Command::Previous);
                    }
                }
                Action::Mini => self.set_mini(ctx, !self.mini),
                Action::Lyrics => self.show_lyrics = !self.show_lyrics,
                Action::Quit => {
                    ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close)
                }
            }
        }
    }

    fn overlay_preferences(&mut self, ctx: &egui::Context) {
        if self.overlay_settings {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings"),
                egui::ViewportBuilder::default()
                    .with_title("EPlayer 設定")
                    .with_icon(crate::theme::icon())
                    .with_inner_size([490.0, 620.0])
                    .with_min_inner_size([490.0, 480.0])
                    .with_decorations(false)
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                |ctx, _| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.overlay_settings = false;
                    }
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(ctx.style().visuals.window_fill)
                                .inner_margin(16.0),
                        )
                        .show(ctx, |ui| {
                            let mut close = false;
                            crate::widgets::title_bar(ui, "EPLAYER · 設定", &mut close, false);
                            if close {
                                self.overlay_settings = false;
                            }
                            ui.separator();
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.checkbox(&mut self.dark, "深色主題");
                                ui.checkbox(
                                    &mut self.library_double_click_enqueue,
                                    "雙擊音樂庫時加入播放清單",
                                );
                                ui.small("關閉時直接播放選取歌曲，不改動播放清單。");
                                ui.checkbox(&mut self.show_lyrics, "顯示獨立歌詞");
                                ui.label("Ctrl+Alt+L：顯示主視窗並解除全部滑鼠穿透");
                                let recovery =
                                    self.desktop.as_ref().is_some_and(|d| d.recovery_available);
                                if !recovery {
                                    ui.colored_label(
                                        Color32::LIGHT_RED,
                                        "恢復快捷鍵不可用，已禁止啟用穿透。",
                                    );
                                }
                                if ui
                                    .checkbox(
                                        &mut self.compact,
                                        "精簡 preset：歌名／檔案名＋播放控制",
                                    )
                                    .changed()
                                {
                                    self.mini_window.size = if self.compact {
                                        [540.0, 60.0]
                                    } else {
                                        [440.0, 320.0]
                                    };
                                }
                                ui.separator();
                                ui.label("浮動播放器");
                                ui.checkbox(&mut self.mini_window.topmost, "播放器置頂");
                                ui.add(
                                    egui::Slider::new(&mut self.mini_window.opacity, 0.2..=1.0)
                                        .text("不透明度"),
                                );
                                ui.add_enabled(
                                    recovery,
                                    egui::Checkbox::new(
                                        &mut self.mini_passthrough,
                                        "播放器滑鼠穿透",
                                    ),
                                );
                                ui.separator();
                                ui.label("獨立歌詞");
                                ui.checkbox(&mut self.lyrics_window.topmost, "歌詞置頂");
                                ui.add(
                                    egui::Slider::new(&mut self.lyrics_window.opacity, 0.2..=1.0)
                                        .text("不透明度"),
                                );
                                ui.add_enabled(
                                    recovery,
                                    egui::Checkbox::new(
                                        &mut self.lyrics_passthrough,
                                        "歌詞滑鼠穿透",
                                    ),
                                );
                                if button(ui, "↻", "重設浮動視窗位置與大小").clicked()
                                {
                                    self.mini_window.position = Some([80.0, 80.0]);
                                    self.mini_window.size = if self.compact {
                                        [540.0, 60.0]
                                    } else {
                                        [440.0, 320.0]
                                    };
                                    self.lyrics_window.position = Some([80.0, 380.0]);
                                    self.lyrics_window.size = [600.0, 130.0];
                                    ctx.send_viewport_cmd_to(
                                        egui::ViewportId::from_hash_of("mini"),
                                        egui::ViewportCommand::OuterPosition(egui::pos2(
                                            80.0, 80.0,
                                        )),
                                    );
                                    ctx.send_viewport_cmd_to(
                                        egui::ViewportId::from_hash_of("lyrics"),
                                        egui::ViewportCommand::OuterPosition(egui::pos2(
                                            80.0, 380.0,
                                        )),
                                    );
                                    self.mini_passthrough = false;
                                    self.lyrics_passthrough = false;
                                }
                                ui.small(
                                "Ctrl+Alt+Space 播放／暫停；Ctrl+Alt+←／→ 切歌。也支援鍵盤媒體鍵。",
                            );
                            });
                        });
                },
            );
        }
    }

    fn floating_windows(&mut self, ctx: &egui::Context) {
        if !self.mini {
            self.mini_window.position_initialized = false;
        }
        if !self.show_lyrics {
            self.lyrics_window.position_initialized = false;
        }
        if !self.mini_window.position_initialized {
            crate::desktop::constrain_window(&mut self.mini_window, ctx.pixels_per_point());
        }
        if !self.lyrics_window.position_initialized {
            crate::desktop::constrain_window(&mut self.lyrics_window, ctx.pixels_per_point());
        }
        if self.mini {
            if !self.main_hidden {
                self.set_mini(ctx, true);
            }
            if self.compact {
                self.mini_window.size[1] = 60.0;
            }
            let mut builder = egui::ViewportBuilder::default()
                .with_title("EPlayer Mini")
                .with_icon(crate::theme::icon())
                .with_inner_size(self.mini_window.size)
                .with_min_inner_size(if self.compact {
                    [420.0, 60.0]
                } else {
                    [340.0, 300.0]
                })
                .with_decorations(false)
                .with_transparent(false)
                .with_mouse_passthrough(self.mini_passthrough)
                .with_window_level(if self.mini_window.topmost {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                });
            builder = overlay_position(builder, &mut self.mini_window);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("mini"),
                builder,
                |ctx, _| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.set_mini(ctx, false);
                    }
                    let fill = ctx.style().visuals.panel_fill;
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(fill).inner_margin(10.0))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if button(ui, "back", "返回主視窗").clicked() {
                                    self.set_mini(ctx, false);
                                }
                                let reserve = if self.compact { 190.0 } else { 76.0 };
                                let drag = crate::widgets::marquee(
                                    ui,
                                    self.title(),
                                    ui.available_width() - reserve,
                                );
                                move_window(ctx, &drag, &mut self.mini_window);
                                if self.compact {
                                    self.transport_buttons(ui);
                                }
                                if button(ui, "gear", "設定").clicked() {
                                    self.overlay_settings = true;
                                }
                                if button(ui, "close", "關閉浮動播放器並返回").clicked() {
                                    self.set_mini(ctx, false);
                                }
                            });
                            if !self.compact {
                                ui.horizontal(|ui| {
                                    if let Some(cover) = &self.cover {
                                        ui.image((cover.id(), egui::vec2(36.0, 36.0)));
                                    } else {
                                        ui.image((self.brand.id(), egui::vec2(36.0, 36.0)));
                                    }
                                    ui.add(
                                        egui::Label::new(if self.current.artist.is_empty() {
                                            "本機音樂"
                                        } else {
                                            &self.current.artist
                                        })
                                        .truncate(),
                                    );
                                });
                            }
                            if !self.compact {
                                self.transport(ui, true);
                            }
                            if !self.compact {
                                ui.label(
                                    RichText::new(self.lyric_text())
                                        .color(crate::theme::accent(self.dark)),
                                );
                                self.spectrum_ui(ui, 45.0);
                            }
                            resize_grip(
                                ui,
                                ctx,
                                &mut self.mini_window,
                                if self.compact {
                                    [420.0, 60.0]
                                } else {
                                    [340.0, 300.0]
                                },
                            );
                        });
                    remember_window(ctx, &mut self.mini_window);
                },
            );
        }
        if self.show_lyrics {
            let mut builder = egui::ViewportBuilder::default()
                .with_title("EPlayer Lyrics")
                .with_icon(crate::theme::icon())
                .with_inner_size(self.lyrics_window.size)
                .with_min_inner_size([250.0, 90.0])
                .with_decorations(false)
                .with_transparent(false)
                .with_mouse_passthrough(self.lyrics_passthrough)
                .with_window_level(if self.lyrics_window.topmost {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                });
            builder = overlay_position(builder, &mut self.lyrics_window);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("lyrics"),
                builder,
                |ctx, _| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        self.show_lyrics = false;
                    }
                    let fill = ctx.style().visuals.panel_fill;
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(fill).inner_margin(10.0))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                let drag = ui.add(
                                    egui::Label::new("EPLAYER · 歌詞")
                                        .selectable(false)
                                        .sense(egui::Sense::drag()),
                                );
                                move_window(ctx, &drag, &mut self.lyrics_window);
                                if button(ui, "close", "關閉歌詞").clicked() {
                                    self.show_lyrics = false;
                                }
                            });
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lyric_text())
                                        .size(26.0)
                                        .color(crate::theme::accent(self.dark)),
                                )
                                .truncate(),
                            );
                            resize_grip(ui, ctx, &mut self.lyrics_window, [250.0, 90.0]);
                        });
                    remember_window(ctx, &mut self.lyrics_window);
                },
            );
        }
    }

    fn spectrum_ui(&self, ui: &mut egui::Ui, height: f32) {
        if !self.show_spectrum {
            return;
        }
        let bands = self
            .engine
            .as_ref()
            .and_then(|e| e.spectrum.shared.bands.lock().ok().map(|b| *b))
            .unwrap_or([0.0; crate::spectrum::BANDS]);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(100.0), height),
            egui::Sense::hover(),
        );
        let width = rect.width() / crate::spectrum::BANDS as f32;
        for (i, level) in bands.iter().enumerate() {
            let h = (level * height).max(1.0);
            let bar = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + i as f32 * width, rect.max.y - h),
                egui::vec2((width - 2.0).max(1.0), h),
            );
            ui.painter()
                .rect_filled(bar, 1.0, crate::theme::accent(self.dark));
        }
    }
    fn equalizer_ui(&mut self, ctx: &egui::Context) {
        if !self.eq_open {
            return;
        }
        let mut changed = false;
        egui::Window::new("等化器 · 10 段")
            .default_width(720.0)
            .max_width((ctx.screen_rect().width() - 40.0).max(280.0))
            .open(&mut self.eq_open)
            .show(ctx, |ui| {
                changed |= ui.checkbox(&mut self.eq.enabled, "啟用 EQ").changed();
                ui.label("提高頻段時，可降低前級增益保留輸出空間。");
                changed |= ui
                    .add(egui::Slider::new(&mut self.eq.preamp, -24.0..=0.0).text("前級 dB"))
                    .changed();
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, gain) in self.eq.gains.iter_mut().enumerate() {
                                ui.vertical(|ui| {
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(gain, -12.0..=12.0)
                                                .vertical()
                                                .suffix(" dB"),
                                        )
                                        .changed();
                                    let hz = crate::dsp::FREQUENCIES[i];
                                    ui.small(if hz >= 1000.0 {
                                        format!("{}k", hz / 1000.0)
                                    } else {
                                        format!("{hz}")
                                    });
                                });
                            }
                        });
                    });
                if button(ui, "↻", "重設為平坦").clicked() {
                    self.eq.gains = [0.0; 10];
                    self.eq.preamp = 0.0;
                    changed = true;
                }
            });
        if changed {
            if let Some(e) = &self.engine {
                e.eq.set(self.eq);
            }
        }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        self.desktop_events(ctx);
        while let Ok((token, result)) = self.lyrics_rx.try_recv() {
            if token == self.current_token {
                match result {
                    Ok(lyrics) => self.lyrics = lyrics,
                    Err(e) => self.error = Some(format!("歌詞：{e}")),
                }
            }
        }
        while let Ok((token, image)) = self.art_rx.try_recv() {
            if token == self.current_token {
                self.cover = Some(ctx.load_texture("cover", image, Default::default()));
            }
        }
        if self.last_save_check.elapsed() > Duration::from_secs(2) {
            self.persist_settings();
            self.last_save_check = std::time::Instant::now();
        }
        if ctx.style().visuals.dark_mode != self.dark || ctx.style().spacing.interact_size.y < 28.0
        {
            crate::theme::apply(ctx, self.dark);
        }
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.window_fill)
                    .inner_margin(12.0),
            )
            .show(ctx, |ui| {
                let mut close = false;
                crate::widgets::title_bar(ui, "EPLAYER", &mut close, true);
                if close {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.horizontal(|ui| {
                    ui.image((self.brand.id(), egui::vec2(32.0, 32.0)));
                    ui.label(RichText::new("本機音樂").small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if button(ui, "◐", "切換深／淺色").clicked() {
                            self.dark = !self.dark;
                        }
                        if button(ui, "▣", "浮動播放器").clicked() {
                            self.set_mini(ctx, true);
                        }
                        if button(ui, "詞", "浮動歌詞").clicked() {
                            self.show_lyrics = !self.show_lyrics;
                        }
                        if button(ui, "+", "加入音樂").clicked() {
                            self.open_files(ctx);
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!("{} 首本機音樂", self.queue.len()));
                if let Some(e) = &self.engine {
                    ui.small(format!("{} · {} Hz", e.device_name, e.sample_rate));
                }
            });
        });
        egui::SidePanel::left("collection_sidebar")
            .resizable(true)
            .default_width(280.0)
            .width_range(230.0..=(ctx.screen_rect().width() - 420.0).clamp(230.0, 480.0))
            .show(ctx, |ui| {
                self.collection_ui(ui, ctx);
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if let Some(cover) = &self.cover {
                        ui.image((cover.id(), egui::vec2(70.0, 70.0)));
                    } else {
                        ui.image((self.brand.id(), egui::vec2(70.0, 70.0)));
                    }
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(RichText::new(self.title()).size(25.0).strong())
                                .truncate(),
                        );
                        ui.add(
                            egui::Label::new(format!(
                                "{}  {}",
                                self.current.artist, self.current.album
                            ))
                            .truncate(),
                        );
                    });
                });
                ui.add_space(12.0);
                self.transport(ui, true);
                self.spectrum_ui(ui, 65.0);
                ui.label(RichText::new(self.lyric_text()).color(crate::theme::accent(self.dark)));
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "EQ", "等化器").clicked() {
                        self.eq_open = true;
                    }
                    ui.checkbox(&mut self.show_spectrum, "頻譜");
                    if enabled_button(
                        ui,
                        !self.current.path.as_os_str().is_empty(),
                        "詞",
                        "載入 LRC",
                    )
                    .clicked()
                    {
                        let tx = self.dialog_tx.clone();
                        let ctx = ctx.clone();
                        let track = self.current.path.clone();
                        std::thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("歌詞", &["lrc"])
                                .pick_file()
                            {
                                let _ = tx.send(DialogResult::Lyrics(track, path));
                                ctx.request_repaint();
                            }
                        });
                    }
                    if button(ui, "gear", "設定").clicked() {
                        self.overlay_settings = true;
                    }
                    if enabled_button(ui, self.desktop.is_some(), "↓", "縮至系統匣").clicked()
                    {
                        self.main_hidden = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    }
                });
                ui.horizontal(|ui| {
                    let mut changed = ui.checkbox(&mut self.shuffle, "隨機播放").changed();
                    egui::ComboBox::from_id_salt("repeat")
                        .selected_text(match self.repeat {
                            Repeat::Off => "不循環",
                            Repeat::All => "清單循環",
                            Repeat::One => "單曲循環",
                        })
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(&mut self.repeat, Repeat::Off, "不循環")
                                .changed();
                            changed |= ui
                                .selectable_value(&mut self.repeat, Repeat::All, "清單循環")
                                .changed();
                            changed |= ui
                                .selectable_value(&mut self.repeat, Repeat::One, "單曲循環")
                                .changed();
                        });
                    if changed {
                        if let Some(e) = &self.engine {
                            e.send(Command::Mode(self.repeat, self.shuffle));
                        }
                    }
                });
                ui.separator();
                if let Some(error) = self.error.clone() {
                    ui.colored_label(Color32::LIGHT_RED, error);
                    if button(ui, "close", "關閉訊息").clicked() {
                        self.error = None;
                    }
                }
            });
        if !ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            egui::Area::new(egui::Id::new("root_resize"))
                .fixed_pos(ctx.screen_rect().right_bottom() - egui::vec2(18.0, 18.0))
                .show(ctx, |ui| {
                    let (_, response) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::drag());
                    let response = response.on_hover_cursor(egui::CursorIcon::ResizeNwSe);
                    if response.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                            egui::ResizeDirection::SouthEast,
                        ));
                    }
                });
        }
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            let start = self.queue.len();
            self.queue
                .extend(dropped.into_iter().filter(|p| crate::library::is_audio(p)));
            self.save_queue();
            self.play_index(start);
        }
        self.overlay_preferences(ctx);
        self.equalizer_ui(ctx);
        self.floating_windows(ctx);
        for (visible, title, opacity) in [
            (self.mini, "EPlayer Mini", self.mini_window.opacity),
            (
                self.show_lyrics,
                "EPlayer Lyrics",
                self.lyrics_window.opacity,
            ),
        ] {
            if visible {
                if let Err(error) = crate::desktop::set_overlay_opacity(title, opacity) {
                    self.error = Some(format!("浮動視窗透明度：{error:#}"));
                }
            }
        }
        let playing = self
            .engine
            .as_ref()
            .is_some_and(|e| e.state.playing.load(Ordering::Relaxed));
        let spectrum_visible = self.show_spectrum
            && ((!self.main_hidden && !ctx.input(|i| i.viewport().minimized.unwrap_or(false)))
                || (self.mini && !self.compact));
        if let Some(e) = &self.engine {
            e.spectrum
                .shared
                .enabled
                .store(spectrum_visible && playing, Ordering::Release);
            if let Some(report) = &mut self.gui_report {
                report.sample(|| {
                    serde_json::json!({
                        "playing": playing, "position": e.position(),
                        "token": e.state.token.load(Ordering::Relaxed),
                        "index": e.state.index.load(Ordering::Relaxed),
                        "underruns": e.state.underruns.load(Ordering::Relaxed),
                        "output_frames": e.state.output_frames.load(Ordering::Relaxed),
                        "fft_enabled": e.spectrum.shared.enabled.load(Ordering::Acquire),
                        "fft_analyses": e.spectrum.shared.analyses.load(Ordering::Relaxed),
                        "main_hidden": self.main_hidden,
                        "main_minimized": ctx.input(|i| i.viewport().minimized.unwrap_or(false)),
                        "mini": self.mini, "compact": self.compact, "lyrics": self.show_lyrics,
                        "spectrum_setting": self.show_spectrum,
                        "lyric_text": self.lyrics.current(e.position()),
                        "volume": self.volume,
                    })
                });
            }
        }
        if self
            .engine
            .as_ref()
            .is_some_and(|e| e.state.playing.load(Ordering::Relaxed))
        {
            ctx.request_repaint_after(Duration::from_millis(if spectrum_visible {
                34
            } else {
                100
            }));
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.persist_settings();
    }
}

fn overlay_position(
    mut builder: egui::ViewportBuilder,
    settings: &mut settings::WindowSettings,
) -> egui::ViewportBuilder {
    // Restore saved placement once. OS observations are persistence data, never
    // instructions to reposition an already-open viewport during native dragging.
    if !settings.position_initialized {
        settings.position_initialized = true;
        if let Some(pos) = settings.position {
            builder = builder.with_position(pos);
        }
    }
    builder
}

fn remember_window(ctx: &egui::Context, settings: &mut settings::WindowSettings) {
    ctx.input(|i| {
        // Do not let delayed OS bounds undo a requested preset/reset/resize.
        settings.observe_geometry(
            i.viewport().outer_rect.map(|r| [r.min.x, r.min.y]),
            i.viewport().inner_rect.map(|r| [r.width(), r.height()]),
        );
    });
}
fn move_window(
    ctx: &egui::Context,
    response: &egui::Response,
    settings: &mut settings::WindowSettings,
) {
    let command = ctx.input(|i| {
        completed_or_native_drag(
            response.rect,
            i.viewport().outer_rect.map(|r| r.min),
            &i.events,
            &mut settings.pending_drag,
        )
    });
    if let Some(command) = command {
        ctx.send_viewport_cmd(command);
        ctx.request_repaint();
    }
}

fn completed_or_native_drag(
    title: egui::Rect,
    origin: Option<egui::Pos2>,
    events: &[egui::Event],
    pending: &mut Option<([f32; 2], [f32; 2])>,
) -> Option<egui::ViewportCommand> {
    let mut command = None;
    for event in events {
        match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } if title.contains(*pos) => {
                *pending = origin.map(|origin| ([origin.x, origin.y], [pos.x, pos.y]));
                command = Some(egui::ViewportCommand::StartDrag);
            }
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } => {
                if let Some((initial, press)) = pending.take() {
                    // A very fast release may precede Windows processing StartDrag.
                    // If native dragging never moved the window, finish this gesture
                    // once on release. Never synthesize movement from PointerMoved.
                    if origin == Some(initial.into()) {
                        let delta = *pos - egui::Pos2::from(press);
                        command = if delta.length_sq() > 0.0 {
                            Some(egui::ViewportCommand::OuterPosition(
                                egui::Pos2::from(initial) + delta,
                            ))
                        } else {
                            None
                        };
                    }
                }
            }
            egui::Event::WindowFocused(false) => {
                *pending = None;
            }
            _ => (),
        }
    }
    command
}

fn resize_grip(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    settings: &mut settings::WindowSettings,
    minimum: [f32; 2],
) {
    let rect = egui::Rect::from_min_size(
        ui.max_rect().right_bottom() - egui::vec2(16.0, 16.0),
        egui::vec2(16.0, 16.0),
    );
    ui.allocate_rect(rect, egui::Sense::hover())
        .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "◢",
        egui::FontId::proportional(14.0),
        Color32::GRAY,
    );
    // A fast drag can arrive entirely within one frame. Preserve its press
    // position rather than hit-testing only the pointer's final position.
    let events = ctx.input(|i| i.events.clone());
    if apply_resize_events(settings, rect, minimum, &events) {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(settings.size.into()));
    }
}

fn apply_resize_events(
    settings: &mut settings::WindowSettings,
    grip: egui::Rect,
    minimum: [f32; 2],
    events: &[egui::Event],
) -> bool {
    let original = settings.size;
    for event in events {
        match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                ..
            } if grip.contains(*pos) => {
                settings.resize_anchor = Some(([pos.x, pos.y], settings.size));
            }
            egui::Event::PointerMoved(pos)
            | egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                ..
            } => {
                if let Some((start, size)) = settings.resize_anchor {
                    settings.size = [
                        (size[0] + pos.x - start[0]).max(minimum[0]),
                        (size[1] + pos.y - start[1]).max(minimum[1]),
                    ];
                }
                if matches!(event, egui::Event::PointerButton { pressed: false, .. }) {
                    settings.resize_anchor = None;
                }
            }
            egui::Event::WindowFocused(false) | egui::Event::PointerGone => {
                settings.resize_anchor = None
            }
            _ => (),
        }
    }
    settings.size != original
}

fn time_label(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod resize_tests {
    use super::*;
    #[test]
    fn completed_fast_drag_moves_once_and_native_drag_starts_on_press() {
        let title = egui::Rect::from_min_size(egui::pos2(40.0, 10.0), egui::vec2(250.0, 32.0));
        let event = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        let press = event(egui::pos2(100.0, 20.0), true);
        let release = event(egui::pos2(340.0, 184.0), false);
        let origin = Some(egui::pos2(400.0, 200.0));
        let mut pending = None;
        assert!(matches!(
            completed_or_native_drag(title, origin, std::slice::from_ref(&press), &mut pending),
            Some(egui::ViewportCommand::StartDrag)
        ));
        // Replay the real captured pattern: release arrives in the next frame,
        // but Windows has not moved because the gesture finished before StartDrag.
        match completed_or_native_drag(title, origin, std::slice::from_ref(&release), &mut pending)
        {
            Some(egui::ViewportCommand::OuterPosition(pos)) => {
                assert_eq!(pos, egui::pos2(640.0, 364.0))
            }
            other => panic!("unexpected command: {other:?}"),
        }
        assert!(pending.is_none());
        assert!(completed_or_native_drag(
            title,
            origin,
            &[egui::Event::PointerMoved(egui::pos2(100.0, 20.0))],
            &mut pending
        )
        .is_none());
        completed_or_native_drag(title, origin, std::slice::from_ref(&press), &mut pending);
        // A successful native drag already moved the window: no second move.
        assert!(completed_or_native_drag(
            title,
            Some(egui::pos2(640.0, 364.0)),
            std::slice::from_ref(&release),
            &mut pending
        )
        .is_none());
        assert!(pending.is_none());
        assert!(matches!(
            completed_or_native_drag(title, origin, &[press, release], &mut pending),
            Some(egui::ViewportCommand::OuterPosition(_))
        ));
    }

    #[test]
    fn observed_drag_positions_never_become_move_commands() {
        let mut window = settings::WindowSettings {
            position: Some([40.0, 100.0]),
            ..Default::default()
        };
        let mut applied = overlay_position(egui::ViewportBuilder::default(), &mut window);
        // OS observations can trail the real cursor/window position during a fast drag.
        // Feeding them back as move commands sends the window to an obsolete location.
        for position in [[240.0, 100.0], [40.0, 100.0], [240.0, 100.0]] {
            window.observe_geometry(Some(position), None);
            let (commands, _) = applied.patch(overlay_position(
                egui::ViewportBuilder::default(),
                &mut window,
            ));
            assert!(
                !commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::OuterPosition(_))),
                "stale geometry replayed: {commands:?}"
            );
        }
    }

    #[test]
    fn fast_drag_in_one_frame_resizes_and_releases_anchor() {
        let mut window = settings::WindowSettings {
            size: [440.0, 300.0],
            ..Default::default()
        };
        let grip = egui::Rect::from_min_max(egui::pos2(414.0, 274.0), egui::pos2(430.0, 290.0));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        };
        let changed = apply_resize_events(
            &mut window,
            grip,
            [300.0, 100.0],
            &[
                button(egui::pos2(422.0, 282.0), true),
                egui::Event::PointerMoved(egui::pos2(350.0, 230.0)),
                button(egui::pos2(350.0, 230.0), false),
            ],
        );
        assert!(changed);
        assert_eq!(window.size, [368.0, 248.0]);
        assert!(window.resize_anchor.is_none());
        assert!(!apply_resize_events(
            &mut window,
            grip,
            [300.0, 100.0],
            &[egui::Event::PointerMoved(egui::pos2(10.0, 10.0))]
        ));
    }
}
