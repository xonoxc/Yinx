use std::collections::HashMap;

use serde::Serialize;
use yinx_core::collections::{Collection, CollectionItem};
use yinx_core::environments::Environment;
use yinx_core::request::RequestBody;

use crate::postman::{
    PostmanBody, PostmanCollection, PostmanCollectionInfo, PostmanFormParam, PostmanHeader,
    PostmanItem, PostmanRequest, PostmanUrl, PostmanVariable,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub json: String,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Empty collection")]
    EmptyCollection,
}

pub fn export_collection(collection: &Collection) -> Result<ExportResult, ExportError> {
    if collection.name.is_empty() {
        return Err(ExportError::EmptyCollection);
    }

    let items: Vec<PostmanItem> = collection.items.iter().map(convert_item).collect();

    let variables = extract_collection_variables(collection);

    let postman = PostmanCollection {
        info: PostmanCollectionInfo {
            name: collection.name.clone(),
            schema_url: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
                .to_string(),
            description: None,
        },
        item: items,
        variable: variables,
    };

    let json = serde_json::to_string_pretty(&postman)
        .map_err(|e| ExportError::Serialization(e.to_string()))?;

    Ok(ExportResult {
        json,
        name: collection.name.clone(),
    })
}

pub fn export_environment(env: &Environment) -> Result<ExportResult, ExportError> {
    let values: Vec<serde_json::Value> = env
        .variables
        .iter()
        .map(|v| {
            serde_json::json!({
                "key": v.key,
                "value": v.value,
                "enabled": v.enabled,
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&serde_json::json!({
        "name": env.name,
        "values": values,
    }))
    .map_err(|e| ExportError::Serialization(e.to_string()))?;

    Ok(ExportResult {
        json,
        name: env.name.clone(),
    })
}

fn convert_item(item: &CollectionItem) -> PostmanItem {
    match item {
        CollectionItem::Request(saved) => {
            let req = &saved.request;
            let method = req.method.as_str().to_string();

            let url = PostmanUrl::String(req.url.as_str().to_string());

            let header: Vec<PostmanHeader> = req
                .headers
                .iter()
                .map(|h| PostmanHeader {
                    key: h.name.clone(),
                    value: h.value.clone(),
                    description: None,
                })
                .collect();

            let body = match &req.body {
                RequestBody::Raw(s) => Some(PostmanBody {
                    mode: "raw".to_string(),
                    raw: Some(s.clone()),
                    urlencoded: None,
                    formdata: None,
                }),
                RequestBody::Json(v) => Some(PostmanBody {
                    mode: "raw".to_string(),
                    raw: Some(serde_json::to_string(v).unwrap_or_default()),
                    urlencoded: None,
                    formdata: None,
                }),
                RequestBody::Form(pairs) => {
                    let params: Vec<PostmanFormParam> = pairs
                        .iter()
                        .map(|(k, v)| PostmanFormParam {
                            key: k.clone(),
                            value: v.clone(),
                        })
                        .collect();
                    Some(PostmanBody {
                        mode: "urlencoded".to_string(),
                        raw: None,
                        urlencoded: Some(params),
                        formdata: None,
                    })
                }
                RequestBody::Multipart(pairs) => {
                    let params: Vec<PostmanFormParam> = pairs
                        .iter()
                        .map(|(k, v)| PostmanFormParam {
                            key: k.clone(),
                            value: v.clone(),
                        })
                        .collect();
                    Some(PostmanBody {
                        mode: "formdata".to_string(),
                        raw: None,
                        urlencoded: None,
                        formdata: Some(params),
                    })
                }
                RequestBody::Binary(data) => Some(PostmanBody {
                    mode: "raw".to_string(),
                    raw: Some(String::from_utf8_lossy(data).to_string()),
                    urlencoded: None,
                    formdata: None,
                }),
                RequestBody::None => None,
            };

            PostmanItem::Request {
                name: saved.name.clone(),
                request: PostmanRequest {
                    method,
                    url,
                    header,
                    body,
                    description: None,
                },
                item: None,
            }
        }
        CollectionItem::Folder { name, children } => {
            let sub_items: Vec<PostmanItem> = children.iter().map(convert_item).collect();
            PostmanItem::Folder {
                name: name.clone(),
                item: sub_items,
                description: None,
            }
        }
    }
}

fn extract_collection_variables(collection: &Collection) -> Vec<PostmanVariable> {
    let mut seen = HashMap::new();
    let mut variables = Vec::new();

    for item in &collection.items {
        extract_variables_from_item(item, &mut seen, &mut variables);
    }

    variables
}

fn extract_variables_from_item(
    item: &CollectionItem,
    seen: &mut HashMap<String, String>,
    variables: &mut Vec<PostmanVariable>,
) {
    match item {
        CollectionItem::Request(saved) => {
            let req = &saved.request;
            let url_str = req.url.as_str();
            let body_str: Option<String> = match &req.body {
                RequestBody::Raw(s) => Some(s.clone()),
                RequestBody::Json(v) => Some(serde_json::to_string(v).unwrap_or_default()),
                _ => None,
            };

            let text = format!("{} {:?}", url_str, req.headers.len());

            if text.contains("{{") {
                for h in req.headers.iter() {
                    let combined = format!("{} {}", h.name, h.value);
                    extract_from_template(&combined, seen, variables);
                }
            }

            extract_from_template(url_str, seen, variables);
            if let Some(ref body) = body_str {
                extract_from_template(body, seen, variables);
            }
        }
        CollectionItem::Folder { children, .. } => {
            for child in children {
                extract_variables_from_item(child, seen, variables);
            }
        }
    }
}

fn extract_from_template(
    input: &str,
    seen: &mut HashMap<String, String>,
    variables: &mut Vec<PostmanVariable>,
) {
    let mut remaining = input;
    while let Some(start) = remaining.find("{{") {
        if let Some(end) = remaining[start..].find("}}") {
            let var_name = &remaining[start + 2..start + end];
            if !var_name.starts_with('$') && !seen.contains_key(var_name) {
                seen.insert(var_name.to_string(), String::new());
                variables.push(PostmanVariable {
                    key: var_name.to_string(),
                    value: String::new(),
                });
            }
            remaining = &remaining[start + end + 2..];
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::collections::CollectionItem;
    use yinx_core::environments::EnvironmentVariable;
    use yinx_core::request::{Method, RequestBuilder};
    use yinx_core::state::SavedRequest;

    fn make_request(name: &str, url: &str) -> SavedRequest {
        SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            request: RequestBuilder::new().url(url).build().unwrap(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_export_single_request_collection() {
        let mut collection = Collection::new("Test API".to_string());
        collection.add_item(CollectionItem::Request(Box::new(make_request(
            "Get Users",
            "https://api.example.com/users",
        ))));

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("Test API"));
        assert!(result.json.contains("Get Users"));
        assert!(result.json.contains("https://api.example.com/users"));
    }

    #[test]
    fn test_export_collection_with_folder() {
        let mut collection = Collection::new("Nested API".to_string());
        collection.add_item(CollectionItem::Folder {
            name: "Users".to_string(),
            children: vec![CollectionItem::Request(Box::new(make_request(
                "Get Users",
                "https://api.example.com/users",
            )))],
        });

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("Users"));
        assert!(result.json.contains("Get Users"));
    }

    #[test]
    fn test_export_empty_collection_errors() {
        let collection = Collection::new("".to_string());
        assert!(export_collection(&collection).is_err());
    }

    #[test]
    fn test_export_environment() {
        let mut env = Environment::new("Staging".to_string());
        env.add_variable(EnvironmentVariable::new(
            "base_url".to_string(),
            "https://staging.example.com".to_string(),
        ));

        let result = export_environment(&env).unwrap();
        assert!(result.json.contains("Staging"));
        assert!(result.json.contains("base_url"));
        assert!(result.json.contains("https://staging.example.com"));
    }

    #[test]
    fn test_export_disabled_variable() {
        let mut env = Environment::new("Test".to_string());
        env.add_variable(EnvironmentVariable {
            key: "secret".to_string(),
            value: "s3cret".to_string(),
            enabled: false,
        });

        let result = export_environment(&env).unwrap();
        assert!(result.json.contains("s3cret"));
        assert!(result.json.contains("\"enabled\": false"));
    }

    #[test]
    fn test_export_collection_with_variable_in_url() {
        let mut collection = Collection::new("API".to_string());
        collection.add_item(CollectionItem::Request(Box::new(make_request(
            "Get Users",
            "{{base_url}}/users",
        ))));

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("{{base_url}}"));
        let postman: PostmanCollection = serde_json::from_str(&result.json).unwrap();
        assert!(postman.variable.iter().any(|v| v.key == "base_url"));
    }

    #[test]
    fn test_export_roundtrip_single_request() {
        let mut collection = Collection::new("Roundtrip".to_string());
        collection.add_item(CollectionItem::Request(Box::new(make_request(
            "Test",
            "https://example.com/test",
        ))));

        let result = export_collection(&collection).unwrap();
        let imported: PostmanCollection = serde_json::from_str(&result.json).unwrap();
        assert_eq!(imported.info.name, "Roundtrip");
        assert_eq!(imported.item.len(), 1);
    }

    #[test]
    fn test_export_request_with_headers() {
        let saved = SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Auth Request".to_string(),
            request: RequestBuilder::new()
                .url("https://api.example.com/auth")
                .header("Authorization", "Bearer token123")
                .header("Content-Type", "application/json")
                .build()
                .unwrap(),
            tags: Vec::new(),
        };

        let mut collection = Collection::new("Auth API".to_string());
        collection.add_item(CollectionItem::Request(Box::new(saved)));

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("Authorization"));
        assert!(result.json.contains("Bearer token123"));
    }

    #[test]
    fn test_export_request_with_json_body() {
        let saved = SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Create User".to_string(),
            request: RequestBuilder::new()
                .method(Method::Post)
                .url("https://api.example.com/users")
                .header("Content-Type", "application/json")
                .body(RequestBody::Json(serde_json::json!({"name": "test"})))
                .build()
                .unwrap(),
            tags: Vec::new(),
        };

        let mut collection = Collection::new("API".to_string());
        collection.add_item(CollectionItem::Request(Box::new(saved)));

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("raw"));
        assert!(result.json.contains("name"));
    }

    #[test]
    fn test_export_deeply_nested_folder() {
        let mut collection = Collection::new("Deep".to_string());
        collection.add_item(CollectionItem::Folder {
            name: "Level1".to_string(),
            children: vec![CollectionItem::Folder {
                name: "Level2".to_string(),
                children: vec![CollectionItem::Request(Box::new(make_request(
                    "Deep Request",
                    "https://example.com/deep",
                )))],
            }],
        });

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("Level1"));
        assert!(result.json.contains("Level2"));
        assert!(result.json.contains("Deep Request"));
    }

    #[test]
    fn test_export_result_name_matches() {
        let collection = Collection::new("My Collection".to_string());
        let result = export_collection(&collection).unwrap();
        assert_eq!(result.name, "My Collection");
    }

    #[test]
    fn test_export_collection_with_form_body() {
        let saved = SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Form Post".to_string(),
            request: RequestBuilder::new()
                .method(Method::Post)
                .url("https://example.com/form")
                .body(RequestBody::Form(vec![
                    ("field1".to_string(), "value1".to_string()),
                    ("field2".to_string(), "value2".to_string()),
                ]))
                .build()
                .unwrap(),
            tags: Vec::new(),
        };

        let mut collection = Collection::new("Forms".to_string());
        collection.add_item(CollectionItem::Request(Box::new(saved)));

        let result = export_collection(&collection).unwrap();
        assert!(result.json.contains("urlencoded"));
        assert!(result.json.contains("field1"));
    }

    #[test]
    fn test_export_serde_roundtrip_full_collection() {
        let mut collection = Collection::new("Full API".to_string());
        collection.add_item(CollectionItem::Folder {
            name: "Auth".to_string(),
            children: vec![
                CollectionItem::Request(Box::new(make_request(
                    "Login",
                    "https://example.com/login",
                ))),
                CollectionItem::Request(Box::new(make_request(
                    "Register",
                    "https://example.com/register",
                ))),
            ],
        });
        collection.add_item(CollectionItem::Request(Box::new(make_request(
            "Health",
            "https://example.com/health",
        ))));

        let result = export_collection(&collection).unwrap();
        let postman: PostmanCollection = serde_json::from_str(&result.json).unwrap();
        assert_eq!(postman.info.name, "Full API");
        assert_eq!(postman.item.len(), 2);
    }
}
