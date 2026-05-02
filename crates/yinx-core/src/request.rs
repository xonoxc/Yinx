use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use url::Url as StdUrl;

#[derive(Error, Debug, PartialEq)]
pub enum RequestError {
    #[error("URL is required")]
    MissingUrl,
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Invalid header name: {0}")]
    InvalidHeaderName(String),
    #[error("Invalid timeout: {0}")]
    InvalidTimeout(String),
    #[error("Content-Type mismatch: body type does not match header")]
    ContentTypeMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }

    pub fn all() -> &'static [Method] {
        &[
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Patch,
            Method::Delete,
            Method::Head,
            Method::Options,
        ]
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Method {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Method::Get),
            "POST" => Ok(Method::Post),
            "PUT" => Ok(Method::Put),
            "PATCH" => Ok(Method::Patch),
            "DELETE" => Ok(Method::Delete),
            "HEAD" => Ok(Method::Head),
            "OPTIONS" => Ok(Method::Options),
            _ => Err(format!("Unknown HTTP method: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl Header {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, RequestError> {
        let name = name.into();
        let value = value.into();
        Self::validate_name(&name)?;
        Ok(Self { name, value })
    }

    fn validate_name(name: &str) -> Result<(), RequestError> {
        if name.is_empty() {
            return Err(RequestError::InvalidHeaderName(name.to_string()));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(RequestError::InvalidHeaderName(name.to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Headers {
    headers: Vec<Header>,
}

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, header: Header) {
        self.headers.push(header);
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), RequestError> {
        self.remove(name);
        let header = Header::new(name, value)?;
        self.headers.push(header);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) {
        self.headers.retain(|h| !h.name.eq_ignore_ascii_case(name));
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case(name))
    }

    pub fn dedup(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.headers.retain(|h| {
            let lower = h.name.to_lowercase();
            seen.insert(lower)
        });
    }

    pub fn iter(&self) -> impl Iterator<Item = &Header> {
        self.headers.iter()
    }

    pub fn len(&self) -> usize {
        self.headers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    pub fn to_pairs(&self) -> Vec<(&str, &str)> {
        self.headers
            .iter()
            .map(|h| (h.name.as_str(), h.value.as_str()))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum RequestBody {
    Raw(String),
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Multipart(Vec<(String, String)>),
    Binary(Vec<u8>),
    #[default]
    None,
}

impl RequestBody {
    pub fn content_type(&self) -> Option<&'static str> {
        match self {
            RequestBody::Raw(_) => Some("text/plain"),
            RequestBody::Json(_) => Some("application/json"),
            RequestBody::Form(_) => Some("application/x-www-form-urlencoded"),
            RequestBody::Multipart(_) => Some("multipart/form-data"),
            RequestBody::Binary(_) => Some("application/octet-stream"),
            RequestBody::None => None,
        }
    }

    pub fn size(&self) -> usize {
        match self {
            RequestBody::Raw(s) => s.len(),
            RequestBody::Json(v) => serde_json::to_string(v).map(|s| s.len()).unwrap_or(0),
            RequestBody::Form(pairs) => pairs.iter().map(|(k, v)| k.len() + v.len() + 1).sum(),
            RequestBody::Multipart(pairs) => pairs.iter().map(|(k, v)| k.len() + v.len() + 1).sum(),
            RequestBody::Binary(b) => b.len(),
            RequestBody::None => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, RequestBody::None)
    }
}

impl From<&str> for RequestBody {
    fn from(s: &str) -> Self {
        RequestBody::Raw(s.to_string())
    }
}

impl From<String> for RequestBody {
    fn from(s: String) -> Self {
        RequestBody::Raw(s)
    }
}

impl From<serde_json::Value> for RequestBody {
    fn from(v: serde_json::Value) -> Self {
        RequestBody::Json(v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestUrl {
    inner: StdUrl,
}

impl RequestUrl {
    pub fn new(url: &str) -> Result<Self, RequestError> {
        let inner = StdUrl::parse(url).map_err(|e| RequestError::InvalidUrl(e.to_string()))?;
        match inner.scheme() {
            "http" | "https" => Ok(Self { inner }),
            _ => Err(RequestError::InvalidUrl(format!(
                "Unsupported scheme: {}",
                inner.scheme()
            ))),
        }
    }

    pub fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    pub fn inner(&self) -> &StdUrl {
        &self.inner
    }

    pub fn normalize(&self) -> Self {
        let url = self.inner.clone();
        Self { inner: url }
    }
}

impl fmt::Display for RequestUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub method: Method,
    pub url: RequestUrl,
    pub headers: Headers,
    pub body: RequestBody,
    pub timeout_secs: u64,
}

impl Request {
    pub fn validate(&self) -> Result<(), RequestError> {
        if self.url.as_str().is_empty() {
            return Err(RequestError::MissingUrl);
        }
        self.validate_content_type()?;
        if self.timeout_secs == 0 {
            return Err(RequestError::InvalidTimeout(
                "Timeout must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    fn validate_content_type(&self) -> Result<(), RequestError> {
        if let Some(content_type) = self.headers.get("content-type") {
            let body_type = self.body.content_type();
            if let Some(body_type) = body_type {
                if !content_type
                    .to_lowercase()
                    .starts_with(&body_type.to_lowercase())
                {
                    return Err(RequestError::ContentTypeMismatch);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RequestBuilder {
    method: Method,
    url: Option<String>,
    headers: Headers,
    body: RequestBody,
    timeout_secs: u64,
}

impl Default for RequestBuilder {
    fn default() -> Self {
        Self {
            method: Method::default(),
            url: None,
            headers: Headers::new(),
            body: RequestBody::default(),
            timeout_secs: 30,
        }
    }
}

impl RequestBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        let _ = self.headers.set(name, value);
        self
    }

    pub fn headers(mut self, headers: Headers) -> Self {
        self.headers = headers;
        self
    }

    pub fn body(mut self, body: RequestBody) -> Self {
        self.body = body;
        self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn build(self) -> Result<Request, RequestError> {
        let url = self.url.ok_or(RequestError::MissingUrl)?;
        let request_url = RequestUrl::new(&url)?;

        let request = Request {
            method: self.method,
            url: request_url,
            headers: self.headers,
            body: self.body,
            timeout_secs: self.timeout_secs,
        };

        request.validate()?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_display() {
        assert_eq!(Method::Get.to_string(), "GET");
        assert_eq!(Method::Post.to_string(), "POST");
        assert_eq!(Method::Put.to_string(), "PUT");
        assert_eq!(Method::Patch.to_string(), "PATCH");
        assert_eq!(Method::Delete.to_string(), "DELETE");
        assert_eq!(Method::Head.to_string(), "HEAD");
        assert_eq!(Method::Options.to_string(), "OPTIONS");
    }

    #[test]
    fn test_method_from_str() {
        assert_eq!("get".parse::<Method>().unwrap(), Method::Get);
        assert_eq!("POST".parse::<Method>().unwrap(), Method::Post);
        assert_eq!("pUt".parse::<Method>().unwrap(), Method::Put);
        assert!("INVALID".parse::<Method>().is_err());
    }

    #[test]
    fn test_method_serde_roundtrip() {
        let method = Method::Post;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, "\"POST\"");
        let decoded: Method = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, Method::Post);
    }

    #[test]
    fn test_method_default() {
        assert_eq!(Method::default(), Method::Get);
    }

    #[test]
    fn test_method_all() {
        let all = Method::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&Method::Get));
        assert!(all.contains(&Method::Post));
    }

    #[test]
    fn test_method_as_str() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Delete.as_str(), "DELETE");
    }

    #[test]
    fn test_header_new_valid() {
        let header = Header::new("Content-Type", "application/json").unwrap();
        assert_eq!(header.name, "Content-Type");
        assert_eq!(header.value, "application/json");
    }

    #[test]
    fn test_header_rejects_empty_name() {
        let result = Header::new("", "value");
        assert_eq!(
            result.unwrap_err(),
            RequestError::InvalidHeaderName("".to_string())
        );
    }

    #[test]
    fn test_header_rejects_invalid_chars() {
        let result = Header::new("Invalid Header!", "value");
        assert!(result.is_err());
    }

    #[test]
    fn test_header_allows_hyphen_and_underscore() {
        assert!(Header::new("X-Custom-Header", "val").is_ok());
        assert!(Header::new("my_header", "val").is_ok());
    }

    #[test]
    fn test_headers_add_and_get() {
        let mut headers = Headers::new();
        headers.add(Header::new("Accept", "application/json").unwrap());
        assert_eq!(headers.get("Accept"), Some("application/json"));
    }

    #[test]
    fn test_headers_case_insensitive_lookup() {
        let mut headers = Headers::new();
        headers.add(Header::new("Content-Type", "text/html").unwrap());
        assert_eq!(headers.get("content-type"), Some("text/html"));
        assert_eq!(headers.get("CONTENT-TYPE"), Some("text/html"));
    }

    #[test]
    fn test_headers_set_overwrites() {
        let mut headers = Headers::new();
        headers.set("Accept", "text/html").unwrap();
        headers.set("Accept", "application/json").unwrap();
        assert_eq!(headers.get("Accept"), Some("application/json"));
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn test_headers_remove() {
        let mut headers = Headers::new();
        headers.add(Header::new("Accept", "text/html").unwrap());
        headers.add(Header::new("Content-Type", "application/json").unwrap());
        headers.remove("Accept");
        assert!(headers.get("Accept").is_none());
        assert_eq!(headers.len(), 1);
    }

    #[test]
    fn test_headers_contains() {
        let mut headers = Headers::new();
        headers.add(Header::new("X-Custom", "value").unwrap());
        assert!(headers.contains("X-Custom"));
        assert!(headers.contains("x-custom"));
        assert!(!headers.contains("X-Other"));
    }

    #[test]
    fn test_headers_dedup() {
        let mut headers = Headers::new();
        headers.add(Header::new("Accept", "text/html").unwrap());
        headers.add(Header::new("Accept", "application/json").unwrap());
        headers.dedup();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers.get("Accept"), Some("text/html"));
    }

    #[test]
    fn test_headers_iter() {
        let mut headers = Headers::new();
        headers.add(Header::new("A", "1").unwrap());
        headers.add(Header::new("B", "2").unwrap());
        let collected: Vec<_> = headers.iter().collect();
        assert_eq!(collected.len(), 2);
    }

    #[test]
    fn test_headers_is_empty() {
        let headers = Headers::new();
        assert!(headers.is_empty());
        let mut headers = Headers::new();
        headers.add(Header::new("A", "1").unwrap());
        assert!(!headers.is_empty());
    }

    #[test]
    fn test_headers_to_pairs() {
        let mut headers = Headers::new();
        headers.add(Header::new("Accept", "application/json").unwrap());
        headers.add(Header::new("Content-Type", "text/plain").unwrap());
        let pairs = headers.to_pairs();
        assert_eq!(
            pairs,
            vec![
                ("Accept", "application/json"),
                ("Content-Type", "text/plain")
            ]
        );
    }

    #[test]
    fn test_request_body_content_type() {
        assert_eq!(
            RequestBody::Raw("test".to_string()).content_type(),
            Some("text/plain")
        );
        assert_eq!(
            RequestBody::Json(serde_json::json!({})).content_type(),
            Some("application/json")
        );
        assert_eq!(RequestBody::None.content_type(), None);
        assert_eq!(
            RequestBody::Binary(vec![0, 1]).content_type(),
            Some("application/octet-stream")
        );
    }

    #[test]
    fn test_request_body_size() {
        assert_eq!(RequestBody::Raw("hello".to_string()).size(), 5);
        assert_eq!(RequestBody::None.size(), 0);
        assert_eq!(RequestBody::Binary(vec![0, 1, 2]).size(), 3);
    }

    #[test]
    fn test_request_body_is_empty() {
        assert!(RequestBody::None.is_empty());
        assert!(!RequestBody::Raw("".to_string()).is_empty());
    }

    #[test]
    fn test_request_body_serde_raw() {
        let body = RequestBody::Raw("hello".to_string());
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_request_body_serde_json() {
        let body = RequestBody::Json(serde_json::json!({"key": "value"}));
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_request_body_serde_form() {
        let body = RequestBody::Form(vec![("name".to_string(), "value".to_string())]);
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_request_body_serde_none() {
        let body = RequestBody::None;
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_request_url_valid() {
        let url = RequestUrl::new("https://example.com/path").unwrap();
        assert_eq!(url.as_str(), "https://example.com/path");
    }

    #[test]
    fn test_request_url_rejects_invalid() {
        assert!(RequestUrl::new("not-a-url").is_err());
        assert!(RequestUrl::new("").is_err());
    }

    #[test]
    fn test_request_url_rejects_unsupported_scheme() {
        assert!(RequestUrl::new("ftp://example.com").is_err());
        assert!(RequestUrl::new("file:///tmp/test").is_err());
    }

    #[test]
    fn test_request_url_allows_http_and_https() {
        assert!(RequestUrl::new("http://example.com").is_ok());
        assert!(RequestUrl::new("https://example.com").is_ok());
    }

    #[test]
    fn test_request_url_display() {
        let url = RequestUrl::new("https://example.com/api").unwrap();
        assert_eq!(url.to_string(), "https://example.com/api");
    }

    #[test]
    fn test_request_url_normalize() {
        let url = RequestUrl::new("https://example.com/").unwrap();
        let normalized = url.normalize();
        assert_eq!(normalized.as_str(), "https://example.com/");
    }

    #[test]
    fn test_request_builder_minimal() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url.as_str(), "https://example.com/");
        assert_eq!(request.body, RequestBody::None);
        assert_eq!(request.timeout_secs, 30);
    }

    #[test]
    fn test_request_builder_full() {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://api.example.com/v1/users")
            .header("Content-Type", "application/json")
            .header("Authorization", "Bearer token123")
            .body(RequestBody::Json(serde_json::json!({"name": "test"})))
            .timeout_secs(60)
            .build()
            .unwrap();
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.url.as_str(), "https://api.example.com/v1/users");
        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.timeout_secs, 60);
    }

    #[test]
    fn test_request_builder_missing_url() {
        let result = RequestBuilder::new().build();
        assert_eq!(result.unwrap_err(), RequestError::MissingUrl);
    }

    #[test]
    fn test_request_builder_invalid_url() {
        let result = RequestBuilder::new().url("not-a-url").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_request_builder_invalid_timeout() {
        let result = RequestBuilder::new()
            .url("https://example.com")
            .timeout_secs(0)
            .build();
        assert_eq!(
            result.unwrap_err(),
            RequestError::InvalidTimeout("Timeout must be greater than 0".to_string())
        );
    }

    #[test]
    fn test_request_validation_url_required() {
        let url = RequestUrl::new("https://example.com").unwrap();
        let request = Request {
            method: Method::Get,
            url,
            headers: Headers::new(),
            body: RequestBody::None,
            timeout_secs: 30,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_request_validation_content_type_mismatch() {
        let url = RequestUrl::new("https://example.com").unwrap();
        let mut headers = Headers::new();
        headers.set("Content-Type", "text/html").unwrap();
        let request = Request {
            method: Method::Post,
            url,
            headers,
            body: RequestBody::Json(serde_json::json!({})),
            timeout_secs: 30,
        };
        assert_eq!(
            request.validate().unwrap_err(),
            RequestError::ContentTypeMismatch
        );
    }

    #[test]
    fn test_request_validation_matching_content_type() {
        let url = RequestUrl::new("https://example.com").unwrap();
        let mut headers = Headers::new();
        headers.set("Content-Type", "application/json").unwrap();
        let request = Request {
            method: Method::Post,
            url,
            headers,
            body: RequestBody::Json(serde_json::json!({})),
            timeout_secs: 30,
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn test_request_serde_roundtrip() {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://example.com/api")
            .header("Accept", "application/json")
            .body(RequestBody::Raw("test body".to_string()))
            .timeout_secs(45)
            .build()
            .unwrap();
        let json = serde_json::to_string(&request).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.method, request.method);
        assert_eq!(decoded.url.as_str(), request.url.as_str());
        assert_eq!(decoded.timeout_secs, request.timeout_secs);
    }

    #[test]
    fn test_request_body_from_str() {
        let body: RequestBody = "hello".into();
        assert_eq!(body, RequestBody::Raw("hello".to_string()));
    }

    #[test]
    fn test_request_body_from_string() {
        let body: RequestBody = String::from("world").into();
        assert_eq!(body, RequestBody::Raw("world".to_string()));
    }

    #[test]
    fn test_request_body_from_json_value() {
        let body: RequestBody = serde_json::json!({"a": 1}).into();
        assert_eq!(body, RequestBody::Json(serde_json::json!({"a": 1})));
    }

    #[test]
    fn test_request_body_multipart_serde() {
        let body = RequestBody::Multipart(vec![("file".to_string(), "data".to_string())]);
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_request_body_binary_serde() {
        let body = RequestBody::Binary(vec![1, 2, 3, 4]);
        let json = serde_json::to_string(&body).unwrap();
        let decoded: RequestBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }
}
