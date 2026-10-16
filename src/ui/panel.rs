//! Right-hand column: writing a note, then the notes themselves.

use std::time::{Duration, Instant};

use egui::text::LayoutJob;
use egui::{Align, Color32, Frame, Key, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, TextFormat, Ui, UiBuilder, pos2, vec2};

use crate::model::secs;
use crate::review::{Review, SpanMode};
use crate::theme::{self, Face, font, icon};
use crate::ui::pencil::{GreasePencil, tag};
use crate::ui::timeline::cover_uv;

const NOTE_W: f32 = 108.0;
/// How long the undo offer stays on screen; Cmd+Z works until the next delete or done.
const UNDO_SHOWN: Duration = Duration::from_secs(8);

#[derive(Default)]
pub struct PanelState {
    pub draft: String,
    /// Set to move keyboard focus into the draft on the next frame.
    pub focus_draft: bool,
    editing: Option<(String, String)>,
    edit_needs_focus: bool,
    armed_delete: Option<(String, Instant)>,
}

pub fn draft_id() -> egui::Id {
    egui::Id::unique("draft-note")
}

pub fn panel(ui: &mut Ui, r: &mut Review, st: &mut PanelState) {
    Frame::NONE.inner_margin(Margin { left: 20, right: 20, top: 18, bottom: 18 }).show(ui, |ui| composer(ui, r, st));
    hairline(ui);
    notices(ui, r);

    let open = r.comments.iter().filter(|c| !c.done).count();
    let done = r.comments.len() - open;
    Frame::NONE.inner_margin(Margin { left: 20, right: 12, top: 14, bottom: 6 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Notes").font(font(Face::UiStrong, 14.0)).color(theme::PAPER));
            ui.add_space(4.0);
            if !r.comments.is_empty() {
                let s = if open == 0 { "all done".to_owned() } else { format!("{open} open") };
                ui.label(RichText::new(s).font(font(Face::Ui, 12.5)).color(theme::MUTED));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if done > 0 {
                    let label = if r.hide_done { format!("Show done ({done})") } else { "Hide done".into() };
                    if quiet_button(ui, &label).clicked() {
                        r.hide_done = !r.hide_done;
                    }
                }
            });
        });
    });

    if r.comments.is_empty() {
        Frame::NONE.inner_margin(Margin { left: 20, right: 24, top: 8, bottom: 0 }).show(ui, |ui| {
            ui.label(
                RichText::new(
                    "No notes yet. Pause the video where you have something to say, \
                     then start typing or press M.",
                )
                .font(font(Face::Note, 14.0))
                .color(theme::MUTED),
            );
        });
        return;
    }

    let mut hovered: Option<String> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        Frame::NONE.inner_margin(Margin { left: 8, right: 8, top: 0, bottom: 20 }).show(ui, |ui| {
            let ids: Vec<(usize, String)> = r
                .comments
                .iter()
                .enumerate()
                .filter(|(_, c)| !(r.hide_done && c.done))
                .map(|(i, c)| (i + 1, c.id.clone()))
                .collect();
            for (number, id) in ids {
                if note_row(ui, r, st, number, &id) {
                    hovered = Some(id);
                }
                ui.add_space(2.0);
            }
        });
    });
    r.hover_id = hovered;
}

/// File problems, and the undo offer after a delete or done.
fn notices(ui: &mut Ui, r: &mut Review) {
    let undo_left = r.undo.as_ref().map(|u| UNDO_SHOWN.saturating_sub(u.at.elapsed())).filter(|d| !d.is_zero());
    if r.error.is_none() && undo_left.is_none() {
        return;
    }
    Frame::NONE.inner_margin(Margin { left: 20, right: 20, top: 12, bottom: 0 }).show(ui, |ui| {
        if let Some(e) = r.error.clone() {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(vec2(ui.available_width() - 26.0, 20.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(e).font(font(Face::Ui, 12.5)).color(theme::RED)).wrap());
                });
                if icon_button(ui, icon::CLOSE, 14.0, vec2(20.0, 20.0), theme::MUTED, "Close").clicked() {
                    r.error = None;
                }
            });
        }
        if let (Some(left), Some(label)) = (undo_left, r.undo.as_ref().map(|u| u.label)) {
            ui.ctx().request_repaint_after(left);
            Frame::NONE.fill(theme::RAISED).corner_radius(5).inner_margin(Margin { left: 12, right: 6, top: 4, bottom: 4 }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).font(font(Face::Ui, 12.5)).color(theme::PAPER));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let b = egui::Button::new(RichText::new("Undo").font(font(Face::UiStrong, 12.5)).color(theme::PENCIL)).frame(false);
                        if ui.add(b).on_hover_text("Cmd+Z").clicked() {
                            r.undo();
                        }
                    });
                });
            });
        }
    });
}

fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE);
}

fn quiet_button(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).font(font(Face::Ui, 12.0)).color(theme::MUTED)).frame(false))
}

/// Icon glyph button with a soft hover background.
pub fn icon_button(ui: &mut Ui, glyph: char, size: f32, box_size: egui::Vec2, color: Color32, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(box_size, Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, box_size.y / 2.0, theme::RAISED);
    }
    let c = if resp.hovered() && color == theme::MUTED { theme::PAPER } else { color };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, glyph, font(Face::Icons, size), c);
    resp.on_hover_text(tip)
}

fn composer(ui: &mut Ui, r: &mut Review, st: &mut PanelState) {
    ui.label(RichText::new("New note").font(font(Face::UiStrong, 14.0)).color(theme::PAPER));
    ui.add_space(6.0);
    mode_switch(ui, r);
    ui.add_space(4.0);
    target(ui, r);
    ui.add_space(4.0);

    let id = draft_id();
    let has_focus = ui.memory(|m| m.has_focus(id));
    if has_focus {
        // Enter saves, Shift+Enter is a new line.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) && !st.draft.trim().is_empty() {
            r.add_comment(&st.draft);
            st.draft.clear();
            ui.memory_mut(|m| m.surrender_focus(id));
        }
    }
    let focused_look = has_focus || st.focus_draft;
    Frame::NONE
        .fill(theme::SURROUND)
        .stroke(Stroke::new(1.0, if focused_look { theme::PENCIL } else { theme::HAIRLINE }))
        .corner_radius(5)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let edit = egui::TextEdit::multiline(&mut st.draft)
                .id(id)
                .font(font(Face::Note, 15.0))
                .hint_text(RichText::new("What's wrong here, or what works?").font(font(Face::Note, 15.0)).color(theme::FAINT))
                .frame(Frame::NONE)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .lock_focus(true);
            let resp = ui.add(edit);
            if st.focus_draft {
                resp.request_focus();
                st.focus_draft = false;
            }
            if resp.gained_focus() || resp.clicked() {
                r.pause();
            }
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Enter saves, Shift+Enter starts a new line").font(font(Face::Ui, 11.5)).color(theme::FAINT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let enabled = !st.draft.trim().is_empty();
            let (fill, fg) = if enabled { (theme::PENCIL, theme::ON_PENCIL) } else { (theme::RAISED, theme::FAINT) };
            let btn = egui::Button::new(RichText::new("Save").font(font(Face::UiStrong, 13.0)).color(fg))
                .fill(fill)
                .stroke(Stroke::NONE)
                .corner_radius(5)
                .min_size(vec2(86.0, 38.0));
            if ui.add_enabled(enabled, btn).clicked() {
                r.add_comment(&st.draft);
                st.draft.clear();
            }
        });
    });
}

/// Moment / Scene / Range as one quiet control.
fn mode_switch(ui: &mut Ui, r: &mut Review) {
    let items = [
        (SpanMode::Moment, "Moment", "Just this one frame", true),
        (SpanMode::Scene, "Scene", "The whole scene (S, Shift+S extends)", r.scene_count() > 0),
        (SpanMode::Range, "Range", "Any range: drag on the ruler, or press I and O", true),
    ];
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
    ui.painter().rect_filled(outer, 6.0, theme::SURROUND);
    let inner = outer.shrink(3.0);
    let w = inner.width() / 3.0;
    let current = r.mode();
    for (i, (mode, label, tip, enabled)) in items.into_iter().enumerate() {
        let cell = Rect::from_min_size(pos2(inner.left() + i as f32 * w, inner.top()), vec2(w, inner.height()));
        let resp = ui.interact(cell, ui.scope_id().with(("mode", i)), if enabled { Sense::click() } else { Sense::hover() });
        let on = current == mode;
        if on {
            ui.painter().rect(cell, 4.0, theme::RAISED, Stroke::new(1.0, theme::HAIRLINE), StrokeKind::Inside);
        } else if resp.hovered() && enabled {
            ui.painter().rect_filled(cell, 4.0, Color32::from_white_alpha(6));
        }
        let color = if !enabled {
            theme::FAINT
        } else if on {
            theme::PAPER
        } else {
            theme::MUTED
        };
        let f = if on { font(Face::UiStrong, 13.0) } else { font(Face::Ui, 13.0) };
        ui.painter().text(cell.center(), egui::Align2::CENTER_CENTER, label, f, color);
        if resp.on_hover_text(tip).clicked() {
            r.set_mode(mode);
        }
    }
}

/// Spells out what the note will be attached to.
fn target(ui: &mut Ui, r: &mut Review) {
    let t = r.draft_target();
    let mut clear_time = false;
    let mut clear_area = false;
    if let Some(pi) = r.pending_in {
        clear_time |= line(ui, icon::FIRST_PAGE, &format!("In point {}. Go to the end and press O.", r.tc(pi)), theme::LEADER, true);
    } else if let (Some(sc), Some(out)) = (t.scenes, t.t_out) {
        let s = format!("{}, {} – {} ({})", Review::scene_label(sc), r.tc(t.t_in), r.tc(out), secs(t.t_in, out));
        clear_time |= line(ui, icon::MOVIE, &s, theme::LEADER, true);
    } else if let Some(out) = t.t_out {
        let s = format!("{} – {} ({})", r.tc(t.t_in), r.tc(out), secs(t.t_in, out));
        clear_time |= line(ui, icon::STRAIGHTEN, &s, theme::LEADER, true);
    } else {
        line(ui, icon::ADJUST, &format!("Frame {}", r.tc(t.t_in)), theme::MUTED, false);
    }
    if let Some(a) = r.area {
        let (g, s) = if a.is_pin() { (icon::PIN, "Marked point on the picture") } else { (icon::CROP, "Marked area on the picture") };
        clear_area |= line(ui, g, s, theme::PENCIL, true);
    } else if !r.playing && r.current.is_some() {
        line(ui, icon::TOUCH, "Click or drag on the picture to mark a spot", theme::FAINT, false);
    }
    if clear_time {
        r.clear_time();
    }
    if clear_area {
        r.area = None;
    }
}

/// One line of the target description. Returns true when its × was clicked.
fn line(ui: &mut Ui, glyph: char, text: &str, color: Color32, clearable: bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(glyph.to_string()).font(font(Face::Icons, 15.0)).color(color));
        let avail = ui.available_width() - if clearable { 26.0 } else { 0.0 };
        ui.allocate_ui_with_layout(vec2(avail, 20.0), Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(text).font(font(Face::Tc, 12.5)).color(color)).wrap());
        });
        if clearable {
            clicked = icon_button(ui, icon::CLOSE, 14.0, vec2(20.0, 20.0), theme::MUTED, "Clear").clicked();
        }
    });
    clicked
}

/// One note. Returns true while hovered.
fn note_row(ui: &mut Ui, r: &mut Review, st: &mut PanelState, number: usize, id: &str) -> bool {
    let Some(cm) = r.comments.iter().find(|c| c.id == id).cloned() else { return false };
    let focused = r.focus_id.as_deref() == Some(id);
    let active = r.is_active(&cm, r.position());
    let aspect = if r.video_w > 0 && r.video_h > 0 { r.video_w as f32 / r.video_h as f32 } else { 16.0 / 9.0 };
    let frame_h = NOTE_W / aspect;
    let editing = st.editing.as_ref().is_some_and(|(eid, _)| eid == id);
    let armed = st.armed_delete.as_ref().is_some_and(|(aid, at)| aid == id && at.elapsed() < Duration::from_millis(2500));
    let row_id = ui.scope_id().with(("note", id));
    let was_hovered = ui.ctx().data(|d| d.get_temp::<bool>(row_id)).unwrap_or(false);

    let bg = ui.painter().add(egui::Shape::Noop);
    let builder = UiBuilder::new().id_salt(row_id).sense(Sense::click()).layout(Layout::left_to_right(Align::Min));
    let resp = ui
        .scope_builder(builder, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.add_space(6.0);
            // Active marker: lit while the playhead is inside the note's time.
            let (bar, _) = ui.allocate_exact_size(vec2(3.0, frame_h + 20.0), Sense::hover());
            if active {
                ui.painter().rect_filled(
                    Rect::from_min_size(pos2(bar.left(), bar.top() + 10.0), vec2(3.0, frame_h)),
                    2.0,
                    theme::PENCIL,
                );
            }
            ui.add_space(9.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                note_frame(ui, r, &cm, number, frame_h);
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.set_width(ui.available_width() - 8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let when = match cm.t_out {
                        Some(out) => format!("{} – {}", r.tc(cm.t_in), r.tc(out)),
                        None => r.tc(cm.t_in),
                    };
                    let mut job = LayoutJob::default();
                    job.append(&when, 0.0, TextFormat { font_id: font(Face::Tc, 12.0), color: theme::MUTED, ..Default::default() });
                    if let Some(sc) = cm.scenes {
                        job.append(
                            &format!("   {}", Review::scene_label(sc)),
                            0.0,
                            TextFormat { font_id: font(Face::Ui, 11.5), color: theme::FAINT, ..Default::default() },
                        );
                    }
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    let tools_w = if was_hovered || armed { 92.0 } else if cm.done { 30.0 } else { 0.0 };
                    let avail = (ui.available_width() - tools_w).max(40.0);
                    ui.allocate_ui_with_layout(vec2(avail, 26.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(job).truncate());
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let show = was_hovered || armed || cm.done;
                        if !show {
                            return;
                        }
                        if was_hovered || armed {
                            if armed {
                                let b = egui::Button::new(RichText::new("Delete?").font(font(Face::UiStrong, 12.0)).color(theme::RED))
                                    .frame(false)
                                    .min_size(vec2(0.0, 26.0));
                                if ui.add(b).clicked() {
                                    st.armed_delete = None;
                                    r.remove_comment(id);
                                    return;
                                }
                            } else if icon_button(ui, icon::DELETE, 16.0, vec2(28.0, 26.0), theme::MUTED, "Delete").clicked() {
                                st.armed_delete = Some((id.to_owned(), Instant::now()));
                                ui.ctx().request_repaint_after(Duration::from_millis(2600));
                            }
                            if icon_button(ui, icon::EDIT, 16.0, vec2(28.0, 26.0), theme::MUTED, "Edit").clicked() {
                                st.editing = Some((id.to_owned(), cm.text.clone()));
                                st.edit_needs_focus = true;
                            }
                        }
                        let (g, c, tip) = if cm.done {
                            (icon::CHECK, theme::DONE, "Reopen")
                        } else {
                            (icon::CHECK_OUTLINE, theme::MUTED, "Done")
                        };
                        if icon_button(ui, g, 16.0, vec2(28.0, 26.0), c, tip).clicked() {
                            r.set_done(id, !cm.done);
                        }
                    });
                });
                ui.add_space(3.0);
                if editing {
                    editor(ui, r, st, id);
                } else {
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = ui.available_width();
                    job.append(
                        &cm.text,
                        0.0,
                        TextFormat {
                            font_id: font(Face::Note, 14.5),
                            color: if cm.done { theme::MUTED } else { theme::PAPER },
                            strikethrough: if cm.done { Stroke::new(1.0, theme::FAINT) } else { Stroke::NONE },
                            line_height: Some(14.5 * 1.45),
                            ..Default::default()
                        },
                    );
                    ui.add(egui::Label::new(job).selectable(false));
                }
                ui.add_space(10.0);
            });
        })
        .response;

    let hovered = resp.hovered() || resp.contains_pointer();
    ui.ctx().data_mut(|d| d.insert_temp(row_id, hovered));
    let pointed = r.hover_id.as_deref() == Some(id); // e.g. from the ruler
    if focused || hovered || pointed {
        ui.painter().set(bg, egui::Shape::rect_filled(resp.rect, 6.0, theme::RAISED));
    }
    if resp.clicked() && !editing {
        r.focus_comment(id);
    }
    hovered
}

fn editor(ui: &mut Ui, r: &mut Review, st: &mut PanelState, id: &str) {
    let edit_id = egui::Id::unique(("edit-note", id));
    let has_focus = ui.memory(|m| m.has_focus(edit_id));
    let mut finish: Option<bool> = None;
    if has_focus {
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)) {
            finish = Some(true);
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            finish = Some(false);
        }
    }
    let Some((_, text)) = st.editing.as_mut() else { return };
    let resp = Frame::NONE
        .fill(theme::SURROUND)
        .stroke(Stroke::new(1.0, theme::PENCIL))
        .corner_radius(4)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .id(edit_id)
                    .font(font(Face::Note, 14.5))
                    .frame(Frame::NONE)
                    .desired_rows(1)
                    .desired_width(f32::INFINITY),
            )
        })
        .inner;
    if st.edit_needs_focus {
        resp.request_focus();
        st.edit_needs_focus = false;
    } else if finish.is_none() && resp.lost_focus() {
        finish = Some(true); // clicking elsewhere keeps the edit
    }
    if let Some(save) = finish
        && let Some((_, text)) = st.editing.take()
            && save {
                r.update_text(id, &text);
            }
}

/// The note's own frame, with its mark drawn on it.
fn note_frame(ui: &mut Ui, r: &mut Review, cm: &crate::model::Comment, number: usize, h: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(NOTE_W, h), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, theme::FILM_BASE);
    let tex = if cm.done { r.thumbs.gray_at(cm.t_in) } else { r.thumbs.at(cm.t_in) };
    let tint = if cm.done { Color32::from_white_alpha(140) } else { Color32::WHITE };
    if let Some(tex) = tex {
        p.add(
            egui::epaint::RectShape::filled(rect, 3.0, tint)
                .with_texture(tex.id(), cover_uv(tex.size(), rect)),
        );
    }
    if let Some(area) = cm.area {
        GreasePencil::new(&cm.id).draw(&p, rect, &area, 1.8, false);
    }
    tag(&p, rect.left_top() + vec2(4.0, 18.0), &number.to_string(), 0.85);
}
