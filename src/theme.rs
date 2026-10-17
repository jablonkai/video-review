//! Palette and type. A neutral graphite surround (grading-suite practice, so
//! the UI does not tint how the picture is judged), warm paper text, and
//! chinagraph-pencil colours for everything that is a note.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};

use crate::i18n::Lang;

pub const SURROUND: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x1F);
pub const PANEL: Color32 = Color32::from_rgb(0x22, 0x23, 0x26);
pub const RAISED: Color32 = Color32::from_rgb(0x2B, 0x2D, 0x31);
pub const HAIRLINE: Color32 = Color32::from_rgb(0x39, 0x3B, 0x40);
pub const PAPER: Color32 = Color32::from_rgb(0xE9, 0xE6, 0xDF);
pub const MUTED: Color32 = Color32::from_rgb(0x9A, 0x97, 0x8F);
pub const FAINT: Color32 = Color32::from_rgb(0x6B, 0x6A, 0x66);
/// Chinagraph yellow: notes, marks on the frame, the save action.
pub const PENCIL: Color32 = Color32::from_rgb(0xF3, 0xD3, 0x5B);
pub const ON_PENCIL: Color32 = Color32::from_rgb(0x26, 0x21, 0x0C);
/// Leader blue: time selections (scenes, ranges).
pub const LEADER: Color32 = Color32::from_rgb(0x7F, 0xB2, 0xE5);
pub const LEADER_WASH: Color32 = Color32::from_rgba_unmultiplied_const(0x7F, 0xB2, 0xE5, 0x33);
pub const RED: Color32 = Color32::from_rgb(0xE8, 0x63, 0x4A);
pub const DONE: Color32 = Color32::from_rgb(0x8F, 0xBF, 0x8A);
pub const FILM_BASE: Color32 = Color32::from_rgb(0x11, 0x11, 0x12);

/// Archivo weights/widths and Source Serif, as static instances (see tools/make_fonts.py).
#[derive(Clone, Copy)]
pub enum Face {
    Ui,
    UiStrong,
    /// Narrow bold, for the little number tags on marks.
    Tag,
    TcLight,
    Tc,
    TcMedium,
    TcStrong,
    Note,
    NoteDisplay,
    Icons,
}

impl Face {
    fn name(self) -> &'static str {
        match self {
            Face::Ui => "Archivo-400",
            Face::UiStrong => "Archivo-600",
            Face::Tag => "Archivo-700n",
            Face::TcLight => "ArchivoTC-300",
            Face::Tc => "ArchivoTC-450",
            Face::TcMedium => "ArchivoTC-520",
            Face::TcStrong => "ArchivoTC-600",
            Face::Note => "SourceSerif-420",
            Face::NoteDisplay => "SourceSerif-display",
            Face::Icons => "MaterialIcons",
        }
    }
}

pub fn font(face: Face, size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(face.name().into()))
}

/// Embedded fonts, plus a system font behind them for scripts they lack
/// (CJK). Safe to call again when the language changes.
pub fn install_fonts(ctx: &egui::Context, lang: Lang) {
    macro_rules! f {
        ($file:literal) => {
            include_bytes!(concat!("../assets/fonts/", $file))
        };
    }
    let files: [(&str, &'static [u8]); 10] = [
        ("Archivo-400", f!("Archivo-400.ttf")),
        ("Archivo-600", f!("Archivo-600.ttf")),
        ("Archivo-700n", f!("Archivo-700n.ttf")),
        ("ArchivoTC-300", f!("ArchivoTC-300.ttf")),
        ("ArchivoTC-450", f!("ArchivoTC-450.ttf")),
        ("ArchivoTC-520", f!("ArchivoTC-520.ttf")),
        ("ArchivoTC-600", f!("ArchivoTC-600.ttf")),
        ("SourceSerif-420", f!("SourceSerif-420.ttf")),
        ("SourceSerif-display", f!("SourceSerif-display.ttf")),
        ("MaterialIcons", f!("MaterialIcons-Regular.otf")),
    ];
    let mut defs = FontDefinitions::default();
    let mut fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if lang.needs_cjk()
        && let Some(data) = system_cjk_font(lang)
    {
        defs.font_data.insert("SystemCJK".into(), data.into());
        fallback.push("SystemCJK".into());
    }
    for (name, bytes) in files {
        defs.font_data.insert(name.into(), FontData::from_static(bytes).into());
        let mut chain = vec![name.to_owned()];
        if name != "MaterialIcons" {
            chain.push("MaterialIcons".into());
        }
        chain.extend(fallback.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    let mut ui = vec!["Archivo-400".to_owned(), "MaterialIcons".into()];
    ui.extend(fallback);
    defs.families.insert(FontFamily::Proportional, ui);
    ctx.set_fonts(defs);
}

/// The first installed CJK font we know of, with its face index in a collection.
fn system_cjk_font(lang: Lang) -> Option<FontData> {
    let ja = lang == Lang::Ja;
    let candidates: &[(&str, u32)] = if cfg!(target_os = "macos") {
        if ja {
            &[
                ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
                ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
                ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
            ]
        } else {
            &[
                ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
                ("/System/Library/Fonts/STHeiti Light.ttc", 0),
                ("/System/Library/Fonts/Supplemental/Arial Unicode.ttf", 0),
            ]
        }
    } else if cfg!(windows) {
        if ja {
            &[("C:\\Windows\\Fonts\\YuGothR.ttc", 0), ("C:\\Windows\\Fonts\\meiryo.ttc", 0), ("C:\\Windows\\Fonts\\msgothic.ttc", 0)]
        } else {
            &[("C:\\Windows\\Fonts\\msyh.ttc", 0), ("C:\\Windows\\Fonts\\simhei.ttf", 0), ("C:\\Windows\\Fonts\\simsun.ttc", 0)]
        }
    } else {
        // Noto Sans CJK collections: face 0 is Japanese, 2 is Simplified Chinese.
        let face = if ja { 0 } else { 2 };
        &[
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", face),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", face),
            ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", 0),
            ("/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc", 0),
        ]
    };
    candidates.iter().find_map(|(path, index)| {
        let bytes = std::fs::read(path).ok()?;
        Some(FontData { index: *index, ..FontData::from_owned(bytes) })
    })
}

/// Material icon code points (same glyphs as the Flutter version).
pub mod icon {
    pub const MOVIE: char = '\u{f1f5}';
    pub const WHOLE_VIDEO: char = '\u{f23c}';
    pub const FOLDER: char = '\u{f090}';
    pub const KEYBOARD: char = '\u{f144}';
    pub const SHARE: char = '\u{e6b8}';
    pub const SKIP_PREV: char = '\u{f3a1}';
    pub const SKIP_NEXT: char = '\u{f3a0}';
    pub const CHEVRON_LEFT: char = '\u{e15e}';
    pub const CHEVRON_RIGHT: char = '\u{e15f}';
    pub const PLAY: char = '\u{e4cb}';
    pub const PAUSE: char = '\u{e47c}';
    pub const FIRST_PAGE: char = '\u{e28b}';
    pub const STRAIGHTEN: char = '\u{e60e}';
    pub const ADJUST: char = '\u{e061}';
    pub const PIN: char = '\u{e504}';
    pub const CROP: char = '\u{e1a9}';
    pub const TOUCH: char = '\u{f453}';
    pub const CLOSE: char = '\u{e16a}';
    pub const CHECK: char = '\u{e159}';
    pub const CHECK_OUTLINE: char = '\u{e15a}';
    pub const EDIT: char = '\u{f00d}';
    pub const DELETE: char = '\u{e1bb}';
    pub const EXPAND: char = '\u{e246}';
    pub const VOLUME_UP: char = '\u{e6c5}';
    pub const VOLUME_DOWN: char = '\u{e6c2}';
    pub const VOLUME_OFF: char = '\u{e6c4}';
    pub const REFRESH: char = '\u{e514}';
    pub const LANGUAGE: char = '\u{e366}';
}

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = PANEL;
        v.window_fill = PANEL;
        v.extreme_bg_color = SURROUND;
        v.faint_bg_color = RAISED;
        v.override_text_color = None;
        v.weak_text_color = Some(FAINT);
        v.widgets.noninteractive.fg_stroke.color = PAPER;
        v.window_stroke = egui::Stroke::new(1.0, HAIRLINE);
        v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xF3, 0xD3, 0x5B, 0x55);
        v.selection.stroke = egui::Stroke::new(1.0, PENCIL);
        v.text_cursor.stroke = egui::Stroke::new(2.0, PENCIL);
        v.hyperlink_color = LEADER;
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = egui::CornerRadius::same(5);
        }
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, HAIRLINE);
        v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        v.widgets.inactive.bg_fill = SURROUND;
        v.widgets.hovered.weak_bg_fill = RAISED;
        v.widgets.hovered.bg_fill = RAISED;
        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        v.widgets.active.weak_bg_fill = HAIRLINE;
        v.widgets.inactive.fg_stroke.color = MUTED;
        v.widgets.hovered.fg_stroke.color = PAPER;
        v.popup_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(110) };
        v.window_shadow = v.popup_shadow;
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.interaction.tooltip_delay = 0.45;
        s.text_styles.insert(egui::TextStyle::Body, font(Face::Ui, 13.5));
        s.text_styles.insert(egui::TextStyle::Button, font(Face::Ui, 13.0));
        s.text_styles.insert(egui::TextStyle::Small, font(Face::Ui, 11.5));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Laying out CJK text loads every font in the chain, so a system font
    /// egui cannot parse would panic here rather than at the first frame.
    #[test]
    fn cjk_fonts_load_and_shape() {
        for lang in [Lang::Ja, Lang::Zh] {
            let ctx = egui::Context::default();
            install_fonts(&ctx, lang);
            ctx.begin_pass(Default::default());
            let has = ctx.fonts_mut(|f| f.has_glyphs(&font(Face::Ui, 13.0), "日本語"));
            ctx.end_pass().textures_delta.clear(); // nothing renders the font atlas here
            assert_eq!(has, system_cjk_font(lang).is_some(), "{}", lang.code());
        }
    }
}
