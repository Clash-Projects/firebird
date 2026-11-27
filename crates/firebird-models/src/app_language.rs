use serde::{Deserialize, Serialize};

/// The 15 fully-localized languages of LastWave (parity with LastWave-Native)
/// plus System Default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AppLanguage {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "tr")]
    Turkish,
    #[serde(rename = "zh-CN")]
    ChineseSimplified,
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "pt-BR")]
    PortugueseBrazil,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "id")]
    Indonesian,
    #[serde(rename = "hi")]
    Hindi,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "it")]
    Italian,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "ar")]
    Arabic,
    #[serde(rename = "vi")]
    Vietnamese,
}

impl AppLanguage {
    /// All enum variants in declaration order.
    pub const ALL: [Self; 16] = [
        Self::System,
        Self::English,
        Self::Turkish,
        Self::ChineseSimplified,
        Self::Russian,
        Self::PortugueseBrazil,
        Self::Spanish,
        Self::Indonesian,
        Self::Hindi,
        Self::German,
        Self::French,
        Self::Italian,
        Self::Japanese,
        Self::Korean,
        Self::Arabic,
        Self::Vietnamese,
    ];

    /// Supported concrete BCP-47 locale tags (excluding [`Self::System`]).
    pub const SUPPORTED_LOCALE_TAGS: [&'static str; 15] = [
        "en", "tr", "zh-CN", "ru", "pt-BR", "es", "id", "hi", "de", "fr", "it", "ja", "ko", "ar",
        "vi",
    ];

    /// Canonical tag persisted in prefs and serialized via serde.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en",
            Self::Turkish => "tr",
            Self::ChineseSimplified => "zh-CN",
            Self::Russian => "ru",
            Self::PortugueseBrazil => "pt-BR",
            Self::Spanish => "es",
            Self::Indonesian => "id",
            Self::Hindi => "hi",
            Self::German => "de",
            Self::French => "fr",
            Self::Italian => "it",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Arabic => "ar",
            Self::Vietnamese => "vi",
        }
    }

    /// English display name for the language.
    pub const fn english_name(self) -> &'static str {
        match self {
            Self::System => "System default",
            Self::English => "English",
            Self::Turkish => "Turkish",
            Self::ChineseSimplified => "Chinese (Simplified)",
            Self::Russian => "Russian",
            Self::PortugueseBrazil => "Portuguese (Brazil)",
            Self::Spanish => "Spanish",
            Self::Indonesian => "Indonesian",
            Self::Hindi => "Hindi",
            Self::German => "German",
            Self::French => "French",
            Self::Italian => "Italian",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::Arabic => "Arabic",
            Self::Vietnamese => "Vietnamese",
        }
    }

    /// Native display name for the language.
    pub const fn native_display_name(self) -> &'static str {
        match self {
            Self::System => "System default",
            Self::English => "English",
            Self::Turkish => "Türkçe",
            Self::ChineseSimplified => "简体中文",
            Self::Russian => "Русский",
            Self::PortugueseBrazil => "Português (Brasil)",
            Self::Spanish => "Español",
            Self::Indonesian => "Bahasa Indonesia",
            Self::Hindi => "हिन्दी",
            Self::German => "Deutsch",
            Self::French => "Français",
            Self::Italian => "Italiano",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
            Self::Arabic => "العربية",
            Self::Vietnamese => "Tiếng Việt",
        }
    }

    /// Whether this language uses right-to-left script (Arabic).
    pub const fn is_rtl(self) -> bool {
        matches!(self, Self::Arabic)
    }

    /// Resolves [`Self::System`] against an optional OS locale tag, falling back
    /// to [`Self::English`] when no OS locale is available.
    pub fn effective_language_for_locale(self, os_locale: Option<&str>) -> Self {
        if self != Self::System {
            return self;
        }
        match os_locale {
            Some(tag) if !tag.trim().is_empty() => Self::from_tag(Some(tag)),
            _ => Self::English,
        }
    }

    /// Active effective [`AppLanguage`] (falls back to [`Self::English`] for [`Self::System`]).
    pub fn effective_language(self) -> Self {
        self.effective_language_for_locale(None)
    }

    /// InnerTube `hl` parameter mapped strictly from the Limusic supported whitelist:
    /// `en, tr, zh-Hans, ru, pt-BR, es, id, de, fr, it, ja, ko, vi`.
    ///
    /// Non-Limusic languages (e.g. `hi`, `ar`) strictly fall back to `en`.
    pub fn innertube_hl_for_locale(self, os_locale: Option<&str>) -> &'static str {
        match self.effective_language_for_locale(os_locale) {
            Self::English => "en",
            Self::Turkish => "tr",
            Self::ChineseSimplified => "zh-Hans",
            Self::Russian => "ru",
            Self::PortugueseBrazil => "pt-BR",
            Self::Spanish => "es",
            Self::Indonesian => "id",
            Self::German => "de",
            Self::French => "fr",
            Self::Italian => "it",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::Vietnamese => "vi",
            _ => "en",
        }
    }

    /// InnerTube `hl` parameter without an explicit OS locale override.
    pub fn innertube_hl(self) -> &'static str {
        self.innertube_hl_for_locale(None)
    }

    /// Normalizes any tag to a supported [`AppLanguage`].
    pub fn from_tag(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::System;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Self::System;
        }
        let cleaned = trimmed.replace('_', "-");
        let cleaned_lower = cleaned.to_lowercase();

        for &lang in &Self::ALL {
            if lang.tag().eq_ignore_ascii_case(&cleaned) {
                return lang;
            }
        }

        match cleaned_lower.as_str() {
            "zh-hans" | "zh-cn" | "zh-sg" | "zh" => return Self::ChineseSimplified,
            "in" => return Self::Indonesian,
            "pt" | "pt-pt" => return Self::PortugueseBrazil,
            _ => {}
        }

        let base = cleaned_lower.split('-').next().unwrap_or("");
        for &lang in &Self::ALL {
            if lang.tag().to_lowercase() == base {
                return lang;
            }
        }

        Self::System
    }
}
