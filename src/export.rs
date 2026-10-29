//! Export of the review notes for the edit: CMX3600 EDL with DaVinci Resolve
//! markers, FCPXML 1.10 markers and a Markdown list.
//!
//! Timecodes are always non-drop-frame, counted at the nominal (rounded) rate:
//! 23.976 fps material is labelled at 24 frames per second and 29.97 at 30,
//! the same way `model::timecode` labels them. The timeline is assumed to
//! start at 00:00:00:00 and to hold only this video.

use std::fmt::Write as _;
use std::path::Path;

use crate::i18n::{tr, trf, trn};
use crate::model::{Comment, frame_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Edl,
    Fcpxml,
    Markdown,
}

impl Format {
    pub const EXTENSIONS: &[&str] = &["edl", "fcpxml", "md"];

    /// Picks the format from the file extension (case-insensitive).
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_lowercase().as_str() {
            "edl" => Some(Self::Edl),
            "fcpxml" => Some(Self::Fcpxml),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }
}

/// What the exports need to know about the video besides the notes.
pub struct Meta<'a> {
    /// File name of the video, used as title / clip name.
    pub name: &'a str,
    pub fps: f64,
    /// Length in seconds; 0 when unknown (then the last note sets the length).
    pub duration: f64,
}

pub fn render(format: Format, comments: &[Comment], meta: &Meta) -> String {
    match format {
        Format::Edl => edl(comments, meta),
        Format::Fcpxml => fcpxml(comments, meta),
        Format::Markdown => markdown(comments, meta),
    }
}

// ---- time ------------------------------------------------------------------

fn nominal(fps: f64) -> i64 {
    (fps.round() as i64).clamp(1, 1000)
}

/// Non-drop-frame timecode HH:MM:SS:FF of a frame index at the nominal rate.
pub fn frame_tc(frame: i64, fps: f64) -> String {
    let n = nominal(fps);
    let f = frame.max(0);
    let s = f / n;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f % n)
}

/// First and last+1 frame of a note; a single moment covers one frame.
fn frames(c: &Comment, fps: f64) -> (i64, i64) {
    let a = frame_of(c.t_in.max(0.0), fps);
    let b = c.t_out.map_or(a + 1, |t| frame_of(t.max(0.0), fps)).max(a + 1);
    (a, b)
}

/// Frame durations (numerator, denominator in seconds) of the rates Final Cut
/// Pro offers: 23.976, 24, 25, 29.97, 30, 50, 59.94 and 60.
const STANDARD_RATES: [(i64, i64); 8] = [(1001, 24000), (1, 24), (1, 25), (1001, 30000), (1, 30), (1, 50), (1001, 60000), (1, 60)];

/// How far (relative) a reported rate may be from a standard one and still be
/// snapped to it. Variable frame rate phone clips report averages like 29.87.
const SNAP_TOLERANCE: f64 = 0.02;

/// Frame duration as an exact rational (numerator, denominator) in seconds,
/// so 23.976 becomes 1001/24000 instead of a rounded float. Integer and NTSC
/// rates are kept as they are; anything else is snapped to the nearest
/// standard rate with the same nominal rate (so the FCPXML counts frames like
/// the EDL and Markdown exports). `None` when no such rate is near.
pub fn standard_frame_duration(fps: f64) -> Option<(i64, i64)> {
    let n = nominal(fps);
    if (fps - n as f64).abs() < 1e-3 {
        return Some((1, n));
    }
    if (fps - n as f64 * 1000.0 / 1001.0).abs() < 1e-2 {
        return Some((1001, n * 1000));
    }
    STANDARD_RATES
        .into_iter()
        .map(|(num, den)| (den as f64 / num as f64, (num, den)))
        .filter(|&(rate, _)| nominal(rate) == n && ((fps - rate) / rate).abs() <= SNAP_TOLERANCE)
        .min_by(|a, b| (fps - a.0).abs().total_cmp(&(fps - b.0).abs()))
        .map(|(_, fd)| fd)
}

/// [`standard_frame_duration`], falling back to the rate rounded to 1/100 fps
/// for rates far from every standard one (the export then warns about it).
pub fn frame_duration(fps: f64) -> (i64, i64) {
    standard_frame_duration(fps).unwrap_or_else(|| (100, (fps * 100.0).round().max(1.0) as i64))
}

fn rational(frames: i64, (num, den): (i64, i64)) -> String {
    if frames == 0 { "0s".to_owned() } else { format!("{}/{}s", frames * num, den) }
}

/// In the app's order: whole-video notes first (as markers on the first frame), then by time.
fn sorted(comments: &[Comment]) -> Vec<&Comment> {
    let mut v: Vec<&Comment> = comments.iter().collect();
    v.sort_by(|a, b| b.general.cmp(&a.general).then(a.t_in.total_cmp(&b.t_in)));
    v
}

/// Note text on one line: newlines and other control characters become spaces.
fn one_line(text: &str) -> String {
    let s: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---- EDL -------------------------------------------------------------------

/// `|` separates the Resolve marker fields, so it cannot appear in the text.
pub fn edl_escape(text: &str) -> String {
    one_line(text).replace('|', "/")
}

/// CMX3600 event numbers have three digits, so an EDL holds at most this many
/// notes; the latest ones beyond it are left out.
pub const EDL_MAX_EVENTS: usize = 999;

/// CMX3600 EDL in the shape DaVinci Resolve writes and reads as timeline
/// markers: one one-frame event per note, followed by a
/// ` |C:<color> |M:<name> |D:<duration in frames>` line. Open notes are blue,
/// done ones green.
pub fn edl(comments: &[Comment], meta: &Meta) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "TITLE: {}", edl_escape(meta.name));
    out.push_str("FCM: NON-DROP FRAME\n\n");
    for (i, c) in sorted(comments).into_iter().take(EDL_MAX_EVENTS).enumerate() {
        let (a, b) = frames(c, meta.fps);
        let (tin, tout) = (frame_tc(a, meta.fps), frame_tc(a + 1, meta.fps));
        let _ = writeln!(out, "{:03}  001      V     C        {tin} {tout} {tin} {tout}  ", i + 1);
        let color = if c.done { "ResolveColorGreen" } else { "ResolveColorBlue" };
        let _ = writeln!(out, " |C:{color} |M:{} |D:{}", edl_escape(&c.text), b - a);
        out.push('\n');
    }
    out
}

// ---- FCPXML ----------------------------------------------------------------

/// Escapes text for XML attributes and content. Characters XML 1.0 does not
/// allow are dropped; newlines and tabs are kept as character references.
pub fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// FCPXML 1.10: one project whose spine holds a gap as long as the video,
/// carrying one marker per note. Done notes become completed to-do markers,
/// open ones open to-dos.
pub fn fcpxml(comments: &[Comment], meta: &Meta) -> String {
    let fd = frame_duration(meta.fps);
    let list = sorted(comments);
    let last = list.iter().map(|c| frames(c, meta.fps).1).max().unwrap_or(1);
    let total = frame_of(meta.duration.max(0.0), meta.fps).max(last);
    let dur = rational(total, fd);
    let name = xml_escape(meta.name);
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.10\">\n");
    out.push_str("  <resources>\n");
    let _ = writeln!(out, "    <format id=\"r1\" frameDuration=\"{}\"/>", rational(1, fd));
    out.push_str("  </resources>\n  <library>\n");
    let _ = writeln!(out, "    <event name=\"{name}\">");
    let _ = writeln!(out, "      <project name=\"{name}\">");
    let _ = writeln!(out, "        <sequence format=\"r1\" duration=\"{dur}\" tcStart=\"0s\" tcFormat=\"NDF\">");
    out.push_str("          <spine>\n");
    let _ = writeln!(out, "            <gap name=\"{name}\" offset=\"0s\" start=\"0s\" duration=\"{dur}\">");
    for c in list {
        let (a, b) = frames(c, meta.fps);
        let _ = writeln!(
            out,
            "              <marker start=\"{}\" duration=\"{}\" value=\"{}\" completed=\"{}\"/>",
            rational(a, fd),
            rational(b - a, fd),
            xml_escape(&c.text),
            u8::from(c.done),
        );
    }
    out.push_str("            </gap>\n          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n");
    out
}

// ---- Markdown --------------------------------------------------------------

/// Backslash-escapes the characters Markdown would read as formatting.
pub fn md_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Task list, one item per note: timecode (range), scenes, text. Continuation
/// lines of a multi-line note are indented so they stay inside the item.
pub fn markdown(comments: &[Comment], meta: &Meta) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", md_escape(meta.name));
    let list = sorted(comments);
    let done = list.iter().filter(|c| c.done).count();
    let _ = writeln!(out, "{}\n", trn("md.summary", list.len() as u64, &[("done", &done)]));
    for c in list {
        let (a, _) = frames(c, meta.fps);
        let mut tc = if c.general { tr("note.whole_video") } else { frame_tc(a, meta.fps) };
        if let Some(t) = c.t_out.filter(|_| !c.general) {
            let _ = write!(tc, "–{}", frame_tc(frame_of(t.max(0.0), meta.fps), meta.fps));
        }
        let scenes = match c.scenes {
            Some((x, y)) if x == y => trf("md.scene", &[("n", &x)]),
            Some((x, y)) => trf("md.scenes", &[("a", &x), ("b", &y)]),
            None => String::new(),
        };
        let text = c.text.trim().lines().map(md_escape).collect::<Vec<_>>().join("  \n  ");
        let _ = writeln!(out, "- [{}] **{tc}**{scenes} — {text}", if c.done { 'x' } else { ' ' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str, t_in: f64, t_out: Option<f64>, done: bool) -> Comment {
        Comment {
            id: "a".into(),
            text: text.into(),
            t_in,
            t_out,
            area: None,
            scenes: Some((2, 3)),
            done,
            created: None,
            general: false,
            extra: Default::default(),
        }
    }

    fn meta(fps: f64) -> Meta<'static> {
        Meta { name: "cut & <v1>.mp4", fps, duration: 10.0 }
    }

    #[test]
    fn timecode_integer_and_fractional_rates() {
        assert_eq!(frame_tc(0, 25.0), "00:00:00:00");
        assert_eq!(frame_tc(25 * 3661 + 7, 25.0), "01:01:01:07");
        // 23.976 counts 24 frames per timecode second (NDF).
        assert_eq!(frame_tc(24, 23.976), "00:00:01:00");
        assert_eq!(frame_tc(1799, 29.97), "00:00:59:29");
        assert_eq!(frame_tc(-5, 25.0), "00:00:00:00");
    }

    #[test]
    fn exact_frame_durations() {
        assert_eq!(frame_duration(25.0), (1, 25));
        assert_eq!(frame_duration(23.976), (1001, 24000));
        assert_eq!(frame_duration(24000.0 / 1001.0), (1001, 24000));
        assert_eq!(frame_duration(29.97), (1001, 30000));
        assert_eq!(frame_duration(59.94), (1001, 60000));
        assert_eq!(frame_duration(12.5), (100, 1250));
        assert_eq!(standard_frame_duration(12.5), None);
        assert_eq!(frame_duration(48.0), (1, 48));
        assert_eq!(rational(48, frame_duration(23.976)), "48048/24000s");
        assert_eq!(rational(0, (1, 25)), "0s");
    }

    #[test]
    fn vfr_rates_snap_to_a_standard_rate() {
        // Averages mpv reports for variable frame rate phone clips.
        assert_eq!(frame_duration(29.87), (1001, 30000));
        assert_eq!(frame_duration(30.2), (1, 30));
        assert_eq!(frame_duration(24.3), (1, 24));
        assert_eq!(frame_duration(59.5), (1001, 60000));
        assert_eq!(frame_duration(25.4), (1, 25));
        assert!(fcpxml(&[], &meta(29.87)).contains("frameDuration=\"1001/30000s\""));
        // Snapping keeps the nominal rate the EDL and Markdown count at.
        assert_eq!(standard_frame_duration(29.4), None);
        assert_eq!(standard_frame_duration(40.3), None);
    }

    #[test]
    fn escaping() {
        assert_eq!(xml_escape("a & b <c> \"d\" 'e'\nf\u{1}"), "a &amp; b &lt;c&gt; &quot;d&quot; &apos;e&apos;&#10;f");
        assert_eq!(edl_escape("one | two\nthree\t four"), "one / two three four");
        assert_eq!(md_escape("*a* _b_ [c](d) #e"), "\\*a\\* \\_b\\_ \\[c\\](d) \\#e");
    }

    #[test]
    fn format_from_extension() {
        assert_eq!(Format::from_path(Path::new("x/notes.EDL")), Some(Format::Edl));
        assert_eq!(Format::from_path(Path::new("notes.fcpxml")), Some(Format::Fcpxml));
        assert_eq!(Format::from_path(Path::new("notes.md")), Some(Format::Markdown));
        assert_eq!(Format::from_path(Path::new("notes.txt")), None);
        assert_eq!(Format::from_path(Path::new("notes")), None);
    }

    #[test]
    fn edl_shape() {
        let list = [note("later | x", 2.0, Some(4.0), true), note("first\nline", 1.0, None, false)];
        let s = edl(&list, &meta(25.0));
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines[0], "TITLE: cut & <v1>.mp4");
        assert_eq!(lines[1], "FCM: NON-DROP FRAME");
        assert_eq!(lines[3], "001  001      V     C        00:00:01:00 00:00:01:01 00:00:01:00 00:00:01:01  ");
        assert_eq!(lines[4], " |C:ResolveColorBlue |M:first line |D:1");
        assert_eq!(lines[6], "002  001      V     C        00:00:02:00 00:00:02:01 00:00:02:00 00:00:02:01  ");
        assert_eq!(lines[7], " |C:ResolveColorGreen |M:later / x |D:50");
    }

    #[test]
    fn edl_caps_event_numbers_at_999() {
        let list: Vec<Comment> = (0..1000).map(|i| note("n", i as f64 / 25.0, None, false)).collect();
        let events = |s: &str| s.lines().filter(|l| l.contains("  001      V     C  ")).count();
        let s = edl(&list[..999], &meta(25.0));
        assert_eq!(events(&s), 999);
        assert!(s.contains("\n999  001 "), "{s}");
        let s = edl(&list, &meta(25.0));
        assert_eq!(events(&s), 999);
        assert!(!s.contains("1000  001 "), "{s}");
        // The latest note is the one left out.
        assert!(s.contains(&format!("999  001      V     C        {}", frame_tc(998, 25.0))), "{s}");
        assert!(!s.contains(&frame_tc(1000, 25.0)), "{s}");
    }

    #[test]
    fn fcpxml_shape() {
        let list = [note("a & \"b\"", 1.0, Some(2.0), false), note("c", 0.5, None, true)];
        let s = fcpxml(&list, &meta(23.976));
        assert!(s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.10\">"));
        assert!(s.contains("<format id=\"r1\" frameDuration=\"1001/24000s\"/>"), "{s}");
        assert!(s.contains("<project name=\"cut &amp; &lt;v1&gt;.mp4\">"), "{s}");
        // 10 s at 23.976 is frame 239.
        assert!(s.contains("<gap name=\"cut &amp; &lt;v1&gt;.mp4\" offset=\"0s\" start=\"0s\" duration=\"239239/24000s\">"), "{s}");
        // 0.5 s is frame 11, a moment lasts one frame.
        assert!(s.contains("<marker start=\"11011/24000s\" duration=\"1001/24000s\" value=\"c\" completed=\"1\"/>"), "{s}");
        // 1.0 s is frame 23 (23.976 fps), 2.0 s frame 47.
        assert!(s.contains("<marker start=\"23023/24000s\" duration=\"24024/24000s\" value=\"a &amp; &quot;b&quot;\" completed=\"0\"/>"), "{s}");
        assert!(s.find("value=\"c\"").unwrap() < s.find("value=\"a").unwrap());
        assert!(s.trim_end().ends_with("</fcpxml>"));
        assert_eq!(s.matches("<marker ").count(), 2);
    }

    #[test]
    fn markdown_shape() {
        let list = [note("fix *this*\nand that", 1.0, Some(2.0), true), note("open", 0.0, None, false)];
        let s = markdown(&list, &meta(25.0));
        assert!(s.starts_with("# cut & \\<v1\\>.mp4\n"), "{s}");
        assert!(s.contains("2 notes, 1 done."), "{s}");
        assert!(s.contains("- [ ] **00:00:00:00** (scenes 2–3) — open\n"), "{s}");
        assert!(s.contains("- [x] **00:00:01:00–00:00:02:00** (scenes 2–3) — fix \\*this\\*  \n  and that\n"), "{s}");
    }

    #[test]
    fn whole_video_notes_come_first() {
        let general = Comment { scenes: None, general: true, ..note("too long overall", 0.0, None, false) };
        let list = [note("open", 0.0, None, false), general];
        let s = markdown(&list, &meta(25.0));
        let (g, o) = (s.find("- [ ] **Whole video** — too long overall\n"), s.find("open"));
        assert!(g.is_some() && g < o, "{s}");
        assert!(edl(&list, &meta(25.0)).find("too long").unwrap() < edl(&list, &meta(25.0)).find("|M:open").unwrap());
    }

    /// Notes at fractional rates with the expected NDF timecodes (counted at
    /// 24/30/60 per second), worked out by hand: frame = floor(t * fps), then
    /// HH:MM:SS from frame / nominal and FF from frame % nominal.
    /// Each expectation is (in, in + 1 frame, out or None, duration in frames).
    #[allow(clippy::type_complexity)]
    fn fractional_cases() -> Vec<(f64, Vec<Comment>, Vec<(&'static str, &'static str, Option<&'static str>, i64)>)> {
        vec![
            (
                24000.0 / 1001.0,
                vec![note("range", 10.0, Some(150.15), false), note("pin", 0.5, None, true)],
                vec![
                    // 0.5 s -> frame 11.
                    ("00:00:00:11", "00:00:00:12", None, 1),
                    // 10 s -> frame 239 = 9 s + 23; 150.15 s -> frame 3600 = 150 s.
                    ("00:00:09:23", "00:00:10:00", Some("00:02:30:00"), 3361),
                ],
            ),
            (
                30000.0 / 1001.0,
                vec![note("hour", 60.0, Some(3600.0), false), note("pin", 0.5, None, true)],
                vec![
                    // 0.5 s -> frame 14.
                    ("00:00:00:14", "00:00:00:15", None, 1),
                    // 60 s -> frame 1798 = 59 s + 28; 3600 s -> frame 107892 = 3596 s + 12.
                    ("00:00:59:28", "00:00:59:29", Some("00:59:56:12"), 106094),
                ],
            ),
            (
                60000.0 / 1001.0,
                vec![note("late", 3600.0, None, false), note("range", 1.0, Some(120.12), true)],
                vec![
                    // 1 s -> frame 59, the next frame rolls over to 1 s; 120.12 s -> frame 7200 = 120 s.
                    ("00:00:00:59", "00:00:01:00", Some("00:02:00:00"), 7141),
                    // 3600 s -> frame 215784 = 3596 s + 24.
                    ("00:59:56:24", "00:59:56:25", None, 1),
                ],
            ),
        ]
    }

    #[test]
    fn edl_fractional_rates() {
        for (fps, list, want) in fractional_cases() {
            let s = edl(&list, &meta(fps));
            let lines: Vec<&str> = s.lines().collect();
            assert_eq!(lines[1], "FCM: NON-DROP FRAME");
            let mut sorted = list.clone();
            sorted.sort_by(|a, b| a.t_in.total_cmp(&b.t_in));
            for (i, ((tin, tout, _, dur), c)) in want.iter().zip(&sorted).enumerate() {
                let event = format!("{:03}  001      V     C        {tin} {tout} {tin} {tout}  ", i + 1);
                assert_eq!(lines[3 + 3 * i], event, "{fps} fps\n{s}");
                let color = if c.done { "ResolveColorGreen" } else { "ResolveColorBlue" };
                assert_eq!(lines[4 + 3 * i], format!(" |C:{color} |M:{} |D:{dur}", c.text), "{fps} fps\n{s}");
            }
            assert_eq!(lines.len(), 3 + 3 * want.len(), "{fps} fps\n{s}");
        }
    }

    #[test]
    fn fcpxml_fractional_rates() {
        for (fps, list, want) in fractional_cases() {
            let s = fcpxml(&list, &meta(fps));
            // NTSC rates: 1001/(nominal * 1000) s per frame.
            let nominal = fps.round() as i64;
            let den = nominal * 1000;
            assert!(s.contains(&format!("<format id=\"r1\" frameDuration=\"1001/{den}s\"/>")), "{fps} fps\n{s}");
            let mut sorted = list.clone();
            sorted.sort_by(|a, b| a.t_in.total_cmp(&b.t_in));
            let mut last = 0;
            let markers: Vec<String> = want
                .iter()
                .zip(&sorted)
                .map(|((tin, _, _, dur), c)| {
                    // Frame index back from the NDF timecode: HH:MM:SS counts nominal frames per second.
                    let p: Vec<i64> = tin.split(':').map(|x| x.parse().unwrap()).collect();
                    let a = ((p[0] * 60 + p[1]) * 60 + p[2]) * nominal + p[3];
                    last = last.max(a + dur);
                    format!(
                        "<marker start=\"{}/{den}s\" duration=\"{}/{den}s\" value=\"{}\" completed=\"{}\"/>",
                        a * 1001,
                        dur * 1001,
                        c.text,
                        u8::from(c.done)
                    )
                })
                .collect();
            let got: Vec<&str> = s.lines().map(str::trim).filter(|l| l.starts_with("<marker ")).collect();
            assert_eq!(got, markers, "{fps} fps\n{s}");
            // Every case runs past the 10 s video, so the last marker's end sets the length.
            let dur = format!("duration=\"{}/{den}s\"", last * 1001);
            assert!(s.contains(&format!("<sequence format=\"r1\" {dur} tcStart=\"0s\" tcFormat=\"NDF\">")), "{fps} fps\n{s}");
            assert!(s.contains(&format!("start=\"0s\" {dur}>")), "{fps} fps\n{s}");
        }
    }

    #[test]
    fn markdown_fractional_rates() {
        for (fps, list, want) in fractional_cases() {
            let s = markdown(&list, &meta(fps));
            let mut sorted = list.clone();
            sorted.sort_by(|a, b| a.t_in.total_cmp(&b.t_in));
            let items: Vec<String> = want
                .iter()
                .zip(&sorted)
                .map(|((tin, _, tout, _), c)| {
                    let tc = tout.map_or(tin.to_string(), |o| format!("{tin}–{o}"));
                    format!("- [{}] **{tc}** (scenes 2–3) — {}", if c.done { 'x' } else { ' ' }, c.text)
                })
                .collect();
            let got: Vec<&str> = s.lines().filter(|l| l.starts_with("- [")).collect();
            assert_eq!(got, items, "{fps} fps\n{s}");
        }
    }
}
