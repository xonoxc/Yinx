use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use yinx_core::request::{
    Header, Headers, Method, Request, RequestBody, RequestUrl, RequestBuilder,
    request_to_curl, shell_escape,
};

#[derive(Debug, PartialEq)]
pub enum CurlParseError {
    InvalidMethod(String),
    MissingUrl,
    InvalidUrl(String),
    TokenizeError(String),
}

pub fn tokenize(input: &str) -> Result<Vec<String>, CurlParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices();
    let mut current = String::new();
    let mut in_quotes: Option<char> = None;
    let mut escaped = false;

    while let Some((_, c)) = chars.next() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_quotes.is_none() => {
                escaped = true;
            }
            '\\' if in_quotes == Some('"') || in_quotes == Some('\'') => {
                current.push('\\');
                if let Some((_, next)) = chars.next() {
                    current.push(next);
                }
            }
            '"' | '\'' => {
                if in_quotes == Some(c) {
                    in_quotes = None;
                } else if in_quotes.is_none() {
                    in_quotes = Some(c);
                } else {
                    current.push(c);
                }
            }
            ' ' if in_quotes.is_none() => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    Ok(tokens)
}

pub fn parse_curl(command: &str) -> Result<Request, CurlParseError> {
    let tokens = tokenize(command)?;
    let mut method = Method::Get;
    let mut url = None;
    let mut headers = Headers::new();
    let mut body = RequestBody::None;
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i].as_str() {
            "curl" => {}
            "-X" | "--request" => {
                i += 1;
                if i >= tokens.len() {
                    break;
                }
                method = tokens[i]
                    .parse::<Method>()
                    .map_err(|e| CurlParseError::InvalidMethod(e))?;
            }
            "-H" | "--header" => {
                i += 1;
                if i >= tokens.len() {
                    break;
                }
                let header_str = &tokens[i];
                if let Some((name, value)) = header_str.split_once(':') {
                    let value = value.trim();
                    let _ = headers.set(name.trim(), value);
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                i += 1;
                if i >= tokens.len() {
                    break;
                }
                let data = &tokens[i];
                if headers.get("content-type") == Some("application/json") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        body = RequestBody::Json(json);
                    } else {
                        body = RequestBody::Raw(data.clone());
                    }
                } else if data.contains('=') && data.contains('&') {
                    let pairs: Vec<(String, String)> = data
                        .split('&')
                        .filter_map(|pair| {
                            let mut parts = pair.splitn(2, '=');
                            match (parts.next(), parts.next()) {
                                (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                                _ => None,
                            }
                        })
                        .collect();
                    if !pairs.is_empty() {
                        body = RequestBody::Form(pairs);
                    } else {
                        body = RequestBody::Raw(data.clone());
                    }
                } else {
                    body = RequestBody::Raw(data.clone());
                }
            }
            "--user" => {
                i += 1;
                if i >= tokens.len() {
                    break;
                }
                let user_pass = &tokens[i];
                let encoded = BASE64.encode(user_pass.as_bytes());
                let _ = headers.set("Authorization", &format!("Basic {}", encoded));
            }
            "--cookie" => {
                i += 1;
                if i >= tokens.len() {
                    break;
                }
                let _ = headers.set("Cookie", &tokens[i]);
            }
            s if !s.starts_with('-') && url.is_none() => {
                url = Some(s.to_string());
            }
            _ => {}
        }
        i += 1;
    }

    let url_str = url.ok_or(CurlParseError::MissingUrl)?;
    let request_url =
        RequestUrl::new(&url_str).map_err(|e| CurlParseError::InvalidUrl(e.to_string()))?;

    Ok(Request {
        method,
        url: request_url,
        headers,
        body,
        timeout_secs: 30,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // 5.1: Curl command tokenizer
    #[test]
    fn test_tokenize_simple_command() {
        let result = tokenize("curl https://example.com").unwrap();
        assert_eq!(result, vec!["curl", "https://example.com"]);
    }

    #[test]
    fn test_tokenize_with_flags() {
        let result = tokenize("curl -X POST https://example.com").unwrap();
        assert_eq!(result, vec!["curl", "-X", "POST", "https://example.com"]);
    }

    #[test]
    fn test_tokenize_double_quoted_args() {
        let result =
            tokenize(r#"curl -H "Content-Type: application/json" https://example.com"#).unwrap();
        assert_eq!(
            result,
            vec![
                "curl",
                "-H",
                "Content-Type: application/json",
                "https://example.com"
            ]
        );
    }

    #[test]
    fn test_tokenize_single_quoted_args() {
        let result =
            tokenize(r#"curl -H 'Content-Type: application/json' https://example.com"#).unwrap();
        assert_eq!(
            result,
            vec![
                "curl",
                "-H",
                "Content-Type: application/json",
                "https://example.com"
            ]
        );
    }

    #[test]
    fn test_tokenize_escaped_spaces() {
        let result = tokenize("curl -d hello\\ world https://example.com").unwrap();
        assert_eq!(
            result,
            vec!["curl", "-d", "hello world", "https://example.com"]
        );
    }

    #[test]
    fn test_tokenize_complex_command() {
        let cmd = r#"curl -X POST -H "Content-Type: application/json" -H 'Accept: application/json' -d '{"key":"value"}' https://api.example.com/v1/users"#;
        let result = tokenize(cmd).unwrap();
        assert_eq!(result.len(), 10);
        assert_eq!(result[0], "curl");
        assert_eq!(result[1], "-X");
        assert_eq!(result[2], "POST");
        assert_eq!(result[3], "-H");
        assert_eq!(result[4], "Content-Type: application/json");
        assert_eq!(result[5], "-H");
        assert_eq!(result[6], "Accept: application/json");
        assert_eq!(result[7], "-d");
        assert_eq!(result[8], r#"{"key":"value"}"#);
        assert_eq!(result[9], "https://api.example.com/v1/users");
    }

    // 5.2: Parse -X/--request -> Method
    #[test]
    fn test_parse_method_get_default() {
        let request = parse_curl("curl https://example.com").unwrap();
        assert_eq!(request.method, Method::Get);
    }

    #[test]
    fn test_parse_method_post_short_flag() {
        let request = parse_curl("curl -X POST https://example.com").unwrap();
        assert_eq!(request.method, Method::Post);
    }

    #[test]
    fn test_parse_method_put_long_flag() {
        let request = parse_curl("curl --request PUT https://example.com").unwrap();
        assert_eq!(request.method, Method::Put);
    }

    #[test]
    fn test_parse_method_patch() {
        let request = parse_curl("curl -X PATCH https://example.com").unwrap();
        assert_eq!(request.method, Method::Patch);
    }

    #[test]
    fn test_parse_method_delete() {
        let request = parse_curl("curl -X DELETE https://example.com").unwrap();
        assert_eq!(request.method, Method::Delete);
    }

    #[test]
    fn test_parse_method_head() {
        let request = parse_curl("curl -X HEAD https://example.com").unwrap();
        assert_eq!(request.method, Method::Head);
    }

    #[test]
    fn test_parse_method_options() {
        let request = parse_curl("curl -X OPTIONS https://example.com").unwrap();
        assert_eq!(request.method, Method::Options);
    }

    // 5.3: Parse -H/--header -> Headers
    #[test]
    fn test_parse_single_header() {
        let request =
            parse_curl("curl -H 'Content-Type: application/json' https://example.com").unwrap();
        assert_eq!(
            request.headers.get("Content-Type"),
            Some("application/json")
        );
    }

    #[test]
    fn test_parse_multiple_headers() {
        let request = parse_curl(
            "curl -H 'Content-Type: application/json' -H 'Accept: application/json' https://example.com",
        )
        .unwrap();
        assert_eq!(
            request.headers.get("Content-Type"),
            Some("application/json")
        );
        assert_eq!(request.headers.get("Accept"), Some("application/json"));
    }

    #[test]
    fn test_parse_header_with_special_chars() {
        let request =
            parse_curl(r#"curl -H "Authorization: Bearer token123!$%^&*()" https://example.com"#)
                .unwrap();
        assert_eq!(
            request.headers.get("Authorization"),
            Some("Bearer token123!$%^&*()")
        );
    }

    #[test]
    fn test_parse_header_long_form() {
        let request =
            parse_curl("curl --header 'Content-Type: text/plain' https://example.com").unwrap();
        assert_eq!(request.headers.get("Content-Type"), Some("text/plain"));
    }

    // 5.4: Parse -d/--data -> Body
    #[test]
    fn test_parse_raw_data() {
        let request = parse_curl("curl -d 'hello world' https://example.com").unwrap();
        assert_eq!(request.body, RequestBody::Raw("hello world".to_string()));
    }

    #[test]
    fn test_parse_json_data() {
        let json = r#"{"key":"value"}"#;
        let request = parse_curl(&format!(
            "curl -H 'Content-Type: application/json' -d '{}' https://example.com",
            json
        ))
        .unwrap();
        assert_eq!(
            request.body,
            RequestBody::Json(serde_json::json!({"key":"value"}))
        );
    }

    #[test]
    fn test_parse_form_data() {
        let request = parse_curl("curl -d 'name=john&age=30' https://example.com").unwrap();
        match &request.body {
            RequestBody::Form(pairs) => {
                assert_eq!(pairs.len(), 2);
                assert_eq!(pairs[0], ("name".to_string(), "john".to_string()));
                assert_eq!(pairs[1], ("age".to_string(), "30".to_string()));
            }
            _ => panic!("Expected Form body"),
        }
    }

    #[test]
    fn test_parse_data_long_form() {
        let request = parse_curl("curl --data 'test data' https://example.com").unwrap();
        assert_eq!(request.body, RequestBody::Raw("test data".to_string()));
    }

    // 5.5: Parse URL (positional arg)
    #[test]
    fn test_parse_url_basic() {
        let request = parse_curl("curl https://example.com").unwrap();
        assert_eq!(request.url.as_str(), "https://example.com/");
    }

    #[test]
    fn test_parse_url_with_query_params() {
        let request = parse_curl("curl 'https://example.com/api?key=value&foo=bar'").unwrap();
        assert_eq!(
            request.url.as_str(),
            "https://example.com/api?key=value&foo=bar"
        );
    }

    #[test]
    fn test_parse_url_with_fragment() {
        let request = parse_curl("curl https://example.com/page#section").unwrap();
        assert_eq!(request.url.as_str(), "https://example.com/page#section");
    }

    #[test]
    fn test_parse_url_with_path() {
        let request = parse_curl("curl https://api.example.com/v1/users/123").unwrap();
        assert_eq!(request.url.as_str(), "https://api.example.com/v1/users/123");
    }

    // 5.6: Parse --user -> Basic auth
    #[test]
    fn test_parse_basic_auth() {
        let request = parse_curl("curl --user 'username:password' https://example.com").unwrap();
        let auth_header = request.headers.get("Authorization").unwrap();
        assert!(auth_header.starts_with("Basic "));
    }

    #[test]
    fn test_parse_basic_auth_generates_correct_value() {
        let request = parse_curl("curl --user 'admin:secret' https://example.com").unwrap();
        let auth_header = request.headers.get("Authorization").unwrap();
        let encoded = base64::encode("admin:secret");
        assert_eq!(auth_header, &format!("Basic {}", encoded));
    }

    // 5.7: Parse --cookie -> Cookie header
    #[test]
    fn test_parse_single_cookie() {
        let request = parse_curl("curl --cookie 'session=abc123' https://example.com").unwrap();
        assert_eq!(request.headers.get("Cookie"), Some("session=abc123"));
    }

    #[test]
    fn test_parse_multiple_cookies() {
        let request =
            parse_curl("curl --cookie 'session=abc123; user=john' https://example.com").unwrap();
        assert_eq!(
            request.headers.get("Cookie"),
            Some("session=abc123; user=john")
        );
    }

    // 5.8: Full curl integration test
    #[test]
    fn test_full_curl_post_json() {
        let cmd = r#"curl -X POST -H "Content-Type: application/json" -H "Accept: application/json" -d '{"name":"test"}' https://api.example.com/v1/users"#;
        let request = parse_curl(cmd).unwrap();
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.url.as_str(), "https://api.example.com/v1/users");
        assert_eq!(
            request.headers.get("Content-Type"),
            Some("application/json")
        );
        assert_eq!(request.headers.get("Accept"), Some("application/json"));
        assert_eq!(
            request.body,
            RequestBody::Json(serde_json::json!({"name":"test"}))
        );
    }

    #[test]
    fn test_full_curl_get_with_auth_and_cookies() {
        let cmd = "curl -X GET --user 'admin:pass' --cookie 'session=xyz789' https://api.example.com/v1/me";
        let request = parse_curl(cmd).unwrap();
        assert_eq!(request.method, Method::Get);
        assert!(request
            .headers
            .get("Authorization")
            .unwrap()
            .starts_with("Basic "));
        assert_eq!(request.headers.get("Cookie"), Some("session=xyz789"));
    }

    #[test]
    fn test_full_curl_put_form_data() {
        let cmd = "curl -X PUT -d 'status=active&role=admin' https://api.example.com/v1/users/123";
        let request = parse_curl(cmd).unwrap();
        assert_eq!(request.method, Method::Put);
        match &request.body {
            RequestBody::Form(pairs) => {
                assert!(pairs.contains(&("status".to_string(), "active".to_string())));
                assert!(pairs.contains(&("role".to_string(), "admin".to_string())));
            }
            _ => panic!("Expected Form body"),
        }
    }

    // 13.1: "Copy as curl" - generate curl command from request
    #[test]
    fn test_request_to_curl_get() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert_eq!(curl, "curl 'https://example.com/'");
    }

    #[test]
    fn test_request_to_curl_post_json() {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://api.example.com/v1/users")
            .header("Content-Type", "application/json")
            .body(RequestBody::Json(serde_json::json!({"name": "test"})))
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("curl"));
        assert!(curl.contains("-X POST"));
        assert!(curl.contains("-H 'Content-Type: application/json'"));
        assert!(curl.contains("-d"));
        assert!(curl.contains("'https://api.example.com/v1/users'"));
    }

    #[test]
    fn test_request_to_curl_with_multiple_headers() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .header("Accept", "application/json")
            .header("Authorization", "Bearer token123")
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("-H 'Accept: application/json'"));
        assert!(curl.contains("-H 'Authorization: Bearer token123'"));
    }

    #[test]
    fn test_request_to_curl_put_form() {
        let request = RequestBuilder::new()
            .method(Method::Put)
            .url("https://example.com/api")
            .body(RequestBody::Form(vec![
                ("status".to_string(), "active".to_string()),
                ("role".to_string(), "admin".to_string()),
            ]))
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("-X PUT"));
        assert!(curl.contains("-d"));
        assert!(curl.contains("status=active"));
        assert!(curl.contains("role=admin"));
    }

    #[test]
    fn test_request_to_curl_delete() {
        let request = RequestBuilder::new()
            .method(Method::Delete)
            .url("https://api.example.com/v1/users/123")
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("-X DELETE"));
        assert!(curl.contains("'https://api.example.com/v1/users/123'"));
    }

    // 13.3: Curl command validation (round-trip)
    #[test]
    fn test_round_trip_get() {
        let original = RequestBuilder::new()
            .url("https://example.com")
            .build()
            .unwrap();
        let curl = request_to_curl(&original);
        let parsed = parse_curl(&curl).unwrap();
        assert_eq!(parsed.method, original.method);
        assert_eq!(parsed.url.as_str(), original.url.as_str());
    }

    #[test]
    fn test_round_trip_post_json() {
        let original = RequestBuilder::new()
            .method(Method::Post)
            .url("https://api.example.com/v1/users")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(RequestBody::Json(serde_json::json!({"key": "value"})))
            .build()
            .unwrap();
        let curl = request_to_curl(&original);
        let parsed = parse_curl(&curl).unwrap();
        assert_eq!(parsed.method, original.method);
        assert_eq!(parsed.url.as_str(), original.url.as_str());
        assert_eq!(parsed.headers.get("Content-Type"), Some("application/json"));
        assert_eq!(parsed.headers.get("Accept"), Some("application/json"));
    }

    #[test]
    fn test_round_trip_put_form() {
        let original = RequestBuilder::new()
            .method(Method::Put)
            .url("https://example.com/api")
            .body(RequestBody::Form(vec![
                ("name".to_string(), "john".to_string()),
                ("age".to_string(), "30".to_string()),
            ]))
            .build()
            .unwrap();
        let curl = request_to_curl(&original);
        let parsed = parse_curl(&curl).unwrap();
        assert_eq!(parsed.method, original.method);
        match &parsed.body {
            RequestBody::Form(pairs) => {
                assert!(pairs.contains(&("name".to_string(), "john".to_string())));
                assert!(pairs.contains(&("age".to_string(), "30".to_string())));
            }
            _ => panic!("Expected Form body after round-trip"),
        }
    }

    // 13.4: Special character escaping in curl output
    #[test]
    fn test_shell_escape_no_special_chars() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("hello world"), "'hello world'");
    }

    #[test]
    fn test_shell_escape_with_single_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_escape_with_double_quote() {
        assert_eq!(shell_escape("say \"hi\""), "'say \"hi\"'");
    }

    #[test]
    fn test_shell_escape_complex_string() {
        let input = "test'with\"quotes\\and\\backslashes";
        let escaped = shell_escape(input);
        assert!(escaped.starts_with('\''));
        assert!(escaped.ends_with('\''));
    }

    #[test]
    fn test_request_to_curl_with_special_chars_in_header() {
        let request = RequestBuilder::new()
            .url("https://example.com")
            .header("X-Custom", "value with 'quotes'")
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("-H"));
        assert!(curl.contains("value with"));
    }

    #[test]
    fn test_request_to_curl_with_special_chars_in_body() {
        let request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://example.com")
            .body(RequestBody::Raw("it's a test".to_string()))
            .build()
            .unwrap();
        let curl = request_to_curl(&request);
        assert!(curl.contains("-d"));
        assert!(curl.contains("it'\\''s a test"));
    }
}
