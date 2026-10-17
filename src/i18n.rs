//! User-facing strings. Each language is a flat JSON catalog in
//! `assets/i18n/<code>.json`, embedded at build time: `key -> text`, or
//! `key -> {"one": …, "few": …, "many": …, "other": …}` for texts with a count.
//! English is the fallback for any key a catalog lacks. Placeholders are
//! `{name}`; `trn()` fills `{n}` itself.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Lang {
    En,
    Hu,
    De,
    Fr,
    Es,
    It,
    Pt,
    Nl,
    Pl,
    Ru,
    Ja,
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 12] =
        [Lang::En, Lang::Hu, Lang::De, Lang::Fr, Lang::Es, Lang::It, Lang::Pt, Lang::Nl, Lang::Pl, Lang::Ru, Lang::Ja, Lang::Zh];

    /// BCP 47 primary subtag, as stored in prefs.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Hu => "hu",
            Lang::De => "de",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::It => "it",
            Lang::Pt => "pt",
            Lang::Nl => "nl",
            Lang::Pl => "pl",
            Lang::Ru => "ru",
            Lang::Ja => "ja",
            Lang::Zh => "zh",
        }
    }

    /// The language's own name, shown in the picker (never translated).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Hu => "Magyar",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::It => "Italiano",
            Lang::Pt => "Português",
            Lang::Nl => "Nederlands",
            Lang::Pl => "Polski",
            Lang::Ru => "Русский",
            Lang::Ja => "日本語",
            Lang::Zh => "中文",
        }
    }

    /// Accepts `hu`, `hu-HU`, `hu_HU.UTF-8`, `zh-Hans-CN`… Only the primary subtag counts.
    pub fn from_code(s: &str) -> Option<Lang> {
        let primary = s.trim().split(['-', '_', '.', '@']).next().unwrap_or_default().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    /// Needs glyphs the embedded fonts do not have (a system font is added then).
    pub fn needs_cjk(self) -> bool {
        matches!(self, Lang::Ja | Lang::Zh)
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../assets/i18n/en.json"),
            Lang::Hu => include_str!("../assets/i18n/hu.json"),
            Lang::De => include_str!("../assets/i18n/de.json"),
            Lang::Fr => include_str!("../assets/i18n/fr.json"),
            Lang::Es => include_str!("../assets/i18n/es.json"),
            Lang::It => include_str!("../assets/i18n/it.json"),
            Lang::Pt => include_str!("../assets/i18n/pt.json"),
            Lang::Nl => include_str!("../assets/i18n/nl.json"),
            Lang::Pl => include_str!("../assets/i18n/pl.json"),
            Lang::Ru => include_str!("../assets/i18n/ru.json"),
            Lang::Ja => include_str!("../assets/i18n/ja.json"),
            Lang::Zh => include_str!("../assets/i18n/zh.json"),
        }
    }

    /// CLDR plural category of `n` for this language.
    pub fn plural(self, n: u64) -> &'static str {
        let (m10, m100) = (n % 10, n % 100);
        match self {
            Lang::Ja | Lang::Zh => "other",
            Lang::Fr => {
                if n <= 1 {
                    "one"
                } else {
                    "other"
                }
            }
            Lang::Pl => {
                if n == 1 {
                    "one"
                } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                    "few"
                } else {
                    "many"
                }
            }
            Lang::Ru => {
                if m10 == 1 && m100 != 11 {
                    "one"
                } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                    "few"
                } else {
                    "many"
                }
            }
            _ => {
                if n == 1 {
                    "one"
                } else {
                    "other"
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Entry {
    Text(String),
    Forms(HashMap<String, String>),
}

type Catalog = HashMap<String, Entry>;

fn catalog(lang: Lang) -> &'static Catalog {
    static CATALOGS: [OnceLock<Catalog>; 12] = [const { OnceLock::new() }; 12];
    CATALOGS[lang as usize]
        .get_or_init(|| serde_json::from_str(lang.source()).unwrap_or_else(|e| panic!("assets/i18n/{}.json: {e}", lang.code())))
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize]
}

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// The system language if we have it, else English.
pub fn system() -> Lang {
    sys_locale::get_locale().as_deref().and_then(Lang::from_code).unwrap_or(Lang::En)
}

fn entry(lang: Lang, key: &str) -> Option<&'static Entry> {
    catalog(lang).get(key).or_else(|| catalog(Lang::En).get(key))
}

/// Replaces each `{name}` in `text` with its value from `args` in a single scan, so inserted
/// values are never scanned again. Unknown placeholders are kept as is.
fn fill(text: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        let found = tail.find('}').and_then(|close| {
            let name = &tail[..close];
            args.iter().find(|(n, _)| *n == name).map(|(_, v)| (close, v))
        });
        match found {
            Some((close, value)) => {
                out.push_str(&value.to_string());
                rest = &tail[close + 1..];
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Text for `key` in `lang`; the key itself when no catalog has it.
pub fn tr_in(lang: Lang, key: &str) -> String {
    match entry(lang, key) {
        Some(Entry::Text(s)) => s.clone(),
        Some(Entry::Forms(f)) => f.get("other").or_else(|| f.values().next()).cloned().unwrap_or_else(|| key.to_owned()),
        None => key.to_owned(),
    }
}

/// Text for `key` in `lang` with a count: picks the plural form and fills `{n}` and `args`.
pub fn trn_in(lang: Lang, key: &str, n: u64, args: &[(&str, &dyn Display)]) -> String {
    let text = match entry(lang, key) {
        Some(Entry::Forms(f)) => {
            let cat = lang.plural(n);
            f.get(cat).or_else(|| f.get("other")).or_else(|| f.get("one")).cloned().unwrap_or_else(|| key.to_owned())
        }
        Some(Entry::Text(s)) => s.clone(),
        None => key.to_owned(),
    };
    let mut all: Vec<(&str, &dyn Display)> = vec![("n", &n)];
    all.extend_from_slice(args);
    fill(&text, &all)
}

/// Text for `key` in the current language.
pub fn tr(key: &str) -> String {
    tr_in(current(), key)
}

/// `tr()` with `{name}` placeholders filled from `args`.
pub fn trf(key: &str, args: &[(&str, &dyn Display)]) -> String {
    fill(&tr(key), args)
}

/// `tr()` for a text with a count.
pub fn trn(key: &str, n: u64, args: &[(&str, &dyn Display)]) -> String {
    trn_in(current(), key, n, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let Some(j) = rest[i..].find('}') else { break };
            out.insert(rest[i + 1..i + j].to_owned());
            rest = &rest[i + j + 1..];
        }
        out
    }

    fn forms(e: &Entry) -> Vec<&String> {
        match e {
            Entry::Text(s) => vec![s],
            Entry::Forms(f) => f.values().collect(),
        }
    }

    #[test]
    fn every_catalog_matches_english() {
        let en = catalog(Lang::En);
        assert!(!en.is_empty());
        for lang in Lang::ALL {
            let cat = catalog(lang);
            let missing: Vec<_> = en.keys().filter(|k| !cat.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{}: missing {missing:?}", lang.code());
            let extra: Vec<_> = cat.keys().filter(|k| !en.contains_key(*k)).collect();
            assert!(extra.is_empty(), "{}: unknown {extra:?}", lang.code());
            for (key, e) in cat {
                let want: BTreeSet<_> = forms(&en[key]).iter().flat_map(|s| placeholders(s)).collect();
                for f in forms(e) {
                    assert!(!f.trim().is_empty(), "{}: {key} is empty", lang.code());
                    assert_eq!(placeholders(f), want, "{}: {key} placeholders", lang.code());
                }
                if let Entry::Forms(f) = e {
                    assert!(f.contains_key("other"), "{}: {key} has no \"other\" form", lang.code());
                    let cats: Vec<_> = (0..30).map(|n| lang.plural(n)).collect();
                    for c in cats {
                        assert!(f.contains_key(c), "{}: {key} lacks the \"{c}\" form", lang.code());
                    }
                }
            }
        }
    }

    #[test]
    fn plural_forms() {
        assert_eq!(trn_in(Lang::En, "panel.open", 1, &[]), "1 open");
        assert_eq!(trn_in(Lang::En, "panel.open", 2, &[]), "2 open");
        assert_eq!(Lang::Ru.plural(1), "one");
        assert_eq!(Lang::Ru.plural(11), "many");
        assert_eq!(Lang::Ru.plural(22), "few");
        assert_eq!(Lang::Ru.plural(5), "many");
        assert_eq!(Lang::Pl.plural(1), "one");
        assert_eq!(Lang::Pl.plural(3), "few");
        assert_eq!(Lang::Pl.plural(12), "many");
        assert_eq!(Lang::Fr.plural(0), "one");
        assert_eq!(Lang::Ja.plural(1), "other");
        assert!(trn_in(Lang::Ru, "ruler.stack", 5, &[]).starts_with("5 "));
    }

    #[test]
    fn fills_and_falls_back() {
        assert_eq!(fill("{a}-{b}-{a}", &[("a", &1), ("b", &"x")]), "1-x-1");
        // Inserted values are not expanded again; unknown or unclosed braces stay literal.
        assert_eq!(fill("{name}: {err}", &[("name", &"a{err}.mp4"), ("err", &"bad")]), "a{err}.mp4: bad");
        assert_eq!(fill("{x} {a} {", &[("a", &"{a}")]), "{x} {a} {");
        assert_eq!(fill("{n} {file}", &[("n", &2), ("file", &"{n}")]), "2 {n}");
        assert_eq!(tr_in(Lang::Hu, "no.such.key"), "no.such.key");
        assert_eq!(tr_in(Lang::Hu, "panel.notes"), "Jegyzetek");
        assert_eq!(Lang::from_code("hu_HU.UTF-8"), Some(Lang::Hu));
        assert_eq!(Lang::from_code("zh-Hans-CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_code("pt-BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_code("xx"), None);
    }
}
