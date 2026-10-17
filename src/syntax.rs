//! Syntax highlighting of bodies via syntect, converted to ratatui lines.

use ratatui::prelude::*;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;


/// Dark theme; only its foreground colours are used, so the terminal background shows through
const THEME: &str = "base16-ocean.dark";

/// Languages we highlight, by syntect syntax name
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Json,
    Html,
    Xml,
    JavaScript,
    Css,
}

impl Language {
    /// Larger bodies are shown plain to keep the UI responsive. syntect's JavaScript-family
    /// grammars cost ~3µs per byte (200KB of JS would block for ~0.6s); JSON and XML are
    /// ~10x cheaper
    pub fn max_highlight_bytes(self) -> usize {
        match self {
            Self::Json | Self::Xml => 200 * 1024,
            Self::JavaScript | Self::Css | Self::Html => 32 * 1024,
        }
    }

    pub fn name(self) -> &'static str {
        self.syntax_name()
    }

    fn syntax_name(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Html => "HTML",
            Self::Xml => "XML",
            Self::JavaScript => "JavaScript",
            Self::Css => "CSS",
        }
    }

    /// From a MIME type such as `application/json; charset=utf-8` or `image/svg+xml`
    pub fn from_mime(mime_type: &str) -> Option<Self> {
        let essence = mime_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        Some(match essence.as_str() {
            "application/json" | "text/json" => Self::Json,
            "text/html" | "application/xhtml+xml" => Self::Html,
            "application/xml" | "text/xml" => Self::Xml,
            "text/css" => Self::Css,
            "application/javascript"
            | "text/javascript"
            | "application/x-javascript"
            | "application/ecmascript"
            | "text/ecmascript" => Self::JavaScript,
            other if other.ends_with("+json") => Self::Json,
            other if other.ends_with("+xml") => Self::Xml,
            _ => return None,
        })
    }

    /// Guess from content, for bodies with no useful MIME type (e.g. decoded base64 fields)
    pub fn sniff(text: &str) -> Option<Self> {
        let start = text.trim_start();
        if start.starts_with('{') || start.starts_with('[') {
            Some(Self::Json)
        } else if start
            .get(..15)
            .is_some_and(|head| head.eq_ignore_ascii_case("<!doctype html>"))
            || start.get(..5).is_some_and(|head| head.eq_ignore_ascii_case("<html"))
        {
            Some(Self::Html)
        } else if start.starts_with('<') {
            Some(Self::Xml)
        } else {
            None
        }
    }
}

struct Assets {
    syntaxes: SyntaxSet,
    theme: Theme,
}

/// Loaded on first use (tens of milliseconds), then shared
fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let mut themes = ThemeSet::load_defaults();
        Assets {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            theme: themes.themes.remove(THEME).unwrap_or_default(),
        }
    })
}

fn syntax(language: Language) -> Option<&'static SyntaxReference> {
    assets().syntaxes.find_syntax_by_name(language.syntax_name())
}

/// Highlights `text` line by line; `None` if the language is unavailable or highlighting fails,
/// so the caller shows it plain
pub fn highlight(text: &str, language: Language) -> Option<Vec<Line<'static>>> {
    let assets = assets();
    let mut highlighter = HighlightLines::new(syntax(language)?, &assets.theme);

    LinesWithEndings::from(text)
        .map(|line| {
            let ranges = highlighter.highlight_line(line, &assets.syntaxes).ok()?;
            let spans: Vec<Span<'static>> = ranges
                .into_iter()
                .map(|(style, piece)| {
                    (
                        to_ratatui_style(style),
                        piece.trim_end_matches(['\n', '\r']).to_string(),
                    )
                })
                .filter(|(_, piece)| !piece.is_empty())
                .map(|(style, piece)| Span::styled(piece, style))
                .collect();
            Some(Line::from(spans))
        })
        .collect()
}

fn to_ratatui_style(style: syntect::highlighting::Style) -> Style {
    let color = style.foreground;
    let mut result = Style::default().fg(Color::Rgb(color.r, color.g, color.b));
    if style.font_style.contains(FontStyle::BOLD) {
        result = result.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        result = result.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        result = result.add_modifier(Modifier::UNDERLINED);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn every_language_has_a_bundled_syntax() {
        let languages = [
            Language::Json,
            Language::Html,
            Language::Xml,
            Language::JavaScript,
            Language::Css,
        ];

        let missing: Vec<_> = languages
            .into_iter()
            .filter(|language| syntax(*language).is_none())
            .collect();

        assert_eq!(missing, vec![]);
    }

    #[test]
    fn from_mime_detects_languages_with_parameters_and_suffixes() {
        let languages = [
            Language::from_mime("application/json; charset=utf-8"),
            Language::from_mime("application/problem+json"),
            Language::from_mime("image/svg+xml"),
            Language::from_mime("TEXT/HTML"),
            Language::from_mime("application/x-javascript"),
            Language::from_mime("text/css"),
            Language::from_mime("text/plain"),
            Language::from_mime("image/png"),
        ];

        assert_eq!(
            languages,
            [
                Some(Language::Json),
                Some(Language::Json),
                Some(Language::Xml),
                Some(Language::Html),
                Some(Language::JavaScript),
                Some(Language::Css),
                None,
                None,
            ]
        );
    }

    #[test]
    fn sniff_detects_json_html_and_xml() {
        let languages = [
            Language::sniff("  {\"a\": 1}"),
            Language::sniff("<!DOCTYPE html><html>"),
            Language::sniff("<samlp:Response>"),
            Language::sniff("plain text"),
        ];

        assert_eq!(
            languages,
            [
                Some(Language::Json),
                Some(Language::Html),
                Some(Language::Xml),
                None
            ]
        );
    }

    #[test]
    fn max_highlight_bytes_is_lower_for_expensive_grammars() {
        let limits = [
            Language::Json.max_highlight_bytes(),
            Language::Xml.max_highlight_bytes(),
            Language::JavaScript.max_highlight_bytes(),
            Language::Css.max_highlight_bytes(),
            Language::Html.max_highlight_bytes(),
        ];

        assert_eq!(limits, [204_800, 204_800, 32_768, 32_768, 32_768]);
    }

    #[test]
    fn highlight_keeps_text_and_drops_newlines() {
        let lines = highlight("{\n  \"a\": 1\n}\n", Language::Json).expect("highlighted");

        assert_eq!(text_of(&lines), vec!["{", "  \"a\": 1", "}"]);
    }

    #[test]
    fn highlight_colours_tokens_differently() {
        let lines = highlight(r#"{"key": 42}"#, Language::Json).expect("highlighted");

        let colours: std::collections::HashSet<_> =
            lines[0].spans.iter().map(|span| span.style.fg).collect();
        assert!(colours.len() > 1, "expected several colours, got {:?}", colours);
    }

    #[test]
    fn highlight_keeps_empty_lines() {
        let lines = highlight("a {}\n\nb {}", Language::Css).expect("highlighted");

        assert_eq!(text_of(&lines), vec!["a {}", "", "b {}"]);
    }
}
