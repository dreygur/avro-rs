use std::io::{self, Write};

use avro_core::{dict::{SuffixDict, WordDict}, AvroEngine, AvroGrammar};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, ClearType},
};

fn render(out: &mut io::Stdout, committed: &str, preedit: &str, suggestions: &[String]) -> io::Result<u16> {
    execute!(out, cursor::MoveToColumn(0), terminal::Clear(ClearType::FromCursorDown))?;
    write!(out, "> {committed}")?;
    if !preedit.is_empty() {
        write!(out, "\x1b[4m{preedit}\x1b[0m")?;
    }
    if !suggestions.is_empty() {
        let hint = suggestions.iter().take(5).map(String::as_str).collect::<Vec<_>>().join("  ");
        write!(out, "\r\n  \x1b[2m{hint}\x1b[0m")?;
        out.flush()?;
        return Ok(1);
    }
    out.flush()?;
    Ok(0)
}

fn main() -> io::Result<()> {
    // Try to load JSON grammar first; fall back to hardcoded rules.
    let mut engine = if let Ok(src) = std::fs::read_to_string("avro.json") {
        if let Ok(grammar) = AvroGrammar::from_json(&src) {
            println!("grammar: JSON ({} patterns)", grammar.layout.patterns.len());
            AvroEngine::from_grammar(&grammar)
        } else {
            println!("grammar: hardcoded (JSON parse failed)");
            AvroEngine::new()
        }
    } else {
        println!("grammar: hardcoded (avro.json not found)");
        AvroEngine::new()
    };

    if let Ok(src) = std::fs::read_to_string("avrodict.js") {
        if let Ok(dict) = WordDict::from_js(&src) {
            let n = dict.total_words();
            engine.load_dict(dict);
            print!("dict: {n} words");
        }
    }
    if let Ok(src) = std::fs::read_to_string("suffixdict.js") {
        if let Ok(dict) = SuffixDict::from_js(&src) {
            let n = dict.len();
            engine.load_suffix_dict(dict);
            println!("  suffixes: {n}");
        } else {
            println!();
        }
    } else {
        println!();
    }

    println!("Space=commit word  Enter=commit line  ,=conjunct  Ctrl+C=quit\n");

    let mut out = io::stdout();
    terminal::enable_raw_mode()?;

    let mut committed = String::new();
    let mut prev = render(&mut out, "", "", &[])?;

    loop {
        let Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) = event::read()? else {
            continue;
        };
        match (code, modifiers) {
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => break,
            (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => break,
            (KeyCode::Backspace, _) => {
                let p = engine.handle_backspace();
                let s = engine.suggest_extended(5);
                if prev > 0 { execute!(out, cursor::MoveUp(prev))?; }
                prev = render(&mut out, &committed, &p, &s)?;
            }
            (KeyCode::Char(' '), _) => {
                let word = engine.commit();
                if !word.is_empty() { committed.push_str(&word); committed.push(' '); }
                if prev > 0 { execute!(out, cursor::MoveUp(prev))?; }
                prev = render(&mut out, &committed, "", &[])?;
            }
            (KeyCode::Enter, _) => {
                let word = engine.commit();
                committed.push_str(&word);
                if prev > 0 { execute!(out, cursor::MoveUp(prev))?; }
                execute!(out, cursor::MoveToColumn(0), terminal::Clear(ClearType::FromCursorDown))?;
                write!(out, "{committed}\r\n")?;
                out.flush()?;
                committed.clear();
                prev = render(&mut out, "", "", &[])?;
            }
            (KeyCode::Char(c), _) => {
                let p = engine.handle_input(c);
                let s = engine.suggest_extended(5);
                if prev > 0 { execute!(out, cursor::MoveUp(prev))?; }
                prev = render(&mut out, &committed, &p, &s)?;
            }
            _ => {}
        }
    }

    if prev > 0 { execute!(out, cursor::MoveUp(prev))?; }
    execute!(out, cursor::MoveToColumn(0), terminal::Clear(ClearType::FromCursorDown))?;
    terminal::disable_raw_mode()?;
    writeln!(out)?;
    out.flush()
}
