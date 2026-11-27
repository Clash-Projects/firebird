use std::collections::HashSet;
use std::sync::OnceLock;

/// Shared matching vocabulary for the two YouTube/addon matchers.
///
/// Safety note: these words only ever *demote* a candidate that carries
/// them unasked (penalty, pool exclusion, faithful-version veto). A
/// request that names the word itself ("Live Forever", "Stereo Hearts",
/// "Part of Me", "Tribute") always matches itself exactly, so genuine
/// titles containing these words are unaffected.
pub struct MatchVocab;

impl MatchVocab {
    /// Language names that appear in titles/albums of per-language releases
    /// (one soundtrack released in Telugu / Hindi / Tamil under the same
    /// title and artist). Port of Native's `LANGUAGE_TOKENS`.
    pub const LANGUAGE_TOKENS: &'static [&'static str] = &[
        "telugu",
        "tamil",
        "hindi",
        "kannada",
        "malayalam",
        "punjabi",
        "marathi",
        "gujarati",
        "bengali",
        "bhojpuri",
        "odia",
        "oriya",
        "assamese",
        "urdu",
        "sanskrit",
        "haryanvi",
        "rajasthani",
        "nepali",
        "sinhala",
        "sinhalese",
        "english",
        "spanish",
        "espanol",
        "french",
        "francais",
        "german",
        "deutsch",
        "italian",
        "italiano",
        "portuguese",
        "portugues",
        "japanese",
        "korean",
        "chinese",
        "mandarin",
        "cantonese",
        "arabic",
        "turkish",
        "persian",
        "farsi",
        "russian",
        "indonesian",
        "malay",
        "thai",
        "vietnamese",
        "tagalog",
        "filipino",
    ];

    /// Version billing words (excluding [`Self::LANGUAGE_TOKENS`], which are
    /// included in [`Self::version_words`]).
    pub const BASE_VERSION_WORDS: &'static [&'static str] = &[
        // Core versions
        "live",
        "remix",
        "karaoke",
        "cover",
        "instrumental",
        "slowed",
        "sped",
        "nightcore",
        "acoustic",
        "demo",
        "edit",
        "remaster",
        "remastered",
        "mono",
        "stereo",
        "version",
        "deluxe",
        "bonus",
        "mix",
        "extended",
        "radio",
        "clean",
        "explicit",
        "original",
        "orchestral",
        "unplugged",
        "rerecorded",
        "anniversary",
        "edition",
        // Android VERSION_WORDS + identity variants
        "remixes",
        "rmx",
        "refix",
        "flip",
        "bootleg",
        "mashup",
        "medley",
        "concert",
        "vocals",
        "vocal",
        "acapella",
        "acappella",
        "backing",
        "stems",
        "stem",
        "reprise",
        "remake",
        "rework",
        "reverb",
        "lofi",
        "symphonic",
        "part",
        "pt",
        "chapter",
        "atmos",
        "dolby",
        "spatial",
        "tribute",
        "chopped",
        "screwed",
        "boosted",
        "8d",
        "session",
        "unreleased",
        "instrumentals",
        "offvocal",
        "minus",
        "bgm",
        "piano",
        "guitar",
        "violin",
        "cello",
        "lullaby",
        "ringtone",
        "8bit",
        // Dubbed / translated releases
        "dub",
        "dubbed",
    ];

    /// Label filler that never distinguishes recordings: parental tags,
    /// upload/packaging labels and the bare feat keywords left over after
    /// the credit itself is removed. Real version billing is NOT here.
    pub const NEUTRAL_WORDS: &'static [&'static str] = &[
        "explicit",
        "clean",
        "official",
        "audio",
        "video",
        "visualizer",
        "lyrics",
        "lyric",
        "lyrical",
        "hd",
        "hq",
        "4k",
        "track",
        "music",
        "song",
        "songs",
        "full",
        "mv",
        "ost",
        "soundtrack",
        "feat",
        "ft",
    ];

    /// Cached set of all version words (base version words + language tokens).
    pub fn version_words() -> &'static HashSet<&'static str> {
        static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
        SET.get_or_init(|| {
            let mut set = HashSet::with_capacity(
                Self::BASE_VERSION_WORDS.len() + Self::LANGUAGE_TOKENS.len(),
            );
            set.extend(Self::BASE_VERSION_WORDS.iter().copied());
            set.extend(Self::LANGUAGE_TOKENS.iter().copied());
            set
        })
    }

    /// Cached set of [`Self::LANGUAGE_TOKENS`].
    pub fn language_tokens() -> &'static HashSet<&'static str> {
        static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
        SET.get_or_init(|| Self::LANGUAGE_TOKENS.iter().copied().collect())
    }

    /// Cached set of [`Self::NEUTRAL_WORDS`].
    pub fn neutral_words() -> &'static HashSet<&'static str> {
        static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
        SET.get_or_init(|| Self::NEUTRAL_WORDS.iter().copied().collect())
    }

    /// Languages a title/album names, by script and by language word.
    /// Port of Native's `extractLanguages`.
    pub fn languages_of(raw: &str) -> HashSet<String> {
        if raw.trim().is_empty() {
            return HashSet::new();
        }
        let mut out = HashSet::new();

        for ch in raw.chars() {
            match ch {
                '\u{0C00}'..='\u{0C7F}' => {
                    out.insert("telugu".to_string());
                }
                '\u{0B80}'..='\u{0BFF}' => {
                    out.insert("tamil".to_string());
                }
                '\u{0C80}'..='\u{0CFF}' => {
                    out.insert("kannada".to_string());
                }
                '\u{0D00}'..='\u{0D7F}' => {
                    out.insert("malayalam".to_string());
                }
                '\u{0A00}'..='\u{0A7F}' => {
                    out.insert("punjabi".to_string());
                }
                '\u{0980}'..='\u{09FF}' => {
                    out.insert("bengali".to_string());
                }
                '\u{0A80}'..='\u{0AFF}' => {
                    out.insert("gujarati".to_string());
                }
                '\u{0900}'..='\u{097F}' => {
                    out.insert("hindi".to_string());
                }
                '\u{AC00}'..='\u{D7AF}'
                | '\u{1100}'..='\u{11FF}'
                | '\u{3130}'..='\u{318F}' => {
                    out.insert("korean".to_string());
                }
                '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' => {
                    out.insert("japanese".to_string());
                }
                _ => {}
            }
        }

        let tokens = Self::language_tokens();
        let lower = raw.to_lowercase();
        let sanitized: String = lower
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect();

        for w in sanitized.split_whitespace() {
            if tokens.contains(w) {
                out.insert(w.to_string());
            }
            match w {
                "panjabi" => {
                    out.insert("punjabi".to_string());
                }
                "bangla" => {
                    out.insert("bengali".to_string());
                }
                "espanol" => {
                    out.insert("spanish".to_string());
                }
                "francais" => {
                    out.insert("french".to_string());
                }
                "deutsch" => {
                    out.insert("german".to_string());
                }
                "italiano" => {
                    out.insert("italian".to_string());
                }
                "portugues" => {
                    out.insert("portuguese".to_string());
                }
                "farsi" => {
                    out.insert("persian".to_string());
                }
                "sinhalese" => {
                    out.insert("sinhala".to_string());
                }
                "filipino" => {
                    out.insert("tagalog".to_string());
                }
                _ => {}
            }
        }

        out
    }
}
