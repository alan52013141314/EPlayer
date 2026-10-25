use anyhow::Result;
use eframe::egui;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

/// Apply compositor opacity to this UI thread's own overlay only. This avoids
/// relying on per-pixel OpenGL alpha support in the selected Windows GL driver.
pub fn set_overlay_opacity(title: &str, opacity: f32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            EnumThreadWindows, GetLayeredWindowAttributes, GetWindowLongPtrW, GetWindowTextW,
            SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED,
        },
    };
    struct Request {
        title: Vec<u16>,
        alpha: u8,
        error: Option<std::io::Error>,
    }
    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> BOOL {
        // SAFETY: EnumThreadWindows invokes this synchronously with the live
        // caller-owned Request; enumeration is limited to our own UI thread.
        let request = unsafe { &mut *(data as *mut Request) };
        let mut title = [0u16; 64];
        let length = unsafe { GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32) };
        if length < 0 || title[..length as usize] != request.title {
            return 1;
        }
        // SAFETY: this is an existing window enumerated on the current thread.
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            let mut alpha = 0;
            let mut flags = 0;
            let current =
                GetLayeredWindowAttributes(window, std::ptr::null_mut(), &mut alpha, &mut flags);
            if style & WS_EX_LAYERED as isize != 0
                && current != 0
                && flags & LWA_ALPHA != 0
                && alpha == request.alpha
            {
                return 0;
            }
            SetWindowLongPtrW(window, GWL_EXSTYLE, style | WS_EX_LAYERED as isize);
            if SetLayeredWindowAttributes(window, 0, request.alpha, LWA_ALPHA) == 0 {
                request.error = Some(std::io::Error::last_os_error());
            }
        }
        0
    }
    let mut request = Request {
        title: title.encode_utf16().collect(),
        alpha: (opacity.clamp(0.2, 1.0) * 255.0).round() as u8,
        error: None,
    };
    // SAFETY: the callback's Request remains valid for the synchronous call.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(visit),
            &mut request as *mut Request as LPARAM,
        );
    }
    if let Some(error) = request.error {
        return Err(error.into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Restore,
    Toggle,
    Next,
    Previous,
    Mini,
    Lyrics,
    Quit,
}
pub struct Desktop {
    pub events: crossbeam_channel::Receiver<Action>,
    pub recovery_available: bool,
    pub warnings: Vec<String>,
    _tray: TrayIcon,
    manager: GlobalHotKeyManager,
    registered: Vec<HotKey>,
}
impl Desktop {
    pub fn new(ctx: egui::Context) -> Result<Self> {
        let (tx, events) = crossbeam_channel::unbounded();
        let menu = Menu::new();
        let items = [
            ("restore", "顯示主視窗／解除穿透", Action::Restore),
            ("toggle", "播放／暫停", Action::Toggle),
            ("previous", "上一首", Action::Previous),
            ("next", "下一首", Action::Next),
            ("mini", "浮動播放器", Action::Mini),
            ("lyrics", "浮動歌詞", Action::Lyrics),
            ("quit", "結束 EPlayer", Action::Quit),
        ];
        for (id, label, _) in items {
            menu.append(&MenuItem::with_id(id, label, true, None))?;
        }
        let image = crate::theme::icon();
        let icon = tray_icon::Icon::from_rgba(image.rgba.clone(), image.width, image.height)?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("EPlayer")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()?;
        let sender = tx.clone();
        let context = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some((_, _, action)) = items.iter().find(|(id, _, _)| event.id.as_ref() == *id) {
                let _ = sender.send(*action);
                context.request_repaint();
            }
        }));
        let sender = tx.clone();
        let context = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                let _ = sender.send(Action::Restore);
                context.request_repaint();
            }
        }));
        let manager = GlobalHotKeyManager::new()?;
        let mut registered = Vec::new();
        let mut mappings = Vec::new();
        let mut warnings = Vec::new();
        let mut recovery_available = false;
        for (code, mods, action, label) in [
            (
                Code::KeyL,
                Some(Modifiers::CONTROL | Modifiers::ALT),
                Action::Restore,
                "Ctrl+Alt+L：還原／解除穿透",
            ),
            (
                Code::Space,
                Some(Modifiers::CONTROL | Modifiers::ALT),
                Action::Toggle,
                "Ctrl+Alt+Space：播放／暫停",
            ),
            (
                Code::ArrowRight,
                Some(Modifiers::CONTROL | Modifiers::ALT),
                Action::Next,
                "Ctrl+Alt+Right：下一首",
            ),
            (
                Code::ArrowLeft,
                Some(Modifiers::CONTROL | Modifiers::ALT),
                Action::Previous,
                "Ctrl+Alt+Left：上一首",
            ),
            (Code::MediaPlayPause, None, Action::Toggle, "媒體播放鍵"),
            (Code::MediaTrackNext, None, Action::Next, "媒體下一首鍵"),
            (
                Code::MediaTrackPrevious,
                None,
                Action::Previous,
                "媒體上一首鍵",
            ),
        ] {
            let hotkey = HotKey::new(mods, code);
            match manager.register(hotkey) {
                Ok(()) => {
                    if matches!(action, Action::Restore) {
                        recovery_available = true;
                    }
                    mappings.push((hotkey.id(), action));
                    registered.push(hotkey);
                }
                Err(e) => warnings.push(format!("快捷鍵無法註冊（可能被占用）：{label}，{e}")),
            }
        }
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                if let Some((_, action)) = mappings.iter().find(|(id, _)| *id == event.id) {
                    let _ = tx.send(*action);
                    ctx.request_repaint();
                }
            }
        }));
        Ok(Self {
            events,
            recovery_available,
            warnings,
            _tray: tray,
            manager,
            registered,
        })
    }
}
impl Drop for Desktop {
    fn drop(&mut self) {
        for key in &self.registered {
            let _ = self.manager.unregister(*key);
        }
    }
}

/// Clamp restored coordinates to a live monitor's work area (including negative
/// monitor origins). This runs in the application, not in the automation host.
pub fn constrain_window(settings: &mut crate::settings::WindowSettings, pixels_per_point: f32) {
    use windows_sys::Win32::{
        Foundation::RECT,
        Graphics::Gdi::{GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    };
    let scale = if pixels_per_point.is_finite() {
        pixels_per_point.max(0.5)
    } else {
        1.0
    };
    settings.opacity = if settings.opacity.is_finite() {
        settings.opacity.clamp(0.2, 1.0)
    } else {
        0.94
    };
    for size in &mut settings.size {
        *size = if size.is_finite() {
            size.clamp(48.0, 4096.0)
        } else {
            300.0
        };
    }
    let Some(mut position) = settings.position else {
        return;
    };
    if position.iter().any(|x| !x.is_finite()) {
        settings.position = None;
        return;
    }
    let rect = RECT {
        left: (position[0] * scale) as i32,
        top: (position[1] * scale) as i32,
        right: ((position[0] + settings.size[0]) * scale) as i32,
        bottom: ((position[1] + settings.size[1]) * scale) as i32,
    };
    // SAFETY: RECT/MONITORINFO pointers remain valid for these synchronous calls.
    unsafe {
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0 {
            let work = info.rcWork;
            settings.size[0] = settings.size[0].min((work.right - work.left) as f32 / scale);
            settings.size[1] = settings.size[1].min((work.bottom - work.top) as f32 / scale);
            position[0] = position[0].clamp(
                work.left as f32 / scale,
                (work.right as f32 / scale - settings.size[0]).max(work.left as f32 / scale),
            );
            position[1] = position[1].clamp(
                work.top as f32 / scale,
                (work.bottom as f32 / scale - settings.size[1]).max(work.top as f32 / scale),
            );
            settings.position = Some(position);
        }
    }
}

/// Called on a dedicated worker so Explorer cannot stall rendering or playback.
pub fn reveal_file(path: &std::path::Path) -> Result<()> {
    use windows_sys::Win32::{
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
        UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems},
    };
    let absolute = path.canonicalize()?;
    let raw = absolute.to_string_lossy();
    let display = if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_owned()
    };
    let wide: Vec<u16> = display.encode_utf16().chain(Some(0)).collect();
    // SAFETY: this worker owns its COM apartment. PIDL and UTF-16 buffers remain
    // alive for the synchronous shell call; each allocation/init is paired below.
    unsafe {
        let initialized = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        anyhow::ensure!(
            initialized >= 0,
            "COM initialization failed: {initialized:#x}"
        );
        let pidl = ILCreateFromPathW(wide.as_ptr());
        let result = if pidl.is_null() {
            Err(anyhow::anyhow!("Windows 無法識別檔案路徑"))
        } else {
            let result = SHOpenFolderAndSelectItems(pidl, 0, std::ptr::null(), 0);
            ILFree(pidl);
            if result >= 0 {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Explorer selection failed: {result:#x}"))
            }
        };
        CoUninitialize();
        result
    }
}
