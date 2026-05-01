use serde::{Deserialize, Serialize};
use std::fmt;

use crate::request::Headers;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatusCode(pub u16);

impl StatusCode {
    pub fn new(code: u16) -> Self {
        Self(code)
    }

    pub fn code(&self) -> u16 {
        self.0
    }

    pub fn category(&self) -> StatusCategory {
        match self.0 {
            100..=199 => StatusCategory::Informational,
            200..=299 => StatusCategory::Success,
            300..=399 => StatusCategory::Redirection,
            400..=499 => StatusCategory::ClientError,
            500..=599 => StatusCategory::ServerError,
            _ => StatusCategory::Unknown,
        }
    }

    pub fn is_informational(&self) -> bool {
        matches!(self.category(), StatusCategory::Informational)
    }

    pub fn is_success(&self) -> bool {
        matches!(self.category(), StatusCategory::Success)
    }

    pub fn is_redirection(&self) -> bool {
        matches!(self.category(), StatusCategory::Redirection)
    }

    pub fn is_client_error(&self) -> bool {
        matches!(self.category(), StatusCategory::ClientError)
    }

    pub fn is_server_error(&self) -> bool {
        matches!(self.category(), StatusCategory::ServerError)
    }

    pub fn is_error(&self) -> bool {
        self.is_client_error() || self.is_server_error()
    }

    pub fn phrase(&self) -> &'static str {
        match self.0 {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            304 => "Not Modified",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }
    }
}

impl fmt::Display for StatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.0, self.phrase())
    }
}

impl Default for StatusCode {
    fn default() -> Self {
        Self(200)
    }
}

impl From<u16> for StatusCode {
    fn from(code: u16) -> Self {
        Self(code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusCategory {
    Informational,
    Success,
    Redirection,
    ClientError,
    ServerError,
    Unknown,
}

pub type ResponseHeaders = Headers;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum ResponseBody {
    Text(String),
    Json(serde_json::Value),
    Binary(Vec<u8>),
    Stream(Vec<u8>),
    #[default]
    None,
}

impl ResponseBody {
    pub fn size(&self) -> usize {
        match self {
            ResponseBody::Text(s) => s.len(),
            ResponseBody::Json(v) => serde_json::to_string(v).map(|s| s.len()).unwrap_or(0),
            ResponseBody::Binary(b) => b.len(),
            ResponseBody::Stream(b) => b.len(),
            ResponseBody::None => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, ResponseBody::None)
    }

    pub fn as_text(&self) -> Option<String> {
        match self {
            ResponseBody::Text(s) => Some(s.clone()),
            ResponseBody::Json(v) => Some(v.to_string()),
            _ => None,
        }
    }

    pub fn pretty_json(&self) -> Option<String> {
        match self {
            ResponseBody::Json(v) => serde_json::to_string_pretty(v).ok(),
            _ => None,
        }
    }
}

impl From<&str> for ResponseBody {
    fn from(s: &str) -> Self {
        ResponseBody::Text(s.to_string())
    }
}

impl From<String> for ResponseBody {
    fn from(s: String) -> Self {
        ResponseBody::Text(s)
    }
}

impl From<serde_json::Value> for ResponseBody {
    fn from(v: serde_json::Value) -> Self {
        ResponseBody::Json(v)
    }
}

impl fmt::Display for ResponseBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseBody::Text(s) => write!(f, "{s}"),
            ResponseBody::Json(v) => write!(f, "{v}"),
            ResponseBody::Binary(b) => write!(f, "<binary {} bytes>", b.len()),
            ResponseBody::Stream(b) => write!(f, "<stream {} bytes>", b.len()),
            ResponseBody::None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub status: StatusCode,
    pub headers: ResponseHeaders,
    pub body: ResponseBody,
    pub timing_ms: u64,
}

impl Response {
    pub fn builder() -> ResponseBuilder {
        ResponseBuilder::default()
    }

    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type")
    }

    pub fn body_size(&self) -> usize {
        self.body.size()
    }
}

#[derive(Debug, Clone)]
pub struct ResponseBuilder {
    status: StatusCode,
    headers: ResponseHeaders,
    body: ResponseBody,
    timing_ms: u64,
}

impl Default for ResponseBuilder {
    fn default() -> Self {
        Self {
            status: StatusCode::default(),
            headers: ResponseHeaders::new(),
            body: ResponseBody::default(),
            timing_ms: 0,
        }
    }
}

impl ResponseBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(mut self, code: u16) -> Self {
        self.status = StatusCode::new(code);
        self
    }

    pub fn status_code(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        let _ = self.headers.set(name, value);
        self
    }

    pub fn headers(mut self, headers: ResponseHeaders) -> Self {
        self.headers = headers;
        self
    }

    pub fn body(mut self, body: ResponseBody) -> Self {
        self.body = body;
        self
    }

    pub fn timing_ms(mut self, ms: u64) -> Self {
        self.timing_ms = ms;
        self
    }

    pub fn build(self) -> Response {
        Response {
            status: self.status,
            headers: self.headers,
            body: self.body,
            timing_ms: self.timing_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_categories() {
        assert!(StatusCode::new(100).is_informational());
        assert!(StatusCode::new(200).is_success());
        assert!(StatusCode::new(301).is_redirection());
        assert!(StatusCode::new(404).is_client_error());
        assert!(StatusCode::new(500).is_server_error());
    }

    #[test]
    fn test_status_code_unknown_category() {
        let status = StatusCode::new(999);
        assert!(!status.is_informational());
        assert!(!status.is_success());
        assert!(!status.is_redirection());
        assert!(!status.is_client_error());
        assert!(!status.is_server_error());
    }

    #[test]
    fn test_status_code_is_error() {
        assert!(StatusCode::new(400).is_error());
        assert!(StatusCode::new(500).is_error());
        assert!(!StatusCode::new(200).is_error());
        assert!(!StatusCode::new(301).is_error());
    }

    #[test]
    fn test_status_code_display() {
        assert_eq!(StatusCode::new(200).to_string(), "200 OK");
        assert_eq!(StatusCode::new(404).to_string(), "404 Not Found");
        assert_eq!(StatusCode::new(500).to_string(), "500 Internal Server Error");
        assert_eq!(StatusCode::new(999).to_string(), "999 Unknown");
    }

    #[test]
    fn test_status_code_phrase() {
        assert_eq!(StatusCode::new(200).phrase(), "OK");
        assert_eq!(StatusCode::new(201).phrase(), "Created");
        assert_eq!(StatusCode::new(204).phrase(), "No Content");
        assert_eq!(StatusCode::new(401).phrase(), "Unauthorized");
        assert_eq!(StatusCode::new(403).phrase(), "Forbidden");
        assert_eq!(StatusCode::new(429).phrase(), "Too Many Requests");
    }

    #[test]
    fn test_status_code_default() {
        assert_eq!(StatusCode::default(), StatusCode::new(200));
    }

    #[test]
    fn test_status_code_from_u16() {
        let status: StatusCode = 200.into();
        assert_eq!(status, StatusCode::new(200));
    }

    #[test]
    fn test_status_code_code_accessor() {
        assert_eq!(StatusCode::new(404).code(), 404);
    }

    #[test]
    fn test_status_code_serde_roundtrip() {
        let status = StatusCode::new(201);
        let json = serde_json::to_string(&status).unwrap();
        let decoded: StatusCode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, status);
    }

    #[test]
    fn test_response_body_size() {
        assert_eq!(ResponseBody::Text("hello".to_string()).size(), 5);
        assert_eq!(ResponseBody::Binary(vec![0, 1, 2]).size(), 3);
        assert_eq!(ResponseBody::None.size(), 0);
    }

    #[test]
    fn test_response_body_is_empty() {
        assert!(ResponseBody::None.is_empty());
        assert!(!ResponseBody::Text("".to_string()).is_empty());
    }

    #[test]
    fn test_response_body_as_text() {
        let body = ResponseBody::Text("hello".to_string());
        assert_eq!(body.as_text(), Some("hello".to_string()));
    }

    #[test]
    fn test_response_body_pretty_json() {
        let body = ResponseBody::Json(serde_json::json!({"a": 1}));
        let pretty = body.pretty_json().unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"a\": 1"));
    }

    #[test]
    fn test_response_body_pretty_json_non_json_returns_none() {
        let body = ResponseBody::Text("not json".to_string());
        assert!(body.pretty_json().is_none());
    }

    #[test]
    fn test_response_body_from_str() {
        let body: ResponseBody = "hello".into();
        assert_eq!(body, ResponseBody::Text("hello".to_string()));
    }

    #[test]
    fn test_response_body_from_string() {
        let body: ResponseBody = String::from("world").into();
        assert_eq!(body, ResponseBody::Text("world".to_string()));
    }

    #[test]
    fn test_response_body_from_json_value() {
        let body: ResponseBody = serde_json::json!({"key": "value"}).into();
        assert_eq!(body, ResponseBody::Json(serde_json::json!({"key": "value"})));
    }

    #[test]
    fn test_response_body_display_text() {
        let body = ResponseBody::Text("hello".to_string());
        assert_eq!(body.to_string(), "hello");
    }

    #[test]
    fn test_response_body_display_binary() {
        let body = ResponseBody::Binary(vec![1, 2, 3]);
        assert_eq!(body.to_string(), "<binary 3 bytes>");
    }

    #[test]
    fn test_response_body_display_stream() {
        let body = ResponseBody::Stream(vec![1, 2, 3, 4]);
        assert_eq!(body.to_string(), "<stream 4 bytes>");
    }

    #[test]
    fn test_response_body_display_none() {
        let body = ResponseBody::None;
        assert_eq!(body.to_string(), "");
    }

    #[test]
    fn test_response_body_serde_text() {
        let body = ResponseBody::Text("hello".to_string());
        let json = serde_json::to_string(&body).unwrap();
        let decoded: ResponseBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_response_body_serde_json() {
        let body = ResponseBody::Json(serde_json::json!({"a": 1}));
        let json = serde_json::to_string(&body).unwrap();
        let decoded: ResponseBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_response_body_serde_binary() {
        let body = ResponseBody::Binary(vec![1, 2, 3]);
        let json = serde_json::to_string(&body).unwrap();
        let decoded: ResponseBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_response_body_serde_none() {
        let body = ResponseBody::None;
        let json = serde_json::to_string(&body).unwrap();
        let decoded: ResponseBody = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn test_response_builder_minimal() {
        let response = Response::builder().build();
        assert_eq!(response.status, StatusCode::new(200));
        assert!(response.headers.is_empty());
        assert_eq!(response.body, ResponseBody::None);
        assert_eq!(response.timing_ms, 0);
    }

    #[test]
    fn test_response_builder_full() {
        let response = Response::builder()
            .status(201)
            .header("Content-Type", "application/json")
            .body(ResponseBody::Json(serde_json::json!({"id": 1})))
            .timing_ms(150)
            .build();
        assert_eq!(response.status, StatusCode::new(201));
        assert_eq!(response.headers.len(), 1);
        assert_eq!(response.timing_ms, 150);
    }

    #[test]
    fn test_response_is_success() {
        let response = Response::builder().status(200).build();
        assert!(response.is_success());
        assert!(!response.is_error());
    }

    #[test]
    fn test_response_is_error() {
        let response = Response::builder().status(500).build();
        assert!(response.is_error());
        assert!(!response.is_success());
    }

    #[test]
    fn test_response_content_type() {
        let response = Response::builder()
            .header("Content-Type", "application/json")
            .build();
        assert_eq!(response.content_type(), Some("application/json"));
    }

    #[test]
    fn test_response_body_size_from_builder() {
        let response = Response::builder()
            .body(ResponseBody::Text("hello".to_string()))
            .build();
        assert_eq!(response.body_size(), 5);
    }

    #[test]
    fn test_response_headers_reuse() {
        let mut headers = Headers::new();
        headers.add(
            crate::request::Header::new("X-Custom", "value").unwrap(),
        );
        let response = Response::builder().headers(headers).build();
        assert_eq!(response.headers.get("X-Custom"), Some("value"));
    }

    #[test]
    fn test_response_serde_roundtrip() {
        let response = Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(ResponseBody::Json(serde_json::json!({"key": "value"})))
            .timing_ms(100)
            .build();
        let json = serde_json::to_string(&response).unwrap();
        let decoded: Response = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.status, response.status);
        assert_eq!(decoded.timing_ms, response.timing_ms);
    }
}
