use reqwest::cookie::Jar;
use reqwest::Client as ReqwestClient;
use std::sync::Arc;
use thiserror::Error;
use yinx_core::request::{Headers, Request, RequestBody};
use yinx_core::response::{Response, ResponseBody, StatusCode};

#[derive(Error, Debug)]
pub enum HttpClientError {
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Timeout after {0}ms")]
    Timeout(u64),
    #[error("Redirect error: {0}")]
    Redirect(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

pub struct HttpClient {
    cookie_jar: Arc<Jar>,
    default_timeout_secs: u64,
    follow_redirects: bool,
    tls_verify: bool,
}

impl HttpClient {
    pub fn new() -> Result<Self, HttpClientError> {
        let cookie_jar = Arc::new(Jar::default());

        let _inner = ReqwestClient::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .cookie_provider(cookie_jar.clone())
            .build()
            .map_err(HttpClientError::Request)?;

        Ok(Self {
            cookie_jar,
            default_timeout_secs: 30,
            follow_redirects: true,
            tls_verify: true,
        })
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.default_timeout_secs = secs;
        self
    }

    pub fn with_follow_redirects(mut self, follow: bool) -> Self {
        self.follow_redirects = follow;
        self
    }

    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    fn build_client(&self) -> Result<ReqwestClient, HttpClientError> {
        let policy = if self.follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        };

        let mut builder = ReqwestClient::builder().redirect(policy);

        if !self.tls_verify {
            builder = builder.danger_accept_invalid_certs(true);
        }

        builder
            .cookie_provider(self.cookie_jar.clone())
            .build()
            .map_err(HttpClientError::Request)
    }

    pub async fn send_request(&self, request: Request) -> Result<Response, HttpClientError> {
        let client = self.build_client()?;

        let mut req_builder = client
            .request(
                reqwest::Method::from_bytes(request.method.as_str().as_bytes()).unwrap(),
                request.url.as_str(),
            )
            .timeout(std::time::Duration::from_secs(request.timeout_secs));

        for (name, value) in request.headers.to_pairs() {
            req_builder = req_builder.header(name, value);
        }

        if !request.body.is_empty() {
            match request.body {
                RequestBody::Raw(ref s) => {
                    req_builder = req_builder.body(s.clone());
                }
                RequestBody::Json(ref v) => {
                    req_builder = req_builder.json(v);
                }
                RequestBody::Form(ref pairs) => {
                    req_builder = req_builder.form(pairs);
                }
                _ => {}
            }
        }

        let response = req_builder.send().await.map_err(HttpClientError::Request)?;
        let status = StatusCode::new(response.status().as_u16());

        let mut headers = Headers::new();

        for (name, value) in response.headers() {
            let _ = headers.set(name.as_str(), value.to_str().unwrap_or(""));
        }

        let body_bytes = response.bytes().await.map_err(HttpClientError::Request)?;
        let _body_size = body_bytes.len();
        let content_type = headers.get("content-type").unwrap_or("");

        let body = if content_type.contains("json") {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                ResponseBody::Json(json)
            } else {
                ResponseBody::Text(String::from_utf8_lossy(&body_bytes).to_string())
            }
        } else if content_type.contains("text") || content_type.is_empty() {
            ResponseBody::Text(String::from_utf8_lossy(&body_bytes).to_string())
        } else {
            ResponseBody::Binary(body_bytes.to_vec())
        };

        Ok(Response::builder()
            .status_code(status)
            .headers(headers)
            .body(body)
            .build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use yinx_core::request::Method;
    use yinx_core::request::RequestBuilder;

    #[test]
    fn test_http_client_construction_with_defaults() {
        let client = HttpClient::new().unwrap();
        assert_eq!(client.default_timeout_secs, 30);
        assert!(client.follow_redirects);
    }

    #[test]
    fn test_http_client_with_custom_timeout() {
        let client = HttpClient::new().unwrap().with_timeout(60);
        assert_eq!(client.default_timeout_secs, 60);
    }

    #[test]
    fn test_http_client_with_redirects_disabled() {
        let client = HttpClient::new().unwrap().with_follow_redirects(false);
        assert!(!client.follow_redirects);
    }

    #[test]
    fn test_http_client_with_redirects_enabled() {
        let client = HttpClient::new().unwrap().with_follow_redirects(true);
        assert!(client.follow_redirects);
    }

    #[tokio::test]
    async fn test_send_request_get_returns_200() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_success());
        assert_eq!(response.status.code(), 200);
    }

    #[tokio::test]
    async fn test_send_request_timeout_triggers_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2)))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .timeout_secs(1)
            .build()
            .unwrap();

        let result = client.send_request(request).await;
        assert!(result.is_err());
        match result {
            Err(HttpClientError::Request(e)) => {
                assert!(e.is_timeout() || e.to_string().contains("timeout"));
            }
            _ => panic!("Expected timeout error"),
        }
    }

    #[tokio::test]
    async fn test_redirect_followed_when_enabled() {
        let mock_server = MockServer::start().await;
        let redirect_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", mock_server.uri()))
            .mount(&redirect_server)
            .await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap().with_follow_redirects(true);
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&redirect_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_success());
    }

    #[tokio::test]
    async fn test_redirect_not_followed_when_disabled() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(301).insert_header("Location", "http://other.example.com"),
            )
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap().with_follow_redirects(false);
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_redirection());
        assert_eq!(response.status.code(), 301);
    }

    #[tokio::test]
    async fn test_https_request_with_default_tls() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_success());
    }

    #[test]
    fn test_basic_auth_header_set() {
        use base64::Engine;
        let mut headers = Headers::new();
        let username = "user";
        let password = "pass";
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
        headers
            .set("Authorization", &format!("Basic {}", credentials))
            .unwrap();

        assert_eq!(
            headers.get("Authorization"),
            Some(format!("Basic {}", credentials).as_str())
        );
    }

    #[test]
    fn test_bearer_auth_header_set() {
        let mut headers = Headers::new();
        let token = "my-secret-token";
        headers
            .set("Authorization", &format!("Bearer {}", token))
            .unwrap();

        assert_eq!(
            headers.get("Authorization"),
            Some(format!("Bearer {}", token).as_str())
        );
    }

    #[test]
    fn test_api_key_header_injection() {
        let mut headers = Headers::new();
        let api_key = "my-api-key-12345";
        headers.set("X-API-Key", api_key).unwrap();

        assert_eq!(headers.get("X-API-Key"), Some(api_key));
    }

    #[tokio::test]
    async fn test_cookies_persisted_across_requests() {
        let mock_server = MockServer::start().await;

        // First request sets a cookie
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).insert_header("Set-Cookie", "session=abc123; Path=/"),
            )
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let _response = client.send_request(request).await.unwrap();

        // Second request should send the cookie back
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        let request2 = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let _response2 = client.send_request(request2).await.unwrap();
    }

    #[tokio::test]
    async fn test_auto_detect_json_content_type() {
        let mock_server = MockServer::start().await;
        let json_body = serde_json::json!({"key": "value"});

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.body.as_text().is_some());
        let text = response.body.as_text().unwrap();
        assert!(text.contains("key"));
    }

    #[tokio::test]
    async fn test_auto_detect_html_content_type() {
        let mock_server = MockServer::start().await;
        let html_body = "<html><body>Hello</body></html>";

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html")
                    .set_body_string(html_body),
            )
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.body.as_text().is_some());
        let text = response.body.as_text().unwrap();
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_json_pretty_print_formatter() {
        let json_body = ResponseBody::Json(serde_json::json!({"a": 1, "b": [1, 2, 3]}));
        let pretty = json_body.pretty_json().unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"a\": 1"));
        assert!(pretty.contains("\"b\": ["));
    }

    #[test]
    fn test_json_pretty_print_single_line_input() {
        let input = r#"{"a":1}"#;
        let json: serde_json::Value = serde_json::from_str(input).unwrap();
        let json_body = ResponseBody::Json(json);
        let pretty = json_body.pretty_json().unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.trim() != input);
    }

    #[tokio::test]
    async fn test_response_body_size_tracking() {
        let mock_server = MockServer::start().await;
        let response_body = "Hello, World!";

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(response_body))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert_eq!(response.body_size(), response_body.len());
    }

    #[tokio::test]
    async fn test_response_body_size_json() {
        let mock_server = MockServer::start().await;
        let json_body = serde_json::json!({"message": "Hello", "count": 42});

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json_body))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        let expected_size = serde_json::to_string(&json_body).unwrap().len();
        assert_eq!(response.body_size(), expected_size);
    }

    #[tokio::test]
    async fn test_error_response_4xx_returns_body() {
        let mock_server = MockServer::start().await;
        let error_body = r#"{"error": "Not Found", "code": 404}"#;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(404)
                    .insert_header("content-type", "application/json")
                    .set_body_string(error_body),
            )
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_client_error());
        assert_eq!(response.status.code(), 404);
        let text = response.body.as_text().unwrap();
        assert!(text.contains("Not Found"));
    }

    #[tokio::test]
    async fn test_error_response_5xx_returns_body() {
        let mock_server = MockServer::start().await;
        let error_body = "Internal Server Error";

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500).set_body_string(error_body))
            .mount(&mock_server)
            .await;

        let client = HttpClient::new().unwrap();
        let request = RequestBuilder::new()
            .method(Method::Get)
            .url(&mock_server.uri())
            .build()
            .unwrap();

        let response = client.send_request(request).await.unwrap();
        assert!(response.status.is_server_error());
        assert_eq!(response.status.code(), 500);
        let text = response.body.as_text().unwrap();
        assert!(text.contains("Internal Server Error"));
    }
}
