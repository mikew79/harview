use serde::{Deserialize, Deserializer};
use std::path::Path;
use url::Url;

impl Har {
    pub async fn from_file(path: &Path) -> anyhow::Result<Self> {
        let bytes = tokio::fs::read(path).await?;
        let har = serde_json::from_slice(&bytes)?;

        Ok(har)
    }
}

/// HAR uses -1 for "size not available"
fn unknown_size() -> i64 {
    -1
}

/// Some exporters (e.g. Chrome) write `null` instead of leaving a field out
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

fn null_as_unknown_size<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(|size| size.unwrap_or_else(unknown_size))
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Url::parse(&s).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Har {
    pub log: Log,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    pub version: Option<String>,
    pub creator: Option<Creator>,
    pub browser: Option<Browser>,
    pub pages: Option<Vec<Page>>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Browser {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub id: String,
    pub page_timings: PageTimings,
    pub started_date_time: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTimings {
    /// `null` or missing when the page hadn't finished loading at export time
    #[serde(default)]
    pub on_content_load: Option<f64>,
    #[serde(default)]
    pub on_load: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub started_date_time: String,
    pub request: Request,
    pub response: Response,
    #[serde(default, deserialize_with = "null_as_default")]
    pub cache: Cache,
    #[serde(default, deserialize_with = "null_as_default")]
    pub timings: Timings,
    #[serde(default, deserialize_with = "null_as_default")]
    pub time: f64,
    #[serde(rename = "_securityState")]
    pub security_state: Option<String>,
    pub pageref: Option<String>,
    #[serde(rename = "serverIPAddress")]
    pub server_ipaddress: Option<String>,
    pub connection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    #[serde(default = "unknown_size", deserialize_with = "null_as_unknown_size")]
    pub body_size: i64,
    pub method: String,
    #[serde(deserialize_with = "deserialize_url")]
    pub url: url::Url,
    #[serde(default, deserialize_with = "null_as_default")]
    pub http_version: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub headers: Vec<Header>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub cookies: Vec<Cookie>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub query_string: Vec<QueryString>,
    #[serde(default = "unknown_size", deserialize_with = "null_as_unknown_size")]
    pub headers_size: i64,
    pub post_data: Option<PostData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryString {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostData {
    pub mime_type: String,
    pub params: Option<Vec<Param>>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub status: i64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub status_text: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub http_version: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub headers: Vec<Header>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub cookies: Vec<Cookie>,
    pub content: Content,
    #[serde(rename = "redirectURL", default, deserialize_with = "null_as_default")]
    pub redirect_url: String,
    #[serde(default = "unknown_size", deserialize_with = "null_as_unknown_size")]
    pub headers_size: i64,
    #[serde(default = "unknown_size", deserialize_with = "null_as_unknown_size")]
    pub body_size: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub text: Option<String>,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cache {}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    pub blocked: Option<f64>,
    pub dns: Option<f64>,
    pub ssl: Option<f64>,
    pub connect: Option<f64>,
    pub send: Option<f64>,
    pub wait: Option<f64>,
    pub receive: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[tokio::test]
    async fn from_file_sample_loads_all_entries() {
        let har = Har::from_file(&fixture("sample.har")).await.expect("sample parses");

        assert_eq!(har.log.entries.len(), 11);
    }

    #[tokio::test]
    async fn from_file_fractional_page_timings_parses() {
        let har = Har::from_file(&fixture("sample.har")).await.expect("sample parses");

        let timings = &har.log.pages.expect("pages present")[0].page_timings;
        assert_eq!((timings.on_content_load, timings.on_load), (Some(123.45), Some(234.5)));
    }

    #[tokio::test]
    async fn from_file_missing_optional_fields_uses_defaults() {
        let har = Har::from_file(&fixture("minimal.har")).await.expect("minimal parses");

        let entry = &har.log.entries[0];
        assert_eq!(
            (
                entry.request.headers.len(),
                entry.request.body_size,
                entry.response.redirect_url.as_str(),
                entry.response.content.text.is_none(),
            ),
            (0, -1, "", true)
        );
    }

    #[tokio::test]
    async fn from_file_null_page_on_load_parses_as_none() {
        let har = Har::from_file(&fixture("nulls.har")).await.expect("nulls parses");

        let timings = &har.log.pages.expect("pages present")[0].page_timings;
        assert_eq!((timings.on_content_load, timings.on_load), (Some(1287.2), None));
    }

    #[tokio::test]
    async fn from_file_null_optional_fields_use_defaults() {
        let har = Har::from_file(&fixture("nulls.har")).await.expect("nulls parses");

        let entry = &har.log.entries[0];
        assert_eq!(
            (
                entry.time,
                entry.request.headers.len(),
                entry.request.body_size,
                entry.response.status_text.as_str(),
                entry.response.redirect_url.as_str(),
                entry.response.body_size,
            ),
            (0.0, 0, -1, "", "", -1)
        );
    }

    #[tokio::test]
    async fn from_file_empty_har_has_no_entries() {
        let har = Har::from_file(&fixture("empty.har")).await.expect("empty parses");

        assert!(har.log.entries.is_empty());
    }

    #[tokio::test]
    async fn from_file_missing_file_returns_error() {
        let result = Har::from_file(&fixture("does-not-exist.har")).await;

        assert!(result.is_err());
    }
}
