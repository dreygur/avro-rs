use avro_core::AvroEngine;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct AvroState {
    engine: AvroEngine,
    suggestions: Vec<String>,
}

#[wasm_bindgen]
impl AvroState {
    #[wasm_bindgen(constructor)]
    pub fn new(
        grammar_json: Option<String>,
        dict_js: Option<String>,
        suffix_js: Option<String>,
    ) -> AvroState {
        let engine = AvroEngine::from_sources(
            grammar_json.as_deref(),
            dict_js.as_deref(),
            suffix_js.as_deref(),
        );
        AvroState {
            engine,
            suggestions: Vec::new(),
        }
    }

    pub fn handle_input(&mut self, ch: char) -> String {
        self.engine.handle_input(ch);
        self.refresh_suggestions();
        self.engine.preedit().to_string()
    }

    pub fn handle_backspace(&mut self) -> String {
        self.engine.handle_backspace();
        self.refresh_suggestions();
        self.engine.preedit().to_string()
    }

    pub fn commit(&mut self) -> String {
        self.suggestions.clear();
        self.engine.commit()
    }

    pub fn commit_suggestion(&mut self, index: usize) -> String {
        let word = self.suggestions.get(index).cloned().unwrap_or_default();
        self.suggestions.clear();
        self.engine.commit();
        word
    }

    pub fn has_preedit(&self) -> bool {
        !self.engine.preedit().is_empty()
    }

    pub fn preedit(&self) -> String {
        self.engine.preedit().to_string()
    }

    pub fn suggestions(&self) -> Vec<String> {
        self.suggestions.clone()
    }

    pub fn load_dict(&mut self, dict_js: &str) {
        if let Ok(dict) = avro_core::dict::WordDict::from_js(dict_js) {
            self.engine.load_dict(dict);
        }
    }

    pub fn load_suffix_dict(&mut self, suffix_js: &str) {
        if let Ok(dict) = avro_core::dict::SuffixDict::from_js(suffix_js) {
            self.engine.load_suffix_dict(dict);
        }
    }
}

impl AvroState {
    fn refresh_suggestions(&mut self) {
        self.suggestions = self.engine.suggest_extended(5);
    }
}
