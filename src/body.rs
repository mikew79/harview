//! Body processing pipeline shared by the preview, export and external pager:
//! raw → decode → pretty print → (syntax highlight → search highlight, in later phases).
//!
//! Kept free of UI types so non-rendering consumers (export, pager) can use it directly.

use std::borrow::Cow;

/// User-toggleable pipeline settings; part of the preview cache key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyOptions {
    pub pretty: bool,
}

impl Default for BodyOptions {
    fn default() -> Self {
        Self { pretty: true }
    }
}

/// A request or response body as stored in the HAR
#[derive(Debug, Clone, Copy)]
pub struct Body<'a> {
    pub text: Option<&'a str>,
    pub encoding: Option<&'a str>,
}

#[derive(Debug, PartialEq)]
pub enum Processed<'a> {
    Text(Cow<'a, str>),
    Empty,
    NotCaptured,
    /// Encoded content the pipeline cannot decode (yet)
    Encoded { encoding: String, len: usize },
}

pub fn process<'a>(body: &Body<'a>, options: BodyOptions) -> Processed<'a> {
    let Some(text) = body.text else {
        return Processed::NotCaptured;
    };
    if text.is_empty() {
        return Processed::Empty;
    }
    if let Some(encoding @ "base64") = body.encoding {
        return Processed::Encoded {
            encoding: encoding.to_string(),
            len: text.len(),
        };
    }

    if options.pretty {
        if let Some(pretty) = pretty_json(text) {
            return Processed::Text(Cow::Owned(pretty));
        }
    }
    Processed::Text(Cow::Borrowed(text))
}

fn pretty_json(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(text: Option<&str>) -> Body<'_> {
        Body {
            text,
            encoding: None,
        }
    }

    #[test]
    fn process_json_with_pretty_returns_indented_json() {
        let result = process(&body(Some(r#"{"a":1,"b":[true]}"#)), BodyOptions::default());

        assert_eq!(
            result,
            Processed::Text(Cow::Owned(
                "{\n  \"a\": 1,\n  \"b\": [\n    true\n  ]\n}".to_string()
            ))
        );
    }

    #[test]
    fn process_json_without_pretty_returns_raw_text() {
        let raw = r#"{"a":1}"#;

        let result = process(&body(Some(raw)), BodyOptions { pretty: false });

        assert_eq!(result, Processed::Text(Cow::Borrowed(raw)));
    }

    #[test]
    fn process_non_json_text_returns_raw_text() {
        let raw = "function f(x){return x*2}";

        let result = process(&body(Some(raw)), BodyOptions::default());

        assert_eq!(result, Processed::Text(Cow::Borrowed(raw)));
    }

    #[test]
    fn process_empty_text_returns_empty() {
        let result = process(&body(Some("")), BodyOptions::default());

        assert_eq!(result, Processed::Empty);
    }

    #[test]
    fn process_missing_text_returns_not_captured() {
        let result = process(&body(None), BodyOptions::default());

        assert_eq!(result, Processed::NotCaptured);
    }

    #[test]
    fn process_base64_returns_encoded_with_length() {
        let encoded = Body {
            text: Some("aGVsbG8="),
            encoding: Some("base64"),
        };

        let result = process(&encoded, BodyOptions::default());

        assert_eq!(
            result,
            Processed::Encoded {
                encoding: "base64".to_string(),
                len: 8
            }
        );
    }
}
