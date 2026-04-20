use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read as _},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use quick_xml::events::BytesText;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Contains,
    StartsWith,
    WholeWord,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_true")]
    pub ignore_diacritics: bool,
    #[serde(default)]
    pub max_results: Option<usize>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerseHit {
    pub translation_id: String,
    pub bnumber: u16,
    pub cnumber: u16,
    pub vnumber: u16,
    pub reference: String,
    pub text: String,
}

#[derive(Debug, Clone)]
struct VerseRecord {
    bnumber: u16,
    cnumber: u16,
    vnumber: u16,
    text: String,
    normalized: String,
    words: Vec<String>,
}

#[derive(Debug, Clone)]
struct LoadedTranslation {
    id: String,
    verses: Vec<VerseRecord>,
    word_index: HashMap<String, Vec<usize>>,
}

#[derive(Default)]
struct AppState {
    loaded: Option<LoadedTranslation>,
}

type SharedState = Arc<Mutex<AppState>>;

fn fold_diacritics(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization as _;
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

fn normalize_text(s: &str, case_sensitive: bool, ignore_diacritics: bool) -> String {
    let mut out = s.trim().to_string();
    if ignore_diacritics {
        out = fold_diacritics(&out);
    }
    if !case_sensitive {
        out = out.to_lowercase();
    }
    out
}

fn tokenize_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '’' {
            cur.push(ch);
        } else if !cur.is_empty() {
            words.push(cur.clone());
            cur.clear();
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn translation_specs() -> Vec<(String, String, String)> {
    vec![
        (
            "de_sch1951".to_string(),
            "Deutsch – Schlachter 1951".to_string(),
            "SF_2009-01-20_GER_SCH1951_(SCHLACHTER 1951).zip".to_string(),
        ),
        (
            "ro_rccv".to_string(),
            "Română – RCCV".to_string(),
            "SF_2013-09-09_RUM_RCCV_(ROMANIAN CORRECTED CORNILESCU BIBLE).zip".to_string(),
        ),
    ]
}

fn book_name_de(bnumber: u16) -> &'static str {
    const N: [&str; 66] = [
        "1. Mose", "2. Mose", "3. Mose", "4. Mose", "5. Mose", "Josua", "Richter", "Ruth",
        "1. Samuel", "2. Samuel", "1. Könige", "2. Könige", "1. Chronik", "2. Chronik", "Esra",
        "Nehemia", "Ester", "Hiob", "Psalm", "Sprüche", "Prediger", "Hoheslied", "Jesaja",
        "Jeremia", "Klagelieder", "Hesekiel", "Daniel", "Hosea", "Joel", "Amos", "Obadja",
        "Jona", "Micha", "Nahum", "Habakuk", "Zefanja", "Haggai", "Sacharja", "Maleachi",
        "Matthäus", "Markus", "Lukas", "Johannes", "Apostelgeschichte", "Römer",
        "1. Korinther", "2. Korinther", "Galater", "Epheser", "Philipper", "Kolosser",
        "1. Thessalonicher", "2. Thessalonicher", "1. Timotheus", "2. Timotheus", "Titus",
        "Philemon", "Hebräer", "Jakobus", "1. Petrus", "2. Petrus", "1. Johannes",
        "2. Johannes", "3. Johannes", "Judas", "Offenbarung",
    ];
    N.get((bnumber.saturating_sub(1)) as usize)
        .copied()
        .unwrap_or("Buch")
}

fn resolve_resource_zip(app: &tauri::AppHandle, zip_name: &str) -> anyhow::Result<PathBuf> {
    // In packaged builds, resources live in BaseDirectory::Resource.
    let bundled = app
        .path()
        .resolve(zip_name, tauri::path::BaseDirectory::Resource)?;
    if bundled.exists() {
        return Ok(bundled);
    }

    // Dev fallback: read ZIPs directly from workspace folder.
    let project_zip = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("Zefania XML Bibelübersetzungen")
        .join(if zip_name.contains("_GER_") {
            "Deutsch"
        } else {
            "Rumänisch"
        })
        .join(zip_name);

    if project_zip.exists() {
        return Ok(project_zip);
    }

    anyhow::bail!("Resource ZIP not found: {zip_name}")
}

fn load_zefania_from_zip(zip_path: &PathBuf) -> anyhow::Result<Vec<(u16, u16, u16, String)>> {
    let bytes = fs::read(zip_path)?;
    let reader = Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader)?;
    if zip.len() == 0 {
        anyhow::bail!("ZIP is empty");
    }
    let mut file = zip.by_index(0)?;
    let mut xml = String::new();
    file.read_to_string(&mut xml)?;

    // quick-xml pull parser: gather (b,c,v,text)
    #[derive(Debug)]
    enum Ctx {
        None,
        BibleBook { b: u16 },
        Chapter { b: u16, c: u16 },
        Vers { b: u16, c: u16, v: u16 },
    }

    let mut reader = quick_xml::Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut ctx = Ctx::None;
    let mut out: Vec<(u16, u16, u16, String)> = Vec::new();

    loop {
        use quick_xml::events::Event;
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                if name.as_ref() == b"BIBLEBOOK" {
                    let mut b: u16 = 0;
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == b"bnumber" {
                            b = std::str::from_utf8(&a.value)?.parse::<u16>().unwrap_or(0);
                        }
                    }
                    ctx = Ctx::BibleBook { b };
                } else if name.as_ref() == b"CHAPTER" {
                    let mut c: u16 = 0;
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == b"cnumber" {
                            c = std::str::from_utf8(&a.value)?.parse::<u16>().unwrap_or(0);
                        }
                    }
                    ctx = match ctx {
                        Ctx::BibleBook { b } => Ctx::Chapter { b, c },
                        Ctx::Chapter { b, .. } => Ctx::Chapter { b, c },
                        Ctx::Vers { b, c: prev, .. } => Ctx::Chapter { b, c: prev },
                        Ctx::None => Ctx::Chapter { b: 0, c },
                    };
                } else if name.as_ref() == b"VERS" {
                    let mut v: u16 = 0;
                    for a in e.attributes().flatten() {
                        if a.key.as_ref() == b"vnumber" {
                            v = std::str::from_utf8(&a.value)?.parse::<u16>().unwrap_or(0);
                        }
                    }
                    if let Ctx::Chapter { b, c } = ctx {
                        ctx = Ctx::Vers { b, c, v };
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Ctx::Vers { b, c, v } = ctx {
                    let text = bytes_text_to_string(&reader, &t)?;
                    if !text.is_empty() {
                        out.push((b, c, v, text));
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                if name.as_ref() == b"VERS" {
                    ctx = match ctx {
                        Ctx::Vers { b, c, .. } => Ctx::Chapter { b, c },
                        other => other,
                    };
                } else if name.as_ref() == b"CHAPTER" {
                    ctx = match ctx {
                        Ctx::Chapter { b, .. } => Ctx::BibleBook { b },
                        other => other,
                    };
                } else if name.as_ref() == b"BIBLEBOOK" {
                    ctx = Ctx::None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(out)
}

fn bytes_text_to_string(
    reader: &quick_xml::Reader<&[u8]>,
    t: &BytesText<'_>,
) -> anyhow::Result<String> {
    // quick-xml 0.38: `BytesText` doesn't expose `unescape()`; decode + unescape entities manually.
    let raw = t.as_ref();
    let s = reader.decoder().decode(raw)?.to_string();
    Ok(s
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'"))
}

fn build_loaded_translation(
    id: String,
    _label: String,
    verses_raw: Vec<(u16, u16, u16, String)>,
) -> LoadedTranslation {
    // Index built on normalized text: case-insensitive + diacritics folded (fast default).
    let mut verses = Vec::with_capacity(verses_raw.len());
    let mut word_index: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, (b, c, v, text)) in verses_raw.into_iter().enumerate() {
        let normalized = normalize_text(&text, false, true);
        let words = tokenize_words(&normalized);
        for w in &words {
            word_index.entry(w.clone()).or_default().push(idx);
        }
        verses.push(VerseRecord {
            bnumber: b,
            cnumber: c,
            vnumber: v,
            text,
            normalized,
            words,
        });
    }

    // De-dup index postings (just in case)
    for postings in word_index.values_mut() {
        postings.sort_unstable();
        postings.dedup();
    }

    LoadedTranslation {
        id,
        verses,
        word_index,
    }
}

fn reference_for(_translation_id: &str, b: u16, c: u16, v: u16) -> String {
    // Default reference format: German book names (works fine for both langs).
    // Can be extended later with per-translation book name maps.
    let book = book_name_de(b);
    format!("{book} {c}:{v}")
}

fn search_loaded(
    loaded: &LoadedTranslation,
    query: &str,
    mode: SearchMode,
    options: &SearchOptions,
) -> anyhow::Result<Vec<VerseHit>> {
    let qn = normalize_text(query, options.case_sensitive, options.ignore_diacritics);
    if qn.is_empty() {
        return Ok(vec![]);
    }

    let max_results = options.max_results.unwrap_or(500);

    let mut candidate_ids: Option<Vec<usize>> = None;

    if matches!(mode, SearchMode::WholeWord) {
        let w = qn.clone();
        candidate_ids = Some(loaded.word_index.get(&w).cloned().unwrap_or_default());
    } else if !matches!(mode, SearchMode::Regex) {
        // Cheap prefilter when query has multiple words: intersect postings lists.
        let words = tokenize_words(&qn);
        if words.len() >= 2 {
            let mut lists: Vec<Vec<usize>> = words
                .iter()
                .filter_map(|w| loaded.word_index.get(w).cloned())
                .collect();
            if !lists.is_empty() {
                lists.sort_by_key(|l| l.len());
                let mut acc: HashSet<usize> = lists[0].iter().copied().collect();
                for l in lists.iter().skip(1) {
                    let set: HashSet<usize> = l.iter().copied().collect();
                    acc = acc.intersection(&set).copied().collect();
                    if acc.is_empty() {
                        break;
                    }
                }
                let mut ids: Vec<usize> = acc.into_iter().collect();
                ids.sort_unstable();
                candidate_ids = Some(ids);
            }
        }
    }

    let re = if matches!(mode, SearchMode::Regex) {
        Some(Regex::new(query)?)
    } else {
        None
    };

    let iter: Box<dyn Iterator<Item = (usize, &VerseRecord)> + '_> = match candidate_ids {
        Some(ids) => Box::new(ids.into_iter().filter_map(|i| loaded.verses.get(i).map(|v| (i, v)))),
        None => Box::new(loaded.verses.iter().enumerate()),
    };

    let mut hits = Vec::new();
    for (_i, vr) in iter {
        if hits.len() >= max_results {
            break;
        }

        let hay = if options.case_sensitive && !options.ignore_diacritics {
            &vr.text
        } else if options.case_sensitive && options.ignore_diacritics {
            // compute on the fly (rare)
            // (still fast enough, only used when user toggles options)
            // We could cache this later if needed.
            // NOTE: uses folded but original case.
            // We'll just fold original text without lowercasing.
            // (This doesn't match exact casing with combined diacritics perfectly, but acceptable.)
            // If needed we can store more variants in VerseRecord.
            // For now: fallback to normalized + compare with lowercased? Not correct for case_sensitive.
            // So: do a folded-only string.
            // We'll allocate:
            // (kept minimal for v1)
            // 
            // SAFETY: fine.
            // 
            // return string and compare with qn
            // 
            // We'll do it below.
            ""
        } else {
            &vr.normalized
        };

        let is_match = match mode {
            SearchMode::Contains => {
                if hay.is_empty() && options.case_sensitive && options.ignore_diacritics {
                    let folded = fold_diacritics(vr.text.as_str());
                    folded.contains(&qn)
                } else {
                    hay.contains(&qn)
                }
            }
            SearchMode::StartsWith => {
                if hay.is_empty() && options.case_sensitive && options.ignore_diacritics {
                    let folded = fold_diacritics(vr.text.as_str());
                    folded.starts_with(&qn)
                } else {
                    hay.starts_with(&qn)
                }
            }
            SearchMode::WholeWord => vr.words.iter().any(|w| w == &qn),
            SearchMode::Regex => re.as_ref().unwrap().is_match(vr.text.as_str()),
        };

        if is_match {
            hits.push(VerseHit {
                translation_id: loaded.id.clone(),
                bnumber: vr.bnumber,
                cnumber: vr.cnumber,
                vnumber: vr.vnumber,
                reference: reference_for(&loaded.id, vr.bnumber, vr.cnumber, vr.vnumber),
                text: vr.text.clone(),
            });
        }
    }

    Ok(hits)
}

#[tauri::command]
fn list_translations() -> Vec<TranslationInfo> {
    translation_specs()
        .into_iter()
        .map(|(id, label, _zip)| TranslationInfo { id, label })
        .collect()
}

#[tauri::command]
fn load_translation(app: tauri::AppHandle, state: tauri::State<SharedState>, translation_id: String) -> Result<TranslationInfo, String> {
    let spec = translation_specs()
        .into_iter()
        .find(|(id, _, _)| id == &translation_id)
        .ok_or_else(|| "Unknown translation_id".to_string())?;

    let zip_path = resolve_resource_zip(&app, &spec.2).map_err(|e| e.to_string())?;
    let verses_raw = load_zefania_from_zip(&zip_path).map_err(|e| e.to_string())?;
    let loaded = build_loaded_translation(spec.0.clone(), spec.1.clone(), verses_raw);

    let mut guard = state.lock().map_err(|_| "State poisoned".to_string())?;
    guard.loaded = Some(loaded);

    Ok(TranslationInfo {
        id: spec.0,
        label: spec.1,
    })
}

#[tauri::command]
fn search(
    state: tauri::State<SharedState>,
    query: String,
    mode: SearchMode,
    options: SearchOptions,
) -> Result<Vec<VerseHit>, String> {
    let guard = state.lock().map_err(|_| "State poisoned".to_string())?;
    let loaded = guard.loaded.as_ref().ok_or_else(|| "No translation loaded".to_string())?;
    search_loaded(loaded, &query, mode, &options).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state: SharedState = Arc::new(Mutex::new(AppState::default()));
    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_translations,
            load_translation,
            search
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn german_zip() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Zefania XML Bibelübersetzungen")
            .join("Deutsch")
            .join("SF_2009-01-20_GER_SCH1951_(SCHLACHTER 1951).zip")
    }

    fn romanian_zip() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("Zefania XML Bibelübersetzungen")
            .join("Rumänisch")
            .join("SF_2013-09-09_RUM_RCCV_(ROMANIAN CORRECTED CORNILESCU BIBLE).zip")
    }

    fn default_opts() -> SearchOptions {
        SearchOptions {
            case_sensitive: false,
            ignore_diacritics: true,
            max_results: Some(500),
        }
    }

    #[test]
    fn german_starts_with_genesis_1_1() {
        let verses_raw = load_zefania_from_zip(&german_zip()).expect("load zip");
        let loaded = build_loaded_translation(
            "de_sch1951".to_string(),
            "Deutsch – Schlachter 1951".to_string(),
            verses_raw,
        );
        let hits = search_loaded(&loaded, "Im Anfang", SearchMode::StartsWith, &default_opts())
            .expect("search");
        assert!(
            hits.iter()
                .any(|h| h.bnumber == 1 && h.cnumber == 1 && h.vnumber == 1),
            "expected to find 1. Mose 1:1"
        );
    }

    #[test]
    fn german_whole_word_gott_returns_some() {
        let verses_raw = load_zefania_from_zip(&german_zip()).expect("load zip");
        let loaded = build_loaded_translation(
            "de_sch1951".to_string(),
            "Deutsch – Schlachter 1951".to_string(),
            verses_raw,
        );
        let hits =
            search_loaded(&loaded, "Gott", SearchMode::WholeWord, &default_opts()).expect("search");
        assert!(!hits.is_empty(), "expected hits for whole word 'Gott'");
    }

    #[test]
    fn romanian_starts_with_genesis_1_1() {
        let verses_raw = load_zefania_from_zip(&romanian_zip()).expect("load zip");
        let loaded = build_loaded_translation(
            "ro_rccv".to_string(),
            "Română – RCCV".to_string(),
            verses_raw,
        );
        let hits =
            search_loaded(&loaded, "La început", SearchMode::StartsWith, &default_opts()).expect("search");
        assert!(
            hits.iter()
                .any(|h| h.bnumber == 1 && h.cnumber == 1 && h.vnumber == 1),
            "expected to find Genesis 1:1"
        );
    }
}
