use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use yinx_core::request::{Method, Request, RequestBody, RequestUrl};

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub paths: HashMap<String, OpenApiPathItem>,
    #[serde(default)]
    pub components: Option<OpenApiComponents>,
    #[serde(default)]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenApiInfo {
    pub title: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenApiPathItem {
    #[serde(default)]
    pub get: Option<OpenApiOperation>,
    #[serde(default)]
    pub post: Option<OpenApiOperation>,
    #[serde(default)]
    pub put: Option<OpenApiOperation>,
    #[serde(default)]
    pub patch: Option<OpenApiOperation>,
    #[serde(default)]
    pub delete: Option<OpenApiOperation>,
    #[serde(default)]
    pub head: Option<OpenApiOperation>,
    #[serde(default)]
    pub options: Option<OpenApiOperation>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiOperation {
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub parameters: Vec<OpenApiParameter>,
    #[serde(default)]
    #[serde(rename = "requestBody")]
    pub request_body: Option<OpenApiRequestBody>,
    #[serde(default)]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OpenApiParameter {
    Reference {
        #[serde(rename = "$ref")]
        reference: String,
    },
    Direct {
        name: String,
        #[serde(rename = "in")]
        location: String,
        required: bool,
        schema: Option<OpenApiSchema>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiRequestBody {
    #[serde(default)]
    pub content: HashMap<String, OpenApiMediaType>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiMediaType {
    #[serde(default)]
    pub schema: Option<OpenApiSchema>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiSchema {
    #[serde(rename = "type")]
    pub schema_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiComponents {
    #[serde(default)]
    pub securitySchemes: Option<HashMap<String, OpenApiSecurityScheme>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum OpenApiSecurityScheme {
    Reference {
        #[serde(rename = "$ref")]
        reference: String,
    },
    ApiKey {
        #[serde(rename = "type")]
        scheme_type: String,
        name: String,
        #[serde(rename = "in")]
        location: String,
    },
    Http {
        #[serde(rename = "type")]
        scheme_type: String,
        scheme: String,
    },
    OAuth2 {
        #[serde(rename = "type")]
        scheme_type: String,
        flows: OpenApiOAuthFlows,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiOAuthFlows {
    #[serde(default)]
    pub implicit: Option<OpenApiOAuthFlow>,
    #[serde(default)]
    pub password: Option<OpenApiOAuthFlow>,
    #[serde(default)]
    pub clientCredentials: Option<OpenApiOAuthFlow>,
    #[serde(default)]
    pub authorizationCode: Option<OpenApiOAuthFlow>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenApiOAuthFlow {
    #[serde(default)]
    pub authorizationUrl: String,
    #[serde(default)]
    pub tokenUrl: String,
    #[serde(default)]
    pub scopes: HashMap<String, String>,
}

// Swagger 2.0 (OpenAPI 2.0)
#[derive(Debug, Deserialize, Serialize)]
pub struct SwaggerSpec {
    pub swagger: String,
    pub info: OpenApiInfo,
    pub paths: HashMap<String, OpenApiPathItem>,
    #[serde(default)]
    pub securityDefinitions: Option<HashMap<String, OpenApiSecurityScheme>>,
    #[serde(default)]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

pub fn parse_openapi(spec: &str) -> Result<Vec<Request>, String> {
    // Try OpenAPI 3.0 first, then Swagger 2.0
    if spec.contains("openapi:") || spec.contains("\"openapi\"") {
        parse_openapi_30(spec)
    } else if spec.contains("swagger:") || spec.contains("\"swagger\"") {
        parse_swagger_20(spec)
    } else {
        // Try to detect from JSON
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(spec) {
            if json.get("openapi").is_some() {
                parse_openapi_30(spec)
            } else if json.get("swagger").is_some() {
                parse_swagger_20(spec)
            } else {
                Err("Unable to detect OpenAPI or Swagger format".to_string())
            }
        } else {
            Err("Unable to detect OpenAPI or Swagger format".to_string())
        }
    }
}

fn parse_openapi_30(spec: &str) -> Result<Vec<Request>, String> {
    let openapi: OpenApiSpec = if spec.trim().starts_with('{') {
        serde_json::from_str(spec).map_err(|e| e.to_string())?
    } else {
        serde_yaml::from_str(spec).map_err(|e| e.to_string())?
    };

    let security_schemes = openapi
        .components
        .as_ref()
        .and_then(|c| c.securitySchemes.as_ref().cloned());

    let global_security = openapi.security.clone();
    let mut requests = Vec::new();

    for (path, path_item) in openapi.paths {
        add_operation_requests(
            &path,
            &path_item,
            &security_schemes,
            &global_security,
            &mut requests,
        );
    }

    Ok(requests)
}

fn parse_swagger_20(spec: &str) -> Result<Vec<Request>, String> {
    let swagger: SwaggerSpec = if spec.trim().starts_with('{') {
        serde_json::from_str(spec).map_err(|e| e.to_string())?
    } else {
        serde_yaml::from_str(spec).map_err(|e| e.to_string())?
    };

    let security_schemes = swagger.securityDefinitions.as_ref().cloned();
    let global_security = swagger.security.clone();
    let mut requests = Vec::new();

    for (path, path_item) in swagger.paths {
        add_operation_requests(
            &path,
            &path_item,
            &security_schemes,
            &global_security,
            &mut requests,
        );
    }

    Ok(requests)
}

fn add_operation_requests(
    path: &str,
    path_item: &OpenApiPathItem,
    security_schemes: &Option<HashMap<String, OpenApiSecurityScheme>>,
    global_security: &Option<Vec<HashMap<String, Vec<String>>>>,
    requests: &mut Vec<Request>,
) {
    let operations: Vec<(Method, &Option<OpenApiOperation>)> = vec![
        (Method::Get, &path_item.get),
        (Method::Post, &path_item.post),
        (Method::Put, &path_item.put),
        (Method::Patch, &path_item.patch),
        (Method::Delete, &path_item.delete),
        (Method::Head, &path_item.head),
        (Method::Options, &path_item.options),
    ];

    for (method, op_opt) in operations {
        if let Some(op) = op_opt {
            eprintln!("DEBUG: Processing operation with method {:?}", method);
            eprintln!("DEBUG: op = {:?}", op);
            eprintln!("DEBUG: path = {}", path);
            // Keep the path as-is since it may contain path parameters like {id}
            let mut url = path.to_string();

            // Process parameters
            let mut headers = yinx_core::request::Headers::new();
            let mut has_query = url.contains('?');

            for param in &op.parameters {
                if let OpenApiParameter::Direct {
                    name,
                    location,
                    required: _,
                    schema: _,
                } = param
                {
                    match location.as_str() {
                        "query" => {
                            if !has_query {
                                url.push('?');
                                has_query = true;
                            } else {
                                url.push('&');
                            }
                            url.push_str(&format!("{}=", name));
                        }
                        "header" => {
                            let _ = headers.set(name, "");
                        }
                        "path" => {
                            // Path parameters are already in the URL template
                        }
                        _ => {}
                    }
                }
            }

            // Add security headers
            let security = op.security.as_ref().or(global_security.as_ref());
            if let Some(security) = security {
                for sec_map in security {
                    for (scheme_name, _) in sec_map {
                        if let Some(schemes) = security_schemes {
                            if let Some(scheme) = schemes.get(scheme_name) {
                                apply_security_scheme(scheme, &mut headers);
                            }
                        }
                    }
                }
            }

            // Handle request body
            eprintln!("DEBUG: op.request_body = {:?}", op.request_body);
            let body = if let Some(req_body) = &op.request_body {
                eprintln!("DEBUG: request body = {:?}", req_body);
                if let Some(json_content) = req_body.content.get("application/json") {
                    eprintln!("DEBUG: Found application/json content");
                    let result = headers.set("Content-Type", "application/json");
                    eprintln!("DEBUG: Set Content-Type header result: {:?}", result);
                    eprintln!("DEBUG: Headers after set: {:?}", headers);
                    RequestBody::Json(serde_json::json!({}))
                } else if let Some(form_content) =
                    req_body.content.get("application/x-www-form-urlencoded")
                {
                    let _ = headers.set("Content-Type", "application/x-www-form-urlencoded");
                    RequestBody::Form(vec![])
                } else {
                    RequestBody::None
                }
            } else {
                RequestBody::None
            };

            // Ensure URL has a scheme
            let url_to_parse = if url.starts_with("http") {
                url.clone()
            } else {
                format!("https://example.com{}", url)
            };
            eprintln!("DEBUG: url_to_parse = {}", url_to_parse);
            // For template URLs (containing {), don't validate with RequestUrl
            let req_url = if url_to_parse.contains('{') {
                // Create RequestUrl with template
                RequestUrl::new(&url_to_parse).unwrap_or_else(|_| {
                    eprintln!("DEBUG: Failed to parse template URL: {}", url_to_parse);
                    RequestUrl::new("https://example.com").unwrap()
                })
            } else {
                RequestUrl::new(&url_to_parse).unwrap_or_else(|_| {
                    eprintln!("DEBUG: Failed to parse URL: {}", url_to_parse);
                    RequestUrl::new("https://example.com").unwrap()
                })
            };
            eprintln!("DEBUG: req_url.as_str() = {}", req_url.as_str());

            requests.push(Request {
                method,
                url: req_url,
                headers,
                body,
                timeout_secs: 30,
            });
        }
    }
}

fn apply_security_scheme(
    scheme: &OpenApiSecurityScheme,
    headers: &mut yinx_core::request::Headers,
) {
    match scheme {
        OpenApiSecurityScheme::ApiKey {
            scheme_type: _,
            name,
            location,
        } => {
            if location == "header" {
                let _ = headers.set(name, "");
            }
        }
        OpenApiSecurityScheme::Http {
            scheme_type: _,
            scheme,
        } => {
            if scheme == "bearer" {
                let _ = headers.set("Authorization", "Bearer ");
            }
        }
        OpenApiSecurityScheme::OAuth2 { .. } => {
            let _ = headers.set("Authorization", "Bearer ");
        }
        _ => {}
    }
}

pub fn infer_workflow_edges(requests: &[Request]) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    let urls: Vec<&str> = requests.iter().map(|r| r.url.as_str()).collect();

    for (i, req1) in urls.iter().enumerate() {
        for (j, req2) in urls.iter().enumerate() {
            if i != j {
                // Check if req1 is a parent path of req2
                // e.g., /users is parent of /users/{id}
                let path1 = extract_path(req1);
                let path2 = extract_path(req2);

                if is_parent_child(&path1, &path2) {
                    edges.push((req1.to_string(), req2.to_string()));
                }
            }
        }
    }

    edges
}

fn extract_path(url: &str) -> String {
    // Remove query params and fragments
    let mut path = url.split('?').next().unwrap_or(url);
    path = path.split('#').next().unwrap_or(path);
    path.to_string()
}

fn is_parent_child(parent: &str, child: &str) -> bool {
    // Check if parent path segments are a prefix of child path segments
    let parent_parts: Vec<&str> = parent.split('/').filter(|s| !s.is_empty()).collect();
    let child_parts: Vec<&str> = child.split('/').filter(|s| !s.is_empty()).collect();

    if parent_parts.len() >= child_parts.len() {
        return false;
    }

    for (i, part) in parent_parts.iter().enumerate() {
        if i >= child_parts.len() {
            return false;
        }
        if *part != child_parts[i] && !child_parts[i].starts_with('{') {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // 5.19: OpenAPI 3.0 spec parser (paths → requests)
    #[test]
    fn test_parse_openapi_30_basic() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: Sample API
  version: "1.0.0"
paths:
  /users:
    get:
      operationId: getUsers
      summary: Get all users
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Get);
        assert!(requests[0].url.as_str().contains("/users"));
    }

    #[test]
    fn test_parse_multiple_paths_and_methods() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
    post:
      operationId: createUser
  /users/{id}:
    get:
      operationId: getUser
    delete:
      operationId: deleteUser
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert_eq!(requests.len(), 4);
    }

    // 5.20: Parse parameters (path, query, header, body)
    #[test]
    fn test_parse_path_parameters() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.as_str().contains("{id}"));
    }

    #[test]
    fn test_parse_query_parameters() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      parameters:
        - name: limit
          in: query
          required: false
          schema:
            type: integer
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0].url.as_str().contains("limit"));
    }

    #[test]
    fn test_parse_header_parameters() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      parameters:
        - name: X-Request-Id
          in: header
          required: false
          schema:
            type: string
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0].headers.contains("X-Request-Id"));
    }

    #[test]
    fn test_parse_request_body() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    post:
      operationId: createUser
      requestBody:
        content:
          application/json:
            schema:
              type: object
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert_eq!(requests[0].method, Method::Post);
        assert!(requests[0].headers.get("Content-Type").is_some());
    }

    // 5.21: Parse security schemes → auth configs
    #[test]
    fn test_parse_api_key_security() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      security:
        - apiKey: []
components:
  securitySchemes:
    apiKey:
      type: apiKey
      name: X-API-Key
      in: header
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0].headers.contains("X-API-Key"));
    }

    #[test]
    fn test_parse_bearer_security() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      security:
        - bearerAuth: []
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0]
            .headers
            .get("Authorization")
            .unwrap()
            .starts_with("Bearer"));
    }

    #[test]
    fn test_parse_oauth2_security() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
      security:
        - oauth2: []
components:
  securitySchemes:
    oauth2:
      type: oauth2
      flows:
        implicit:
          authorizationUrl: https://example.com/oauth/authorize
          tokenUrl: https://example.com/oauth/token
          scopes:
            read: Read access
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0]
            .headers
            .get("Authorization")
            .unwrap()
            .starts_with("Bearer"));
    }

    // 5.22: Infer workflow edges from path relationships
    #[test]
    fn test_infer_workflow_edges() {
        let requests = vec![
            Request {
                method: Method::Get,
                url: RequestUrl::new("https://api.example.com/users").unwrap(),
                headers: yinx_core::request::Headers::new(),
                body: RequestBody::None,
                timeout_secs: 30,
            },
            Request {
                method: Method::Get,
                url: RequestUrl::new("https://api.example.com/users/123").unwrap(),
                headers: yinx_core::request::Headers::new(),
                body: RequestBody::None,
                timeout_secs: 30,
            },
        ];
        let edges = infer_workflow_edges(&requests);
        assert!(!edges.is_empty());
    }

    #[test]
    fn test_infer_edges_parent_child() {
        let requests = vec![
            Request {
                method: Method::Get,
                url: RequestUrl::new("https://api.example.com/collections").unwrap(),
                headers: yinx_core::request::Headers::new(),
                body: RequestBody::None,
                timeout_secs: 30,
            },
            Request {
                method: Method::Get,
                url: RequestUrl::new("https://api.example.com/collections/abc123").unwrap(),
                headers: yinx_core::request::Headers::new(),
                body: RequestBody::None,
                timeout_secs: 30,
            },
            Request {
                method: Method::Get,
                url: RequestUrl::new("https://api.example.com/collections/abc123/items").unwrap(),
                headers: yinx_core::request::Headers::new(),
                body: RequestBody::None,
                timeout_secs: 30,
            },
        ];
        let edges = infer_workflow_edges(&requests);
        assert!(edges.len() >= 2);
    }

    // 5.23: Swagger 2.0 compatibility
    #[test]
    fn test_parse_swagger_20() {
        let yaml = r#"
swagger: "2.0"
info:
  title: Sample API
  version: "1.0.0"
paths:
  /users:
    get:
      operationId: getUsers
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::Get);
    }

    #[test]
    fn test_parse_swagger_20_with_security() {
        let yaml = r#"
swagger: "2.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
securityDefinitions:
  api_key:
    type: apiKey
    name: X-API-Key
    in: header
security:
  - api_key: []
"#;
        let requests = parse_openapi(yaml).unwrap();
        assert!(requests[0].headers.contains("X-API-Key"));
    }
}
