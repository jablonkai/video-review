//! Window layout, header, transport, shortcuts, help.

use std::path::PathBuf;

use egui::text::LayoutJob;
use egui::{Align, Align2, Color32, Frame, Key, Layout, Margin, Rect, RichText, Sense, Stroke, TextFormat, Ui, UiBuilder, pos2, vec2};

use crate::export;
use crate::i18n::{self, Lang, tr, trf, trn};
use crate::model::{VIDEO_EXTENSIONS, secs};
use crate::review::{Review, ScenesStatus};
use crate::theme::{self, Face, font, icon};
use crate::ui::panel::{self, PanelState, icon_button};
use crate::ui::stage::{StageDrag, stage};
use crate::ui::timeline::{RulerDrag, filmstrip, ruler};

pub struct App {
    r: Review,
    panel: PanelState,
    ruler: RulerDrag,
    stage: StageDrag,
    help: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Result<Self, String> {
        theme::apply_style(&cc.egui_ctx);
        let mut r = Review::new(&cc.egui_ctx)?; // picks the language from prefs
        theme::install_fonts(&cc.egui_ctx, i18n::current());
        r.init_gl(cc)?;
        if paths.is_empty() {
            r.restore_last_session()
        } else {
            r.open_paths(paths)
        }
        Ok(Self { r, panel: PanelState::default(), ruler: RulerDrag::default(), stage: StageDrag::default(), help: false })
    }

    fn focus_draft(&mut self) {
        self.r.pause();
        self.panel.focus_draft = true;
    }

    fn open_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new().add_filter(tr("dialog.video_filter"), VIDEO_EXTENSIONS).pick_files() {
            self.r.open_paths(files);
        }
    }

    fn open_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.r.open_paths(vec![dir]);
        }
    }

    /// Saves the notes as EDL / FCPXML / Markdown, picked by the extension.
    fn export_notes(&mut self) {
        let Some(video) = self.r.current.clone() else { return };
        let stem = video.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut dialog = rfd::FileDialog::new()
            .set_title(tr("export.title"))
            .set_file_name(format!("{stem}.edl"))
            .add_filter(tr("export.edl"), &["edl"])
            .add_filter("FCPXML", &["fcpxml"])
            .add_filter("Markdown", &["md"]);
        if let Some(dir) = video.parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else { return };
        let Some(format) = export::Format::from_path(&path) else {
            self.r.error = Some(trf("export.unknown_format", &[("exts", &export::Format::EXTENSIONS.join(", "))]));
            return;
        };
        let name = self.r.current_name();
        let meta = export::Meta { name: &name, fps: self.r.fps, duration: self.r.duration };
        if let Err(e) = std::fs::write(&path, export::render(format, &self.r.comments, &meta)) {
            self.r.error = Some(trf("export.failed", &[("path", &path.display()), ("err", &e)]));
        } else if format == export::Format::Edl && self.r.comments.len() > export::EDL_MAX_EVENTS {
            let n = self.r.comments.len() as u64;
            self.r.error = Some(trn("export.edl_capped", n, &[("max", &export::EDL_MAX_EVENTS)]));
        }
    }

    /// Global shortcuts. Ignored while typing or when the help is on top.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for e in events {
            match e {
                egui::Event::Text(t) if t == "?" => self.help = !self.help,
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if key == Key::Z && modifiers.command && !modifiers.shift && !modifiers.alt && !self.help {
                        self.r.undo();
                        continue;
                    }
                    if modifiers.command || modifiers.ctrl || modifiers.alt {
                        continue;
                    }
                    if key == Key::F1 {
                        self.help = !self.help;
                        continue;
                    }
                    if self.help {
                        if key == Key::Escape {
                            self.help = false;
                        }
                        continue;
                    }
                    if self.r.current.is_none() {
                        continue;
                    }
                    self.key(ctx, key, modifiers.shift);
                }
                _ => {}
            }
        }
    }

    fn key(&mut self, ctx: &egui::Context, key: Key, shift: bool) {
        let r = &mut self.r;
        let t = r.position();
        match key {
            Key::Space => r.toggle_play(),
            Key::K => r.pause(),
            Key::L => r.faster(),
            Key::J => r.seek(t - 2.0),
            Key::ArrowLeft if shift => r.seek(t - 1.0),
            Key::ArrowLeft => r.step_frames(-1),
            Key::ArrowRight if shift => r.seek(t + 1.0),
            Key::ArrowRight => r.step_frames(1),
            Key::ArrowUp => r.jump_cut(-1),
            Key::ArrowDown => r.jump_cut(1),
            Key::I => r.mark_in(),
            Key::O => r.mark_out(),
            Key::S if r.scene_count() > 0 => {
                r.pause();
                let n = r.scene_at(t);
                if shift {
                    r.extend_scenes(n);
                } else {
                    r.select_scenes(n, n);
                    if let Some(s) = r.sel {
                        r.seek(s.t_in);
                    }
                }
            }
            Key::M | Key::Enter => self.focus_draft(),
            Key::N => r.jump_comment(1),
            Key::P => r.jump_comment(-1),
            Key::Escape => r.clear_all(),
            _ => return,
        }
        // Keep arrows/space from also moving egui's own focus.
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    }

    fn header(&mut self, ui: &mut Ui) {
        let r = &mut self.r;
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            let name = RichText::new(r.current_name()).font(font(Face::UiStrong, 14.0)).color(theme::PAPER);
            if r.videos.len() > 1 {
                let mut pick = None;
                let btn = egui::Button::new(
                    RichText::new(format!("{}  {}", r.current_name(), icon::EXPAND)).font(font(Face::UiStrong, 14.0)).color(theme::PAPER),
                )
                .frame(false);
                let resp = ui.add(btn).on_hover_text(tr("header.pick_video"));
                egui::Popup::menu(&resp).show(|ui| {
                    ui.set_min_width(260.0);
                    for v in &r.videos {
                        let cur = r.current.as_ref() == Some(v);
                        let label = v.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let text = RichText::new(label).font(font(if cur { Face::UiStrong } else { Face::Ui }, 13.0)).color(if cur {
                            theme::PAPER
                        } else {
                            theme::MUTED
                        });
                        if ui.add(egui::Button::new(text).frame(false)).clicked() {
                            pick = Some(v.clone());
                        }
                    }
                });
                if let Some(v) = pick {
                    r.load_video(&v);
                }
            } else {
                ui.label(name);
            }
            ui.add_space(12.0);
            let fps = format!("{:.3}", r.fps).trim_end_matches('0').trim_end_matches('.').to_owned();
            let mut meta = Vec::new();
            if r.video_w > 0 {
                meta.push(format!("{}×{}", r.video_w, r.video_h));
            }
            meta.push(trf("header.fps", &[("fps", &fps)]));
            if r.duration > 0.0 {
                meta.push(format!("{}:{:02}", (r.duration / 60.0) as i64, (r.duration % 60.0).floor() as i64));
            }
            ui.label(RichText::new(meta.join("    ")).font(font(Face::Ui, 12.0)).color(theme::FAINT));
        });
    }

    fn header_buttons(&mut self, ui: &mut Ui) -> (bool, bool, bool, bool) {
        let mut out = (false, false, false, false);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(12.0);
            let b = vec2(36.0, 36.0);
            language_menu(ui, &mut self.r, false);
            out.2 = icon_button(ui, icon::KEYBOARD, 19.0, b, theme::MUTED, &tr("header.shortcuts")).clicked();
            out.3 = icon_button(ui, icon::SHARE, 19.0, b, theme::MUTED, &tr("header.export")).clicked();
            out.1 = icon_button(ui, icon::FOLDER, 19.0, b, theme::MUTED, &tr("open.folder")).clicked();
            out.0 = icon_button(ui, icon::MOVIE, 19.0, b, theme::MUTED, &tr("open.video")).clicked();
        });
        out
    }

    fn transport(&mut self, ui: &mut Ui) {
        let row = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover()).0;
        let r = &mut self.r;
        let t = r.position();

        // Timecode: seconds large, frames lighter.
        let tc = r.tc(t);
        let mut job = LayoutJob::default();
        job.append(&tc[..8], 0.0, TextFormat { font_id: font(Face::TcMedium, 28.0), color: theme::PAPER, ..Default::default() });
        job.append(&tc[8..], 0.0, TextFormat { font_id: font(Face::TcLight, 28.0), color: theme::MUTED, ..Default::default() });
        let g = ui.painter().layout_job(job);
        let tc_w = g.size().x;
        let tc_pos = pos2(row.left(), row.center().y - g.size().y / 2.0);
        ui.painter().galley(tc_pos, g, theme::PAPER);

        // Buttons, centered.
        let bw = 36.0 * 4.0 + 42.0 + 12.0;
        let mid = Rect::from_center_size(row.center(), vec2(bw, 44.0));
        let left = Rect::from_min_max(pos2(row.left() + tc_w + 14.0, row.top() + 3.0), pos2(mid.left() - 8.0, row.bottom() + 3.0));
        if left.width() > 0.0 {
            scene_status(ui, r, t, left);
        }
        let mut ui2 = ui.new_child(UiBuilder::new().max_rect(mid).layout(Layout::left_to_right(Align::Center)));
        ui2.spacing_mut().item_spacing.x = 0.0;
        let b = vec2(36.0, 36.0);
        if icon_button(&mut ui2, icon::SKIP_PREV, 22.0, b, theme::MUTED, &tr("transport.prev_cut")).clicked() {
            r.jump_cut(-1);
        }
        if icon_button(&mut ui2, icon::CHEVRON_LEFT, 24.0, b, theme::MUTED, &tr("transport.frame_back")).clicked() {
            r.step_frames(-1);
        }
        ui2.add_space(6.0);
        let (pr, presp) = ui2.allocate_exact_size(vec2(42.0, 42.0), Sense::click());
        let fill = if presp.hovered() { Color32::WHITE } else { theme::PAPER };
        ui2.painter().circle_filled(pr.center(), 21.0, fill);
        let glyph = if r.playing { icon::PAUSE } else { icon::PLAY };
        ui2.painter().text(pr.center(), Align2::CENTER_CENTER, glyph, font(Face::Icons, 24.0), theme::SURROUND);
        if presp.on_hover_text(tr("transport.play_pause")).clicked() {
            r.toggle_play();
        }
        ui2.add_space(6.0);
        if icon_button(&mut ui2, icon::CHEVRON_RIGHT, 24.0, b, theme::MUTED, &tr("transport.frame_fwd")).clicked() {
            r.step_frames(1);
        }
        if icon_button(&mut ui2, icon::SKIP_NEXT, 22.0, b, theme::MUTED, &tr("transport.next_cut")).clicked() {
            r.jump_cut(1);
        }

        // Right: volume, then what the pointer is over, then the speed badge.
        let mut x = volume(ui, r, row) - 18.0;
        if !r.hover_info.is_empty() {
            let g = ui.painter().layout_no_wrap(r.hover_info.clone(), font(Face::Tc, 12.5), theme::MUTED);
            x -= g.size().x;
            ui.painter().galley(pos2(x, row.center().y - g.size().y / 2.0), g, theme::MUTED);
            x -= 12.0;
        }
        if r.rate != 1.0 {
            let g = ui.painter().layout_no_wrap(format!("{:.0}×", r.rate), font(Face::Tc, 12.5), theme::PAPER);
            let br = Rect::from_min_size(pos2(x - g.size().x - 14.0, row.center().y - 11.0), vec2(g.size().x + 14.0, 22.0));
            ui.painter().rect_stroke(br, 4.0, Stroke::new(1.0, theme::HAIRLINE), egui::StrokeKind::Inside);
            ui.painter().galley(pos2(br.left() + 7.0, br.center().y - g.size().y / 2.0), g, theme::PAPER);
        }
    }

    fn empty(&mut self, ui: &mut Ui) {
        let area = ui.available_rect_before_wrap();
        let w = 440.0_f32.min(area.width() - 40.0);
        let r = Rect::from_center_size(area.center(), vec2(w, 220.0));
        let mut ui = ui.new_child(UiBuilder::new().max_rect(r).layout(Layout::top_down(Align::Min)));
        ui.label(RichText::new(tr("empty.title")).font(font(Face::NoteDisplay, 24.0)).color(theme::PAPER));
        ui.add_space(10.0);
        ui.label(RichText::new(tr("empty.body")).font(font(Face::Ui, 13.5)).color(theme::MUTED));
        ui.add_space(22.0);
        let (mut files, mut folder) = (false, false);
        ui.horizontal(|ui| {
            let b = egui::Button::new(
                RichText::new(format!("{}  {}", icon::MOVIE, tr("open.video"))).font(font(Face::UiStrong, 13.5)).color(theme::ON_PENCIL),
            )
            .fill(theme::PENCIL)
            .corner_radius(5)
            .min_size(vec2(0.0, 44.0));
            files = ui.add(b).clicked();
            ui.add_space(10.0);
            let b = egui::Button::new(
                RichText::new(format!("{}  {}", icon::FOLDER, tr("open.folder"))).font(font(Face::Ui, 13.5)).color(theme::PAPER),
            )
            .frame(false)
            .min_size(vec2(0.0, 44.0));
            folder = ui.add(b).clicked();
        });
        ui.add_space(6.0);
        language_menu(&mut ui, &mut self.r, true);
        if let Some(e) = &self.r.error {
            ui.add_space(16.0);
            ui.label(RichText::new(e).font(font(Face::Ui, 13.0)).color(theme::RED));
        }
        if files {
            self.open_files();
        }
        if folder {
            self.open_folder();
        }
    }

    fn help_dialog(&mut self, ctx: &egui::Context) {
        if !self.help {
            return;
        }
        // Catalog keys: `help.<row>.k` is the key or gesture, `help.<row>.d` what it does.
        const GROUPS: &[(&str, &[&str])] = &[
            ("help.playback", &["space", "k", "l", "j", "arrows", "shift_arrows", "updown"]),
            ("help.target", &["s", "io", "drag_ruler", "click_film", "redetect", "click_pic", "drag_pic", "esc"]),
            ("help.notes", &["m", "enter", "np", "undo", "mark"]),
        ];
        let frame = Frame::NONE.fill(theme::PANEL).stroke(Stroke::new(1.0, theme::HAIRLINE)).corner_radius(8).inner_margin(Margin {
            left: 28,
            right: 28,
            top: 24,
            bottom: 24,
        });
        let resp = egui::Modal::new(egui::Id::unique("help")).frame(frame).backdrop_color(Color32::from_black_alpha(140)).show(ctx, |ui| {
            ui.set_width(504.0);
            ui.label(RichText::new(tr("help.title")).font(font(Face::UiStrong, 17.0)).color(theme::PAPER));
            for (title, rows) in GROUPS {
                ui.add_space(14.0);
                ui.label(RichText::new(tr(title)).font(font(Face::UiStrong, 13.0)).color(theme::MUTED));
                ui.add_space(4.0);
                for row in *rows {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let k = ui.label(RichText::new(tr(&format!("help.{row}.k"))).font(font(Face::TcStrong, 13.0)).color(theme::PENCIL));
                        ui.add_space((190.0 - k.rect.width()).max(8.0));
                        ui.add(
                            egui::Label::new(RichText::new(tr(&format!("help.{row}.d"))).font(font(Face::Ui, 13.5)).color(theme::PAPER)).wrap(),
                        );
                    });
                }
            }
        });
        if resp.should_close() {
            self.help = false;
        }
    }

    fn drop_hint(&self, ctx: &egui::Context) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering {
            return;
        }
        let screen = ctx.content_rect().shrink(12.0);
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("drop")));
        p.rect(screen, 10.0, Color32::from_rgba_unmultiplied(0x1C, 0x1D, 0x1F, 0xCC), Stroke::new(2.0, theme::PENCIL), egui::StrokeKind::Inside);
        p.text(screen.center(), Align2::CENTER_CENTER, tr("drop.hint"), font(Face::NoteDisplay, 22.0), theme::PAPER);
    }
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.r.save_pending();
        self.r.flush_volume();
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.r.update();
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if !dropped.is_empty() {
            self.r.open_paths(dropped);
        }
        self.shortcuts(&ctx);
        self.r.hover_info.clear();
        let video = self.r.video_texture(frame);

        egui::Panel::right("notes")
            .exact_size(400.0)
            .resizable(false)
            .frame(Frame::NONE.fill(theme::PANEL).stroke(Stroke::new(1.0, theme::HAIRLINE)))
            .show(ui, |ui| panel::panel(ui, &mut self.r, &mut self.panel));

        egui::CentralPanel::default().frame(Frame::NONE.fill(theme::SURROUND)).show(ui, |ui| {
            if self.r.current.is_none() {
                self.empty(ui);
                return;
            }
            let mut buttons = (false, false, false, false);
            egui::Panel::top("header").exact_size(52.0).frame(Frame::NONE.fill(theme::PANEL)).show_separator_line(false).show(ui, |ui| {
                let full = ui.max_rect();
                let mut left = ui.new_child(UiBuilder::new().max_rect(full).layout(Layout::left_to_right(Align::Center)));
                self.header(&mut left);
                let mut right = ui.new_child(UiBuilder::new().max_rect(full).layout(Layout::right_to_left(Align::Center)));
                buttons = self.header_buttons(&mut right);
            });
            let mut drafted = false;
            egui::Panel::bottom("transport")
                .frame(Frame::NONE.fill(theme::PANEL).inner_margin(Margin { left: 20, right: 20, top: 14, bottom: 18 }))
                .show_separator_line(false)
                .resizable(false)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    self.transport(ui);
                    ui.add_space(14.0);
                    filmstrip(ui, &mut self.r);
                    ui.add_space(4.0);
                    drafted |= ruler(ui, &mut self.r, &mut self.ruler);
                });
            egui::CentralPanel::default().frame(Frame::NONE.fill(Color32::BLACK)).show(ui, |ui| {
                drafted |= stage(ui, &mut self.r, video, &mut self.stage);
            });
            if drafted {
                self.focus_draft();
            }
            if buttons.0 {
                self.open_files();
            }
            if buttons.1 {
                self.open_folder();
            }
            if buttons.2 {
                self.help = true;
            }
            if buttons.3 {
                self.export_notes();
            }
        });

        self.help_dialog(&ctx);
        self.drop_hint(&ctx);
        if self.r.playing {
            ctx.request_repaint();
        }
    }
}

/// Mute button and a slider; drag, click or scroll to change. Returns its left edge.
/// Scene number and length next to the timecode, or why there are none yet,
/// plus the re-detect button.
fn scene_status(ui: &mut Ui, r: &mut Review, t: f64, area: Rect) {
    let mut ui = ui.new_child(UiBuilder::new().max_rect(area).layout(Layout::left_to_right(Align::Center)));
    ui.spacing_mut().item_spacing.x = 6.0;
    let label = |ui: &mut Ui, s: &str, c: Color32| ui.add(egui::Label::new(RichText::new(s).font(font(Face::Ui, 12.5)).color(c)).truncate());
    let no_ffmpeg = r.ffmpeg_missing || r.scenes_status == ScenesStatus::NoFfmpeg;
    match r.scenes_status {
        ScenesStatus::Ready => {
            let n = r.scene_at(t);
            if let Some((a, b)) = r.scene_range(n) {
                label(&mut ui, &trf("scene.status", &[("n", &n), ("total", &r.scene_count()), ("secs", &secs(a, b))]), theme::MUTED);
            }
        }
        ScenesStatus::Pending if r.current.is_some() => {
            ui.add(egui::Spinner::new().size(13.0).color(theme::LEADER));
            let text = match r.scenes_progress() {
                Some(f) if f > 0.0 => trf("scene.detecting_pct", &[("pct", &format!("{:.0}", f * 100.0))]),
                _ => tr("scene.detecting"),
            };
            label(&mut ui, &text, theme::LEADER).on_hover_text(tr("scene.detecting_tip"));
        }
        ScenesStatus::Error => {
            label(&mut ui, &tr("scene.failed"), theme::RED).on_hover_text(format!("ffmpeg: {}", r.scenes_error));
        }
        _ => {}
    }
    if no_ffmpeg {
        label(&mut ui, &tr("scene.no_ffmpeg"), theme::RED).on_hover_text(tr("scene.no_ffmpeg_tip"));
    } else if r.current.is_some()
        && r.scenes_status != ScenesStatus::Pending
        && icon_button(&mut ui, icon::REFRESH, 16.0, vec2(24.0, 24.0), theme::MUTED, &tr("scene.redetect")).clicked()
    {
        r.redetect_scenes();
    }
}

fn volume(ui: &mut Ui, r: &mut Review, row: Rect) -> f32 {
    let track_w = 96.0;
    let slider = Rect::from_min_max(pos2(row.right() - track_w, row.center().y - 10.0), pos2(row.right(), row.center().y + 10.0));
    let btn = Rect::from_center_size(pos2(slider.left() - 20.0, row.center().y), vec2(32.0, 32.0));

    let shown = if r.muted { 0.0 } else { r.volume };
    let glyph = if shown == 0.0 {
        icon::VOLUME_OFF
    } else if shown < 50.0 {
        icon::VOLUME_DOWN
    } else {
        icon::VOLUME_UP
    };
    let mut b = ui.new_child(UiBuilder::new().max_rect(btn).layout(Layout::left_to_right(Align::Center)));
    let tip = tr(if r.muted { "volume.unmute" } else { "volume.mute" });
    if icon_button(&mut b, glyph, 20.0, vec2(32.0, 32.0), theme::MUTED, &tip).clicked() {
        r.toggle_mute();
    }

    let resp = ui.interact(slider, ui.scope_id().with("volume"), Sense::click_and_drag());
    let track = Rect::from_min_max(pos2(slider.left() + 5.0, slider.center().y - 1.5), pos2(slider.right() - 5.0, slider.center().y + 1.5));
    let value_at = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) * 100.0) as f64;
    if resp.is_pointer_button_down_on()
        && let Some(p) = resp.interact_pointer_pos()
    {
        r.set_volume(value_at(p.x).round());
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.smooth_scroll_delta.x);
        if scroll != 0.0 {
            r.set_volume((shown + scroll as f64 * 0.25).round());
            r.persist_volume_soon();
        }
    }
    if resp.drag_stopped() || resp.clicked() {
        r.persist_volume();
    }
    let shown = if r.muted { 0.0 } else { r.volume };
    let hot = resp.hovered() || resp.dragged();
    let level = track.left() + track.width() * (shown / 100.0) as f32;
    let p = ui.painter();
    p.rect_filled(track, 1.5, theme::HAIRLINE);
    p.rect_filled(Rect::from_min_max(track.left_top(), pos2(level, track.bottom())), 1.5, if hot { theme::PAPER } else { theme::MUTED });
    p.circle_filled(pos2(level, track.center().y), if hot { 6.0 } else { 5.0 }, theme::PAPER);
    resp.on_hover_text(trf("volume.level", &[("pct", &format!("{shown:.0}"))]));
    btn.left()
}

/// Globe button with the language list; `named` also shows the current language's name.
/// Picking one switches the UI at once and is remembered in prefs.
fn language_menu(ui: &mut Ui, r: &mut Review, named: bool) {
    let current = i18n::current();
    let resp = if named {
        let text = RichText::new(format!("{}  {}", icon::LANGUAGE, current.native_name())).font(font(Face::Ui, 13.0)).color(theme::MUTED);
        ui.add(egui::Button::new(text).frame(false).min_size(vec2(0.0, 32.0)))
    } else {
        icon_button(ui, icon::LANGUAGE, 19.0, vec2(36.0, 36.0), theme::MUTED, &tr("header.language"))
    };
    let mut pick = None;
    egui::Popup::menu(&resp).show(|ui| {
        ui.set_min_width(170.0);
        for lang in Lang::ALL {
            let on = lang == current;
            let text = RichText::new(lang.native_name()).font(font(if on { Face::UiStrong } else { Face::Ui }, 13.0)).color(if on {
                theme::PAPER
            } else {
                theme::MUTED
            });
            if ui.add(egui::Button::new(text).frame(false)).clicked() {
                pick = Some(lang);
            }
        }
    });
    if let Some(lang) = pick.filter(|l| *l != current) {
        r.set_language(lang);
    }
}
