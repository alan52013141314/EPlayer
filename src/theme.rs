use eframe::egui::{self, Color32, Stroke};
use std::sync::{Arc, OnceLock};

pub fn icon() -> Arc<egui::IconData> {
    static ICON: OnceLock<Arc<egui::IconData>> = OnceLock::new();
    ICON.get_or_init(|| {
        let image = image::load_from_memory(include_bytes!("../icon.png"))
            .expect("validated embedded icon")
            .resize_exact(128, 128, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        Arc::new(egui::IconData {
            rgba: image.into_raw(),
            width: 128,
            height: 128,
        })
    })
    .clone()
}

pub fn accent(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(194, 169, 255)
    } else {
        Color32::from_rgb(92, 54, 159)
    }
}
pub fn apply(ctx: &egui::Context, dark: bool) {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    let (base, panel, raised, hover, text, muted, border) = if dark {
        (
            Color32::from_rgb(18, 8, 36),
            Color32::from_rgb(28, 16, 47),
            Color32::from_rgb(45, 29, 69),
            Color32::from_rgb(65, 43, 96),
            Color32::from_rgb(242, 234, 255),
            Color32::from_rgb(177, 160, 201),
            Color32::from_rgb(79, 59, 104),
        )
    } else {
        (
            Color32::from_rgb(249, 245, 255),
            Color32::from_rgb(239, 231, 250),
            Color32::from_rgb(229, 216, 248),
            Color32::from_rgb(214, 194, 241),
            Color32::from_rgb(40, 20, 66),
            Color32::from_rgb(105, 82, 133),
            Color32::from_rgb(190, 169, 216),
        )
    };
    v.panel_fill = base;
    v.window_fill = panel;
    v.extreme_bg_color = base;
    v.faint_bg_color = panel;
    v.override_text_color = Some(text);
    v.hyperlink_color = accent(dark);
    v.selection.bg_fill = if dark {
        Color32::from_rgb(83, 53, 123)
    } else {
        Color32::from_rgb(205, 182, 237)
    };
    v.selection.stroke = Stroke::new(1.0_f32, text);
    v.window_stroke = Stroke::new(1.0_f32, border);
    v.window_corner_radius = 12.into();
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = raised;
        w.weak_bg_fill = raised;
        w.bg_stroke = Stroke::new(1.0_f32, border);
        w.fg_stroke = Stroke::new(1.0_f32, text);
        w.corner_radius = 8.into();
    }
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.active.bg_fill = v.selection.bg_fill;
    v.widgets.active.weak_bg_fill = v.selection.bg_fill;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, muted);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border);
    let mut style = (*ctx.style()).clone();
    style.visuals = v;
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 28.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    ctx.set_style(style);
}
