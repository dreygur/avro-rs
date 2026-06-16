# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Coding Rules

- **Minimum code:** Least code that correctly solves the problem. No extra abstraction, no speculative generality, no padding.
- **DRY:** Never write the same logic twice. Extract shared logic into functions, constants, or type aliases immediately — don't wait for a third occurrence.
- **Reuse first:** Before writing anything new, look for an existing function, constant, or component that already does it. Prefer extending what exists over adding new things.
- **No dead code:** Remove unused functions, fields, imports, and variables. Don't leave things "just in case."

## Commands

```bash
# Build
cargo build
cargo build --release

# Test
cargo test
cargo test -p avro-engine              # single crate
cargo test <test_name>                  # single test

# Check / lint
cargo check
cargo clippy -- -D warnings
cargo fmt
```

## Project Vision (Avro-Next)

High-performance, memory-safe, cross-platform rewrite of Avro Phonetic. Unified Rust core with OS adapters.

**This is a port of [mugli/Avro-Keyboard](https://github.com/mugli/Avro-Keyboard) (the OmicronLab Avro Phonetic engine) to Rust.** The goal is a drop-in replacement: identical transliteration behavior, byte-for-byte compatible with the same data files. `avro.json`, `avrodict.js`, and `suffixdict.js` at the repo root are vendored from upstream — treat them as fixtures, not source to hand-edit. If Rust output diverges from upstream Avro Phonetic for some input, the bug is in the Rust port (`grammar.rs`/`engine.rs`), not the data files.

**Target platforms (priority order):**
1. Fedora 44 / GNOME / Wayland via Fcitx5
2. Windows 11 via Text Services Framework (TSF)
3. Web via WebAssembly

**Tech stack:** Rust core + C-FFI / WASM for interop. No unsafe raw pointers in core logic (FFI boundary code in `fcitx5-adapter` is the necessary exception).

## Architecture

### Crate structure (Cargo workspace)

```
crates/avro-core/        ← standalone engine library (no OS deps): trie, grammar parser, dict loaders
crates/avro-repl/        ← terminal REPL for manual testing (crossterm)
crates/fcitx5-adapter/   ← cdylib: C FFI (src/lib.rs) + C++ shim (src/shim.cpp) bridging libfcitx5
crates/wasm-adapter/     ← cdylib+rlib: wasm-bindgen wrapper (src/lib.rs) around AvroEngine for web hosts
```

Future: `tsf-adapter/` as a separate crate.

### Core data flow

Keystroke (`char`) → `AvroEngine::handle_input()` → preedit string (updated in-place)
Space/confirm → `AvroEngine::commit()` → final Bangla Unicode string
Each keystroke also refreshes autosuggest candidates via `suggest_extended()`.

### Key types (`avro-core/src/types.rs`)

- **`BanglaOutput`** — three variants:
  - `Static(String)`: consonants and fixed mappings (e.g. `k` → `ক`)
  - `Contextual { independent, dependent }`: vowels that render differently — `independent` after word boundary or vowel (e.g. `আ`), `dependent` after consonant (e.g. `া`)
  - `Conditional { rules: Vec<ConditionalRule>, fallback: String }`: patterns loaded from the JSON grammar whose output depends on surrounding prefix/suffix scope (consonant/vowel/number/punctuation/exact-char checks); first matching rule wins, else `fallback`
- **`OutputContext`** — `Neutral` (start/after vowel) vs `AfterConsonant`
- **`TrieNode`** — internal; holds `Option<BanglaOutput>` and `HashMap<char, TrieNode>` children

### Grammar parser (`avro-core/src/grammar.rs`)

`AvroGrammar::from_json` deserializes `avro.json` (OmicronLab's Avro Phonetic JSON grammar — same format Riti/OpenBangla consume) into `GrammarLayout { vowel, consonant, number, casesensitive, patterns }`. `patterns_as_outputs()` converts each `GrammarPattern` into a trie-ready `(find_key, BanglaOutput)` pair — simple patterns become `Static`, patterns with `rules` become `Conditional`. `AvroEngine::from_grammar()` builds an engine entirely from this JSON rather than the hardcoded fallback rules in `engine.rs::load_rules()`. `AvroEngine::from_sources(grammar_json, dict_js, suffix_js)` composes `from_grammar()`/`load_dict()`/`load_suffix_dict()` from optional source strings in one call — the shared entry point both `fcitx5-adapter` and `wasm-adapter` use so the "parse sources → build engine" logic isn't duplicated per adapter.

### Dictionaries (`avro-core/src/dict.rs`)

- **`WordDict`** — parses `avrodict.js` (`var tables = {...}`, ~7 MB, word lists keyed by phonetic prefix e.g. `"w_kh"`) by extracting the embedded JSON object and deserializing with `serde_json`.
- **`SuffixDict`** — parses `suffixdict.js` the same way; maps phonetic suffix → Bangla string (e.g. `"er"` → `"ের"`).
- Loaded into the engine via `AvroEngine::load_dict` / `load_suffix_dict`, which builds a `bangla_to_key` index and a longest-first sorted suffix list for fast lookup.

### Engine (`avro-core/src/engine.rs`)

`AvroEngine` fields:
- `root: TrieNode` — the phonetic rule trie (built via `insert(key, value)`, from either `load_rules()` hardcoded fallback or `from_grammar()`)
- `state_stack: Vec<EngineState>` — snapshot before each keystroke; `handle_backspace()` pops O(1)
- `prefix: String` — Latin keystrokes in flight (e.g. `"kh"` while deciding খ vs ক+হ)
- `word_buffer: String` — resolved Bangla for chars before current prefix (full preedit = `word_buffer + resolve(prefix)`)
- `context: OutputContext`, `last_roman: Option<char>` — feed `Conditional` rule scope checks (prefix/suffix conditions look at the romanized char just before/after the match)
- `dict`, `bangla_to_key`, `suffix_bangla_sorted` — autosuggest state

**Greedy trie traversal:** on each `handle_input(c)`, try to extend `prefix+c` in trie. If the node exists, stay in trie. If not, flush `prefix` output into `word_buffer`, then start fresh with `c`. The `,` joiner is special-cased to emit হসন্ত (U+09CD) between consonants for conjunct formation.

**Context detection:** after flushing, inspect the last Unicode char of the output — Bangla consonant range (U+0995–U+09B9 + ৎ/ড়/ঢ়/য়) → `AfterConsonant`; vowel ranges → `Neutral`. This drives dependent vs independent vowel selection and `Conditional` rule scope checks.

**Autosuggest:** `suggest()` does a prefix lookup in `WordDict` by first-Bangla-char index. `suggest_extended()` additionally tries stripping a known Bangla suffix from the preedit, looking up the base word, and reattaching the suffix — falls back to `suggest()` if no suffix matches.

### fcitx5-adapter (`crates/fcitx5-adapter`)

C FFI (`src/lib.rs`, `avro_state_new`/`avro_handle_input`/`avro_commit`/... exported `extern "C"`) wrapped by a C++ shim (`src/shim.cpp`) implementing `fcitx::InputMethodEngine`. `build.rs` uses `pkg-config` to locate `Fcitx5Core`/`Fcitx5Utils` and compiles the shim with `cc` (C++20). Installed paths and addon metadata are driven by the `Makefile` (`PKGDATADIR`, `dist/addon/AvroPhonetic.conf`, `dist/inputmethod/avro.conf`) — see `make build` / `sudo make install`.

### wasm-adapter (`crates/wasm-adapter`)

`wasm-bindgen`-wrapped `AvroState` (`src/lib.rs`) exposing `new`/`handle_input`/`handle_backspace`/`commit`/`commit_suggestion`/`has_preedit`/`preedit`/`suggestions` — the same logical surface as `fcitx5-adapter`'s C FFI, minus manual string marshaling. The constructor takes the grammar/dict/suffix JSON/JS as plain strings (host page fetches `avro.json`/`avrodict.js`/`suffixdict.js` and passes them in) and delegates to `AvroEngine::from_sources`. Builds for `wasm32-unknown-unknown`; no JS glue package (`wasm-pack`/`wasm-bindgen-cli` output) generated yet.

## Roadmap

| Phase | Goal | Status |
|-------|------|--------|
| 1 | Engine foundation: trie + stateful traversal + backspace | Done |
| 2 | JSON rule parser (`serde_json`) → trie insertions | Done |
| 3 | Autosuggest: dict/suffix-dict lookup + suffix stripping | Done |
| 4 | Fcitx5 FFI bridge + deployment to `/usr/lib64/fcitx5/` | Done |
| 5 | TSF adapter (Windows) | Pending |
| 6 | WASM adapter (Web) | In progress |

## Phonetic accuracy notes

- Rules must follow **Avro Phonetic Standard** (compatible with OpenBangla/Riti JSON grammar) — match upstream [mugli/Avro-Keyboard](https://github.com/mugli/Avro-Keyboard) behavior exactly for drop-in compatibility.
- Backspace must revert Bangla output state, not just the Latin buffer — use `state_stack`.
- Vowel form selection (`dependent` vs `independent`) is driven by `OutputContext`, not caller flags.
- When fixing a transliteration mismatch, write a failing test against `avro.json`-driven (`from_grammar`) behavior first — both the hardcoded `load_rules()` path and the JSON path are tested in `engine.rs`, and they should agree.
- `dist/addon/AvroPhonetic.conf` must set `Type=SharedLibrary` (fcitx5 silently skips the addon with no error if missing) and `Library=` must include the literal `lib` prefix (e.g. `libfcitx5-adapter`, not `fcitx5-adapter`) — fcitx5's addon loader does not auto-prepend it.
