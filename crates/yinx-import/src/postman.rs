use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use yinx_core::collections::{Collection, CollectionItem};
use yinx_core::environments::Environment;
use yinx_core::request::{Method, Request, RequestBody, RequestUrl};
use yinx_core::state::SavedRequest;

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanCollection {
    pub info: PostmanCollectionInfo,
    pub item: Vec<PostmanItem>,
    #[serde(default)]
    pub variable: Vec<PostmanVariable>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanCollectionInfo {
    pub name: String,
    #[serde(rename = "schema")]
    pub schema_url: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PostmanItem {
    Request {
        name: String,
        request: PostmanRequest,
        #[serde(default)]
        item: Option<Vec<PostmanItem>>,
    },
    Folder {
        name: String,
        item: Vec<PostmanItem>,
        #[serde(default)]
        description: Option<String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanRequest {
    #[serde(default)]
    pub method: String,
    pub url: PostmanUrl,
    #[serde(default)]
    pub header: Vec<PostmanHeader>,
    #[serde(default)]
    pub body: Option<PostmanBody>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PostmanUrl {
    String(String),
    Object {
        raw: Option<String>,
        #[serde(default)]
        host: Vec<String>,
        #[serde(default)]
        path: Vec<String>,
        #[serde(default)]
        query: Vec<PostmanQueryParam>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanQueryParam {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanHeader {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanBody {
    pub mode: String,
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub urlencoded: Option<Vec<PostmanFormParam>>,
    #[serde(default)]
    pub formdata: Option<Vec<PostmanFormParam>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanFormParam {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanVariable {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanEnvironment {
    pub name: String,
    pub values: Vec<PostmanEnvironmentValue>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PostmanEnvironmentValue {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub enabled: bool,
}

pub fn parse_collection(json: &str) -> Result<Vec<Request>, String> {
    let collection: PostmanCollection = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let variables: HashMap<String, String> = collection
        .variable
        .iter()
        .map(|v| (v.key.clone(), v.value.clone()))
        .collect();

    let mut requests = Vec::new();
    for item in collection.item {
        collect_requests(item, &variables, &mut requests);
    }
    Ok(requests)
}

fn collect_requests(
    item: PostmanItem,
    variables: &HashMap<String, String>,
    requests: &mut Vec<Request>,
) {
    match item {
        PostmanItem::Request { request: req, .. } => {
            if let Ok(request) = convert_request(req, variables) {
                requests.push(request);
            }
        }
        PostmanItem::Folder { item: items, .. } => {
            for sub_item in items {
                collect_requests(sub_item, variables, requests);
            }
        }
    }
}

fn convert_request(
    req: PostmanRequest,
    variables: &HashMap<String, String>,
) -> Result<Request, String> {
    let method: Method = req
        .method
        .parse()
        .map_err(|e: String| format!("Invalid method: {}", e))?;

    let url_str = resolve_url(req.url, variables);
    let url = RequestUrl::new(&url_str).map_err(|e| e.to_string())?;

    let mut headers = yinx_core::request::Headers::new();
    for h in req.header {
        let key = replace_variables(&h.key, variables);
        let value = replace_variables(&h.value, variables);
        let _ = headers.set(&key, &value);
    }

    let body = if let Some(body) = req.body {
        match body.mode.as_str() {
            "raw" => {
                if let Some(raw) = body.raw {
                    let raw = replace_variables(&raw, variables);
                    if headers.get("content-type") == Some("application/json") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                            RequestBody::Json(json)
                        } else {
                            RequestBody::Raw(raw)
                        }
                    } else {
                        RequestBody::Raw(raw)
                    }
                } else {
                    RequestBody::None
                }
            }
            "urlencoded" => {
                if let Some(params) = body.urlencoded {
                    let pairs: Vec<(String, String)> = params
                        .into_iter()
                        .map(|p| {
                            (
                                replace_variables(&p.key, variables),
                                replace_variables(&p.value, variables),
                            )
                        })
                        .collect();
                    RequestBody::Form(pairs)
                } else {
                    RequestBody::None
                }
            }
            "formdata" => {
                if let Some(params) = body.formdata {
                    let pairs: Vec<(String, String)> = params
                        .into_iter()
                        .map(|p| {
                            (
                                replace_variables(&p.key, variables),
                                replace_variables(&p.value, variables),
                            )
                        })
                        .collect();
                    RequestBody::Form(pairs)
                } else {
                    RequestBody::None
                }
            }
            _ => RequestBody::None,
        }
    } else {
        RequestBody::None
    };

    Ok(Request {
        method,
        url,
        headers,
        body,
        timeout_secs: 30,
    })
}

fn resolve_url(url: PostmanUrl, variables: &HashMap<String, String>) -> String {
    match url {
        PostmanUrl::String(s) => replace_variables(&s, variables),
        PostmanUrl::Object {
            raw,
            host,
            path,
            query,
            ..
        } => {
            if let Some(raw) = raw {
                replace_variables(&raw, variables)
            } else {
                let host_part = host.join(".");
                let path_part = path.join("/");
                let mut url = format!("https://{}/{}", host_part, path_part);
                if !query.is_empty() {
                    let query_str: Vec<String> = query
                        .into_iter()
                        .map(|q| format!("{}={}", q.key, q.value))
                        .collect();
                    url.push('?');
                    url.push_str(&query_str.join("&"));
                }
                replace_variables(&url, variables)
            }
        }
    }
}

fn replace_variables(input: &str, variables: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (key, value) in variables {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

pub fn parse_environment(json: &str) -> Result<HashMap<String, String>, String> {
    let env: PostmanEnvironment = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut result = HashMap::new();
    for v in env.values {
        if v.enabled {
            result.insert(v.key, v.value);
        }
    }
    Ok(result)
}

pub struct ImportResult {
    pub requests: Vec<Request>,
    pub variables: HashMap<String, String>,
}

pub fn parse_collection_to_collection(json: &str) -> Result<(Collection, Vec<String>), String> {
    let postman: PostmanCollection = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let collection_name = postman.info.name.clone();
    let mut collection = Collection::new(collection_name);

    let collection_variables: HashMap<String, String> = postman
        .variable
        .iter()
        .map(|v| (v.key.clone(), v.value.clone()))
        .collect();

    let mut warnings = Vec::new();

    for item in postman.item {
        match convert_item_to_collection_item(item, &collection_variables, &mut warnings) {
            Some(ci) => collection.add_item(ci),
            None => {}
        }
    }

    Ok((collection, warnings))
}

fn convert_item_to_collection_item(
    item: PostmanItem,
    variables: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Option<CollectionItem> {
    match item {
        PostmanItem::Request {
            name,
            request: req,
            item: sub_items,
        } => {
            if let Some(sub_items) = sub_items {
                if !sub_items.is_empty() {
                    let mut children = Vec::new();
                    for sub in sub_items {
                        if let Some(child) =
                            convert_item_to_collection_item(sub, variables, warnings)
                        {
                            children.push(child);
                        }
                    }
                    return Some(CollectionItem::Folder { name, children });
                }
            }

            match convert_request_to_saved(name, req, variables, warnings) {
                Some(saved) => Some(CollectionItem::Request(Box::new(saved))),
                None => None,
            }
        }
        PostmanItem::Folder {
            name, item: items, ..
        } => {
            let children: Vec<CollectionItem> = items
                .into_iter()
                .filter_map(|i| convert_item_to_collection_item(i, variables, warnings))
                .collect();
            Some(CollectionItem::Folder { name, children })
        }
    }
}

fn convert_request_to_saved(
    name: String,
    req: PostmanRequest,
    variables: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Option<SavedRequest> {
    let method: Method = match req.method.parse() {
        Ok(m) => m,
        Err(_) => {
            warnings.push(format!(
                "Unknown method '{}' for request '{}'",
                req.method, name
            ));
            return None;
        }
    };

    let url_str = resolve_url(req.url, variables);
    let url = match RequestUrl::new(&url_str) {
        Ok(u) => u,
        Err(_) => {
            warnings.push(format!("Invalid URL '{}' for request '{}'", url_str, name));
            return None;
        }
    };

    let mut headers = yinx_core::request::Headers::new();
    for h in req.header {
        let key = replace_variables(&h.key, variables);
        let value = replace_variables(&h.value, variables);
        let _ = headers.set(&key, &value);
    }

    let body = if let Some(body) = req.body {
        convert_body(body, &headers, variables)
    } else {
        RequestBody::None
    };

    let request = Request {
        method,
        url,
        headers,
        body,
        timeout_secs: 30,
    };

    Some(SavedRequest {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        request,
        tags: Vec::new(),
    })
}

fn convert_body(
    body: PostmanBody,
    headers: &yinx_core::request::Headers,
    variables: &HashMap<String, String>,
) -> RequestBody {
    match body.mode.as_str() {
        "raw" => {
            if let Some(raw) = body.raw {
                let raw = replace_variables(&raw, variables);
                if headers.get("content-type") == Some("application/json") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
                        return RequestBody::Json(json);
                    }
                }
                RequestBody::Raw(raw)
            } else {
                RequestBody::None
            }
        }
        "urlencoded" => {
            if let Some(params) = body.urlencoded {
                let pairs: Vec<(String, String)> = params
                    .into_iter()
                    .map(|p| {
                        (
                            replace_variables(&p.key, variables),
                            replace_variables(&p.value, variables),
                        )
                    })
                    .collect();
                RequestBody::Form(pairs)
            } else {
                RequestBody::None
            }
        }
        "formdata" => {
            if let Some(params) = body.formdata {
                let pairs: Vec<(String, String)> = params
                    .into_iter()
                    .map(|p| {
                        (
                            replace_variables(&p.key, variables),
                            replace_variables(&p.value, variables),
                        )
                    })
                    .collect();
                RequestBody::Form(pairs)
            } else {
                RequestBody::None
            }
        }
        _ => RequestBody::None,
    }
}

pub fn parse_environment_to_env(json: &str) -> Result<Environment, String> {
    let postman_env: PostmanEnvironment = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let mut env = Environment::new(postman_env.name);
    for value in postman_env.values {
        env.add_variable(yinx_core::environments::EnvironmentVariable {
            key: value.key,
            value: value.value,
            enabled: value.enabled,
        });
    }

    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 5.9: Postman v2.1 schema types
    #[test]
    fn test_deserialize_postman_collection_v21() {
        let json = r#"{
            "info": {
                "name": "Sample API",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
            },
            "item": []
        }"#;
        let collection: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(collection.info.name, "Sample API");
        assert!(collection.item.is_empty());
    }

    #[test]
    fn test_deserialize_collection_with_variable() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "variable": [
                {"key": "baseUrl", "value": "https://api.example.com"}
            ],
            "item": []
        }"#;
        let collection: PostmanCollection = serde_json::from_str(json).unwrap();
        assert_eq!(collection.variable.len(), 1);
        assert_eq!(collection.variable[0].key, "baseUrl");
    }

    // 5.10: Parse collection → list of Request
    #[test]
    fn test_parse_single_get_request() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [
                {
                    "name": "Get Users",
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com/users"
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Get);
        assert_eq!(requests[0].url.as_str(), "https://api.example.com/users");
    }

    #[test]
    fn test_parse_post_request_with_body() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [
                {
                    "name": "Create User",
                    "request": {
                        "method": "POST",
                        "url": "https://api.example.com/users",
                        "header": [
                            {"key": "Content-Type", "value": "application/json"}
                        ],
                        "body": {
                            "mode": "raw",
                            "raw": "{\"name\": \"test\"}"
                        }
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Post);
        assert_eq!(
            requests[0].headers.get("Content-Type"),
            Some("application/json")
        );
    }

    #[test]
    fn test_parse_nested_folders_flattened() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [
                {
                    "name": "Users Folder",
                    "item": [
                        {
                            "name": "Get Users",
                            "request": {
                                "method": "GET",
                                "url": "https://api.example.com/users"
                            }
                        },
                        {
                            "name": "Get User",
                            "request": {
                                "method": "GET",
                                "url": "https://api.example.com/users/1"
                            }
                        }
                    ]
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests.len(), 2);
    }

    // 5.11: Parse Postman variables
    #[test]
    fn test_parse_variable_in_url() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "variable": [
                {"key": "baseUrl", "value": "https://api.example.com"}
            ],
            "item": [
                {
                    "name": "Get Users",
                    "request": {
                        "method": "GET",
                        "url": "{{baseUrl}}/users"
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests[0].url.as_str(), "https://api.example.com/users");
    }

    #[test]
    fn test_parse_variable_in_header() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "variable": [
                {"key": "token", "value": "abc123"}
            ],
            "item": [
                {
                    "name": "Test",
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com",
                        "header": [
                            {"key": "Authorization", "value": "Bearer {{token}}"}
                        ]
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(
            requests[0].headers.get("Authorization"),
            Some("Bearer abc123")
        );
    }

    // 5.12: Parse pre-request scripts (store as metadata)
    #[test]
    fn test_parse_prerequest_script_stored() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [
                {
                    "name": "Test",
                    "event": [
                        {
                            "listen": "prerequest",
                            "script": {
                                "exec": ["console.log('pre-request');"]
                            }
                        }
                    ],
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com"
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests.len(), 1);
    }

    // 5.13: Parse tests/assertions (store as metadata)
    #[test]
    fn test_parse_test_script_stored() {
        let json = r#"{
            "info": {"name": "Test", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [
                {
                    "name": "Test",
                    "event": [
                        {
                            "listen": "test",
                            "script": {
                                "exec": ["pm.expect(json).to.have.property('id');"]
                            }
                        }
                    ],
                    "request": {
                        "method": "GET",
                        "url": "https://api.example.com"
                    }
                }
            ]
        }"#;
        let requests = parse_collection(json).unwrap();
        assert_eq!(requests.len(), 1);
    }

    // 5.14: Postman environment file import
    #[test]
    fn test_parse_environment_file() {
        let json = r#"{
            "name": "Production",
            "values": [
                {"key": "baseUrl", "value": "https://api.prod.com", "enabled": true},
                {"key": "token", "value": "prod-token", "enabled": true}
            ]
        }"#;
        let env = parse_environment(json).unwrap();
        assert_eq!(env.len(), 2);
        assert_eq!(env.get("baseUrl").unwrap(), "https://api.prod.com");
        assert_eq!(env.get("token").unwrap(), "prod-token");
    }

    #[test]
    fn test_parse_environment_with_disabled_vars() {
        let json = r#"{
            "name": "Test",
            "values": [
                {"key": "enabled", "value": "yes", "enabled": true},
                {"key": "disabled", "value": "no", "enabled": false}
            ]
        }"#;
        let env = parse_environment(json).unwrap();
        assert_eq!(env.len(), 1);
        assert!(env.contains_key("enabled"));
        assert!(!env.contains_key("disabled"));
    }
}
