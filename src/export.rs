//! Exporting a request or response to a file or the clipboard. Kept free of UI types.

use crate::body::{self, Body, BodyOptions, Processed};
use crate::har::{Entry, Header};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::io::Write;
use std::path::Path;

/// Default directory for exported files (git-ignored)
pub const EXPORT_DIR: &str = "exports";

/// Larger content is not sent to the clipboard: terminals cap OSC 52 sequences (often ~100KB)
/// and silently drop or truncate longer ones
pub const MAX_CLIPBOARD_BYTES: usize = 100 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    RequestBody,
    ResponseBody,
    /// Request line, headers and body as raw HTTP text
    FullRequest,
    /// Status line, headers and body as raw HTTP text
    FullResponse,
}

impl ExportTarget {
    pub const ALL: [ExportTarget; 4] = [
        Self::RequestBody,
        Self::ResponseBody,
        Self::FullRequest,
        Self::FullResponse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RequestBody => "Request body",
            Self::ResponseBody => "Response body",
            Self::FullRequest => "Full request",
            Self::FullResponse => "Full response",
        }
    }

    fn file_suffix(self) -> &'static str {
        match self {
            Self::RequestBody => "request-body",
            Self::ResponseBody => "response-body",
            Self::FullRequest => "request",
            Self::FullResponse => "response",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Exported {
    pub bytes: Vec<u8>,
    /// Contains decoded binary content (can't go to the clipboard)
    pub binary: bool,
}

/// Builds the content to export. `decoded` applies the pipeline (decoding, and pretty printing
/// when `pretty`) as the preview does; otherwise bodies are exactly as stored in the HAR
pub fn build(
    entry: &Entry,
    target: ExportTarget,
    decoded: bool,
    pretty: bool,
) -> Result<Exported, String> {
    let options = BodyOptions {
        pretty,
        decode: true,
        highlight: false,
    };
    let non_empty = |body: Exported, which: &str| {
        if body.bytes.is_empty() {
            Err(format!("the {} body is empty", which))
        } else {
            Ok(body)
        }
    };
    match target {
        ExportTarget::RequestBody => request_body(entry, decoded, options)
            .ok_or_else(|| "no request body".to_string())
            .and_then(|body| non_empty(body, "request")),
        ExportTarget::ResponseBody => {
            response_body(entry, decoded, options).and_then(|body| non_empty(body, "response"))
        }
        ExportTarget::FullRequest => {
            let request = &entry.request;
            let head = http_head(
                &first_line(&[
                    &request.method,
                    &path_and_query(&request.url),
                    &request.http_version,
                ]),
                &request.headers,
            );
            let body = request_body(entry, decoded, options);
            Ok(join(head, body))
        }
        ExportTarget::FullResponse => {
            let response = &entry.response;
            let head = http_head(
                &first_line(&[
                    &response.http_version,
                    &response.status.to_string(),
                    &response.status_text,
                ]),
                &response.headers,
            );
            // A missing body is fine for a full response: export the head alone
            let body = response_body(entry, decoded, options).ok();
            Ok(join(head, body))
        }
    }
}

fn path_and_query(url: &url::Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{}", url.path(), query),
        None => url.path().to_string(),
    }
}

/// Space-separated, skipping parts the HAR left empty (e.g. a missing HTTP version)
fn first_line(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

fn http_head(first_line: &str, headers: &[Header]) -> String {
    let mut head = format!("{}\n", first_line);
    for header in headers {
        head.push_str(&format!("{}: {}\n", header.name, header.value));
    }
    head
}

fn join(head: String, body: Option<Exported>) -> Exported {
    let mut bytes = head.into_bytes();
    let mut binary = false;
    if let Some(body) = body.filter(|body| !body.bytes.is_empty()) {
        bytes.push(b'\n');
        bytes.extend(body.bytes);
        binary = body.binary;
    }
    Exported { bytes, binary }
}

fn text(text: String) -> Exported {
    Exported {
        bytes: text.into_bytes(),
        binary: false,
    }
}

fn request_body(entry: &Entry, decoded: bool, options: BodyOptions) -> Option<Exported> {
    let post_data = entry.request.post_data.as_ref()?;
    if !decoded {
        return Some(text(post_data.text.clone()));
    }

    // A decoded form body is its decoded fields, e.g. a SAMLResponse as its XML assertion
    let fields: Vec<(&str, &str)> = match &post_data.params {
        Some(params) if !params.is_empty() => params
            .iter()
            .map(|p| (p.name.as_str(), p.value.as_str()))
            .collect(),
        _ if body::is_form_urlencoded(&post_data.mime_type) => body::form_fields(&post_data.text),
        _ => Vec::new(),
    };
    if !fields.is_empty() {
        return Some(text(decoded_fields(&fields, options)));
    }

    let request_body = Body {
        text: Some(&post_data.text),
        encoding: None,
        mime_type: Some(&post_data.mime_type),
    };
    Some(from_processed(body::process(&request_body, options)).unwrap_or_else(|| text(String::new())))
}

/// `name: value` per field; multi-line values (pretty printed XML / JSON) go below the name
fn decoded_fields(fields: &[(&str, &str)], options: BodyOptions) -> String {
    fields
        .iter()
        .map(|(name, value)| {
            let name = body::percent_decode(name, true);
            let value = body::decode_field(value, options).text;
            if value.contains('\n') {
                format!("{}:\n{}\n", name, value)
            } else {
                format!("{}: {}\n", name, value)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn response_body(entry: &Entry, decoded: bool, options: BodyOptions) -> Result<Exported, String> {
    let content = &entry.response.content;
    let Some(raw) = content.text.as_deref() else {
        return Err("response body not captured in HAR".to_string());
    };
    if !decoded {
        return Ok(text(raw.to_string()));
    }
    let response_body = Body {
        text: Some(raw),
        encoding: content.encoding.as_deref(),
        mime_type: content.mime_type.as_deref(),
    };
    match body::process(&response_body, options) {
        Processed::InvalidEncoding { encoding, error } => {
            Err(format!("could not decode {} body: {}", encoding, error))
        }
        processed => Ok(from_processed(processed).unwrap_or_else(|| text(String::new()))),
    }
}

fn from_processed(processed: Processed) -> Option<Exported> {
    match processed {
        Processed::Text(content) => Some(text(content.into_owned())),
        Processed::Binary(bytes) => Some(Exported {
            bytes,
            binary: true,
        }),
        Processed::Empty => Some(text(String::new())),
        _ => None,
    }
}

/// e.g. `exports/0012-api.example.com-users-response-body.json`; `number` is 1-based
pub fn default_path(number: usize, entry: &Entry, target: ExportTarget, decoded: bool) -> String {
    let url = &entry.request.url;
    let host = url.host_str().unwrap_or("unknown-host");
    let last_segment = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .unwrap_or("root");
    // The extension comes from the file name only if the MIME type doesn't say
    let stem = last_segment
        .rsplit_once('.')
        .map_or(last_segment, |(stem, _)| stem);

    let extension = match target {
        ExportTarget::FullRequest | ExportTarget::FullResponse => "http",
        ExportTarget::RequestBody => entry
            .request
            .post_data
            .as_ref()
            .map_or("txt", |post_data| extension_for(Some(&post_data.mime_type), last_segment, decoded)),
        ExportTarget::ResponseBody => {
            let base64 = entry.response.content.encoding.as_deref() == Some("base64");
            if base64 && !decoded {
                "b64"
            } else {
                extension_for(entry.response.content.mime_type.as_deref(), last_segment, decoded)
            }
        }
    };

    let name = format!(
        "{:04}-{}-{}-{}",
        number,
        sanitize(host),
        sanitize(stem),
        target.file_suffix()
    );
    let name: String = name.chars().take(120).collect();
    format!("{}/{}.{}", EXPORT_DIR, name, extension)
}

fn extension_for(mime_type: Option<&str>, file_name: &str, decoded: bool) -> &'static str {
    let essence = mime_type
        .and_then(|mime| mime.split(';').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let from_mime = match essence.as_str() {
        "application/json" | "text/json" => Some("json"),
        "text/html" | "application/xhtml+xml" => Some("html"),
        "application/xml" | "text/xml" => Some("xml"),
        "image/svg+xml" => Some("svg"),
        "text/css" => Some("css"),
        "application/javascript" | "text/javascript" | "application/x-javascript" => Some("js"),
        // A decoded form body is exported as readable fields, not as form encoding
        "application/x-www-form-urlencoded" if decoded => Some("txt"),
        "application/x-www-form-urlencoded" => Some("form"),
        "text/plain" => Some("txt"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        "font/woff2" => Some("woff2"),
        "font/woff" => Some("woff"),
        "font/ttf" | "application/x-font-ttf" => Some("ttf"),
        "application/pdf" => Some("pdf"),
        other if other.ends_with("+json") => Some("json"),
        other if other.ends_with("+xml") => Some("xml"),
        _ => None,
    };
    from_mime
        .or_else(|| match file_name.rsplit_once('.') {
            Some((_, "js")) => Some("js"),
            Some((_, "css")) => Some("css"),
            Some((_, "html")) => Some("html"),
            Some((_, "json")) => Some("json"),
            _ => None,
        })
        .unwrap_or("txt")
}

/// Keeps letters, digits, `.`, `_` and `-`; anything else becomes `_`
fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Writes `bytes` to `path`, creating parent directories. Readable by the owner only, since
/// exports often contain cookies and tokens. Refuses to replace an existing file unless
/// `overwrite`
pub fn write_file(path: &Path, bytes: &[u8], overwrite: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

/// The OSC 52 escape sequence that asks the terminal to put `bytes` on the system clipboard
pub fn osc52(exported: &Exported) -> Result<String, String> {
    if exported.binary {
        return Err("binary content can't be copied to the clipboard; save it to a file".to_string());
    }
    if exported.bytes.len() > MAX_CLIPBOARD_BYTES {
        return Err(format!(
            "too large for the clipboard ({}KB, limit {}KB); save it to a file",
            exported.bytes.len().div_ceil(1024),
            MAX_CLIPBOARD_BYTES / 1024
        ));
    }
    Ok(format!("\x1b]52;c;{}\x07", STANDARD.encode(&exported.bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::har::Har;

    fn entry(json: &str) -> Entry {
        let har: Har = serde_json::from_str(&format!(r#"{{"log": {{"entries": [{}]}}}}"#, json))
            .expect("valid entry");
        har.log.entries.into_iter().next().expect("one entry")
    }

    const LOGIN: &str = r#"{
        "startedDateTime": "2026-10-17T10:00:00Z",
        "request": {
            "method": "POST", "url": "https://api.example.com/v2/login?next=%2Fhome", "httpVersion": "HTTP/1.1",
            "headers": [{"name": "Host", "value": "api.example.com"}, {"name": "Content-Type", "value": "application/json"}],
            "postData": {"mimeType": "application/json", "text": "{\"user\":\"alice\"}"}
        },
        "response": {
            "status": 401, "statusText": "Unauthorized", "httpVersion": "HTTP/1.1",
            "headers": [{"name": "Content-Type", "value": "application/json"}],
            "content": {"mimeType": "application/json", "text": "{\"error\":\"denied\"}"}
        }
    }"#;

    const SAML: &str = r#"{
        "startedDateTime": "2026-10-17T10:00:00Z",
        "request": {
            "method": "POST", "url": "https://sp.example.com/acs",
            "postData": {"mimeType": "application/x-www-form-urlencoded",
                         "text": "SAMLResponse=PGE%2BPGI%2BeDwvYj48L2E%2B&RelayState=abc"}
        },
        "response": {"status": 302, "content": {}}
    }"#;

    const IMAGE: &str = r#"{
        "startedDateTime": "2026-10-17T10:00:00Z",
        "request": {"method": "GET", "url": "https://cdn.example.com/img/logo.png"},
        "response": {"status": 200, "httpVersion": "HTTP/2",
                     "content": {"mimeType": "image/png", "encoding": "base64", "text": "iVBORw0KGgo="}}
    }"#;

    fn exported_text(json: &str, target: ExportTarget, decoded: bool) -> String {
        let exported = build(&entry(json), target, decoded, true).expect("exported");
        String::from_utf8(exported.bytes).expect("utf-8")
    }

    #[test]
    fn build_full_request_formats_raw_http() {
        let result = exported_text(LOGIN, ExportTarget::FullRequest, false);

        assert_eq!(
            result,
            "POST /v2/login?next=%2Fhome HTTP/1.1\nHost: api.example.com\nContent-Type: application/json\n\n{\"user\":\"alice\"}"
        );
    }

    #[test]
    fn build_full_response_decoded_pretty_prints_body() {
        let result = exported_text(LOGIN, ExportTarget::FullResponse, true);

        assert_eq!(
            result,
            "HTTP/1.1 401 Unauthorized\nContent-Type: application/json\n\n{\n  \"error\": \"denied\"\n}"
        );
    }

    #[test]
    fn build_full_response_without_body_exports_head_only() {
        let result = exported_text(SAML, ExportTarget::FullResponse, true);

        assert_eq!(result, "302\n");
    }

    #[test]
    fn build_request_body_raw_is_exactly_as_stored() {
        let result = exported_text(SAML, ExportTarget::RequestBody, false);

        assert_eq!(result, "SAMLResponse=PGE%2BPGI%2BeDwvYj48L2E%2B&RelayState=abc");
    }

    #[test]
    fn build_request_body_decoded_form_exports_decoded_fields() {
        let result = exported_text(SAML, ExportTarget::RequestBody, true);

        assert_eq!(result, "SAMLResponse:\n<a>\n  <b>x</b>\n</a>\n\nRelayState: abc\n");
    }

    #[test]
    fn build_request_body_missing_is_an_error() {
        let result = build(&entry(IMAGE), ExportTarget::RequestBody, true, true);

        assert_eq!(result, Err("no request body".to_string()));
    }

    #[test]
    fn build_empty_body_is_an_error_but_full_response_is_not() {
        let empty = r#"{"startedDateTime": "x", "request": {"method": "GET", "url": "https://a.example/"},
                        "response": {"status": 204, "statusText": "No Content", "content": {"text": ""}}}"#;

        let results = (
            build(&entry(empty), ExportTarget::ResponseBody, true, true),
            build(&entry(empty), ExportTarget::FullResponse, true, true).map(|e| e.bytes),
        );

        assert_eq!(
            results,
            (
                Err("the response body is empty".to_string()),
                Ok(b"204 No Content\n".to_vec())
            )
        );
    }

    #[test]
    fn build_response_body_not_captured_is_an_error() {
        let result = build(&entry(SAML), ExportTarget::ResponseBody, true, true);

        assert_eq!(result, Err("response body not captured in HAR".to_string()));
    }

    #[test]
    fn build_response_body_decoded_binary_is_raw_bytes() {
        let result = build(&entry(IMAGE), ExportTarget::ResponseBody, true, true);

        assert_eq!(
            result,
            Ok(Exported {
                bytes: vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
                binary: true
            })
        );
    }

    #[test]
    fn build_response_body_as_stored_keeps_base64() {
        let result = exported_text(IMAGE, ExportTarget::ResponseBody, false);

        assert_eq!(result, "iVBORw0KGgo=");
    }

    #[test]
    fn build_full_response_with_binary_body_is_binary() {
        let result = build(&entry(IMAGE), ExportTarget::FullResponse, true, true).expect("exported");

        assert_eq!(
            (result.binary, result.bytes.starts_with(b"HTTP/2 200\n\n\x89PNG")),
            (true, true)
        );
    }

    #[test]
    fn default_path_uses_number_host_last_segment_and_mime_extension() {
        let path = default_path(12, &entry(LOGIN), ExportTarget::ResponseBody, true);

        assert_eq!(path, "exports/0012-api.example.com-login-response-body.json");
    }

    #[test]
    fn default_path_full_exports_use_http_extension() {
        let path = default_path(3, &entry(LOGIN), ExportTarget::FullRequest, true);

        assert_eq!(path, "exports/0003-api.example.com-login-request.http");
    }

    #[test]
    fn default_path_image_decoded_and_as_stored() {
        let paths = (
            default_path(1, &entry(IMAGE), ExportTarget::ResponseBody, true),
            default_path(1, &entry(IMAGE), ExportTarget::ResponseBody, false),
        );

        assert_eq!(
            paths,
            (
                "exports/0001-cdn.example.com-logo-response-body.png".to_string(),
                "exports/0001-cdn.example.com-logo-response-body.b64".to_string()
            )
        );
    }

    #[test]
    fn default_path_decoded_form_body_is_text() {
        let path = default_path(7, &entry(SAML), ExportTarget::RequestBody, true);

        assert_eq!(path, "exports/0007-sp.example.com-acs-request-body.txt");
    }

    #[test]
    fn default_path_root_url_and_odd_characters_are_sanitised() {
        let odd = r#"{"startedDateTime": "x", "request": {"method": "GET", "url": "https://example.com/a%20b/"},
                      "response": {"status": 200, "content": {"mimeType": "text/html"}}}"#;
        let root = r#"{"startedDateTime": "x", "request": {"method": "GET", "url": "https://example.com/"},
                       "response": {"status": 200, "content": {"mimeType": "text/html"}}}"#;

        let paths = (
            default_path(1, &entry(odd), ExportTarget::ResponseBody, true),
            default_path(2, &entry(root), ExportTarget::ResponseBody, true),
        );

        assert_eq!(
            paths,
            (
                "exports/0001-example.com-a_20b-response-body.html".to_string(),
                "exports/0002-example.com-root-response-body.html".to_string()
            )
        );
    }

    #[test]
    fn osc52_encodes_text_as_base64() {
        let exported = Exported {
            bytes: b"hello".to_vec(),
            binary: false,
        };

        assert_eq!(osc52(&exported), Ok("\x1b]52;c;aGVsbG8=\x07".to_string()));
    }

    #[test]
    fn osc52_refuses_binary_and_oversized_content() {
        let binary = Exported {
            bytes: vec![0, 1],
            binary: true,
        };
        let large = Exported {
            bytes: vec![b'a'; MAX_CLIPBOARD_BYTES + 1],
            binary: false,
        };

        assert_eq!((osc52(&binary).is_err(), osc52(&large).is_err()), (true, true));
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("harview-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn write_file_creates_directories_and_refuses_to_overwrite() {
        let dir = temp_dir("overwrite");
        let path = dir.join("nested/out.txt");

        let first = write_file(&path, b"one", false);
        let second = write_file(&path, b"two", false);
        let forced = write_file(&path, b"three", true);
        let content = std::fs::read_to_string(&path).expect("written");
        std::fs::remove_dir_all(&dir).expect("cleanup");

        assert_eq!(
            (first.is_ok(), second.is_err(), forced.is_ok(), content.as_str()),
            (true, true, true, "three")
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_file_is_readable_by_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("mode");
        let path = dir.join("secret.txt");

        write_file(&path, b"token", false).expect("written");
        let mode = std::fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        std::fs::remove_dir_all(&dir).expect("cleanup");

        assert_eq!(mode, 0o600);
    }
}
