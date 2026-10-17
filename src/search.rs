//! Search query language and matching. Kept free of UI types.
//!
//! A query is a list of whitespace-separated terms that must all match (`!` negates a term):
//!
//! ```text
//! token                 regex matched against URL, headers, parameters and bodies
//! url:  host:           regex on the full URL / host name
//! header:               regex on request and response headers ("Name: value")
//! param:                regex on decoded query parameters and form fields ("name=value")
//! body:                 regex on decoded request and response bodies
//! type:                 regex on the response content type
//! method:POST           exact (case-insensitive)
//! status:404  status:4xx  status:>=500
//! size:>100k            response size, units b / k / m / g (decimal)
//! dur:>500ms  dur:<1s   total time
//! ```
//!
//! Regexes are case-insensitive unless they contain an uppercase letter ("smart case").
//! Use quotes for spaces: `body:"internal error"`.

use crate::body::{self, Body, BodyOptions, Processed};
use crate::har::Entry;
use regex::{Regex, RegexBuilder};

/// Decoding on, pretty printing off: search the content, not its formatting
const SEARCH_OPTIONS: BodyOptions = BodyOptions {
    pretty: false,
    decode: true,
};

/// Decoded, searchable text of one entry; built once, on the first search
#[derive(Debug, Default)]
pub struct EntryText {
    url: String,
    host: String,
    mime_type: String,
    /// `Name: value` per line, request headers then response headers
    headers: String,
    /// Decoded `name=value` per line, query parameters then form fields
    params: String,
    /// Decoded request body, then response body
    bodies: String,
}

impl EntryText {
    pub fn new(entry: &Entry) -> Self {
        let request = &entry.request;
        let response = &entry.response;

        let headers = request
            .headers
            .iter()
            .chain(&response.headers)
            .map(|header| format!("{}: {}", header.name, header.value))
            .collect::<Vec<_>>()
            .join("\n");

        let mut fields: Vec<(&str, &str)> = request
            .query_string
            .iter()
            .map(|query| (query.name.as_str(), query.value.as_str()))
            .collect();
        if let Some(post_data) = &request.post_data {
            match &post_data.params {
                Some(params) if !params.is_empty() => {
                    fields.extend(params.iter().map(|p| (p.name.as_str(), p.value.as_str())))
                }
                _ if body::is_form_urlencoded(&post_data.mime_type) => {
                    fields.extend(body::form_fields(&post_data.text))
                }
                _ => {}
            }
        }
        let params = fields
            .into_iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    body::percent_decode(name, true),
                    body::decode_field(value, SEARCH_OPTIONS).text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let request_body = request.post_data.as_ref().map(|post_data| Body {
            text: Some(&post_data.text),
            encoding: None,
            mime_type: Some(&post_data.mime_type),
        });
        let response_body = Body {
            text: response.content.text.as_deref(),
            encoding: response.content.encoding.as_deref(),
            mime_type: response.content.mime_type.as_deref(),
        };
        let bodies = request_body
            .iter()
            .chain([&response_body])
            .filter_map(|body| match body::process(body, SEARCH_OPTIONS) {
                Processed::Text(text) => Some(text.into_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Self {
            url: request.url.to_string(),
            host: request.url.host_str().unwrap_or("").to_string(),
            mime_type: response.content.mime_type.clone().unwrap_or_default(),
            headers,
            params,
            bodies,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Comparison {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Comparison {
    /// Splits a leading `>=`, `<=`, `>`, `<` or `=` off `value`
    fn parse(value: &str) -> (Self, &str) {
        for (prefix, comparison) in [
            (">=", Self::Ge),
            ("<=", Self::Le),
            (">", Self::Gt),
            ("<", Self::Lt),
            ("=", Self::Eq),
        ] {
            if let Some(rest) = value.strip_prefix(prefix) {
                return (comparison, rest);
            }
        }
        (Self::Eq, value)
    }

    fn test<T: PartialOrd>(self, actual: T, expected: T) -> bool {
        match self {
            Self::Eq => actual == expected,
            Self::Lt => actual < expected,
            Self::Le => actual <= expected,
            Self::Gt => actual > expected,
            Self::Ge => actual >= expected,
        }
    }
}

#[derive(Debug)]
enum Matcher {
    Any(Regex),
    Url(Regex),
    Host(Regex),
    Header(Regex),
    Param(Regex),
    Body(Regex),
    Type(Regex),
    Method(String),
    /// Three positions, each a digit or `None` for `x`, e.g. `4xx`
    StatusPattern([Option<u8>; 3]),
    Status(Comparison, i64),
    Size(Comparison, i64),
    Duration(Comparison, f64),
}

impl Matcher {
    fn matches(&self, entry: &Entry, text: &EntryText) -> bool {
        match self {
            Self::Any(regex) => [&text.url, &text.headers, &text.params, &text.bodies]
                .iter()
                .any(|haystack| regex.is_match(haystack)),
            Self::Url(regex) => regex.is_match(&text.url),
            Self::Host(regex) => regex.is_match(&text.host),
            Self::Header(regex) => regex.is_match(&text.headers),
            Self::Param(regex) => regex.is_match(&text.params),
            Self::Body(regex) => regex.is_match(&text.bodies),
            Self::Type(regex) => regex.is_match(&text.mime_type),
            Self::Method(method) => entry.request.method.eq_ignore_ascii_case(method),
            Self::StatusPattern(pattern) => {
                let status = entry.response.status.to_string();
                status.len() == 3
                    && status.bytes().zip(pattern).all(|(digit, expected)| match expected {
                        Some(expected) => digit - b'0' == *expected,
                        None => true,
                    })
            }
            Self::Status(comparison, status) => comparison.test(entry.response.status, *status),
            Self::Size(comparison, size) => {
                response_size(entry).is_some_and(|actual| comparison.test(actual, *size))
            }
            Self::Duration(comparison, ms) => comparison.test(entry.time, *ms),
        }
    }

    fn regex(&self) -> Option<&Regex> {
        match self {
            Self::Any(regex)
            | Self::Url(regex)
            | Self::Host(regex)
            | Self::Header(regex)
            | Self::Param(regex)
            | Self::Body(regex)
            | Self::Type(regex) => Some(regex),
            _ => None,
        }
    }
}

/// Response size in bytes, `None` when the HAR doesn't know it
fn response_size(entry: &Entry) -> Option<i64> {
    [entry.response.content.size, Some(entry.response.body_size)]
        .into_iter()
        .flatten()
        .find(|&size| size >= 0)
}

#[derive(Debug)]
struct Term {
    negated: bool,
    matcher: Matcher,
}

#[derive(Debug)]
pub struct Query {
    terms: Vec<Term>,
    /// All positive regex terms combined, for highlighting matches
    highlight: Option<Regex>,
}

impl Query {
    pub fn parse(input: &str) -> Result<Self, String> {
        let terms = tokenize(input)?
            .iter()
            .map(|token| parse_term(token))
            .collect::<Result<Vec<_>, _>>()?;

        let patterns: Vec<String> = terms
            .iter()
            .filter(|term| !term.negated)
            .filter_map(|term| term.matcher.regex())
            .map(|regex| format!("(?:{})", regex.as_str()))
            .collect();
        let highlight = if patterns.is_empty() {
            None
        } else {
            // Each term's case sensitivity is baked into its pattern by `smart_regex`
            Some(Regex::new(&patterns.join("|")).map_err(|error| error.to_string())?)
        };

        Ok(Self { terms, highlight })
    }

    pub fn matches(&self, entry: &Entry, text: &EntryText) -> bool {
        self.terms
            .iter()
            .all(|term| term.matcher.matches(entry, text) != term.negated)
    }

    pub fn highlight(&self) -> Option<&Regex> {
        self.highlight.as_ref()
    }
}

/// Compiles `pattern` case-insensitively unless it contains an uppercase letter
pub fn smart_regex(pattern: &str) -> Result<Regex, String> {
    let insensitive = !has_uppercase(pattern);
    // Inline the flag so the pattern keeps its case sensitivity when combined with others
    let pattern = if insensitive {
        format!("(?im:{})", pattern)
    } else {
        format!("(?m:{})", pattern)
    };
    RegexBuilder::new(&pattern)
        .build()
        .map_err(|error| {
            let detail = error.to_string();
            let reason = detail.lines().last().unwrap_or("").trim();
            format!("invalid regex: {}", reason.trim_start_matches("error: "))
        })
}

/// Ignores escaped characters, so `\S` or `\D` don't count as uppercase
fn has_uppercase(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c.is_uppercase() {
            return true;
        }
    }
    false
}

/// Splits on whitespace outside double quotes; `\"` is a literal quote
fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("unclosed quote".to_string());
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn parse_term(token: &str) -> Result<Term, String> {
    let (negated, token) = match token.strip_prefix('!') {
        Some(rest) if !rest.is_empty() => (true, rest),
        _ => (false, token),
    };

    let field = token
        .split_once(':')
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .filter(|(name, _)| FIELDS.contains(&name.as_str()));
    let matcher = match field {
        None => Matcher::Any(smart_regex(token)?),
        Some((name, "")) => return Err(format!("{}: needs a value", name)),
        Some((name, value)) => parse_field(&name, value)?,
    };
    Ok(Term { negated, matcher })
}

const FIELDS: &[&str] = &[
    "url", "host", "header", "param", "body", "type", "method", "status", "size", "dur", "time",
];

fn parse_field(name: &str, value: &str) -> Result<Matcher, String> {
    Ok(match name {
        "url" => Matcher::Url(smart_regex(value)?),
        "host" => Matcher::Host(smart_regex(value)?),
        "header" => Matcher::Header(smart_regex(value)?),
        "param" => Matcher::Param(smart_regex(value)?),
        "body" => Matcher::Body(smart_regex(value)?),
        "type" => Matcher::Type(smart_regex(value)?),
        "method" => Matcher::Method(value.to_string()),
        "status" => parse_status(value)?,
        "size" => {
            let (comparison, size) = Comparison::parse(value);
            Matcher::Size(comparison, parse_size(size)?)
        }
        "dur" | "time" => {
            let (comparison, duration) = Comparison::parse(value);
            Matcher::Duration(comparison, parse_duration(duration)?)
        }
        _ => unreachable!("field list and parser out of sync: {}", name),
    })
}

fn parse_status(value: &str) -> Result<Matcher, String> {
    let lower = value.to_ascii_lowercase();
    if lower.len() == 3 && lower.contains('x') {
        let mut pattern = [None; 3];
        for (slot, c) in pattern.iter_mut().zip(lower.chars()) {
            *slot = match c {
                'x' => None,
                c => Some(
                    c.to_digit(10)
                        .ok_or_else(|| format!("status: invalid pattern `{}`", value))?
                        as u8,
                ),
            };
        }
        return Ok(Matcher::StatusPattern(pattern));
    }
    let (comparison, number) = Comparison::parse(value);
    let status = number
        .parse()
        .map_err(|_| format!("status: expected e.g. 404, 4xx or >=500, got `{}`", value))?;
    Ok(Matcher::Status(comparison, status))
}

fn parse_size(value: &str) -> Result<i64, String> {
    let lower = value.to_ascii_lowercase();
    let (number, multiplier) = [
        ("gb", 1e9),
        ("g", 1e9),
        ("mb", 1e6),
        ("m", 1e6),
        ("kb", 1e3),
        ("k", 1e3),
        ("b", 1.0),
    ]
    .iter()
    .find_map(|(suffix, multiplier)| lower.strip_suffix(suffix).map(|n| (n, *multiplier)))
    .unwrap_or((&lower, 1.0));
    number
        .trim()
        .parse::<f64>()
        .map(|n| (n * multiplier) as i64)
        .map_err(|_| format!("size: expected e.g. >100k or <=2mb, got `{}`", value))
}

/// Milliseconds
fn parse_duration(value: &str) -> Result<f64, String> {
    let lower = value.to_ascii_lowercase();
    let (number, multiplier) = if let Some(n) = lower.strip_suffix("ms") {
        (n, 1.0)
    } else if let Some(n) = lower.strip_suffix('s') {
        (n, 1000.0)
    } else {
        (lower.as_str(), 1.0)
    };
    number
        .trim()
        .parse::<f64>()
        .map(|n| n * multiplier)
        .map_err(|_| format!("dur: expected e.g. >500ms or <2s, got `{}`", value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::har::Har;

    /// One entry built from JSON, so each test only states the fields it cares about
    fn entry(json: &str) -> Entry {
        let har: Har = serde_json::from_str(&format!(r#"{{"log": {{"entries": [{}]}}}}"#, json))
            .expect("valid entry");
        har.log.entries.into_iter().next().expect("one entry")
    }

    const LOGIN: &str = r#"{
        "startedDateTime": "2026-10-17T10:00:00Z", "time": 750.5,
        "request": {
            "method": "POST", "url": "https://api.example.com/v2/login?state=eyJyZXR1cm5UbyI6Ii9ob21lIn0",
            "headers": [{"name": "Authorization", "value": "Bearer abc"}],
            "queryString": [{"name": "state", "value": "eyJyZXR1cm5UbyI6Ii9ob21lIn0"}],
            "postData": {"mimeType": "application/x-www-form-urlencoded",
                         "text": "SAMLResponse=PG5hbWVJRD5hbGljZTwvbmFtZUlEPg%3D%3D&remember=yes"}
        },
        "response": {
            "status": 401, "headers": [{"name": "Content-Type", "value": "application/json"}],
            "content": {"mimeType": "application/json", "size": 2048, "text": "{\"error\":\"Invalid credentials\"}"}
        }
    }"#;

    fn matches(query: &str, json: &str) -> bool {
        let entry = entry(json);
        let text = EntryText::new(&entry);
        Query::parse(query).expect("valid query").matches(&entry, &text)
    }

    #[test]
    fn matches_plain_token_in_url() {
        assert!(matches("login", LOGIN));
    }

    #[test]
    fn matches_plain_token_in_response_body() {
        assert!(matches("credentials", LOGIN));
    }

    #[test]
    fn matches_plain_token_in_decoded_form_field() {
        // SAMLResponse is URL-encoded base64 of <nameID>alice</nameID>
        assert!(matches("nameid>alice", LOGIN));
    }

    #[test]
    fn matches_param_in_decoded_query_parameter() {
        // state is base64 of {"returnTo":"/home"}
        assert!(matches("param:returnTo", LOGIN));
    }

    #[test]
    fn matches_plain_token_absent_everywhere_fails() {
        assert!(!matches("checkout", LOGIN));
    }

    #[test]
    fn matches_smart_case_lowercase_is_insensitive() {
        assert!(matches("invalid", LOGIN));
    }

    #[test]
    fn matches_smart_case_uppercase_is_sensitive() {
        assert!(!matches("INVALID", LOGIN));
    }

    #[test]
    fn matches_regex_token() {
        assert!(matches(r"url:/v\d+/", LOGIN));
    }

    #[test]
    fn matches_host_field() {
        assert!(matches("host:^api\\.example", LOGIN));
    }

    #[test]
    fn matches_header_field_name_and_value() {
        assert!(matches("header:authorization:.bearer", LOGIN));
    }

    #[test]
    fn matches_header_field_does_not_search_body() {
        assert!(!matches("header:credentials", LOGIN));
    }

    #[test]
    fn matches_body_field_does_not_search_headers() {
        assert!(!matches("body:bearer", LOGIN));
    }

    #[test]
    fn matches_type_field() {
        assert!(matches("type:json", LOGIN));
    }

    #[test]
    fn matches_method_is_exact_and_case_insensitive() {
        assert_eq!(
            (matches("method:post", LOGIN), matches("method:PO", LOGIN)),
            (true, false)
        );
    }

    #[test]
    fn matches_status_exact_class_and_comparison() {
        assert_eq!(
            (
                matches("status:401", LOGIN),
                matches("status:4xx", LOGIN),
                matches("status:40x", LOGIN),
                matches("status:>=500", LOGIN),
                matches("status:<500", LOGIN),
            ),
            (true, true, true, false, true)
        );
    }

    #[test]
    fn matches_size_with_decimal_units() {
        // 2048 bytes; k = 1000, matching the table's size column
        assert_eq!(
            (
                matches("size:>3k", LOGIN),
                matches("size:>=2k", LOGIN),
                matches("size:<1mb", LOGIN),
                matches("size:2048", LOGIN),
            ),
            (false, true, true, true)
        );
    }

    #[test]
    fn matches_size_unknown_never_matches() {
        let unknown = r#"{"startedDateTime": "x", "request": {"method": "GET", "url": "https://a.example/"},
                          "response": {"status": 200, "content": {"size": -1}}}"#;

        assert_eq!(
            (matches("size:>0", unknown), matches("size:<1g", unknown)),
            (false, false)
        );
    }

    #[test]
    fn matches_duration_in_ms_and_seconds() {
        assert_eq!(
            (
                matches("dur:>500ms", LOGIN),
                matches("dur:<1s", LOGIN),
                matches("time:>1s", LOGIN),
                matches("dur:>750", LOGIN),
            ),
            (true, true, false, true)
        );
    }

    #[test]
    fn matches_negated_term() {
        assert_eq!(
            (matches("!status:2xx", LOGIN), matches("!login", LOGIN)),
            (true, false)
        );
    }

    #[test]
    fn matches_all_terms_must_match() {
        assert_eq!(
            (
                matches("method:POST status:4xx", LOGIN),
                matches("method:POST status:2xx", LOGIN),
            ),
            (true, false)
        );
    }

    #[test]
    fn matches_quoted_term_with_space() {
        assert!(matches(r#"body:"invalid credentials""#, LOGIN));
    }

    #[test]
    fn matches_unknown_field_is_plain_regex() {
        // `https:` isn't a field, so the whole token is searched for
        assert!(matches("https://api", LOGIN));
    }

    #[test]
    fn matches_empty_query_matches_everything() {
        assert!(matches("", LOGIN));
    }

    #[test]
    fn parse_invalid_regex_returns_error() {
        let error = Query::parse("body:(unclosed").unwrap_err();

        assert!(error.starts_with("invalid regex"), "{}", error);
    }

    #[test]
    fn parse_unclosed_quote_returns_error() {
        assert_eq!(Query::parse(r#"body:"oops"#).unwrap_err(), "unclosed quote");
    }

    #[test]
    fn parse_field_without_value_returns_error() {
        assert_eq!(Query::parse("status:").unwrap_err(), "status: needs a value");
    }

    #[test]
    fn parse_invalid_numbers_return_errors() {
        let errors = [
            Query::parse("status:abc").is_err(),
            Query::parse("status:4yx").is_err(),
            Query::parse("size:>lots").is_err(),
            Query::parse("dur:>soon").is_err(),
        ];

        assert_eq!(errors, [true; 4]);
    }

    #[test]
    fn highlight_combines_positive_regex_terms_only() {
        let query = Query::parse("login !logout status:4xx body:Invalid").expect("valid query");

        let highlight = query.highlight().expect("has regex terms");
        assert_eq!(
            (
                highlight.is_match("LOGIN"),
                highlight.is_match("Invalid"),
                highlight.is_match("invalid"),
                highlight.is_match("logout"),
            ),
            // `logout` is only in a negated term, `body:Invalid` is case-sensitive
            (true, true, false, false)
        );
    }

    #[test]
    fn highlight_without_regex_terms_is_none() {
        let query = Query::parse("status:4xx size:>1k").expect("valid query");

        assert!(query.highlight().is_none());
    }

    #[test]
    fn has_uppercase_ignores_escapes() {
        assert_eq!(
            (has_uppercase(r"\S+\D"), has_uppercase(r"\sFoo")),
            (false, true)
        );
    }

    #[test]
    fn tokenize_handles_quotes_and_escaped_quotes() {
        let tokens = tokenize(r#"a  "b c" d\"e body:"x y""#).expect("valid");

        assert_eq!(tokens, vec!["a", "b c", "d\"e", "body:x y"]);
    }
}
