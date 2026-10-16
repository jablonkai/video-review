//! Draws a mark the way an editor would with a chinagraph pencil on film:
//! a hand-drawn loop for a pin, a slightly overshooting box for an area.
//! The wobble is seeded so a mark looks the same every time it is drawn.

use std::f32::consts::PI;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};

use crate::model::Area;
use crate::theme::{self, Face};

pub struct GreasePencil {
    state: u64,
}

impl GreasePencil {
    pub fn new(seed: &str) -> Self {
        // FNV-1a, so the same id always wobbles the same way.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in seed.bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        Self { state: h | 1 }
    }

    fn next(&mut self) -> f32 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        ((self.state.wrapping_mul(0x2545F4914F6CDD1D) >> 40) as f32) / (1u64 << 24) as f32
    }

    fn j(&mut self, amount: f32) -> f32 {
        (self.next() - 0.5) * 2.0 * amount
    }

    /// Draws `area` inside `frame` (the picture rect). Returns the anchor for a label.
    pub fn draw(&mut self, painter: &Painter, frame: Rect, area: &Area, width: f32, dashed: bool) -> Pos2 {
        let o = pos2(frame.left() + area.x as f32 * frame.width(), frame.top() + area.y as f32 * frame.height());
        let (strokes, anchor) = if area.is_pin() {
            let r = (frame.width().min(frame.height()) * 0.035).max(10.0);
            (vec![self.lp(o, r)], o + vec2(r * 0.8, -r * 1.25))
        } else {
            let rect = Rect::from_min_size(o, vec2(area.w as f32 * frame.width(), area.h as f32 * frame.height()));
            (self.bx(rect), rect.left_top() + vec2(0.0, -6.0))
        };
        let shadow = Stroke::new(width + 2.4, Color32::from_black_alpha(120));
        let ink = Stroke::new(width, theme::PENCIL);
        for pass in [shadow, ink] {
            for pts in &strokes {
                if dashed {
                    painter.extend(Shape::dashed_line(pts, pass, 8.0, 5.0));
                } else {
                    painter.add(Shape::line(pts.clone(), pass));
                }
            }
        }
        // Round the ends, like a pencil tip.
        for pts in &strokes {
            if !dashed {
                for p in [pts[0], pts[pts.len() - 1]] {
                    painter.circle_filled(p, width / 2.0, theme::PENCIL);
                }
            }
        }
        anchor
    }

    /// A loop drawn in one stroke that overlaps its own start, like a real circle by hand.
    fn lp(&mut self, c: Pos2, r: f32) -> Vec<Pos2> {
        let steps = 40;
        let start = self.next() * PI * 2.0;
        let sweep = PI * 2.0 * 1.12;
        let squash = 1.0 + self.j(0.08);
        (0..=steps)
            .map(|i| {
                let a = start + sweep * i as f32 / steps as f32;
                let rr = r * (1.0 + self.j(0.04) + 0.06 * i as f32 / steps as f32); // drifts outward slightly
                c + vec2(a.cos() * rr * squash, a.sin() * rr / squash)
            })
            .collect()
    }

    /// Four strokes that overshoot the corners a little.
    fn bx(&mut self, r: Rect) -> Vec<Vec<Pos2>> {
        let over = (r.width().min(r.height()) * 0.12).min(8.0);
        let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        corners
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let dir = (b - a).normalized();
                let s = a - dir * over * (0.3 + self.next() * 0.5) + vec2(self.j(1.5), self.j(1.5));
                let e = b + dir * over * (0.3 + self.next() * 0.5) + vec2(self.j(1.5), self.j(1.5));
                let mid = s.lerp(e, 0.5) + Vec2::new(-dir.y, dir.x) * self.j(2.2);
                (0..=16)
                    .map(|i| {
                        let t = i as f32 / 16.0;
                        let u = 1.0 - t;
                        pos2(
                            u * u * s.x + 2.0 * u * t * mid.x + t * t * e.x,
                            u * u * s.y + 2.0 * u * t * mid.y + t * t * e.y,
                        )
                    })
                    .collect()
            })
            .collect()
    }
}

/// Small number tag next to a mark.
pub fn tag(painter: &Painter, at: Pos2, label: &str, scale: f32) {
    let galley = painter.layout_no_wrap(label.to_owned(), theme::font(Face::Tag, 11.0 * scale), theme::ON_PENCIL);
    let h = 17.0 * scale;
    let rect = Rect::from_min_size(pos2(at.x, at.y - h), vec2(galley.size().x + 10.0 * scale, h));
    painter.rect_filled(rect.translate(vec2(0.0, 1.0)), 3.0, Color32::from_black_alpha(115));
    painter.rect_filled(rect, 3.0, theme::PENCIL);
    let pos = pos2(rect.left() + 5.0 * scale, rect.center().y - galley.size().y / 2.0);
    painter.galley(pos, galley, theme::ON_PENCIL);
}
