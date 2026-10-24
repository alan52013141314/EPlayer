use eframe::egui::{self, Color32, Pos2, Stroke};

pub fn button(ui: &mut egui::Ui, symbol: &str, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let visuals = ui.style().interact(&response);
    let c = rect.center();
    let color = visuals.fg_stroke.color;
    let stroke = Stroke::new(1.7_f32, color);
    ui.painter()
        .circle(c, 15.0, visuals.bg_fill, visuals.bg_stroke);
    let p = ui.painter();
    match symbol {
        "◐" => {
            p.circle_filled(c, 7.0, color);
            p.circle_filled(c + egui::vec2(4.0, -3.0), 6.0, visuals.bg_fill);
        }
        "✎" => {
            p.line_segment(
                [c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)],
                Stroke::new(3.0_f32, color),
            );
            p.line_segment(
                [c + egui::vec2(-6.0, 7.0), c + egui::vec2(6.0, 7.0)],
                stroke,
            );
        }
        "play" => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-4.0, -7.0),
                    c + egui::vec2(7.0, 0.0),
                    c + egui::vec2(-4.0, 7.0),
                ],
                color,
                Stroke::NONE,
            ));
        }
        "pause" => {
            for x in [-4.0, 4.0] {
                p.line_segment(
                    [c + egui::vec2(x, -7.0), c + egui::vec2(x, 7.0)],
                    Stroke::new(3.0_f32, color),
                );
            }
        }
        "gear" => {
            p.circle_stroke(c, 6.0, stroke);
            p.circle_stroke(c, 2.0, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let v = egui::vec2(a.cos(), a.sin());
                p.line_segment([c + v * 6.0, c + v * 9.0], Stroke::new(3.0_f32, color));
            }
        }
        "back" => {
            p.line_segment([c + egui::vec2(7.0, 0.0), c - egui::vec2(7.0, 0.0)], stroke);
            p.line_segment(
                [c + egui::vec2(-7.0, 0.0), c + egui::vec2(-1.0, -6.0)],
                stroke,
            );
            p.line_segment(
                [c + egui::vec2(-7.0, 0.0), c + egui::vec2(-1.0, 6.0)],
                stroke,
            );
        }
        "close" => {
            for sign in [-1.0, 1.0] {
                p.line_segment(
                    [
                        c + egui::vec2(-5.0, sign * 5.0),
                        c + egui::vec2(5.0, -sign * 5.0),
                    ],
                    stroke,
                );
            }
        }
        "min" => {
            p.line_segment([c - egui::vec2(6.0, 0.0), c + egui::vec2(6.0, 0.0)], stroke);
        }
        "max" => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(12.0, 12.0)),
                0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        "prev" | "next" => {
            let d = if symbol == "next" { 1.0 } else { -1.0 };
            p.add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-5.0 * d, -6.0),
                    c + egui::vec2(4.0 * d, 0.0),
                    c + egui::vec2(-5.0 * d, 6.0),
                ],
                color,
                Stroke::NONE,
            ));
            p.line_segment(
                [c + egui::vec2(6.0 * d, -6.0), c + egui::vec2(6.0 * d, 6.0)],
                stroke,
            );
        }
        _ => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                symbol,
                egui::FontId::proportional(16.0),
                color,
            );
        }
    }
    response.on_hover_text(label)
}

pub fn enabled_button(
    ui: &mut egui::Ui,
    enabled: bool,
    symbol: &str,
    label: &str,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| button(ui, symbol, label))
        .inner
}

pub fn marquee_offset(elapsed: f64, overflow: f32) -> f32 {
    if overflow <= 0.0 {
        return 0.0;
    }
    let travel = f64::from(overflow) / 30.0;
    let phase = elapsed.rem_euclid(2.0 + travel + 1.5);
    ((phase - 2.0).max(0.0) * 30.0).min(f64::from(overflow)) as f32
}

pub fn marquee(ui: &mut egui::Ui, text: &str, width: f32) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width.max(30.0), 32.0), egui::Sense::drag());
    let now = ui.input(|i| i.time);
    let id = ui.id().with("marquee_start");
    let start = ui.data_mut(|d| {
        let state = d.get_temp_mut_or_insert_with(id, || (text.to_owned(), now));
        if state.0 != text {
            *state = (text.to_owned(), now);
        }
        state.1
    });
    let color = ui.visuals().text_color();
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(17.0), color);
    let overflow = (galley.size().x - rect.width()).max(0.0);
    let offset = marquee_offset(now - start, overflow);
    let origin = Pos2::new(
        rect.left() - offset,
        rect.center().y - galley.size().y / 2.0,
    );
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(origin, galley, Color32::WHITE);
    if overflow > 0.0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    response.on_hover_text(text)
}

pub fn title_bar(ui: &mut egui::Ui, title: &str, close: &mut bool, main: bool) {
    ui.horizontal(|ui| {
        let controls = if main { 126.0 } else { 42.0 };
        let (r, drag) = ui.allocate_exact_size(
            egui::vec2((ui.available_width() - controls).max(40.0), 32.0),
            egui::Sense::click_and_drag(),
        );
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(18.0),
            ui.visuals().text_color(),
        );
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if main && drag.double_clicked() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        if main {
            if button(ui, "min", "最小化").clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if button(ui, "max", if maximized { "還原" } else { "最大化" }).clicked() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        if button(ui, "close", "關閉").clicked() {
            *close = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::marquee_offset;
    #[test]
    fn marquee_holds_scrolls_and_returns() {
        assert_eq!(marquee_offset(1.0, 90.0), 0.0);
        assert_eq!(marquee_offset(3.0, 90.0), 30.0);
        assert_eq!(marquee_offset(6.0, 90.0), 90.0);
        assert_eq!(marquee_offset(6.5, 90.0), 0.0);
        assert_eq!(marquee_offset(10.0, 0.0), 0.0);
    }
}
