//! Filmstrip (scenes as real frames) and the time ruler with the notes on it.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::i18n::{trf, trn};
use crate::model::secs;
use crate::review::Review;
use crate::theme::{self, Face};

pub const FILM_H: f32 = 64.0;
pub const RULER_H: f32 = 44.0;
const SPROCKET: f32 = 9.0;

/// Fits `tex` (of `size`) over `dst`, cropping the overflow, like BoxFit.cover.
pub fn cover_uv(size: [usize; 2], dst: Rect) -> Rect {
    let (iw, ih) = (size[0] as f32, size[1] as f32);
    let scale = (dst.width() / iw).max(dst.height() / ih);
    let (sw, sh) = (dst.width() / scale / iw, dst.height() / scale / ih);
    Rect::from_min_size(pos2((1.0 - sw) / 2.0, (1.0 - sh) / 2.0), vec2(sw, sh))
}

/// Scenes as a strip of film: real frames, sprocket holes, a visible gap at
/// every cut. Click selects a scene, Shift+click extends the selection.
pub fn filmstrip(ui: &mut Ui, r: &mut Review) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), FILM_H), Sense::click());
    let scene_at_x = |r: &Review, x: f32| -> Option<u32> {
        (r.scene_count() > 0).then(|| r.scene_at(((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * r.duration))
    };
    let hover = resp.hover_pos().and_then(|p| scene_at_x(r, p.x));
    if let Some(n) = hover
        && let Some((a, b)) = r.scene_range(n)
    {
        r.hover_info = trf("scene.hover", &[("n", &n), ("a", &r.tc(a)), ("b", &r.tc(b)), ("secs", &secs(a, b))]);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked()
        && let Some(n) = resp.interact_pointer_pos().and_then(|p| scene_at_x(r, p.x))
    {
        r.pause();
        if ui.input(|i| i.modifiers.shift) {
            r.extend_scenes(n)
        } else {
            r.select_scenes(n, n)
        }
        if let Some(s) = r.sel {
            r.seek(s.t_in);
        }
    }

    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, theme::FILM_BASE);
    sprockets(&p, rect, rect.top());
    sprockets(&p, rect, rect.bottom() - SPROCKET);
    let d = r.duration;
    if d <= 0.0 {
        return;
    }
    let x = |t: f64| rect.left() + (t / d) as f32 * rect.width();
    let (top, bottom) = (rect.top() + SPROCKET, rect.bottom() - SPROCKET);

    // Before detection finishes the whole video is one "scene".
    let b = if r.scene_count() > 0 { r.bounds() } else { vec![0.0, d] };
    for n in 1..b.len() {
        let (s0, s1) = (b[n - 1], b[n]);
        let left = x(s0) + if n > 1 { 1.5 } else { 0.0 };
        let right = x(s1) - if n < b.len() - 1 { 1.5 } else { 0.0 };
        if right - left < 0.5 {
            continue;
        }
        let tiles = (((s1 - s0) / d * 30.0).round() as usize).max(1);
        let tile_w = (right - left) / tiles as f32;
        for i in 0..tiles {
            let tr = Rect::from_min_max(
                pos2(left + i as f32 * tile_w + if i > 0 { 0.5 } else { 0.0 }, top),
                pos2(left + (i + 1) as f32 * tile_w, bottom),
            );
            match r.thumbs.at(s0 + (s1 - s0) * (i as f64 + 0.5) / tiles as f64) {
                Some(tex) => {
                    p.image(tex.id(), tr, cover_uv(tex.size(), tr), Color32::WHITE);
                }
                None => {
                    p.rect_filled(tr, 0.0, Color32::from_rgb(0x26, 0x27, 0x2A));
                }
            }
        }
        if Some(n as u32) == hover {
            p.rect_filled(Rect::from_min_max(pos2(left, top), pos2(right, bottom)), 0.0, Color32::from_white_alpha(31));
        }
    }

    // Detection running: a leader-blue bar along the top edge shows how far ffmpeg got.
    if let Some(f) = r.scenes_progress() {
        let bar = Rect::from_min_max(pos2(rect.left(), rect.top()), pos2(rect.right(), rect.top() + 2.5));
        p.rect_filled(bar, 0.0, theme::LEADER_WASH);
        p.rect_filled(Rect::from_min_max(bar.min, pos2(rect.left() + f * rect.width(), bar.bottom())), 0.0, theme::LEADER);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }

    if let Some((a, z)) = r.sel.and_then(|s| s.scenes)
        && let (Some((t_in, _)), Some((_, t_out))) = (r.scene_range(a), r.scene_range(z))
    {
        let sr = Rect::from_min_max(pos2(x(t_in), rect.top() + 1.0), pos2(x(t_out), rect.bottom() - 1.0));
        p.rect_filled(sr, 0.0, theme::LEADER_WASH);
        p.rect_stroke(sr.shrink(1.0), 2.0, Stroke::new(2.0, theme::LEADER), StrokeKind::Middle);
    }
    let px = x(r.position());
    p.rect_filled(Rect::from_min_max(pos2(px - 1.0, rect.top()), pos2(px + 1.0, rect.bottom())), 0.0, theme::PAPER);
}

fn sprockets(p: &egui::Painter, rect: Rect, top: f32) {
    let hole = Color32::from_rgb(0x3A, 0x3A, 0x3D);
    let mut x = rect.left() + 4.0;
    while x < rect.right() - 4.0 {
        p.rect_filled(Rect::from_min_size(pos2(x, top + 2.5), vec2(6.0, 4.0)), 1.2, hole);
        x += 12.0;
    }
}

/// Drag state of the ruler between frames.
#[derive(Default)]
pub struct RulerDrag {
    t0: Option<f64>,
    x0: f32,
    moved: bool,
}

const STEPS: [f64; 12] = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0];

/// Time ruler with the notes on it. Click seeks, drag selects a range.
/// Returns true when a range drag just finished (time to write the note).
pub fn ruler(ui: &mut Ui, r: &mut Review, drag: &mut RulerDrag) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), RULER_H), Sense::click_and_drag());
    let d = r.duration;
    let time_at = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64 * d;
    if let Some(p) = resp.hover_pos() {
        if d > 0.0 {
            r.hover_info = r.tc(time_at(p.x));
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let marks = if d > 0.0 { NoteMarks::layout(r, rect) } else { NoteMarks::default() };
    let hit = resp.hover_pos().filter(|_| drag.t0.is_none()).and_then(|p| marks.hit(p));
    if let Some(ids) = &hit {
        // Point at the open one if it is in this stack, else the first.
        let shown = ids.iter().copied().find(|&i| r.focus_id.as_deref() == Some(r.comments[i].id.as_str())).unwrap_or(ids[0]);
        r.hover_id = Some(r.comments[shown].id.clone());
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500)); // tooltip delay
        let lines: Vec<String> = ids
            .iter()
            .map(|&i| {
                let c = &r.comments[i];
                let when = match c.t_out {
                    Some(out) => format!("{} – {}", r.tc(c.t_in), r.tc(out)),
                    None => r.tc(c.t_in),
                };
                let mut text: String = c.text.chars().take(70).collect();
                if c.text.chars().count() > 70 {
                    text = text.trim_end().to_owned() + "…";
                }
                format!("{}.  {when}   {text}", i + 1)
            })
            .collect();
        let many = ids.len() > 1;
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.set_max_width(420.0);
            if many {
                ui.label(RichText::new(trn("ruler.stack", lines.len() as u64, &[])).font(theme::font(Face::UiStrong, 12.0)).color(theme::MUTED));
            }
            for l in &lines {
                ui.label(RichText::new(l).font(theme::font(Face::Ui, 12.5)).color(theme::PAPER));
            }
        });
    }
    let mut finished_range = false;
    let (pressed, down, pointer) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
    if pressed && resp.hovered() && hit.is_some() {
        // On a note: open it. Clicking again steps through notes stacked at the same place.
        let ids = hit.unwrap_or_default();
        let cur = ids.iter().position(|&i| r.focus_id.as_deref() == Some(r.comments[i].id.as_str()));
        let next = cur.map_or(0, |k| (k + 1) % ids.len());
        let id = r.comments[ids[next]].id.clone();
        r.focus_comment(&id);
    } else if pressed && resp.hovered() && d > 0.0 {
        if let Some(p) = pointer {
            let t = time_at(p.x);
            *drag = RulerDrag { t0: Some(t), x0: p.x, moved: false };
            r.seek(t);
        }
    } else if let Some(t0) = drag.t0 {
        if down {
            if let Some(p) = pointer {
                if (p.x - drag.x0).abs() > 4.0 {
                    drag.moved = true;
                }
                if drag.moved {
                    let t = time_at(p.x);
                    r.set_range(t0, t);
                    r.seek(t);
                }
            }
        } else {
            if drag.moved {
                if let Some(s) = r.sel {
                    r.pause();
                    r.seek(s.t_in);
                    finished_range = true;
                }
            } else {
                r.clear_time();
            }
            drag.t0 = None;
        }
    }

    if d <= 0.0 {
        return finished_range;
    }
    let p = ui.painter_at(rect.expand2(vec2(0.0, 0.0)));
    let x = |t: f64| rect.left() + (t / d) as f32 * rect.width();

    // Ticks and labels, spaced so labels never crowd.
    let step = STEPS.iter().copied().find(|s| (s / d) as f32 * rect.width() >= 64.0).unwrap_or(3600.0);
    let minor_step = step / 5.0;
    let mut i = 0;
    while i as f64 * minor_step <= d {
        let tt = i as f64 * minor_step;
        let major = i % 5 == 0;
        let h = if major { 9.0 } else { 5.0 };
        p.rect_filled(
            Rect::from_min_size(pos2(x(tt), rect.bottom() - h), vec2(1.0, h)),
            0.0,
            if major { theme::FAINT } else { theme::HAIRLINE },
        );
        if major {
            let (m, s) = ((tt / 60.0) as i64, (tt % 60.0).round() as i64);
            let g = p.layout_no_wrap(format!("{m}:{s:02}"), theme::font(Face::Ui, 10.0), theme::FAINT);
            if x(tt) + 3.0 + g.size().x < rect.right() {
                p.galley(pos2(x(tt) + 3.0, rect.bottom() - 21.0), g, theme::FAINT);
            }
        }
        i += 1;
    }

    if let Some(s) = r.sel
        && let Some(out) = s.t_out
    {
        let sr = Rect::from_min_max(pos2(x(s.t_in), rect.top()), pos2(x(out), rect.bottom()));
        p.rect_filled(sr, 0.0, theme::LEADER_WASH);
        p.rect_filled(Rect::from_min_size(sr.left_top(), vec2(2.0, rect.height())), 0.0, theme::LEADER);
        p.rect_filled(Rect::from_min_size(pos2(sr.right() - 2.0, sr.top()), vec2(2.0, rect.height())), 0.0, theme::LEADER);
    }
    if let Some(pi) = r.pending_in {
        p.rect_filled(Rect::from_min_size(pos2(x(pi), rect.top()), vec2(2.0, rect.height())), 0.0, theme::LEADER);
    }

    marks.paint(&p, r);

    let px = x(r.position());
    p.rect_filled(Rect::from_min_max(pos2(px - 1.0, rect.top()), pos2(px + 1.0, rect.bottom())), 0.0, theme::PAPER);
    finished_range
}

/// Where the notes sit on the ruler. Overlapping ranges get their own lanes;
/// moments closer than a flag's width are grouped under one flag with a count.
#[derive(Default)]
struct NoteMarks {
    bars: Vec<(Rect, usize)>,
    flags: Vec<(f32, Vec<usize>, Rect)>,
}

const LANES: usize = 4;
const FLAG_GROUP_PX: f32 = 9.0;

impl NoteMarks {
    fn layout(r: &Review, rect: Rect) -> Self {
        let d = r.duration;
        let x = |t: f64| rect.left() + (t / d) as f32 * rect.width();
        let mut out = Self::default();
        let mut spans: Vec<(f32, f32, usize)> = Vec::new();
        let mut points: Vec<(f32, usize)> = Vec::new();
        for (i, c) in r.comments.iter().enumerate() {
            if r.hide_done && c.done {
                continue; // "Hide done" applies here too
            }
            match c.t_out {
                Some(t_out) => spans.push((x(c.t_in), x(t_out).max(x(c.t_in) + 3.0), i)),
                None => points.push((x(c.t_in), i)),
            }
        }
        let lanes = assign_lanes(&spans.iter().map(|&(a, b, _)| (a, b)).collect::<Vec<_>>());
        for (&(a, b, i), lane) in spans.iter().zip(lanes) {
            let top = rect.top() + 11.0 + lane as f32 * 3.5;
            // Small gap at the ends so touching ranges in one lane stay apart.
            let (a, b) = if b - a > 4.0 { (a + 0.75, b - 0.75) } else { (a, b) };
            out.bars.push((Rect::from_min_max(pos2(a, top), pos2(b, top + 2.5)), i));
        }
        out.flags = group_flags(&points).into_iter().map(|(px, ids)| (px, ids, Rect::NOTHING)).collect();
        for (px, ids, hit) in &mut out.flags {
            let badge = if ids.len() > 1 { 6.0 + 7.0 * ids.len().to_string().len() as f32 } else { 0.0 };
            *hit = Rect::from_min_max(pos2(*px - 6.0, rect.top()), pos2(*px + 6.0 + badge, rect.top() + 11.0));
        }
        out
    }

    /// Notes under the pointer, in time order.
    fn hit(&self, p: Pos2) -> Option<Vec<usize>> {
        if let Some((_, ids, _)) = self.flags.iter().find(|(_, _, h)| h.contains(p)) {
            return Some(ids.clone());
        }
        self.bars.iter().rev().find(|(b, _)| b.expand2(vec2(1.0, 1.5)).contains(p)).map(|(_, i)| vec![*i])
    }

    fn paint(&self, p: &egui::Painter, r: &Review) {
        let color = |done: bool| if done { theme::DONE.gamma_multiply(0.7) } else { theme::PENCIL };
        for (b, i) in &self.bars {
            let c = &r.comments[*i];
            let hot = r.hover_id.as_deref() == Some(c.id.as_str()) || r.focus_id.as_deref() == Some(c.id.as_str());
            p.rect_filled(if hot { b.expand2(vec2(0.0, 0.5)) } else { *b }, 1.5, if hot { theme::PAPER } else { color(c.done) });
        }
        for (px, ids, _) in &self.flags {
            let done = ids.iter().all(|&i| r.comments[i].done);
            let hot = ids.iter().any(|&i| {
                let id = r.comments[i].id.as_str();
                r.hover_id.as_deref() == Some(id) || r.focus_id.as_deref() == Some(id)
            });
            let fill = if hot { theme::PAPER } else { color(done) };
            let top = p.clip_rect().top();
            let w = if ids.len() > 1 { 5.5 } else { 4.5 };
            // A stack of notes shows as a deeper flag with a shadow edge behind it.
            if ids.len() > 1 {
                let back = vec![pos2(px - w + 2.0, top + 1.0), pos2(px + w + 2.0, top + 1.0), pos2(px + 2.0, top + 9.0)];
                p.add(egui::Shape::convex_polygon(back, fill.gamma_multiply(0.45), Stroke::NONE));
            }
            let pts = vec![pos2(px - w, top + 1.0), pos2(px + w, top + 1.0), pos2(*px, top + 10.0)];
            p.add(egui::Shape::convex_polygon(pts, fill, Stroke::NONE));
            if ids.len() > 1 {
                let g = p.layout_no_wrap(ids.len().to_string(), theme::font(Face::Tag, 9.5), theme::ON_PENCIL);
                let br = Rect::from_min_size(pos2(px + w + 2.5, top), vec2(g.size().x + 6.0, 11.0));
                p.rect_filled(br, 5.5, fill);
                p.galley(pos2(br.left() + 3.0, br.center().y - g.size().y / 2.0), g, theme::ON_PENCIL);
            }
        }
    }
}

/// Lane (0..LANES) for each bar `(left, right)` in x, given in time order:
/// the first lane that is free again, else a new one, else the one that frees up first.
fn assign_lanes(bars: &[(f32, f32)]) -> Vec<usize> {
    let mut lane_end: Vec<f32> = Vec::new();
    bars.iter()
        .map(|&(a, b)| {
            let lane = match lane_end.iter().position(|&e| e + 2.0 < a) {
                Some(l) => l,
                None if lane_end.len() < LANES => {
                    lane_end.push(f32::MIN);
                    lane_end.len() - 1
                }
                // All lanes busy: share the one that frees up first.
                None => (0..LANES).min_by(|&p, &q| lane_end[p].total_cmp(&lane_end[q])).unwrap_or(0),
            };
            lane_end[lane] = lane_end[lane].max(b);
            lane
        })
        .collect()
}

/// Groups moments `(x, note index)` closer than a flag's width to the group's first one.
fn group_flags(points: &[(f32, usize)]) -> Vec<(f32, Vec<usize>)> {
    let mut flags: Vec<(f32, Vec<usize>)> = Vec::new();
    for &(px, i) in points {
        match flags.last_mut() {
            Some((x0, ids)) if px - *x0 < FLAG_GROUP_PX => ids.push(i),
            _ => flags.push((px, vec![i])),
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_reuse_free_ones_and_cap_at_four() {
        // Apart: all in lane 0. Gap of 2 px or less counts as touching.
        assert_eq!(assign_lanes(&[(0.0, 10.0), (20.0, 30.0), (31.0, 40.0)]), vec![0, 0, 1]);
        // Nested and overlapping ranges stack up.
        assert_eq!(assign_lanes(&[(0.0, 100.0), (10.0, 20.0), (15.0, 50.0), (30.0, 40.0)]), vec![0, 1, 2, 1]);
        // A fifth overlapping range shares the lane that ends first.
        let lanes = assign_lanes(&[(0.0, 50.0), (1.0, 20.0), (2.0, 60.0), (3.0, 70.0), (4.0, 80.0)]);
        assert_eq!(lanes, vec![0, 1, 2, 3, 1]);
        assert!(assign_lanes(&[]).is_empty());
    }

    #[test]
    fn flags_group_close_moments() {
        let groups = group_flags(&[(0.0, 0), (5.0, 1), (8.9, 2), (9.0, 3), (30.0, 4)]);
        assert_eq!(groups, vec![(0.0, vec![0, 1, 2]), (9.0, vec![3]), (30.0, vec![4])]);
    }
}
