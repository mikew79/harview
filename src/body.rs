//! Body processing pipeline shared by the preview, export and external pager:
//! raw → decode → pretty print → (syntax highlight → search highlight, in later phases).
//!
//! Kept free of UI types so non-rendering consumers (export, pager) can use it directly.

use base64::alphabet;
use base64::engine::general_purpose::{GeneralPurpose, PAD_INDIFFERENT, STANDARD};
use base64::Engine;
use quick_xml::events::{BytesText, Event};
use quick_xml::{Reader, Writer};
use std::borrow::Cow;

/// User-toggleable pipeline settings; part of the preview cache key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyOptions {
    pub pretty: bool,
    pub decode: bool,
    /// Syntax highlighting; applied by the UI, `process` ignores it
    pub highlight: bool,
}

impl Default for BodyOptions {
    fn default() -> Self {
        Self {
            pretty: true,
            decode: false,
            highlight: true,
        }
    }
}

/// A request or response body as stored in the HAR
#[derive(Debug, Clone, Copy)]
pub struct Body<'a> {
    pub text: Option<&'a str>,
    pub encoding: Option<&'a str>,
    pub mime_type: Option<&'a str>,
}

/// Content family, used to pick a pretty printer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    Xml,
    Html,
    Other,
}

impl BodyKind {
    pub fn from_mime(mime_type: Option<&str>) -> Self {
        let essence = mime_type
            .and_then(|mime| mime.split(';').next())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match essence.as_str() {
            "application/json" | "text/json" => Self::Json,
            "text/html" | "application/xhtml+xml" => Self::Html,
            "application/xml" | "text/xml" => Self::Xml,
            other if other.ends_with("+json") => Self::Json,
            other if other.ends_with("+xml") => Self::Xml,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Processed<'a> {
    Text(Cow<'a, str>),
    /// Decoded content that is not text (images, fonts, ...)
    Binary(Vec<u8>),
    Empty,
    NotCaptured,
    /// Encoded content shown as-is because decoding is switched off
    Encoded { encoding: String, len: usize },
    InvalidEncoding { encoding: String, error: String },
}

pub fn process<'a>(body: &Body<'a>, options: BodyOptions) -> Processed<'a> {
    let Some(text) = body.text else {
        return Processed::NotCaptured;
    };
    if text.is_empty() {
        return Processed::Empty;
    }

    let decoded: Cow<'a, str> = match body.encoding {
        Some(encoding @ "base64") if !options.decode => {
            return Processed::Encoded {
                encoding: encoding.to_string(),
                len: text.len(),
            }
        }
        Some(encoding @ "base64") => match decode_base64(text) {
            Ok(Decoded::Text(decoded)) => Cow::Owned(decoded),
            Ok(Decoded::Binary(bytes)) => return Processed::Binary(bytes),
            Err(error) => {
                return Processed::InvalidEncoding {
                    encoding: encoding.to_string(),
                    error,
                }
            }
        },
        // Bodies the HAR doesn't mark as encoded can still be base64 (common for request payloads)
        _ if options.decode => match decode_implicit_base64(text) {
            Some(decoded) => Cow::Owned(decoded),
            None => Cow::Borrowed(text),
        },
        _ => Cow::Borrowed(text),
    };
    if decoded.is_empty() {
        return Processed::Empty;
    }

    if options.pretty {
        let kind = match BodyKind::from_mime(body.mime_type) {
            BodyKind::Other => sniff_kind(&decoded),
            kind => kind,
        };
        if let Some(pretty) = pretty_print(&decoded, kind) {
            return Processed::Text(Cow::Owned(pretty));
        }
    }
    Processed::Text(decoded)
}

/// A form field or query parameter value prepared for display
#[derive(Debug, PartialEq)]
pub struct DecodedField<'a> {
    pub text: Cow<'a, str>,
    /// The value was base64 (after URL-decoding)
    pub base64: bool,
}

/// With decoding on, URL-decodes a form / query value, then decodes base64 when that yields
/// readable text (e.g. a SAMLResponse), pretty printing the result
pub fn decode_field(value: &str, options: BodyOptions) -> DecodedField<'_> {
    if !options.decode {
        return DecodedField {
            text: Cow::Borrowed(value),
            base64: false,
        };
    }

    // `+` is a literal base64 character, but a space in form encoding: try base64 with it kept
    if let Some(decoded) = decode_implicit_base64(&percent_decode(value, false)) {
        let text = if options.pretty {
            pretty_print(&decoded, sniff_kind(&decoded)).unwrap_or(decoded)
        } else {
            decoded
        };
        return DecodedField {
            text: Cow::Owned(text),
            base64: true,
        };
    }
    DecodedField {
        text: percent_decode(value, true),
        base64: false,
    }
}

/// Splits an `application/x-www-form-urlencoded` body into (still encoded) name / value pairs
pub fn form_fields(text: &str) -> Vec<(&str, &str)> {
    text.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .collect()
}

pub fn is_form_urlencoded(mime_type: &str) -> bool {
    mime_type
        .split(';')
        .next()
        .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/x-www-form-urlencoded"))
}

/// Decodes `%XX` escapes (and `+` as a space when `plus_as_space`); invalid escapes are kept as-is
pub fn percent_decode(text: &str, plus_as_space: bool) -> Cow<'_, str> {
    if !text.contains('%') && !(plus_as_space && text.contains('+')) {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = bytes
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (escaped, bytes[i]) {
            (Some(byte), _) => {
                out.push(byte);
                i += 3;
            }
            (None, b'+') if plus_as_space => {
                out.push(b' ');
                i += 1;
            }
            (None, byte) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    Cow::Owned(String::from_utf8_lossy(&out).into_owned())
}

const STANDARD_LENIENT: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, PAD_INDIFFERENT);
const URL_SAFE_LENIENT: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, PAD_INDIFFERENT);

/// Shorter tokens are too often plain ids or words to treat as base64
const MIN_IMPLICIT_BASE64_LEN: usize = 16;

/// Decodes text that merely *looks* like base64 (standard or URL-safe, padding optional).
/// Only accepted when the result is readable text, so ids and hashes that happen to use the
/// base64 alphabet are left alone.
fn decode_implicit_base64(text: &str) -> Option<String> {
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let unpadded = compact.trim_end_matches('=');
    if unpadded.len() < MIN_IMPLICIT_BASE64_LEN || compact.len() - unpadded.len() > 2 {
        return None;
    }
    if !unpadded
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_'))
    {
        return None;
    }
    let engine = if unpadded.contains(['-', '_']) {
        &URL_SAFE_LENIENT
    } else {
        &STANDARD_LENIENT
    };
    let decoded = String::from_utf8(engine.decode(&compact).ok()?).ok()?;
    is_readable(&decoded).then_some(decoded)
}

fn is_readable(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}

/// Guesses the kind of content with no (useful) MIME type from how it starts
fn sniff_kind(text: &str) -> BodyKind {
    let start = text.trim_start();
    if start.starts_with('{') || start.starts_with('[') {
        BodyKind::Json
    } else if start.get(..15).is_some_and(|head| head.eq_ignore_ascii_case("<!doctype html>"))
        || start.get(..5).is_some_and(|head| head.eq_ignore_ascii_case("<html"))
    {
        BodyKind::Html
    } else if start.starts_with('<') {
        BodyKind::Xml
    } else {
        BodyKind::Other
    }
}

enum Decoded {
    Text(String),
    Binary(Vec<u8>),
}

fn decode_base64(text: &str) -> Result<Decoded, String> {
    // Some exporters wrap long base64 content across lines
    let compact: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = STANDARD.decode(compact).map_err(|error| error.to_string())?;
    match String::from_utf8(bytes) {
        Ok(text) if !text.contains('\0') => Ok(Decoded::Text(text)),
        Ok(text) => Ok(Decoded::Binary(text.into_bytes())),
        Err(error) => Ok(Decoded::Binary(error.into_bytes())),
    }
}

/// Returns `None` when the content can't be formatted, so the caller shows it raw
fn pretty_print(text: &str, kind: BodyKind) -> Option<String> {
    match kind {
        // Many APIs serve JSON with a generic content type, so try it for unknown types too
        BodyKind::Json | BodyKind::Other => pretty_json(text),
        BodyKind::Xml => pretty_markup(text, false),
        BodyKind::Html => pretty_markup(text, true),
    }
}

fn pretty_json(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

/// HTML elements that never have a closing tag
const HTML_VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// HTML elements whose content is copied through untouched
const HTML_RAW_ELEMENTS: &[&str] = &["script", "style", "pre", "textarea"];

fn is_one_of(name: &str, names: &[&str]) -> bool {
    names.iter().any(|candidate| name.eq_ignore_ascii_case(candidate))
}

/// Re-indents XML, or best-effort HTML (lenient about void elements and mismatched end tags)
fn pretty_markup(text: &str, html: bool) -> Option<String> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = !html;
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);

    loop {
        let event = reader.read_event().ok()?;
        match event {
            Event::Eof => break,
            Event::Start(start) if html && is_one_of(start.name().as_ref(), HTML_VOID_ELEMENTS) => {
                writer.write_event(Event::Empty(start)).ok()?;
            }
            Event::Start(start) if html && is_one_of(start.name().as_ref(), HTML_RAW_ELEMENTS) => {
                let end = start.to_end().into_owned();
                let content = reader.read_text(end.name()).ok()?;
                writer.write_event(Event::Start(start)).ok()?;
                writer.write_event(Event::Text(content)).ok()?;
                writer.write_event(Event::End(end)).ok()?;
            }
            Event::End(end) if html && is_one_of(end.name().as_ref(), HTML_VOID_ELEMENTS) => {}
            Event::Text(text) => {
                let trimmed = trim_formatting(&text);
                if !trimmed.is_empty() {
                    writer
                        .write_event(Event::Text(BytesText::from_escaped(trimmed)))
                        .ok()?;
                }
            }
            // Keep entity references inline with the surrounding text
            Event::GeneralRef(reference) => {
                let escaped = format!("&{};", &*reference);
                writer
                    .write_event(Event::Text(BytesText::from_escaped(escaped)))
                    .ok()?;
            }
            other => writer.write_event(other).ok()?,
        }
    }

    let pretty = String::from_utf8(writer.into_inner()).ok()?;
    Some(pretty.trim().to_string())
}

/// Drops whitespace that only exists to format the source (it contains a line break),
/// but keeps meaningful spaces such as the one in `Hello <b>world</b>`
fn trim_formatting(text: &str) -> &str {
    let start = text.len() - text.trim_start().len();
    let text = if text[..start].contains('\n') {
        &text[start..]
    } else {
        text
    };
    let end = text.trim_end().len();
    if text[end..].contains('\n') {
        &text[..end]
    } else {
        text
    }
}

/// Classic hex dump (`offset  hex bytes  |ascii|`) of at most `limit` bytes
pub fn hex_dump(bytes: &[u8], limit: usize) -> Vec<String> {
    bytes[..bytes.len().min(limit)]
        .chunks(16)
        .enumerate()
        .map(|(row, chunk)| {
            let hex: Vec<String> = chunk.iter().map(|byte| format!("{:02x}", byte)).collect();
            let (left, right) = hex.split_at(hex.len().min(8));
            let ascii: String = chunk
                .iter()
                .map(|&byte| {
                    if byte.is_ascii_graphic() || byte == b' ' {
                        byte as char
                    } else {
                        '.'
                    }
                })
                .collect();
            format!(
                "{:08x}  {:<23}  {:<23}  |{}|",
                row * 16,
                left.join(" "),
                right.join(" "),
                ascii
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body<'a>(text: Option<&'a str>, mime_type: Option<&'a str>) -> Body<'a> {
        Body {
            text,
            encoding: None,
            mime_type,
        }
    }

    fn base64_body(text: &str) -> Body<'_> {
        Body {
            text: Some(text),
            encoding: Some("base64"),
            mime_type: Some("text/plain"),
        }
    }

    const DECODE: BodyOptions = BodyOptions {
        pretty: true,
        decode: true,
        highlight: false,
    };

    fn pretty_text(text: &str, mime_type: &str) -> String {
        match process(&body(Some(text), Some(mime_type)), BodyOptions::default()) {
            Processed::Text(pretty) => pretty.into_owned(),
            other => panic!("expected text, got {:?}", other),
        }
    }

    #[test]
    fn process_json_with_pretty_returns_indented_json() {
        let result = pretty_text(r#"{"a":1,"b":[true]}"#, "application/json");

        assert_eq!(result, "{\n  \"a\": 1,\n  \"b\": [\n    true\n  ]\n}");
    }

    #[test]
    fn process_json_with_unknown_mime_is_still_pretty_printed() {
        let result = pretty_text(r#"{"a":1}"#, "text/plain");

        assert_eq!(result, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn process_json_without_pretty_returns_raw_text() {
        let raw = r#"{"a":1}"#;
        let options = BodyOptions {
            pretty: false,
            decode: false,
            highlight: false,
        };

        let result = process(&body(Some(raw), Some("application/json")), options);

        assert_eq!(result, Processed::Text(Cow::Borrowed(raw)));
    }

    #[test]
    fn process_non_json_text_returns_raw_text() {
        let raw = "function f(x){return x*2}";

        let result = process(
            &body(Some(raw), Some("application/javascript")),
            BodyOptions::default(),
        );

        assert_eq!(result, Processed::Text(Cow::Borrowed(raw)));
    }

    #[test]
    fn process_empty_text_returns_empty() {
        let result = process(&body(Some(""), None), BodyOptions::default());

        assert_eq!(result, Processed::Empty);
    }

    #[test]
    fn process_missing_text_returns_not_captured() {
        let result = process(&body(None, None), BodyOptions::default());

        assert_eq!(result, Processed::NotCaptured);
    }

    #[test]
    fn process_base64_with_decode_off_returns_encoded_with_length() {
        let result = process(&base64_body("aGVsbG8="), BodyOptions::default());

        assert_eq!(
            result,
            Processed::Encoded {
                encoding: "base64".to_string(),
                len: 8
            }
        );
    }

    #[test]
    fn process_base64_text_with_decode_on_returns_decoded_text() {
        let result = process(&base64_body("aGVsbG8gd29ybGQ="), DECODE);

        assert_eq!(result, Processed::Text(Cow::Owned("hello world".to_string())));
    }

    #[test]
    fn process_base64_with_line_breaks_decodes() {
        let result = process(&base64_body("aGVsbG8g\nd29ybGQ="), DECODE);

        assert_eq!(result, Processed::Text(Cow::Owned("hello world".to_string())));
    }

    #[test]
    fn process_base64_json_with_decode_on_is_pretty_printed() {
        let encoded = Body {
            text: Some("eyJhIjoxfQ=="),
            encoding: Some("base64"),
            mime_type: Some("application/json"),
        };

        let result = process(&encoded, DECODE);

        assert_eq!(result, Processed::Text(Cow::Owned("{\n  \"a\": 1\n}".to_string())));
    }

    #[test]
    fn process_base64_binary_with_decode_on_returns_bytes() {
        // PNG signature
        let result = process(&base64_body("iVBORw0KGgo="), DECODE);

        assert_eq!(
            result,
            Processed::Binary(vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
        );
    }

    #[test]
    fn process_base64_utf8_with_nul_bytes_returns_binary() {
        let result = process(&base64_body("YQBi"), DECODE);

        assert_eq!(result, Processed::Binary(vec![b'a', 0, b'b']));
    }

    #[test]
    fn process_invalid_base64_returns_invalid_encoding() {
        let result = process(&base64_body("not base64!"), DECODE);

        assert!(matches!(result, Processed::InvalidEncoding { .. }));
    }

    #[test]
    fn process_xml_with_pretty_returns_indented_xml() {
        let result = pretty_text(
            r#"<?xml version="1.0"?><feed><item id="1"><title>One</title></item></feed>"#,
            "application/xml",
        );

        assert_eq!(
            result,
            "<?xml version=\"1.0\"?>\n<feed>\n  <item id=\"1\">\n    <title>One</title>\n  </item>\n</feed>"
        );
    }

    #[test]
    fn process_malformed_xml_falls_back_to_raw_text() {
        let raw = "<feed><item></feed>";

        let result = pretty_text(raw, "application/xml");

        assert_eq!(result, raw);
    }

    #[test]
    fn process_html_with_void_elements_keeps_indentation_balanced() {
        let result = pretty_text(
            r#"<html><head><meta charset="utf-8"><link rel="stylesheet" href="s.css"></head><body><p>Hi<br>there</p></body></html>"#,
            "text/html",
        );

        assert_eq!(
            result,
            "<html>\n  <head>\n    <meta charset=\"utf-8\"/>\n    <link rel=\"stylesheet\" href=\"s.css\"/>\n  </head>\n  <body>\n    <p>Hi<br/>there</p>\n  </body>\n</html>"
        );
    }

    #[test]
    fn process_html_script_content_is_kept_verbatim() {
        let result = pretty_text(
            "<html><body><script>if (a < b && c > d) { go(); }</script></body></html>",
            "text/html",
        );

        assert!(result.contains("<script>if (a < b && c > d) { go(); }</script>"));
    }

    #[test]
    fn process_html_entities_stay_inline() {
        let result = pretty_text("<p>Fish &amp; chips</p>", "text/html");

        assert_eq!(result, "<p>Fish &amp; chips</p>");
    }

    #[test]
    fn process_html_with_mismatched_end_tags_still_formats() {
        let result = pretty_text("<div><span>a</div>", "text/html");

        assert_eq!(result, "<div>\n  <span>a</div>");
    }

    #[test]
    fn from_mime_with_parameters_and_suffixes_detects_kind() {
        let kinds = [
            BodyKind::from_mime(Some("application/json; charset=utf-8")),
            BodyKind::from_mime(Some("application/problem+json")),
            BodyKind::from_mime(Some("image/svg+xml")),
            BodyKind::from_mime(Some("TEXT/HTML")),
            BodyKind::from_mime(Some("application/xhtml+xml")),
            BodyKind::from_mime(Some("text/css")),
            BodyKind::from_mime(None),
        ];

        assert_eq!(
            kinds,
            [
                BodyKind::Json,
                BodyKind::Json,
                BodyKind::Xml,
                BodyKind::Html,
                BodyKind::Html,
                BodyKind::Other,
                BodyKind::Other,
            ]
        );
    }

    const DECODE_RAW: BodyOptions = BodyOptions {
        pretty: false,
        decode: true,
        highlight: false,
    };

    #[test]
    fn decode_field_percent_encoded_base64_xml_is_decoded_and_pretty_printed() {
        // base64 of <a><b>x</b></a>, percent-encoded as Chrome stores form params
        let result = decode_field("PGE%2BPGI%2BeDwvYj48L2E%2B", DECODE);

        assert_eq!(
            result,
            DecodedField {
                text: Cow::Owned("<a>\n  <b>x</b>\n</a>".to_string()),
                base64: true
            }
        );
    }

    #[test]
    fn decode_field_with_decode_off_returns_value_unchanged() {
        let result = decode_field("PGE%2BPGI%2BeDwvYj48L2E%2B", BodyOptions::default());

        assert_eq!(
            result,
            DecodedField {
                text: Cow::Borrowed("PGE%2BPGI%2BeDwvYj48L2E%2B"),
                base64: false
            }
        );
    }

    #[test]
    fn decode_field_url_safe_base64_without_padding_decodes() {
        // URL-safe base64 of {"to":"/home?a=1"}, padding stripped
        let result = decode_field("eyJ0byI6Ii9ob21lP2E9MSJ9", DECODE_RAW);

        assert_eq!(result.text, r#"{"to":"/home?a=1"}"#);
    }

    #[test]
    fn decode_field_base64_with_literal_plus_decodes() {
        // base64 of "<<<???>>>>>>", contains '+' and '/' unencoded
        let result = decode_field("PDw8Pz8/Pj4+Pj4+", DECODE_RAW);

        assert_eq!(result.text, "<<<???>>>>>>");
    }

    #[test]
    fn decode_field_binary_base64_keeps_original_value() {
        // base64 of 12 random bytes: not readable, so treated as an opaque token
        let result = decode_field("q83vASNFZ4mrze8B", DECODE_RAW);

        assert_eq!(
            result,
            DecodedField {
                text: Cow::Borrowed("q83vASNFZ4mrze8B"),
                base64: false
            }
        );
    }

    #[test]
    fn decode_field_plain_value_is_url_decoded_with_plus_as_space() {
        let result = decode_field("search+term%21", DECODE_RAW);

        assert_eq!(
            result,
            DecodedField {
                text: Cow::Owned("search term!".to_string()),
                base64: false
            }
        );
    }

    #[test]
    fn decode_field_short_token_is_not_treated_as_base64() {
        let result = decode_field("abcd1234", DECODE_RAW);

        assert_eq!(result.base64, false);
    }

    #[test]
    fn percent_decode_invalid_escape_is_kept() {
        let result = percent_decode("100%zz%4", false);

        assert_eq!(result, "100%zz%4");
    }

    #[test]
    fn percent_decode_multibyte_utf8_decodes() {
        let result = percent_decode("caf%C3%A9", false);

        assert_eq!(result, "café");
    }

    #[test]
    fn form_fields_splits_pairs_and_keeps_values_encoded() {
        let fields = form_fields("SAMLResponse=PGE%2B&RelayState=&flag");

        assert_eq!(
            fields,
            vec![("SAMLResponse", "PGE%2B"), ("RelayState", ""), ("flag", "")]
        );
    }

    #[test]
    fn is_form_urlencoded_with_charset_parameter_matches() {
        assert!(is_form_urlencoded("application/x-www-form-urlencoded; charset=UTF-8"));
    }

    #[test]
    fn process_unmarked_base64_body_with_decode_on_is_decoded() {
        // base64 of {"user":"alice"} in a text/plain request body, not marked as encoded in the HAR
        let result = process(&body(Some("eyJ1c2VyIjoiYWxpY2UifQ=="), Some("text/plain")), DECODE);

        assert_eq!(
            result,
            Processed::Text(Cow::Owned("{\n  \"user\": \"alice\"\n}".to_string()))
        );
    }

    #[test]
    fn process_unmarked_base64_body_with_decode_off_is_raw() {
        let result = process(
            &body(Some("eyJhIjoxfQ=="), Some("text/plain")),
            BodyOptions::default(),
        );

        assert_eq!(result, Processed::Text(Cow::Borrowed("eyJhIjoxfQ==")));
    }

    #[test]
    fn process_xml_served_as_text_plain_is_pretty_printed() {
        let result = pretty_text("<a><b>x</b></a>", "text/plain");

        assert_eq!(result, "<a>\n  <b>x</b>\n</a>");
    }

    #[test]
    fn hex_dump_formats_offset_hex_and_ascii() {
        let lines = hex_dump(b"PNG\x00\x01 hello, world!!", 64);

        assert_eq!(
            lines,
            vec![
                "00000000  50 4e 47 00 01 20 68 65  6c 6c 6f 2c 20 77 6f 72  |PNG.. hello, wor|",
                "00000010  6c 64 21 21                                       |ld!!|",
            ]
        );
    }

    #[test]
    fn hex_dump_stops_at_limit() {
        let lines = hex_dump(&[0u8; 100], 32);

        assert_eq!(lines.len(), 2);
    }
}
