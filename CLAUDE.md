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

**Target platforms (priority order):**
1. Fedora 44 / GNOME / Wayland via Fcitx5
2. Windows 11 via Text Services Framework (TSF)
3. Web via WebAssembly

**Tech stack:** Rust core + C-FFI / WASM for interop. No unsafe raw pointers in core logic.

## Architecture

### Crate structure

```
avro-engine/   ← standalone core library (no OS deps)
```

Future: `fcitx5-adapter/`, `tsf-adapter/`, `wasm-adapter/` as separate crates.

### Core data flow

Keystroke (`char`) → `AvroEngine::handle_input()` → preedit string (updated in-place)
Space/confirm → `AvroEngine::commit()` → final Bangla Unicode string

### Key types (`src/types.rs`)

- **`BanglaOutput`** — two variants:
  - `Static(String)`: consonants and fixed mappings (e.g. `k` → `ক`)
  - `Contextual { independent, dependent }`: vowels that render differently — `independent` after word boundary or vowel (e.g. `আ`), `dependent` after consonant (e.g. `া`)
- **`OutputContext`** — `Neutral` (start/after vowel) vs `AfterConsonant`
- **`TrieNode`** — internal; holds `Option<BanglaOutput>` and `HashMap<char, TrieNode>` children

### Engine (`src/engine.rs`)

`AvroEngine` fields:
- `root: TrieNode` — the phonetic rule trie (built via `insert(key, value)`)
- `state_stack: Vec<EngineState>` — snapshot before each keystroke; `handle_backspace()` pops O(1)
- `prefix: String` — Latin keystrokes in flight (e.g. `"kh"` while deciding খ vs ক+হ)
- `word_buffer: String` — resolved Bangla for chars before current prefix (full preedit = `word_buffer + resolve(prefix)`)
- `context: OutputContext` — tracks whether last committed output was consonant or vowel

**Greedy trie traversal:** on each `handle_input(c)`, try to extend `prefix+c` in trie. If the node exists, stay in trie. If not, flush `prefix` output into `word_buffer`, then start fresh with `c`.

**Context detection:** after flushing, inspect the last Unicode char of the output — Bangla consonant range (U+0995–U+09B9 + ড়/ঢ়/য়/ৎ) → `AfterConsonant`; vowel ranges → `Neutral`. This drives dependent vs independent vowel selection.

### Dictionary (`avrodict.js`)

7 MB JS file (`var tables = {...}`) — Bangla word lists keyed by phonetic prefix. Reference dataset for Phase 3 autosuggest; needs a Rust parser to load.

## Roadmap

| Phase | Goal | Status |
|-------|------|--------|
| 1 | Engine foundation: trie + stateful traversal + backspace | In progress |
| 2 | JSON rule parser (`serde_json`) → trie insertions | Pending |
| 3 | Autosuggest: DFS subtree traversal + frequency weighting | Pending |
| 4 | Fcitx5 FFI bridge + deployment to `/usr/lib64/fcitx5/` | Pending |

## Phonetic accuracy notes

- Rules must follow **Avro Phonetic Standard** (compatible with OpenBangla/Riti JSON grammar).
- Backspace must revert Bangla output state, not just the Latin buffer — use `state_stack`.
- Vowel form selection (`dependent` vs `independent`) is driven by `OutputContext`, not caller flags.
