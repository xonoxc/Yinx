use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum VariableError {
    #[error("Variable '{0}' not found in any scope")]
    NotFound(String),
    #[error("Invalid JSONPath expression: {0}")]
    InvalidJsonPath(String),
    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(String),
    #[error("Regex capture failed")]
    RegexCaptureFailed,
    #[error("JSONPath extraction failed: {0}")]
    JsonPathExtractionFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariableScope {
    Global,
    Workflow,
    Local,
}

#[derive(Debug, Clone)]
pub struct VariableStore {
    global: HashMap<String, Value>,
    workflow: HashMap<String, Value>,
    local: HashMap<String, Value>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self {
            global: HashMap::new(),
            workflow: HashMap::new(),
            local: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: Value, scope: VariableScope) {
        match scope {
            VariableScope::Global => self.global.insert(key.into(), value),
            VariableScope::Workflow => self.workflow.insert(key.into(), value),
            VariableScope::Local => self.local.insert(key.into(), value),
        };
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.local
            .get(key)
            .or_else(|| self.workflow.get(key))
            .or_else(|| self.global.get(key))
            .cloned()
    }

    pub fn get_from_scope(&self, key: &str, scope: &VariableScope) -> Option<Value> {
        match scope {
            VariableScope::Global => self.global.get(key).cloned(),
            VariableScope::Workflow => self.workflow.get(key).cloned(),
            VariableScope::Local => self.local.get(key).cloned(),
        }
    }

    pub fn remove(&mut self, key: &str, scope: VariableScope) {
        match scope {
            VariableScope::Global => self.global.remove(key),
            VariableScope::Workflow => self.workflow.remove(key),
            VariableScope::Local => self.local.remove(key),
        };
    }

    pub fn clear_scope(&mut self, scope: VariableScope) {
        match scope {
            VariableScope::Global => self.global.clear(),
            VariableScope::Workflow => self.workflow.clear(),
            VariableScope::Local => self.local.clear(),
        }
    }

    pub fn merge_into(&mut self, other: &VariableStore, target_scope: VariableScope) {
        match target_scope {
            VariableScope::Global => {
                for (k, v) in &other.global {
                    self.global.insert(k.clone(), v.clone());
                }
            }
            VariableScope::Workflow => {
                for (k, v) in &other.workflow {
                    self.workflow.insert(k.clone(), v.clone());
                }
            }
            VariableScope::Local => {
                for (k, v) in &other.local {
                    self.local.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

impl Default for VariableStore {
    fn default() -> Self {
        Self::new()
    }
}

pub fn interpolate(text: &str, store: &VariableStore) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}|\{\{([^}]+)\}\}").unwrap();
    let mut result = text.to_string();
    for cap in re.captures_iter(text) {
        let (full_match, var_name) = if let Some(m) = cap.get(1) {
            (cap.get(0).unwrap().as_str(), m.as_str())
        } else if let Some(m) = cap.get(2) {
            (cap.get(0).unwrap().as_str(), m.as_str())
        } else {
            continue;
        };
        if let Some(value) = store.get(var_name.trim()) {
            let replacement = match value {
                Value::String(s) => s,
                _ => value.to_string(),
            };
            result = result.replace(full_match, &replacement);
        }
    }
    result
}

pub fn extract_jsonpath(json: &Value, path: &str) -> Result<Value, VariableError> {
    if path.is_empty() || !path.starts_with('$') {
        return Err(VariableError::InvalidJsonPath(path.to_string()));
    }
    let parts = parse_jsonpath(path)?;
    let mut current = json;
    for part in &parts {
        match part {
            JsonPathPart::Root => {}
            JsonPathPart::Field(name) => {
                if let Value::Object(map) = current {
                    if let Some(v) = map.get(name) {
                        current = v;
                    } else {
                        return Err(VariableError::JsonPathExtractionFailed(format!(
                            "Field '{}' not found",
                            name
                        )));
                    }
                } else {
                    return Err(VariableError::JsonPathExtractionFailed(
                        "Not an object".to_string(),
                    ));
                }
            }
            JsonPathPart::Index(idx) => {
                if let Value::Array(arr) = current {
                    if let Some(v) = arr.get(*idx) {
                        current = v;
                    } else {
                        return Err(VariableError::JsonPathExtractionFailed(format!(
                            "Index {} out of bounds",
                            idx
                        )));
                    }
                } else {
                    return Err(VariableError::JsonPathExtractionFailed(
                        "Not an array".to_string(),
                    ));
                }
            }
        }
    }
    Ok(current.clone())
}

#[derive(Debug)]
enum JsonPathPart {
    Root,
    Field(String),
    Index(usize),
}

fn parse_jsonpath(path: &str) -> Result<Vec<JsonPathPart>, VariableError> {
    let mut parts = Vec::new();
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0;
    if chars.get(0) == Some(&'$') {
        parts.push(JsonPathPart::Root);
        i = 1;
    }
    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
                if i >= chars.len() {
                    return Err(VariableError::InvalidJsonPath(path.to_string()));
                }
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                    i += 1;
                }
                let field: String = chars[start..i].iter().collect();
                if !field.is_empty() {
                    parts.push(JsonPathPart::Field(field));
                }
            }
            '[' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(VariableError::InvalidJsonPath(path.to_string()));
                }
                let idx_str: String = chars[start..i].iter().collect();
                if let Ok(idx) = idx_str.parse::<usize>() {
                    parts.push(JsonPathPart::Index(idx));
                }
                i += 1;
            }
            _ => {
                return Err(VariableError::InvalidJsonPath(path.to_string()));
            }
        }
    }
    Ok(parts)
}

pub fn extract_regex(
    text: &str,
    pattern: &str,
    capture_name: &str,
) -> Result<String, VariableError> {
    let re = Regex::new(pattern).map_err(|e| VariableError::InvalidRegex(e.to_string()))?;
    if let Some(caps) = re.captures(text) {
        if let Some(m) = caps.name(capture_name) {
            Ok(m.as_str().to_string())
        } else if let Some(m) = caps.get(1) {
            Ok(m.as_str().to_string())
        } else {
            Err(VariableError::RegexCaptureFailed)
        }
    } else {
        Err(VariableError::RegexCaptureFailed)
    }
}

pub fn extract_headers(
    response_headers: &HashMap<String, String>,
    header_name: &str,
) -> Option<String> {
    response_headers.get(header_name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_variable_store_new() {
        let store = VariableStore::new();
        assert!(store.global.is_empty());
        assert!(store.workflow.is_empty());
        assert!(store.local.is_empty());
    }

    #[test]
    fn test_variable_store_set_and_get() {
        let mut store = VariableStore::new();
        store.set(
            "base_url",
            json!("https://api.example.com"),
            VariableScope::Global,
        );
        let val = store.get("base_url").unwrap();
        assert_eq!(val, json!("https://api.example.com"));
    }

    #[test]
    fn test_variable_store_scope_isolation() {
        let mut store = VariableStore::new();
        store.set("var", json!("global"), VariableScope::Global);
        store.set("var", json!("workflow"), VariableScope::Workflow);
        store.set("var", json!("local"), VariableScope::Local);
        assert_eq!(
            store.get_from_scope("var", &VariableScope::Global).unwrap(),
            json!("global")
        );
        assert_eq!(
            store
                .get_from_scope("var", &VariableScope::Workflow)
                .unwrap(),
            json!("workflow")
        );
        assert_eq!(
            store.get_from_scope("var", &VariableScope::Local).unwrap(),
            json!("local")
        );
        assert_eq!(store.get("var").unwrap(), json!("local"));
    }

    #[test]
    fn test_variable_store_scope_priority() {
        let mut store = VariableStore::new();
        store.set("var", json!("global"), VariableScope::Global);
        store.set("var", json!("workflow"), VariableScope::Workflow);
        store.set("var", json!("local"), VariableScope::Local);
        assert_eq!(store.get("var").unwrap(), json!("local"));
        store.remove("var", VariableScope::Local);
        assert_eq!(store.get("var").unwrap(), json!("workflow"));
        store.remove("var", VariableScope::Workflow);
        assert_eq!(store.get("var").unwrap(), json!("global"));
    }

    #[test]
    fn test_variable_store_not_found() {
        let store = VariableStore::new();
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_variable_store_remove() {
        let mut store = VariableStore::new();
        store.set("var", json!("value"), VariableScope::Global);
        assert!(store.get("var").is_some());
        store.remove("var", VariableScope::Global);
        assert!(store.get("var").is_none());
    }

    #[test]
    fn test_variable_store_clear_scope() {
        let mut store = VariableStore::new();
        store.set("var1", json!("v1"), VariableScope::Global);
        store.set("var2", json!("v2"), VariableScope::Global);
        store.set("var3", json!("v3"), VariableScope::Workflow);
        store.clear_scope(VariableScope::Global);
        assert!(store.get("var1").is_none());
        assert!(store.get("var3").is_some());
    }

    #[test]
    fn test_variable_store_merge_into() {
        let mut store1 = VariableStore::new();
        store1.set("a", json!("1"), VariableScope::Global);
        let mut store2 = VariableStore::new();
        store2.set("b", json!("2"), VariableScope::Global);
        store2.set("c", json!("3"), VariableScope::Workflow);
        store1.merge_into(&store2, VariableScope::Workflow);
        assert_eq!(store1.get("a").unwrap(), json!("1"));
        assert!(store1.get("b").is_none());
        assert_eq!(store1.get("c").unwrap(), json!("3"));
    }

    #[test]
    fn test_interpolate_dollar_syntax() {
        let mut store = VariableStore::new();
        store.set(
            "base_url",
            json!("https://api.example.com"),
            VariableScope::Global,
        );
        store.set("endpoint", json!("/users"), VariableScope::Global);
        let result = interpolate("${base_url}${endpoint}", &store);
        assert_eq!(result, "https://api.example.com/users");
    }

    #[test]
    fn test_interpolate_double_brace_syntax() {
        let mut store = VariableStore::new();
        store.set("name", json!("John"), VariableScope::Global);
        let result = interpolate("Hello {{ name }}!", &store);
        assert_eq!(result, "Hello John!");
    }

    #[test]
    fn test_interpolate_mixed_syntax() {
        let mut store = VariableStore::new();
        store.set("a", json!("1"), VariableScope::Global);
        store.set("b", json!("2"), VariableScope::Global);
        let result = interpolate("${a} and {{ b }}", &store);
        assert_eq!(result, "1 and 2");
    }

    #[test]
    fn test_interpolate_missing_var_unchanged() {
        let store = VariableStore::new();
        let result = interpolate("${missing}", &store);
        assert_eq!(result, "${missing}");
    }

    #[test]
    fn test_interpolate_nested_var() {
        let mut store = VariableStore::new();
        store.set("val", json!("world"), VariableScope::Global);
        let result = interpolate("Hello ${val}!", &store);
        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_interpolate_json_value() {
        let mut store = VariableStore::new();
        store.set("count", json!(42), VariableScope::Global);
        let result = interpolate("Count: ${count}", &store);
        assert_eq!(result, "Count: 42");
    }

    #[test]
    fn test_jsonpath_simple_field() {
        let json = json!({"name": "John", "age": 30});
        let result = extract_jsonpath(&json, "$.name").unwrap();
        assert_eq!(result, json!("John"));
    }

    #[test]
    fn test_jsonpath_nested_field() {
        let json = json!({"user": {"name": "John", "age": 30}});
        let result = extract_jsonpath(&json, "$.user.name").unwrap();
        assert_eq!(result, json!("John"));
    }

    #[test]
    fn test_jsonpath_array_index() {
        let json = json!({"items": ["a", "b", "c"]});
        let result = extract_jsonpath(&json, "$.items[0]").unwrap();
        assert_eq!(result, json!("a"));
    }

    #[test]
    fn test_jsonpath_array_in_nested() {
        let json = json!({"data": {"items": [{"id": 1}, {"id": 2}]}});
        let result = extract_jsonpath(&json, "$.data.items[1].id").unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn test_jsonpath_deeply_nested() {
        let json = json!({"a": {"b": {"c": {"d": "deep"}}}});
        let result = extract_jsonpath(&json, "$.a.b.c.d").unwrap();
        assert_eq!(result, json!("deep"));
    }

    #[test]
    fn test_jsonpath_array_multiple_indices() {
        let json = json!({"matrix": [[1, 2], [3, 4]]});
        let result = extract_jsonpath(&json, "$.matrix[1][0]").unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn test_jsonpath_field_in_array() {
        let json = json!({"users": [{"name": "Alice"}, {"name": "Bob"}]});
        let result = extract_jsonpath(&json, "$.users[1].name").unwrap();
        assert_eq!(result, json!("Bob"));
    }

    #[test]
    fn test_jsonpath_number_value() {
        let json = json!({"count": 42});
        let result = extract_jsonpath(&json, "$.count").unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn test_jsonpath_bool_value() {
        let json = json!({"active": true});
        let result = extract_jsonpath(&json, "$.active").unwrap();
        assert_eq!(result, json!(true));
    }

    #[test]
    fn test_jsonpath_null_value() {
        let json = json!({"value": null});
        let result = extract_jsonpath(&json, "$.value").unwrap();
        assert_eq!(result, json!(null));
    }

    #[test]
    fn test_jsonpath_invalid_path() {
        let json = json!({"name": "John"});
        let result = extract_jsonpath(&json, "invalid");
        assert!(matches!(result, Err(VariableError::InvalidJsonPath(_))));
    }

    #[test]
    fn test_jsonpath_field_not_found() {
        let json = json!({"name": "John"});
        let result = extract_jsonpath(&json, "$.age");
        assert!(matches!(
            result,
            Err(VariableError::JsonPathExtractionFailed(_))
        ));
    }

    #[test]
    fn test_jsonpath_index_out_of_bounds() {
        let json = json!({"items": [1, 2]});
        let result = extract_jsonpath(&json, "$.items[5]");
        assert!(matches!(
            result,
            Err(VariableError::JsonPathExtractionFailed(_))
        ));
    }

    #[test]
    fn test_jsonpath_not_object_for_field() {
        let json = json!([1, 2, 3]);
        let result = extract_jsonpath(&json, "$.name");
        assert!(matches!(
            result,
            Err(VariableError::JsonPathExtractionFailed(_))
        ));
    }

    #[test]
    fn test_jsonpath_not_array_for_index() {
        let json = json!({"items": "not array"});
        let result = extract_jsonpath(&json, "$.items[0]");
        assert!(matches!(
            result,
            Err(VariableError::JsonPathExtractionFailed(_))
        ));
    }

    #[test]
    fn test_regex_extract_named_capture() {
        let text = "X-Request-Id: abc-123-def";
        let result = extract_regex(text, r"X-Request-Id: (?P<id>\w+-\w+-\w+)", "id").unwrap();
        assert_eq!(result, "abc-123-def");
    }

    #[test]
    fn test_regex_extract_unnamed_capture() {
        let text = "id=12345";
        let result = extract_regex(text, r"id=(\d+)", "capture").unwrap();
        assert_eq!(result, "12345");
    }

    #[test]
    fn test_regex_extract_header_pattern() {
        let text = "Content-Type: application/json";
        let result = extract_regex(text, r"Content-Type: (.+)", "ct").unwrap();
        assert_eq!(result, "application/json");
    }

    #[test]
    fn test_regex_extract_failed() {
        let text = "no match here";
        let result = extract_regex(text, r"\d+", "num");
        assert!(matches!(result, Err(VariableError::RegexCaptureFailed)));
    }

    #[test]
    fn test_regex_invalid_pattern() {
        let result = extract_regex("text", r"[invalid", "cap");
        assert!(matches!(result, Err(VariableError::InvalidRegex(_))));
    }

    #[test]
    fn test_header_extractor() {
        let mut headers = HashMap::new();
        headers.insert("X-Request-Id".to_string(), "abc-123".to_string());
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        let result = extract_headers(&headers, "X-Request-Id");
        assert_eq!(result, Some("abc-123".to_string()));
    }

    #[test]
    fn test_header_extractor_not_found() {
        let headers = HashMap::new();
        let result = extract_headers(&headers, "X-Request-Id");
        assert_eq!(result, None);
    }

    #[test]
    fn test_header_extractor_case_sensitive() {
        let mut headers = HashMap::new();
        headers.insert("x-request-id".to_string(), "lowercase".to_string());
        let result = extract_headers(&headers, "X-Request-Id");
        assert_eq!(result, None);
    }
}
