use std::collections::HashSet;
use yinx_core::request::Request;

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSource {
    Curl(String),
    Postman(String),
    Insomnia(String),
    OpenApi(String),
}

#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub items: Vec<ImportItem>,
    pub selected: HashSet<usize>,
    pub warnings: Vec<ImportWarning>,
    pub errors: Vec<ImportError>,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: String,
    pub request: Request,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub message: String,
    pub item_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ImportError {
    pub message: String,
    pub item_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ConflictResolution {
    pub original_name: String,
    pub resolved_name: String,
    pub strategy: RenameStrategy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RenameStrategy {
    KeepOriginal,
    Rename,
    Skip,
}

pub fn create_import_preview(source: ImportSource) -> Result<ImportPreview, String> {
    match source {
        ImportSource::Curl(cmd) => {
            let request = super::curl::parse_curl(&cmd).map_err(|e| format!("{:?}", e))?;
            let mut preview = ImportPreview {
                items: vec![ImportItem {
                    name: format!("{} {}", request.method, request.url.as_str()),
                    request,
                    source: "curl".to_string(),
                }],
                selected: HashSet::new(),
                warnings: vec![],
                errors: vec![],
            };
            preview.selected.insert(0);
            Ok(preview)
        }
        ImportSource::Postman(json) => {
            let (collection, _warnings) = super::postman::parse_collection_to_collection(&json)
                .map_err(|e| format!("{}", e))?;
            let items: Vec<ImportItem> = collection
                .flatten_requests()
                .into_iter()
                .map(|sr| ImportItem {
                    name: sr.name.clone(),
                    request: sr.request.clone(),
                    source: "Postman".to_string(),
                })
                .collect();
            let mut preview = ImportPreview {
                items,
                selected: HashSet::new(),
                warnings: vec![],
                errors: vec![],
            };
            for i in 0..preview.items.len() {
                preview.selected.insert(i);
            }
            Ok(preview)
        }
        ImportSource::Insomnia(json) => {
            let requests =
                super::insomnia::parse_insomnia_export(&json).map_err(|e| format!("{}", e))?;
            let items: Vec<ImportItem> = requests
                .into_iter()
                .enumerate()
                .map(|(i, req)| ImportItem {
                    name: format!("Request {}", i),
                    request: req,
                    source: "Insomnia".to_string(),
                })
                .collect();
            let mut preview = ImportPreview {
                items,
                selected: HashSet::new(),
                warnings: vec![],
                errors: vec![],
            };
            for i in 0..preview.items.len() {
                preview.selected.insert(i);
            }
            Ok(preview)
        }
        ImportSource::OpenApi(spec) => {
            let requests = super::openapi::parse_openapi(&spec).map_err(|e| format!("{}", e))?;
            let items: Vec<ImportItem> = requests
                .into_iter()
                .map(|req| ImportItem {
                    name: format!("{} {}", req.method, req.url.as_str()),
                    request: req,
                    source: "OpenAPI".to_string(),
                })
                .collect();
            let mut preview = ImportPreview {
                items,
                selected: HashSet::new(),
                warnings: vec![],
                errors: vec![],
            };
            for i in 0..preview.items.len() {
                preview.selected.insert(i);
            }
            Ok(preview)
        }
    }
}

pub fn toggle_selection(preview: &mut ImportPreview, index: usize) {
    if preview.selected.contains(&index) {
        preview.selected.remove(&index);
    } else {
        preview.selected.insert(index);
    }
}

pub fn select_all(preview: &mut ImportPreview) {
    for i in 0..preview.items.len() {
        preview.selected.insert(i);
    }
}

pub fn deselect_all(preview: &mut ImportPreview) {
    preview.selected.clear();
}

pub fn get_selected_items(preview: &ImportPreview) -> Vec<&ImportItem> {
    preview
        .items
        .iter()
        .enumerate()
        .filter(|(i, _)| preview.selected.contains(i))
        .map(|(_, item)| item)
        .collect()
}

pub fn validate_import(preview: &ImportPreview) -> (Vec<ImportWarning>, Vec<ImportError>) {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    for (i, item) in preview.items.iter().enumerate() {
        // Check for duplicate names
        let duplicates: Vec<usize> = preview
            .items
            .iter()
            .enumerate()
            .filter(|(_, other)| other.name == item.name)
            .map(|(j, _)| j)
            .collect();
        if duplicates.len() > 1 {
            warnings.push(ImportWarning {
                message: format!("Duplicate name: {}", item.name),
                item_index: Some(i),
            });
        }

        // Check for empty URLs
        if item.request.url.as_str().is_empty() {
            errors.push(ImportError {
                message: "Empty URL".to_string(),
                item_index: Some(i),
            });
        }
    }

    (warnings, errors)
}

pub fn resolve_conflicts(items: Vec<ImportItem>) -> (Vec<ImportItem>, Vec<ConflictResolution>) {
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut resolved = Vec::new();
    let mut resolutions = Vec::new();

    for item in items {
        let original_name = item.name.clone();
        if seen_names.contains(&item.name) {
            // Generate a unique name
            let mut new_name = item.name.clone();
            let mut counter = 1;
            while seen_names.contains(&new_name) {
                new_name = format!("{} ({})", item.name, counter);
                counter += 1;
            }
            let resolved_item = ImportItem {
                name: new_name.clone(),
                request: item.request,
                source: item.source,
            };
            resolutions.push(ConflictResolution {
                original_name,
                resolved_name: new_name,
                strategy: RenameStrategy::Rename,
            });
            seen_names.insert(resolved_item.name.clone());
            resolved.push(resolved_item);
        } else {
            seen_names.insert(item.name.clone());
            resolved.push(item);
        }
    }

    (resolved, resolutions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::request::{Method, RequestBody, RequestUrl};

    // 5.24: ImportPreview struct (parsed items + selection state)
    #[test]
    fn test_import_preview_creation() {
        let curl_cmd = "curl https://api.example.com/users";
        let preview = create_import_preview(ImportSource::Curl(curl_cmd.to_string())).unwrap();
        assert_eq!(preview.items.len(), 1);
        assert_eq!(preview.selected.len(), 1);
    }

    #[test]
    fn test_toggle_selection() {
        let curl_cmd = "curl https://api.example.com/users";
        let mut preview = create_import_preview(ImportSource::Curl(curl_cmd.to_string())).unwrap();
        assert!(preview.selected.contains(&0));

        toggle_selection(&mut preview, 0);
        assert!(!preview.selected.contains(&0));

        toggle_selection(&mut preview, 0);
        assert!(preview.selected.contains(&0));
    }

    #[test]
    fn test_select_all_and_deselect_all() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: API
  version: "1.0"
paths:
  /users:
    get:
      operationId: getUsers
  /users/:
    post:
      operationId: createUser
"#;
        let mut preview = create_import_preview(ImportSource::OpenApi(yaml.to_string())).unwrap();
        assert_eq!(preview.items.len(), 2);

        deselect_all(&mut preview);
        assert!(preview.selected.is_empty());

        select_all(&mut preview);
        assert_eq!(preview.selected.len(), 2);
    }

    #[test]
    fn test_get_selected_items() {
        let curl_cmd = "curl https://api.example.com/users";
        let mut preview = create_import_preview(ImportSource::Curl(curl_cmd.to_string())).unwrap();
        let selected = get_selected_items(&preview);
        assert_eq!(selected.len(), 1);

        deselect_all(&mut preview);
        let selected = get_selected_items(&preview);
        assert!(selected.is_empty());
    }

    // 5.25: Import validation report (warnings, errors)
    #[test]
    fn test_validate_import_no_issues() {
        let curl_cmd = "curl https://api.example.com/users";
        let preview = create_import_preview(ImportSource::Curl(curl_cmd.to_string())).unwrap();
        let (warnings, errors) = validate_import(&preview);
        assert!(warnings.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_import_detects_duplicate_names() {
        let items = vec![
            ImportItem {
                name: "Request 0".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
            ImportItem {
                name: "Request 0".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
        ];
        let preview = ImportPreview {
            items,
            selected: HashSet::new(),
            warnings: vec![],
            errors: vec![],
        };
        let (warnings, _) = validate_import(&preview);
        assert!(!warnings.is_empty());
        assert!(warnings[0].message.contains("Duplicate"));
    }

    // 5.26: Conflict resolution (duplicate names)
    #[test]
    fn test_resolve_conflicts_no_conflict() {
        let items = vec![
            ImportItem {
                name: "Request A".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/a").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
            ImportItem {
                name: "Request B".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/b").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
        ];
        let (resolved, resolutions) = resolve_conflicts(items);
        assert_eq!(resolved.len(), 2);
        assert!(resolutions.is_empty());
    }

    #[test]
    fn test_resolve_conflicts_with_duplicates() {
        let items = vec![
            ImportItem {
                name: "Request".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/1").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
            ImportItem {
                name: "Request".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/2").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
        ];
        let (resolved, resolutions) = resolve_conflicts(items);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].strategy, RenameStrategy::Rename);
        assert!(resolutions[0].resolved_name.contains("(1)"));
    }

    #[test]
    fn test_resolve_multiple_conflicts() {
        let items = vec![
            ImportItem {
                name: "Request".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/1").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
            ImportItem {
                name: "Request".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/2").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
            ImportItem {
                name: "Request".to_string(),
                request: Request {
                    method: Method::Get,
                    url: RequestUrl::new("https://example.com/3").unwrap(),
                    headers: yinx_core::request::Headers::new(),
                    body: RequestBody::None,
                    timeout_secs: 30,
                },
                source: "test".to_string(),
            },
        ];
        let (resolved, resolutions) = resolve_conflicts(items);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolutions.len(), 2);
        // Check that all names are unique
        let names: HashSet<_> = resolved.iter().map(|i| i.name.clone()).collect();
        assert_eq!(names.len(), 3);
    }
}
