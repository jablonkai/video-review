//! The picture, plus the layer for marking places on it.

use egui::{Color32, Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::model::Area;
use crate::review::Review;

use crate::ui::pencil::{GreasePencil, tag};

/// Where the picture actually sits inside the box (letterboxing aside).
pub fn content_rect(b: Rect, vw: i64, vh: i64) -> Rect {
    if vw <= 0 || vh <= 0 {
        return b;
    }
    let scale = (b.width() / vw as f32).min(b.height() / vh as f32);
    Rect::from_center_size(b.center(), vec2(vw as f32 * scale, vh as f32 * scale))
}

#[derive(Default)]
pub struct StageDrag {
    start: Option<Pos2>,
    moved: bool,
}

/// Returns true when a place was just marked (time to write the note).
pub fn stage(ui: &mut Ui, r: &mut Review, video: Option<egui::TextureId>, drag: &mut StageDrag) -> bool {
    let full = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full, 0.0, Color32::BLACK);
    let frame = content_rect(full, r.video_w, r.video_h);
    if let Some(id) = video {
        ui.painter().image(id, frame, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    let resp = ui.allocate_rect(frame, Sense::click_and_drag());
    if resp.hovered() && !r.playing {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let norm = |p: Pos2| -> (f64, f64) {
        (
            ((p.x - frame.left()) / frame.width()).clamp(0.0, 1.0) as f64,
            ((p.y - frame.top()) / frame.height()).clamp(0.0, 1.0) as f64,
        )
    };

    let mut placed = false;
    let (pressed, down, pointer) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
    if pressed && resp.hovered() {
        if r.playing {
            r.pause();
            drag.start = None;
        } else {
            drag.start = pointer;
            drag.moved = false;
        }
    } else if let Some(s) = drag.start {
        if down {
            if let Some(p) = pointer
                && (drag.moved || (p - s).length() >= 6.0) {
                    drag.moved = true;
                    let (a, b) = (norm(s), norm(p));
                    r.area = Some(Area { x: a.0.min(b.0), y: a.1.min(b.1), w: (a.0 - b.0).abs(), h: (a.1 - b.1).abs() });
                }
        } else {
            if !drag.moved {
                let (x, y) = norm(s);
                r.area = Some(Area::pin(x, y));
            }
            drag.start = None;
            placed = true;
        }
    }

    // Marks only during their own note's time.
    let painter = ui.painter_at(full);
    let t = r.position();
    for (i, c) in r.comments.iter().enumerate() {
        let Some(area) = c.area else { continue };
        if r.hide_done && c.done {
            continue;
        }
        if !r.is_active(c, t) {
            continue;
        }
        let emphasized = r.focus_id.as_deref() == Some(&c.id) || r.hover_id.as_deref() == Some(&c.id);
        let anchor = GreasePencil::new(&c.id).draw(&painter, frame, &area, if emphasized { 3.4 } else { 2.6 }, false);
        tag(&painter, anchor, &(i + 1).to_string(), 1.0);
    }
    if let Some(area) = r.area {
        let anchor = GreasePencil::new("draft").draw(&painter, frame, &area, 2.6, true);
        tag(&painter, anchor, "new", 1.0);
    }
    placed
}
