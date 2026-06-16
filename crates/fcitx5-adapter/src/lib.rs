use avro_core::AvroEngine;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

pub struct AvroState {
    engine: AvroEngine,
    suggestions: Vec<String>,
}

impl AvroState {
    fn refresh_suggestions(&mut self) {
        self.suggestions = self.engine.suggest_extended(5);
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn avro_state_new(
    grammar_path: *const c_char,
    dict_path: *const c_char,
    suffix_path: *const c_char,
) -> *mut AvroState {
    let engine = load_engine(grammar_path, dict_path, suffix_path);
    Box::into_raw(Box::new(AvroState {
        engine,
        suggestions: Vec::new(),
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn avro_state_free(state: *mut AvroState) {
    if !state.is_null() {
        unsafe {
            drop(Box::from_raw(state));
        }
    }
}

// ── Input handling ───────────────────────────────────────────────────────────

/// Returns a NUL-terminated preedit string. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_handle_input(state: *mut AvroState, ch: u32) -> *mut c_char {
    let Some(c) = char::from_u32(ch) else {
        return std::ptr::null_mut();
    };
    let state = unsafe { &mut *state };
    state.engine.handle_input(c);
    state.refresh_suggestions();
    to_cstring(state.engine.preedit())
}

/// Returns the preedit after backspace. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_handle_backspace(state: *mut AvroState) -> *mut c_char {
    let state = unsafe { &mut *state };
    state.engine.handle_backspace();
    state.refresh_suggestions();
    to_cstring(state.engine.preedit())
}

/// Commits the current word and returns the Bangla string. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_commit(state: *mut AvroState) -> *mut c_char {
    let state = unsafe { &mut *state };
    let word = state.engine.commit();
    state.suggestions.clear();
    to_cstring(word)
}

/// Commits the nth suggestion (0-indexed) and returns it. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_commit_suggestion(state: *mut AvroState, index: c_int) -> *mut c_char {
    let state = unsafe { &mut *state };
    let word = state
        .suggestions
        .get(index as usize)
        .cloned()
        .unwrap_or_default();
    state.engine.commit();
    state.suggestions.clear();
    to_cstring(word)
}

// ── Preedit query ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn avro_has_preedit(state: *const AvroState) -> c_int {
    let state = unsafe { &*state };
    if state.engine.preedit().is_empty() {
        0
    } else {
        1
    }
}

/// Returns current preedit string. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_preedit(state: *const AvroState) -> *mut c_char {
    let state = unsafe { &*state };
    to_cstring(state.engine.preedit())
}

// ── Suggestions ──────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn avro_suggest_count(state: *const AvroState) -> c_int {
    let state = unsafe { &*state };
    state.suggestions.len() as c_int
}

/// Returns the nth suggestion string. Caller must free with `avro_str_free`.
#[unsafe(no_mangle)]
pub extern "C" fn avro_suggest_get(state: *const AvroState, index: c_int) -> *mut c_char {
    let state = unsafe { &*state };
    match state.suggestions.get(index as usize) {
        Some(s) => to_cstring(s.clone()),
        None => std::ptr::null_mut(),
    }
}

// ── Memory ───────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn avro_str_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn to_cstring(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn read_source(path: *const c_char) -> Option<String> {
    if path.is_null() {
        return None;
    }
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    std::fs::read_to_string(path.as_ref()).ok()
}

fn load_engine(
    grammar_path: *const c_char,
    dict_path: *const c_char,
    suffix_path: *const c_char,
) -> AvroEngine {
    let grammar_src = read_source(grammar_path);
    let dict_src = read_source(dict_path);
    let suffix_src = read_source(suffix_path);
    AvroEngine::from_sources(
        grammar_src.as_deref(),
        dict_src.as_deref(),
        suffix_src.as_deref(),
    )
}

// fcitx5's SharedLibraryLoader looks up this exact symbol name via dlsym.
// Re-exported from Rust (rather than directly from shim.cpp) because rustc's
// cdylib export-list generation only picks up #[no_mangle] Rust items —
// a plain extern "C" symbol from the statically-linked C++ object gets
// dropped from the dynamic symbol table since nothing in Rust references it.
unsafe extern "C" {
    fn avro_fcitx_addon_factory_impl() -> *mut std::ffi::c_void;
}

#[unsafe(no_mangle)]
pub extern "C" fn fcitx_addon_factory_instance() -> *mut std::ffi::c_void {
    unsafe { avro_fcitx_addon_factory_impl() }
}
