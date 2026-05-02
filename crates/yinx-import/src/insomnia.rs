use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use yinx_core::request::{Method, Request, RequestBody, RequestUrl};

#[derive(Debug, Deserialize, Serialize)]
pub struct InsomniaExport {
    #[serde(rename = "__export_format")]
    pub export_format: i32,
    #[serde(rename = "__export_date")]
    pub export_date: String,
    pub resources: Vec<InsomniaResource>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "_type")]
pub enum InsomniaResource {
    #[serde(rename = "request_group")]
    RequestGroup {
        _id: String,
        name: String,
        #[serde(default)]
        environment: Option<HashMap<String, String>>,
    },
    #[serde(rename = "request")]
    Request {
        _id: String,
        name: String,
        method: String,
        url: String,
        headers: Vec<InsomniaHeader>,
        body: InsomniaBody,
        #[serde(default)]
        authentication: Option<InsomniaAuth>,
    },
    #[serde(rename = "environment")]
    Environment {
        _id: String,
        name: String,
        data: HashMap<String, String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InsomniaHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InsomniaBody {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: Option<String>,
    #[serde(default)]
    pub params: Vec<InsomniaFormParam>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct InsomniaFormParam {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum InsomniaAuth {
    #[serde(rename = "basic")]
    Basic {
        username: String,
        password: String,
    },
    #[serde(rename = "bearer")]
    Bearer {
        token: String,
    },
    #[serde(rename = "oauth2")]
    OAuth2 {
        #[serde(rename = "accessToken")]
        access_token: String,
    },
    #[serde(rename = "ntlm")]
    NTLM {
        username: String,
        password: String,
    },
}

pub fn parse_insomnia_export(json: &str) -> Result<Vec<Request>, String> {
    let export: InsomniaExport = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut requests = Vec::new();

    for resource in export.resources {
        if let InsomniaResource::Request {
            method,
            url,
            headers,
            body,
            authentication,
            ..
        } = resource
        {
            let req_method: Method = method.parse().map_err(|e: String| format!("Invalid method: {}", e))?;
            let req_url = RequestUrl::new(&url).map_err(|e| e.to_string())?;

            let mut req_headers = yinx_core::request::Headers::new();

            // Add headers
            for h in headers {
                let _ = req_headers.set(&h.name, &h.value);
            }

            // Add auth header if present
            if let Some(auth) = authentication {
                match auth {
                    InsomniaAuth::Basic { username, password } => {
                        let encoded = base64::engine::general_purpose::STANDARD
                            .encode(format!("{}:{}", username, password));
                        let _ = req_headers.set("Authorization", &format!("Basic {}", encoded));
                    }
                    InsomniaAuth::Bearer { token } => {
                        let _ = req_headers.set("Authorization", &format!("Bearer {}", token));
                    }
                    InsomniaAuth::OAuth2 { access_token } => {
                        let _ = req_headers.set("Authorization", &format!("Bearer {}", access_token));
                    }
                    InsomniaAuth::NTLM { username, password } => {
                        let encoded = base64::engine::general_purpose::STANDARD
                            .encode(format!("{}:{}", username, password));
                        let _ = req_headers.set("Authorization", &format!("NTLM {}", encoded));
                    }
                }
            }

            // Parse body
            let req_body = match body.mime_type.as_str() {
                "application/json" => {
                    if let Some(text) = body.text {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                            RequestBody::Json(json)
                        } else {
                            RequestBody::Raw(text)
                        }
                    } else {
                        RequestBody::None
                    }
                }
                "application/x-www-form-urlencoded" => {
                    if !body.params.is_empty() {
                        let pairs: Vec<(String, String)> = body
                            .params
                            .into_iter()
                            .map(|p| (p.name, p.value))
                            .collect();
                        RequestBody::Form(pairs)
                    } else if let Some(text) = body.text {
                        RequestBody::Raw(text)
                    } else {
                        RequestBody::None
                    }
                }
                _ => {
                    if let Some(text) = body.text {
                        RequestBody::Raw(text)
                    } else {
                        RequestBody::None
                    }
                }
            };

            let request = Request {
                method: req_method,
                url: req_url,
                headers: req_headers,
                body: req_body,
                timeout_secs: 30,
            };
            requests.push(request);
        }
    }

    Ok(requests)
}

pub fn parse_insomnia_environments(json: &str) -> Result<HashMap<String, String>, String> {
    let export: InsomniaExport = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut env = HashMap::new();

    for resource in export.resources {
        match resource {
            InsomniaResource::Environment { data, .. } => {
                env.extend(data);
            }
            InsomniaResource::RequestGroup { environment: Some(e), .. } => {
                env.extend(e);
            }
            _ => {}
        }
    }

    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 5.15: Insomnia export schema types
    #[test]
    fn test_deserialize_insomnia_export() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": []
        }"#;
        let export: InsomniaExport = serde_json::from_str(json).unwrap();
        assert_eq!(export.export_format, 4);
        assert!(export.resources.is_empty());
    }

    #[test]
    fn test_deserialize_insomnia_request() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Get Users",
                    "method": "GET",
                    "url": "https://api.example.com/users",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null}
                }
            ]
        }"#;
        let export: InsomniaExport = serde_json::from_str(json).unwrap();
        assert_eq!(export.resources.len(), 1);
    }

    // 5.16: Parse Insomnia requests → Request
    #[test]
    fn test_parse_get_request() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Get Users",
                    "method": "GET",
                    "url": "https://api.example.com/users",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null}
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Get);
        assert_eq!(requests[0].url.as_str(), "https://api.example.com/users");
    }

    #[test]
    fn test_parse_post_request_with_body() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Create User",
                    "method": "POST",
                    "url": "https://api.example.com/users",
                    "headers": [
                        {"name": "Content-Type", "value": "application/json"}
                    ],
                    "body": {
                        "mimeType": "application/json",
                        "text": "{\"name\": \"test\"}"
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Post);
        assert_eq!(requests[0].headers.get("Content-Type"), Some("application/json"));
    }

    #[test]
    fn test_parse_request_with_headers() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "GET",
                    "url": "https://api.example.com",
                    "headers": [
                        {"name": "Accept", "value": "application/json"},
                        {"name": "X-Custom", "value": "value123"}
                    ],
                    "body": {"mimeType": "application/json", "text": null}
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(requests[0].headers.len(), 2);
        assert_eq!(requests[0].headers.get("Accept"), Some("application/json"));
        assert_eq!(requests[0].headers.get("X-Custom"), Some("value123"));
    }

    // 5.17: Parse Insomnia environments
    #[test]
    fn test_parse_environment_from_export() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "environment",
                    "_id": "env_1",
                    "name": "Production",
                    "data": {
                        "baseUrl": "https://api.prod.com",
                        "token": "prod-token"
                    }
                }
            ]
        }"#;
        let env = parse_insomnia_environments(json).unwrap();
        assert_eq!(env.len(), 2);
        assert_eq!(env.get("baseUrl").unwrap(), "https://api.prod.com");
        assert_eq!(env.get("token").unwrap(), "prod-token");
    }

    #[test]
    fn test_parse_environment_merged_from_request_group() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request_group",
                    "_id": "grp_1",
                    "name": "API",
                    "environment": {
                        "baseUrl": "https://api.example.com"
                    }
                }
            ]
        }"#;
        let env = parse_insomnia_environments(json).unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(env.get("baseUrl").unwrap(), "https://api.example.com");
    }

    // 5.18: Handle Insomnia-specific auth types
    #[test]
    fn test_parse_basic_auth() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "GET",
                    "url": "https://api.example.com",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null},
                    "authentication": {
                        "type": "basic",
                        "username": "admin",
                        "password": "secret"
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(requests[0].headers.get("Authorization").unwrap().starts_with("Basic"), true);
    }

    #[test]
    fn test_parse_bearer_auth() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "GET",
                    "url": "https://api.example.com",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null},
                    "authentication": {
                        "type": "bearer",
                        "token": "my-jwt-token"
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(
            requests[0].headers.get("Authorization"),
            Some("Bearer my-jwt-token")
        );
    }

    #[test]
    fn test_parse_oauth2_auth() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "GET",
                    "url": "https://api.example.com",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null},
                    "authentication": {
                        "type": "oauth2",
                        "accessToken": "oauth-token-123"
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(
            requests[0].headers.get("Authorization"),
            Some("Bearer oauth-token-123")
        );
    }

    #[test]
    fn test_parse_ntlm_auth() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "GET",
                    "url": "https://api.example.com",
                    "headers": [],
                    "body": {"mimeType": "application/json", "text": null},
                    "authentication": {
                        "type": "ntlm",
                        "username": "domain\\user",
                        "password": "pass"
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        assert_eq!(requests[0].headers.get("Authorization").unwrap().starts_with("NTLM"), true);
    }

    #[test]
    fn test_parse_form_encoded_body() {
        let json = r#"{
            "__export_format": 4,
            "__export_date": "2024-01-01T00:00:00.000Z",
            "resources": [
                {
                    "_type": "request",
                    "_id": "req_1",
                    "name": "Test",
                    "method": "POST",
                    "url": "https://api.example.com",
                    "headers": [],
                    "body": {
                        "mimeType": "application/x-www-form-urlencoded",
                        "text": null,
                        "params": [
                            {"name": "name", "value": "john"},
                            {"name": "age", "value": "30"}
                        ]
                    }
                }
            ]
        }"#;
        let requests = parse_insomnia_export(json).unwrap();
        match &requests[0].body {
            RequestBody::Form(pairs) => {
                assert_eq!(pairs.len(), 2);
                assert!(pairs.contains(&("name".to_string(), "john".to_string())));
            }
            _ => panic!("Expected Form body"),
        }
    }
}
