use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum BanglaOutput {
    Static(String),
    Contextual { independent: String, dependent: String },
    /// Rules from JSON grammar; first matching rule wins, `fallback` is used when none match.
    Conditional { rules: Vec<ConditionalRule>, fallback: String },
}

#[derive(Debug, Clone)]
pub struct ConditionalRule {
    pub matches: Vec<MatchCond>,
    pub replace: String,
}

#[derive(Debug, Clone)]
pub enum MatchCond {
    Prefix(ScopeCheck),
    Suffix(ScopeCheck),
}

#[derive(Debug, Clone)]
pub enum ScopeCheck {
    IsClass(CharClass),
    NotClass(CharClass),
    IsExact(String),
    NotExact(String),
}

#[derive(Debug, Clone)]
pub enum CharClass {
    Consonant,
    Vowel,
    Number,
    Punctuation,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputContext {
    Neutral,
    AfterConsonant,
}

#[derive(Debug, Clone)]
pub struct EngineState {
    pub prefix: String,
    pub word_buffer: String,
    pub context: OutputContext,
    pub last_roman: Option<char>,
}

pub(crate) struct TrieNode {
    pub output: Option<BanglaOutput>,
    pub children: HashMap<char, TrieNode>,
}

impl TrieNode {
    pub fn new() -> Self {
        Self { output: None, children: HashMap::new() }
    }
}
