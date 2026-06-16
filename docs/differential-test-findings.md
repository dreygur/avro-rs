# Differential test: Rust engine vs. independent JS implementation

**Status:** investigated, not yet acted on. Saved here for follow-up.

## What was done

Ran a large-scale differential test comparing `avro-core`'s transliteration output
against an independent reference implementation, to sanity-check the Rust port's
correctness beyond the existing unit test fixtures.

- **Oracle:** [`torifat/jsAvroPhonetic`](https://github.com/torifat/jsAvroPhonetic)
  (`OmicronLab.Avro.Phonetic.parse()`), confirmed via SHA-256 hash match to be
  byte-identical to the source published as the `avro-phonetic` npm package. Its
  internal ruleset was overridden at runtime with this repo's own `avro.json`, so
  the comparison is same-data / two-independent-implementations, not confounded by
  a different grammar snapshot.
- **Corpus:** 131,156 phonetic Latin input strings — all 289 single pattern keys
  from `avro.json`, ~81k ordered pairs of keys, ~50k random ordered triples, plus
  118 hand-picked realistic Bangla phonetic words (e.g. "bangla", "tomake",
  "valobasha").
- **Rust side:** each input fed character-by-character into a fresh `AvroEngine`
  via `handle_input`, reading `preedit()` at the end (no `commit()`). Ran both
  `AvroEngine::from_grammar(&avro_json)` (the JSON-driven path) and
  `AvroEngine::new()` (the hardcoded fallback path) for every input.

## Results

| Comparison | Total | Exact match | Mismatches |
|---|---|---|---|
| Rust (JSON-grammar path) vs JS oracle | 131,156 | 95.48% | 5,927 |
| Rust (hardcoded fallback path) vs JS oracle | 131,156 | 1.78% | 128,825 |
| Rust grammar path vs Rust hardcoded path | 131,156 | 1.75% | 128,863 |
| — same as above, **118 curated realistic words only** | 118 | **100%** (grammar path) | 0 |

The hardcoded-fallback path's low match rate is expected, not a bug: it's a
deliberately minimal ~40-mapping fallback (see `CLAUDE.md`), never intended to be
JSON-grammar-complete. All mismatches there trace to missing digit/punctuation/
digraph mappings that simply aren't in the hardcoded set (e.g. `0`/`$` pass
through literally instead of becoming `০`/`৳`).

The JSON-grammar path's 5,927 mismatches are **not random** — they cluster
entirely around synthetic punctuation/digit combinations from the fuzz corpus
(2-key and 3-key combos involving `,`, `.`, `:`, `$`, digits), and **zero** of
them occur in the 118 realistic-word inputs. They trace to three distinct,
nameable causes:

### 1. Punctuation-class disagreement on digits

Rust's `CharClass::Punctuation` (in `avro-core/src/engine.rs`) explicitly excludes
characters in `number_chars`. The JS oracle's punctuation check does not exclude
digits. Several `avro.json` rules gate on a `prefix: punctuation` condition (e.g.
the `x` → `এক্স` rule), so any input where a digit precedes one of these patterns
diverges between the two engines.

### 2. JS oracle's `"number"` suffix-scope is a silent no-op

The JS oracle's rule-matching loop only implements
`scope ∈ {punctuation, vowel, consonant, exact}` — an unrecognized scope like
`"number"` is never evaluated and the condition defaults to "satisfied." Both
`avro.json`'s `.` and `:` patterns use a `suffix: number` condition to decide
between literal punctuation and Bangla output (`।`/`ঃ`). Because the JS oracle
never actually checks this condition, it **always** renders `.`/`:` as literal
punctuation, regardless of context — including at end-of-string. Rust correctly
implements the number-suffix check. This looks like a genuine gap in the
reference JS implementation, not a Rust bug — worth keeping in mind if/when
deciding whether to "fix" anything here.

### 3. Comma handling is architecturally different, not a JSON-interpretation bug

`avro.json` defines both a `,` → `,` pattern and a separate `,,` → হসন্ত+ZWNJ
pattern. Rust's `AvroEngine::handle_input` special-cases the `,` character
entirely as a live keystroke-level হসন্ত-joiner for conjunct formation (see
`CLAUDE.md`), bypassing the JSON trie for commas completely — so `avro.json`'s
own `,,` pattern is unreachable from that code path. The JS oracle is a pure
one-shot string-rewrite with no keystroke model, so it matches `,,` against the
JSON pattern list normally. Example: `k,,` → JS `ক্‌` (single হসন্ত+ZWNJ via the
`,,` pattern) vs. Rust `ক্্` (two literal হসন্ত pushes via the hardcoded joiner
branch).

## Possible follow-ups (not yet decided/actioned)

- Decide whether Rust's punctuation class should include digits to match the
  documented Avro Phonetic spec's apparent intent (cause #1), or whether the
  current behavior is actually more correct and the JS oracle has the gap.
- Decide whether the comma/হসন্ত-joiner special case (cause #3) should also
  produce the same output as `avro.json`'s own `,,` pattern, for spec fidelity,
  or whether the current editor-level behavior is intentional/preferred.
- Cause #2 likely needs no Rust-side change — it looks like the reference
  implementation has the gap, not this port.

## Reproducing this test

The corpus generation script, JS oracle setup, and Rust batch-runner used for
this investigation were built in a scratch directory (`/tmp/avro-diff/`) during
the investigation session and were not preserved or committed — they're not
large undertakings to rebuild from the methodology described above if needed
again (clone `torifat/jsAvroPhonetic`, override its `Phonetic.data` with this
repo's `avro.json`, generate the corpus from `avro.json`'s pattern keys, and feed
both engines the same input list).
