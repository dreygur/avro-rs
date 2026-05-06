use std::collections::HashMap;

#[derive(Debug)]
pub enum DictError {
    MissingJson,
    Json(serde_json::Error),
}

impl std::fmt::Display for DictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingJson => write!(f, "no JSON object found in input"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for DictError {}

impl From<serde_json::Error> for DictError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

fn extract_json(src: &str) -> Option<&str> {
    let start = src.find('{')?;
    let end = src.rfind('}')?;
    Some(&src[start..=end])
}

/// Word lists keyed by phonetic prefix (e.g. `"w_k"` → `["কলম", ...]`).
pub struct WordDict(HashMap<String, Vec<String>>);

impl WordDict {
    pub fn from_js(src: &str) -> Result<Self, DictError> {
        let json = extract_json(src).ok_or(DictError::MissingJson)?;
        let map = serde_json::from_str::<HashMap<String, Vec<String>>>(json)?;
        Ok(Self(map))
    }

    /// Returns the word list for the given phonetic prefix key (e.g. `"w_kh"`).
    pub fn words_for(&self, key: &str) -> &[String] {
        self.0.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }

    pub fn total_words(&self) -> usize {
        self.0.values().map(Vec::len).sum()
    }
}

/// Phonetic suffix → Bangla string (e.g. `"er"` → `"ের"`).
pub struct SuffixDict(HashMap<String, String>);

impl SuffixDict {
    pub fn from_js(src: &str) -> Result<Self, DictError> {
        let json = extract_json(src).ok_or(DictError::MissingJson)?;
        let map = serde_json::from_str::<HashMap<String, String>>(json)?;
        Ok(Self(map))
    }

    pub fn bangla_for(&self, suffix: &str) -> Option<&str> {
        self.0.get(suffix).map(String::as_str)
    }

    /// All Bangla suffix strings (values), without their phonetic keys.
    pub fn bangla_values(&self) -> impl Iterator<Item = &str> {
        self.0.values().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    #[test]
    fn word_dict_parses() {
        let Some(src) = load("../../avrodict.js") else { return };
        let dict = WordDict::from_js(&src).unwrap();
        assert_eq!(dict.keys().count(), 47);
        assert!(!dict.words_for("w_k").is_empty());
        assert!(!dict.words_for("w_a").is_empty());
    }

    #[test]
    fn word_dict_total_entries() {
        let Some(src) = load("../../avrodict.js") else { return };
        let dict = WordDict::from_js(&src).unwrap();
        assert!(dict.total_words() > 100_000);
    }

    #[test]
    fn suffix_dict_parses() {
        let Some(src) = load("../../suffixdict.js") else { return };
        let dict = SuffixDict::from_js(&src).unwrap();
        assert_eq!(dict.bangla_for("e"), Some("ে"));
        assert_eq!(dict.bangla_for("er"), Some("ের"));
        assert_eq!(dict.bangla_for("ra"), Some("রা"));
        assert!(dict.len() > 700);
    }

    #[test]
    fn missing_json_error() {
        assert!(matches!(
            WordDict::from_js("no json here"),
            Err(DictError::MissingJson)
        ));
    }

    #[test]
    fn invalid_json_error() {
        assert!(matches!(
            WordDict::from_js("{not: valid}"),
            Err(DictError::Json(_))
        ));
    }
}
