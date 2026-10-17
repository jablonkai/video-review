//! The picture, plus the layer for marking places on it.

use egui::{Color32, Pos2, Rect, Sense, Ui, pos2, vec2};

use crate::i18n::tr;
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

/// Smallest area side (normalized); a thinner drag counts as a pin.
const MIN_AREA_SIDE: f64 = 0.005;

/// The area spanned by a drag from `a` to `b` (normalized). A drag that is
/// too thin along either axis would leave an invisible area, so it becomes a
/// pin at its start point instead.
fn drag_area(a: (f64, f64), b: (f64, f64)) -> Area {
    let (w, h) = ((a.0 - b.0).abs(), (a.1 - b.1).abs());
    if w < MIN_AREA_SIDE || h < MIN_AREA_SIDE {
        return Area::pin(a.0, a.1);
    }
    Area { x: a.0.min(b.0), y: a.1.min(b.1), w, h }
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
        (((p.x - frame.left()) / frame.width()).clamp(0.0, 1.0) as f64, ((p.y - frame.top()) / frame.height()).clamp(0.0, 1.0) as f64)
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
                && (drag.moved || (p - s).length() >= 6.0)
            {
                drag.moved = true;
                r.area = Some(drag_area(norm(s), norm(p)));
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
        tag(&painter, anchor, &tr("mark.new"), 1.0);
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_drags_become_pins_at_the_start() {
        assert_eq!(drag_area((0.5, 1.0), (0.8, 1.0)), Area::pin(0.5, 1.0));
        assert_eq!(drag_area((0.3, 0.6), (0.3, 0.2)), Area::pin(0.3, 0.6));
        assert_eq!(drag_area((0.3, 0.6), (0.7, 0.603)), Area::pin(0.3, 0.6));
        let a = drag_area((0.6, 0.5), (0.2, 0.1));
        assert_eq!((a.x, a.y), (0.2, 0.1));
        assert!((a.w - 0.4).abs() < 1e-9 && (a.h - 0.4).abs() < 1e-9);
    }
}
