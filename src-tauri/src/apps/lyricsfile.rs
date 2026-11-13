//! LRCLIB's word-synced lyrics. Records with `hasWordSync` carry a
//! `lyricsfile`: YAML with every line's start and end and every word's start
//! (in ms). Only that small, regular shape is read here, so there is no YAML
//! dependency:
//!
//! ```yaml
//! lines:
//! - text: Kim olduğumu, ne olduğumu
//!   start_ms: 30550
//!   end_ms: 37220
//!   words:
//!   - text: 'Kim '
//!     start_ms: 30550
//! ```
//!
//! Scalars may be plain, 'single-quoted' ('' is a quote) or "double-quoted"
//! (JSON-like escapes). Anything unexpected makes the whole file count as
//! absent, and the line-synced LRC is used instead.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    /// As written, including the space after it ("Kim "); a word split into
    /// syllables has no space between its parts.
    pub text: String,
    pub start_ms: i64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WordLine {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: Option<i64>,
    pub words: Vec<Word>,
}

fn scalar(raw: &str) -> Option<String> {
    let v = raw.trim();
    if let Some(inner) = v.strip_prefix('\'') {
        return Some(inner.strip_suffix('\'')?.replace("''", "'"));
    }
    if v.starts_with('"') {
        return serde_json::from_str::<String>(v).ok();
    }
    Some(v.to_string())
}

fn number(raw: &str) -> Option<i64> {
    scalar(raw)?.trim().parse::<f64>().ok().map(|n| n.round() as i64)
}

/// Lines with word times, or `None` if the file doesn't have the shape above.
pub fn parse(text: &str) -> Option<Vec<WordLine>> {
    let mut lines: Vec<WordLine> = Vec::new();
    let mut in_lines = false;
    // Indent of a line entry's keys, and of a word entry's keys, once seen.
    let mut line_level: Option<usize> = None;
    let mut word_level: Option<usize> = None;
    let mut expect_words = false;

    for raw in text.lines() {
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let mut s = raw.trim_start();
        let item = s.starts_with("- ");
        if item {
            s = &s[2..];
        }
        let level = if item { indent + 2 } else { indent };

        if indent == 0 && !item {
            in_lines = s.trim_end() == "lines:";
            continue;
        }
        if !in_lines {
            continue;
        }
        let (key, value) = s.split_once(':')?;
        let key = key.trim();

        if item {
            match (line_level, word_level) {
                (None, _) => line_level = Some(level),
                (Some(l), None) if expect_words && level > l => word_level = Some(level),
                _ => {}
            }
            if Some(level) == line_level {
                lines.push(WordLine::default());
                expect_words = false;
            } else if Some(level) == word_level {
                lines.last_mut()?.words.push(Word::default());
            } else {
                return None;
            }
        }

        if Some(level) == line_level {
            let line = lines.last_mut()?;
            match key {
                "text" => line.text = scalar(value)?,
                "start_ms" => line.start_ms = number(value)?,
                "end_ms" => line.end_ms = number(value),
                "words" => expect_words = true,
                _ => {}
            }
        } else if Some(level) == word_level {
            let word = lines.last_mut()?.words.last_mut()?;
            match key {
                "text" => word.text = scalar(value)?,
                "start_ms" => word.start_ms = number(value)?,
                _ => {}
            }
        }
    }

    let usable = !lines.is_empty() && lines.iter().any(|l| !l.words.is_empty());
    usable.then_some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "version: '1.0'
metadata:
  title: Dans Et
  duration_ms: 229000
lines:
- text: Kim olduğumu, ne olduğumu
  start_ms: 30550
  end_ms: 37220
  words:
  - text: 'Kim '
    start_ms: 30550
  - text: 'olduğumu, '
    start_ms: 31027
  - text: \"don't \\\"stop\\\"\"
    start_ms: 33751
- text: 'It''s: here'
  start_ms: 37220
  words:
  - text: It's
    start_ms: 37220.4
";

    #[test]
    fn reads_lines_and_words() {
        let lines = parse(FILE).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Kim olduğumu, ne olduğumu");
        assert_eq!((lines[0].start_ms, lines[0].end_ms), (30550, Some(37220)));
        let words: Vec<(&str, i64)> = lines[0].words.iter().map(|w| (w.text.as_str(), w.start_ms)).collect();
        assert_eq!(words, [("Kim ", 30550), ("olduğumu, ", 31027), ("don't \"stop\"", 33751)]);
        assert_eq!(lines[1].text, "It's: here");
        assert_eq!(lines[1].end_ms, None);
        assert_eq!(lines[1].words[0].start_ms, 37220);
    }

    #[test]
    fn other_list_indents_work() {
        let indented = "lines:\n  - text: a\n    start_ms: 1\n    words:\n      - text: a\n        start_ms: 1\n";
        assert_eq!(parse(indented).unwrap()[0].words[0].start_ms, 1);
    }

    #[test]
    fn unexpected_shapes_are_rejected() {
        assert!(parse("").is_none());
        assert!(parse("lines: []\n").is_none());
        assert!(parse("lines:\n- text: a\n  start_ms: 1\n").is_none(), "no words at all");
        assert!(parse("lines:\n- text: 'unterminated\n  start_ms: 1\n  words:\n  - text: a\n    start_ms: 1\n").is_none());
    }
}
