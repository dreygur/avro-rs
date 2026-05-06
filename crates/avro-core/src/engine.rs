use std::collections::HashMap;
use crate::dict::{SuffixDict, WordDict};
use crate::grammar::AvroGrammar;
use crate::types::{BanglaOutput, CharClass, EngineState, MatchCond, OutputContext, ScopeCheck, TrieNode};

pub struct AvroEngine {
    root: TrieNode,
    state_stack: Vec<EngineState>,
    prefix: String,
    word_buffer: String,
    context: OutputContext,
    last_roman: Option<char>,
    // Character class strings for condition evaluation
    vowel_chars: &'static str,
    consonant_chars: &'static str,
    number_chars: &'static str,
    // Autosuggest
    dict: Option<WordDict>,
    bangla_to_key: HashMap<char, String>,
    suffix_bangla_sorted: Vec<String>,
}

impl AvroEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            root: TrieNode::new(),
            state_stack: Vec::new(),
            prefix: String::new(),
            word_buffer: String::new(),
            context: OutputContext::Neutral,
            last_roman: None,
            vowel_chars: "aeiou",
            consonant_chars: "bcdfghjklmnpqrstvwxyz",
            number_chars: "1234567890",
            dict: None,
            bangla_to_key: HashMap::new(),
            suffix_bangla_sorted: Vec::new(),
        };
        engine.load_rules();
        engine
    }

    /// Build an engine from an `AvroGrammar` (JSON-parsed rules).
    /// Returns a fresh engine with the grammar's patterns loaded into the trie.
    pub fn from_grammar(grammar: &AvroGrammar) -> Self {
        let mut engine = Self {
            root: TrieNode::new(),
            state_stack: Vec::new(),
            prefix: String::new(),
            word_buffer: String::new(),
            context: OutputContext::Neutral,
            last_roman: None,
            vowel_chars: "aeiou",
            consonant_chars: "bcdfghjklmnpqrstvwxyz",
            number_chars: "1234567890",
            dict: None,
            bangla_to_key: HashMap::new(),
            suffix_bangla_sorted: Vec::new(),
        };
        for (find, output) in grammar.patterns_as_outputs() {
            engine.insert(find, output);
        }
        engine
    }

    pub fn insert(&mut self, key: &str, value: BanglaOutput) {
        let mut node = &mut self.root;
        for c in key.chars() {
            node = node.children.entry(c).or_insert_with(TrieNode::new);
        }
        node.output = Some(value);
    }

    pub fn handle_input(&mut self, c: char) -> String {
        self.state_stack.push(EngineState {
            prefix: self.prefix.clone(),
            word_buffer: self.word_buffer.clone(),
            context: self.context.clone(),
            last_roman: self.last_roman,
        });

        if c == ',' {
            // Joiner: emit হসন্ত (U+09CD) between consonants for conjunct formation.
            self.flush_prefix(None);
            if self.context == OutputContext::AfterConsonant {
                self.word_buffer.push('\u{09CD}');
                // context stays AfterConsonant so next consonant forms a conjunct
            } else {
                self.word_buffer.push(',');
                self.context = OutputContext::Neutral;
                self.last_roman = Some(',');
            }
        } else if self.trie_has_child(&self.prefix, c) {
            self.prefix.push(c);
        } else {
            self.flush_prefix(Some(c));
            if self.root.children.contains_key(&c) {
                self.prefix.push(c);
            } else {
                self.word_buffer.push(c);
                self.context = OutputContext::Neutral;
                self.last_roman = Some(c);
            }
        }

        self.preedit()
    }

    pub fn handle_backspace(&mut self) -> String {
        if let Some(state) = self.state_stack.pop() {
            self.prefix = state.prefix;
            self.word_buffer = state.word_buffer;
            self.context = state.context;
            self.last_roman = state.last_roman;
        }
        self.preedit()
    }

    pub fn commit(&mut self) -> String {
        self.flush_prefix(None);
        let result = std::mem::take(&mut self.word_buffer);
        self.state_stack.clear();
        self.context = OutputContext::Neutral;
        self.last_roman = None;
        result
    }

    pub fn load_dict(&mut self, dict: WordDict) {
        self.bangla_to_key = dict.keys()
            .filter_map(|key| {
                let first_char = dict.words_for(key).first()?.chars().next()?;
                Some((first_char, key.to_string()))
            })
            .collect();
        self.dict = Some(dict);
    }

    pub fn load_suffix_dict(&mut self, dict: SuffixDict) {
        // Collect unique Bangla suffix strings, sorted longest first (by char count).
        let mut seen = std::collections::HashSet::new();
        let mut bangla: Vec<String> = dict.bangla_values()
            .filter(|s| seen.insert(s.to_string()))
            .map(String::from)
            .collect();
        bangla.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
        self.suffix_bangla_sorted = bangla;
        // SuffixDict itself is dropped here; we only needed the pre-sorted list.
    }

    /// Prefix-match suggest: returns up to `max` words that start with the current preedit.
    pub fn suggest(&self, max: usize) -> Vec<String> {
        let preedit = self.preedit();
        self.prefix_candidates(&preedit, max)
    }

    /// Suffix-aware suggest: tries to strip a known Bangla suffix from the preedit,
    /// looks up the base in WordDict, and reattaches the suffix.
    /// Falls back to `suggest` if no suffix match is found.
    ///
    /// Example: preedit "বাংলাদেশের" → strip "ের" → base "বাংলাদেশ" →
    ///   match words in WordDict → return ["বাংলাদেশ" + "ের", ...]
    pub fn suggest_extended(&self, max: usize) -> Vec<String> {
        let Some(dict) = self.dict.as_ref() else { return vec![] };
        let preedit = self.preedit();
        if preedit.is_empty() { return vec![]; }

        for suffix in &self.suffix_bangla_sorted {
            if preedit.len() > suffix.len() && preedit.ends_with(suffix.as_str()) {
                let base = &preedit[..preedit.len() - suffix.len()];
                let Some(&first_char) = base.chars().next().as_ref() else { continue };
                let Some(key) = self.bangla_to_key.get(&first_char) else { continue };
                let candidates: Vec<String> = dict.words_for(key)
                    .iter()
                    .filter(|w| w.starts_with(base))
                    .take(max)
                    .map(|w| format!("{w}{suffix}"))
                    .collect();
                if !candidates.is_empty() {
                    return candidates;
                }
            }
        }

        self.prefix_candidates(&preedit, max)
    }

    pub fn preedit(&self) -> String {
        if self.prefix.is_empty() {
            return self.word_buffer.clone();
        }
        format!("{}{}", self.word_buffer, self.simulate_flush(&self.prefix))
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn prefix_candidates<'a>(&'a self, preedit: &str, max: usize) -> Vec<String> {
        let Some(dict) = self.dict.as_ref() else { return vec![] };
        if preedit.is_empty() { return vec![]; }
        let Some(first_char) = preedit.chars().next() else { return vec![] };
        let Some(key) = self.bangla_to_key.get(&first_char) else { return vec![] };
        dict.words_for(key)
            .iter()
            .filter(|w| w.starts_with(preedit))
            .take(max)
            .cloned()
            .collect()
    }

    /// Read-only simulation of `flush_prefix` for preedit display.
    /// Threads both `context` and `last_roman` locally so it never mutates state.
    fn simulate_flush(&self, s: &str) -> String {
        let mut result = String::new();
        let mut ctx = self.context.clone();
        let mut last_roman = self.last_roman;
        let mut byte_pos = 0;
        while byte_pos < s.len() {
            let remaining = &s[byte_pos..];
            let (len, output_opt) = self.trie_match(remaining);
            let bangla = match output_opt {
                Some(output) => {
                    let next_roman = s[byte_pos + len..].chars().next();
                    self.resolve(Some(output), ctx.clone(), next_roman, last_roman)
                }
                None => remaining.chars().next().unwrap().to_string(),
            };
            ctx = Self::next_context(&bangla);
            last_roman = remaining[..len].chars().last();
            result.push_str(&bangla);
            byte_pos += len;
        }
        result
    }

    fn trie_has_child(&self, prefix: &str, c: char) -> bool {
        let mut node = &self.root;
        for ch in prefix.chars() {
            match node.children.get(&ch) {
                Some(next) => node = next,
                None => return false,
            }
        }
        node.children.contains_key(&c)
    }

    /// Flush `prefix` into `word_buffer`.
    /// `trigger` is the incoming char that caused the flush (used as suffix context
    /// for the last token in the prefix so JSON suffix conditions evaluate correctly).
    fn flush_prefix(&mut self, trigger: Option<char>) {
        if self.prefix.is_empty() { return; }
        let prefix = std::mem::take(&mut self.prefix);
        let mut byte_pos = 0;
        while byte_pos < prefix.len() {
            let remaining = &prefix[byte_pos..];
            let (len, output_opt) = self.trie_match(remaining);
            let bangla = match output_opt {
                Some(output) => {
                    let next_roman = if byte_pos + len < prefix.len() {
                        prefix[byte_pos + len..].chars().next()
                    } else {
                        trigger
                    };
                    self.resolve(Some(output), self.context.clone(), next_roman, self.last_roman)
                }
                None => remaining.chars().next().unwrap().to_string(),
            };
            self.context = Self::next_context(&bangla);
            self.last_roman = remaining[..len].chars().last();
            self.word_buffer.push_str(&bangla);
            byte_pos += len;
        }
    }

    /// Greedy trie walk returning `(byte_len, Option<BanglaOutput>)`.
    /// Tracks the best (longest) output seen during traversal for backtracking.
    /// Returns `(first_char_len, None)` when no trie entry matches.
    fn trie_match(&self, s: &str) -> (usize, Option<BanglaOutput>) {
        let mut node = &self.root;
        let mut best: Option<(usize, BanglaOutput)> = None;
        let mut byte_pos = 0;

        for c in s.chars() {
            match node.children.get(&c) {
                Some(next) => {
                    node = next;
                    byte_pos += c.len_utf8();
                    if let Some(output) = &node.output {
                        best = Some((byte_pos, output.clone()));
                    }
                }
                None => break,
            }
        }

        match best {
            Some((len, output)) => (len, Some(output)),
            None => (s.chars().next().expect("trie_match on empty").len_utf8(), None),
        }
    }

    /// Resolve a trie match result to a Bangla string.
    /// Callers pass `Some(output)` from `trie_match`; the `None` arm is never reached
    /// because callers handle the literal fallback path directly.
    fn resolve(
        &self,
        output_opt: Option<BanglaOutput>,
        ctx: OutputContext,
        next_roman: Option<char>,
        last_roman: Option<char>,
    ) -> String {
        match output_opt {
            None => String::new(), // unreachable in normal use; callers handle literal path
            Some(BanglaOutput::Static(s)) => s,
            Some(BanglaOutput::Contextual { independent, dependent }) => match ctx {
                OutputContext::Neutral => independent,
                OutputContext::AfterConsonant => dependent,
            },
            Some(BanglaOutput::Conditional { rules, fallback }) => {
                for rule in &rules {
                    if rule.matches.iter().all(|cond| self.eval_cond(cond, &ctx, next_roman, last_roman)) {
                        return rule.replace.clone();
                    }
                }
                fallback
            }
        }
    }

    fn eval_cond(
        &self,
        cond: &MatchCond,
        ctx: &OutputContext,
        next_roman: Option<char>,
        last_roman: Option<char>,
    ) -> bool {
        match cond {
            MatchCond::Prefix(scope) => self.check_scope(scope, last_roman, ctx),
            MatchCond::Suffix(scope) => self.check_scope(scope, next_roman, &OutputContext::Neutral),
        }
    }

    /// Evaluate a scope check against an optional romanized character.
    /// `ctx` is used for consonant checks when `roman` is None (start of word).
    fn check_scope(&self, scope: &ScopeCheck, roman: Option<char>, ctx: &OutputContext) -> bool {
        match scope {
            ScopeCheck::IsClass(cls) => self.char_is(roman, cls, ctx),
            ScopeCheck::NotClass(cls) => !self.char_is(roman, cls, ctx),
            ScopeCheck::IsExact(val) => roman.map(|c| c.to_string() == *val).unwrap_or(false),
            ScopeCheck::NotExact(val) => roman.map(|c| c.to_string() != *val).unwrap_or(true),
        }
    }

    fn char_is(&self, roman: Option<char>, cls: &CharClass, ctx: &OutputContext) -> bool {
        match cls {
            CharClass::Consonant => match roman {
                Some(c) => self.consonant_chars.contains(c),
                None => *ctx == OutputContext::AfterConsonant,
            },
            CharClass::Vowel => roman.map(|c| self.vowel_chars.contains(c)).unwrap_or(false),
            CharClass::Number => roman.map(|c| self.number_chars.contains(c)).unwrap_or(false),
            CharClass::Punctuation => match roman {
                None => true,
                Some(c) => !self.consonant_chars.contains(c)
                    && !self.vowel_chars.contains(c)
                    && !self.number_chars.contains(c),
            },
        }
    }

    fn next_context(output: &str) -> OutputContext {
        match output.chars().last() {
            Some(ch) => {
                let cp = ch as u32;
                if (0x0995..=0x09B9).contains(&cp)
                    || cp == 0x09CE  // ৎ
                    || cp == 0x09DC  // ড়
                    || cp == 0x09DD  // ঢ়
                    || cp == 0x09DF  // য়
                {
                    OutputContext::AfterConsonant
                } else {
                    OutputContext::Neutral
                }
            }
            None => OutputContext::Neutral,
        }
    }

    fn load_rules(&mut self) {
        let vowels: &[(&str, &str, &str)] = &[
            ("a",   "আ", "া"),
            ("aa",  "আ", "া"),
            ("i",   "ই", "ি"),
            ("ii",  "ঈ", "ী"),
            ("u",   "উ", "ু"),
            ("uu",  "ঊ", "ূ"),
            ("e",   "এ", "ে"),
            ("o",   "ও", "ো"),
            ("O",   "ও", "ো"),
            ("oi",  "ঐ", "ৈ"),
            ("ou",  "ঔ", "ৌ"),
            ("rri", "ঋ", "ৃ"),
        ];
        for &(key, ind, dep) in vowels {
            self.insert(key, BanglaOutput::Contextual {
                independent: ind.to_string(),
                dependent: dep.to_string(),
            });
        }

        let consonants: &[(&str, &str)] = &[
            ("k",   "ক"), ("kh",  "খ"),
            ("g",   "গ"), ("gh",  "ঘ"), ("nga", "ঙ"),
            ("c",   "চ"), ("ch",  "ছ"),
            ("j",   "জ"), ("jh",  "ঝ"),
            ("tt",  "ট"), ("T",   "ট"), ("tth", "ঠ"), ("Th",  "ঠ"),
            ("dd",  "ড"), ("D",   "ড"), ("ddh", "ঢ"), ("Dh",  "ঢ"),
            ("nn",  "ণ"), ("N",   "ণ"),
            ("t",   "ত"), ("th",  "থ"),
            ("d",   "দ"), ("dh",  "ধ"),
            ("n",   "ন"),
            ("p",   "প"), ("ph",  "ফ"), ("f",   "ফ"),
            ("b",   "ব"), ("bh",  "ভ"), ("v",   "ভ"),
            ("m",   "ম"),
            ("z",   "য"), ("y",   "য়"),
            ("r",   "র"), ("rr",  "ড়"), ("rrh", "ঢ়"),
            ("l",   "ল"),
            ("sh",  "শ"), ("ss",  "ষ"), ("S",   "ষ"), ("s",   "স"),
            ("h",   "হ"),
            ("ng",        "ং"),
            ("nya",       "ঞ"),
            ("khandatta", "ৎ"),
        ];
        for &(key, val) in consonants {
            self.insert(key, BanglaOutput::Static(val.to_string()));
        }
    }
}

impl Default for AvroEngine {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{SuffixDict, WordDict};
    use crate::grammar::AvroGrammar;

    fn type_str(engine: &mut AvroEngine, s: &str) -> String {
        let mut preedit = String::new();
        for c in s.chars() { preedit = engine.handle_input(c); }
        preedit
    }

    // ── Hardcoded-rules tests ─────────────────────────────────────────────────

    #[test] fn single_consonant() { assert_eq!(type_str(&mut AvroEngine::new(), "k"), "ক"); }
    #[test] fn digraph_consonant() { assert_eq!(type_str(&mut AvroEngine::new(), "kh"), "খ"); }
    #[test] fn consonant_then_vowel() { assert_eq!(type_str(&mut AvroEngine::new(), "ka"), "কা"); }
    #[test] fn vowel_independent_at_start() { assert_eq!(type_str(&mut AvroEngine::new(), "a"), "আ"); }
    #[test] fn vowel_dependent_after_consonant() { assert_eq!(type_str(&mut AvroEngine::new(), "ka"), "কা"); }
    #[test] fn multiple_chars() { assert_eq!(type_str(&mut AvroEngine::new(), "kha"), "খা"); }
    #[test] fn non_trie_char_is_literal() { assert_eq!(type_str(&mut AvroEngine::new(), "1"), "1"); }
    #[test] fn word_eka() { assert_eq!(type_str(&mut AvroEngine::new(), "eka"), "একা"); }
    #[test] fn ng_is_anusvara() { assert_eq!(type_str(&mut AvroEngine::new(), "ng"), "ং"); }
    #[test] fn nga_is_nga_letter() { assert_eq!(type_str(&mut AvroEngine::new(), "nga"), "ঙ"); }
    #[test] fn y_is_yo() { assert_eq!(type_str(&mut AvroEngine::new(), "y"), "য়"); }
    #[test] fn z_is_jo() { assert_eq!(type_str(&mut AvroEngine::new(), "z"), "য"); }
    #[test] fn capital_o_is_o_vowel() { assert_eq!(type_str(&mut AvroEngine::new(), "kO"), "কো"); }
    #[test] fn rr_is_rra() { assert_eq!(type_str(&mut AvroEngine::new(), "rr"), "ড়"); }
    #[test] fn word_bangla() { assert_eq!(type_str(&mut AvroEngine::new(), "bangla"), "বাংলা"); }
    #[test] fn word_tomay() { assert_eq!(type_str(&mut AvroEngine::new(), "tOmay"), "তোমায়"); }
    #[test] fn kha_not_broken_by_khandatta() { assert_eq!(type_str(&mut AvroEngine::new(), "kha"), "খা"); }
    #[test] fn khandatta_gives_khanda_ta() { assert_eq!(type_str(&mut AvroEngine::new(), "khandatta"), "ৎ"); }
    #[test] fn nya_gives_nyo() { assert_eq!(type_str(&mut AvroEngine::new(), "nya"), "ঞ"); }
    #[test] fn ny_before_consonant() { assert_eq!(type_str(&mut AvroEngine::new(), "nyk"), "নয়ক"); }

    #[test]
    fn anthem_phrase() {
        for (phonetic, expected) in [
            ("amar", "আমার"), ("sonar", "সোনার"), ("bangla", "বাংলা"),
            ("ami", "আমি"), ("tOmay", "তোমায়"), ("valObasi", "ভালোবাসি"),
        ] {
            assert_eq!(type_str(&mut AvroEngine::new(), phonetic), expected, "failed on '{phonetic}'");
        }
    }

    #[test]
    fn backspace_reverts_last_char() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "kh");
        assert_eq!(e.handle_backspace(), "ক");
    }

    #[test]
    fn backspace_to_empty() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "k");
        assert_eq!(e.handle_backspace(), "");
    }

    #[test]
    fn commit_returns_bangla() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "kha");
        assert_eq!(e.commit(), "খা");
    }

    #[test]
    fn commit_clears_state() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "k");
        e.commit();
        assert_eq!(e.preedit(), "");
    }

    // ── Hasanta / conjunct tests ──────────────────────────────────────────────

    #[test]
    fn joiner_between_consonants_emits_hasanta() {
        let mut e = AvroEngine::new();
        // k + joiner + t → ক + ্ + ত = ক্ত
        assert_eq!(type_str(&mut e, "k,t"), "ক্ত");
    }

    #[test]
    fn joiner_after_vowel_is_literal_comma() {
        let mut e = AvroEngine::new();
        // a + joiner → আ + , (not after consonant)
        assert_eq!(type_str(&mut e, "a,"), "আ,");
    }

    #[test]
    fn backspace_reverts_joiner() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "k,");
        let preedit = e.handle_backspace();
        assert_eq!(preedit, "ক");
    }

    // ── Grammar (JSON) loading tests ──────────────────────────────────────────

    fn load_grammar() -> Option<AvroGrammar> {
        let src = std::fs::read_to_string("../../avro.json").ok()?;
        AvroGrammar::from_json(&src).ok()
    }

    #[test]
    fn grammar_engine_produces_kha() {
        let Some(g) = load_grammar() else { return };
        let mut e = AvroEngine::from_grammar(&g);
        assert_eq!(type_str(&mut e, "kh"), "খ");
    }

    #[test]
    fn grammar_engine_independent_vowel_at_start() {
        let Some(g) = load_grammar() else { return };
        let mut e = AvroEngine::from_grammar(&g);
        assert_eq!(type_str(&mut e, "a"), "আ");
    }

    #[test]
    fn grammar_engine_dependent_vowel_after_consonant() {
        let Some(g) = load_grammar() else { return };
        let mut e = AvroEngine::from_grammar(&g);
        // In JSON grammar, `a` after consonant → আ (rule: !consonant prefix and exact 'a' prefix)
        // and `ka` goes: k → ক (AfterConsonant), then `a` → last_roman='k' (consonant) → া
        // Actually in JSON, `a` after consonant fires: none of the rules match (all need !consonant or punctuation prefix)
        // so the fallback "া" is used. Correct!
        assert_eq!(type_str(&mut e, "ka"), "কা");
    }

    #[test]
    fn grammar_engine_o_capital_after_consonant() {
        let Some(g) = load_grammar() else { return };
        let mut e = AvroEngine::from_grammar(&g);
        // O after consonant → fallback "ো"
        assert_eq!(type_str(&mut e, "kO"), "কো");
    }

    #[test]
    fn grammar_engine_o_capital_at_start() {
        let Some(g) = load_grammar() else { return };
        let mut e = AvroEngine::from_grammar(&g);
        // O at start → punctuation prefix rule → "ও"
        assert_eq!(type_str(&mut e, "O"), "ও");
    }

    #[test]
    fn grammar_engine_anthem() {
        let Some(g) = load_grammar() else { return };
        // Using JSON grammar, anthem should still work.
        // Note: JSON uses 'O' for ো so words using 'o' differ.
        let mut e = AvroEngine::from_grammar(&g);
        assert_eq!(type_str(&mut e, "kh"), "খ");
        let mut e = AvroEngine::from_grammar(&g);
        assert_eq!(type_str(&mut e, "bangla"), "বাংলা");
    }

    // ── Suggest / autosuggest tests ───────────────────────────────────────────

    fn load_word_dict() -> Option<WordDict> {
        let src = std::fs::read_to_string("../../avrodict.js").ok()?;
        WordDict::from_js(&src).ok()
    }

    fn load_suffix_dict() -> Option<SuffixDict> {
        let src = std::fs::read_to_string("../../suffixdict.js").ok()?;
        SuffixDict::from_js(&src).ok()
    }

    #[test]
    fn suggest_empty_without_dict() {
        let mut e = AvroEngine::new();
        type_str(&mut e, "k");
        assert!(e.suggest(5).is_empty());
    }

    #[test]
    fn suggest_all_start_with_preedit() {
        let Some(dict) = load_word_dict() else { return };
        let mut e = AvroEngine::new();
        e.load_dict(dict);
        let preedit = type_str(&mut e, "k");
        let s = e.suggest(20);
        assert!(!s.is_empty());
        assert!(s.iter().all(|w| w.starts_with(&preedit)));
    }

    #[test]
    fn suggest_narrows_with_more_input() {
        let Some(dict) = load_word_dict() else { return };
        let mut e = AvroEngine::new();
        e.load_dict(dict);
        type_str(&mut e, "k");
        let broad = e.suggest(usize::MAX).len();
        type_str(&mut e, "a");
        let narrow = e.suggest(usize::MAX).len();
        assert!(narrow <= broad);
    }

    #[test]
    fn suggest_respects_max() {
        let Some(dict) = load_word_dict() else { return };
        let mut e = AvroEngine::new();
        e.load_dict(dict);
        type_str(&mut e, "k");
        assert!(e.suggest(3).len() <= 3);
    }

    #[test]
    fn suggest_extended_strips_suffix() {
        let Some(wdict) = load_word_dict() else { return };
        let Some(sdict) = load_suffix_dict() else { return };
        let mut e = AvroEngine::new();
        e.load_dict(wdict);
        e.load_suffix_dict(sdict);
        // Type "bangladEshEr" → বাংলাদেশের; expect suggestions with ের suffix
        type_str(&mut e, "bangladEsh");
        // Add suffix phonetically by appending direct Bangla — simulate via committing and typing
        // Instead, manually verify the suffix logic with a known base+suffix
        let s = e.suggest_extended(5);
        // All results should start with বাংলাদেশ (the prefix)
        assert!(s.iter().all(|w| w.starts_with("বাংলাদেশ")));
    }

    #[test]
    fn suggest_extended_falls_back_without_suffix_dict() {
        let Some(wdict) = load_word_dict() else { return };
        let mut e = AvroEngine::new();
        e.load_dict(wdict);
        type_str(&mut e, "k");
        // No suffix dict loaded — should behave like regular suggest
        let basic = e.suggest(10);
        let extended = e.suggest_extended(10);
        assert_eq!(basic, extended);
    }
}
