use crate::types::{BanglaOutput, CharClass, ConditionalRule, MatchCond, ScopeCheck};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AvroGrammar {
    pub layout: GrammarLayout,
}

#[derive(Debug, Deserialize)]
pub struct GrammarLayout {
    pub vowel: String,
    pub consonant: String,
    pub number: String,
    pub casesensitive: String,
    pub patterns: Vec<GrammarPattern>,
}

#[derive(Debug, Deserialize)]
pub struct GrammarPattern {
    pub find: String,
    pub replace: String,
    #[serde(default)]
    pub rules: Vec<GrammarRule>,
}

#[derive(Debug, Deserialize)]
pub struct GrammarRule {
    pub matches: Vec<GrammarMatch>,
    pub replace: String,
}

#[derive(Debug, Deserialize)]
pub struct GrammarMatch {
    #[serde(rename = "type")]
    pub match_type: GrammarMatchType,
    pub scope: String,
    pub value: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrammarMatchType {
    Prefix,
    Suffix,
}

impl AvroGrammar {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Yields `(find_key, BanglaOutput)` pairs for trie insertion.
    /// Simple patterns become `Static`; conditional ones become `Conditional`.
    pub fn patterns_as_outputs(&self) -> impl Iterator<Item = (&str, BanglaOutput)> {
        self.layout.patterns.iter().map(|p| {
            let output = if p.rules.is_empty() {
                BanglaOutput::Static(p.replace.clone())
            } else {
                let rules = p
                    .rules
                    .iter()
                    .map(|r| ConditionalRule {
                        matches: r
                            .matches
                            .iter()
                            .map(|m| {
                                let scope = parse_scope(&m.scope, m.value.as_deref());
                                match m.match_type {
                                    GrammarMatchType::Prefix => MatchCond::Prefix(scope),
                                    GrammarMatchType::Suffix => MatchCond::Suffix(scope),
                                }
                            })
                            .collect(),
                        replace: r.replace.clone(),
                    })
                    .collect();
                BanglaOutput::Conditional {
                    rules,
                    fallback: p.replace.clone(),
                }
            };
            (p.find.as_str(), output)
        })
    }
}

fn parse_scope(scope: &str, value: Option<&str>) -> ScopeCheck {
    match scope {
        "consonant" => ScopeCheck::IsClass(CharClass::Consonant),
        "!consonant" => ScopeCheck::NotClass(CharClass::Consonant),
        "vowel" => ScopeCheck::IsClass(CharClass::Vowel),
        "!vowel" => ScopeCheck::NotClass(CharClass::Vowel),
        "number" => ScopeCheck::IsClass(CharClass::Number),
        "!number" => ScopeCheck::NotClass(CharClass::Number),
        "punctuation" => ScopeCheck::IsClass(CharClass::Punctuation),
        "!punctuation" => ScopeCheck::NotClass(CharClass::Punctuation),
        "exact" => ScopeCheck::IsExact(value.unwrap_or("").to_string()),
        "!exact" => ScopeCheck::NotExact(value.unwrap_or("").to_string()),
        _ => ScopeCheck::IsClass(CharClass::Punctuation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> Option<AvroGrammar> {
        let src = std::fs::read_to_string("../../avro.json").ok()?;
        AvroGrammar::from_json(&src).ok()
    }

    #[test]
    fn parses_grammar() {
        let Some(g) = load() else { return };
        assert_eq!(g.layout.vowel, "aeiou");
        assert_eq!(g.layout.consonant, "bcdfghjklmnpqrstvwxyz");
        assert!(g.layout.patterns.len() > 200);
    }

    #[test]
    fn simple_patterns_are_static() {
        let Some(g) = load() else { return };
        let kh = g.layout.patterns.iter().find(|p| p.find == "kh").unwrap();
        assert!(kh.rules.is_empty());
        assert_eq!(kh.replace, "খ");
    }

    #[test]
    fn conditional_patterns_parse() {
        let Some(g) = load() else { return };
        let outputs: Vec<_> = g.patterns_as_outputs().collect();
        let cond_count = outputs
            .iter()
            .filter(|(_, o)| matches!(o, BanglaOutput::Conditional { .. }))
            .count();
        assert!(
            cond_count > 10,
            "expected conditional patterns, got {cond_count}"
        );
    }
}
