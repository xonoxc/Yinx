use std::collections::HashMap;

use crate::environments::Environment;
use crate::request::{Request, RequestBody, RequestUrl};

#[derive(Debug, Clone)]
pub struct VariableEngine {
    active_environment: Option<Environment>,
    global_variables: HashMap<String, String>,
}

impl Default for VariableEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableEngine {
    pub fn new() -> Self {
        Self {
            active_environment: None,
            global_variables: HashMap::new(),
        }
    }

    pub fn with_environment(mut self, env: Environment) -> Self {
        self.active_environment = Some(env);
        self
    }

    pub fn set_environment(&mut self, env: Option<Environment>) {
        self.active_environment = env;
    }

    pub fn active_environment(&self) -> Option<&Environment> {
        self.active_environment.as_ref()
    }

    pub fn set_global(&mut self, key: String, value: String) {
        self.global_variables.insert(key, value);
    }

    pub fn remove_global(&mut self, key: &str) {
        self.global_variables.remove(key);
    }

    pub fn get_global(&self, key: &str) -> Option<&str> {
        self.global_variables.get(key).map(|s| s.as_str())
    }

    pub fn get_variable(&self, key: &str) -> Option<String> {
        if let Some(ref env) = self.active_environment {
            if let Some(var) = env.get_variable(key) {
                if var.enabled {
                    return Some(var.value.clone());
                }
            }
        }
        self.global_variables.get(key).cloned()
    }

    pub fn interpolate(&self, input: &str) -> String {
        let mut result = input.to_string();
        result = self.interpolate_environment(&result);
        result = self.interpolate_globals(&result);
        result = self.interpolate_dynamics(&result);
        result
    }

    fn interpolate_environment(&self, input: &str) -> String {
        if let Some(ref env) = self.active_environment {
            let mut result = input.to_string();
            for var in env.enabled_variables() {
                let pattern = format!("{{{{{}}}}}", var.key);
                result = result.replace(&pattern, &var.value);
            }
            result
        } else {
            input.to_string()
        }
    }

    fn interpolate_globals(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (key, value) in &self.global_variables {
            let pattern = format!("{{{{{}}}}}", key);
            result = result.replace(&pattern, value);
        }
        result
    }

    fn interpolate_dynamics(&self, input: &str) -> String {
        let mut result = input.to_string();
        result = result.replace("{{$guid}}", &uuid::Uuid::new_v4().to_string());
        result = result.replace(
            "{{$timestamp}}",
            &chrono::Utc::now().timestamp().to_string(),
        );
        let random_int: u32 = rand::random::<u32>() % 10000;
        result = result.replace("{{$randomInt}}", &random_int.to_string());
        result
    }

    pub fn interpolate_request(&self, request: &mut Request) {
        let url_str = self.interpolate(request.url.as_str());
        if let Ok(new_url) = RequestUrl::new(&url_str) {
            request.url = new_url;
        }

        let mut new_headers = crate::request::Headers::new();
        for h in request.headers.iter() {
            let key = self.interpolate(&h.name);
            let value = self.interpolate(&h.value);
            let _ = new_headers.set(&key, &value);
        }
        request.headers = new_headers;

        request.body = match std::mem::take(&mut request.body) {
            RequestBody::Raw(s) => RequestBody::Raw(self.interpolate(&s)),
            RequestBody::Json(v) => {
                let json_str = serde_json::to_string(&v).unwrap_or_default();
                let interpolated = self.interpolate(&json_str);
                if let Ok(new_v) = serde_json::from_str(&interpolated) {
                    RequestBody::Json(new_v)
                } else {
                    RequestBody::Raw(interpolated)
                }
            }
            RequestBody::Form(pairs) => {
                let new_pairs: Vec<(String, String)> = pairs
                    .into_iter()
                    .map(|(k, v)| (self.interpolate(&k), self.interpolate(&v)))
                    .collect();
                RequestBody::Form(new_pairs)
            }
            RequestBody::Multipart(pairs) => {
                let new_pairs: Vec<(String, String)> = pairs
                    .into_iter()
                    .map(|(k, v)| (self.interpolate(&k), self.interpolate(&v)))
                    .collect();
                RequestBody::Multipart(new_pairs)
            }
            other => other,
        };
    }

    pub fn extract_variables(&self, input: &str) -> Vec<String> {
        let mut variables = Vec::new();
        let mut remaining = input;
        while let Some(start) = remaining.find("{{") {
            if let Some(end) = remaining[start..].find("}}") {
                let var_name = &remaining[start + 2..start + end];
                if !var_name.starts_with('$') {
                    variables.push(var_name.to_string());
                }
                remaining = &remaining[start + end + 2..];
            } else {
                break;
            }
        }
        variables
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environments::EnvironmentVariable;
    use crate::request::{Method, RequestBuilder};

    fn make_engine() -> VariableEngine {
        let mut env = Environment::new("Staging".to_string());
        env.add_variable(EnvironmentVariable::new(
            "base_url".to_string(),
            "https://api.example.com".to_string(),
        ));
        env.add_variable(EnvironmentVariable::new(
            "token".to_string(),
            "abc123".to_string(),
        ));
        VariableEngine::new().with_environment(env)
    }

    #[test]
    fn test_interpolate_simple() {
        let engine = make_engine();
        assert_eq!(
            engine.interpolate("{{base_url}}/users"),
            "https://api.example.com/users"
        );
    }

    #[test]
    fn test_interpolate_multiple() {
        let engine = make_engine();
        assert_eq!(
            engine.interpolate("{{base_url}}/users?token={{token}}"),
            "https://api.example.com/users?token=abc123"
        );
    }

    #[test]
    fn test_interpolate_no_variables() {
        let engine = make_engine();
        assert_eq!(engine.interpolate("/plain/path"), "/plain/path");
    }

    #[test]
    fn test_interpolate_missing_variable() {
        let engine = make_engine();
        let result = engine.interpolate("{{missing}}/path");
        assert_eq!(result, "{{missing}}/path");
    }

    #[test]
    fn test_interpolate_global_variable() {
        let mut engine = make_engine();
        engine.set_global("api_key".to_string(), "secret".to_string());
        assert_eq!(engine.interpolate("key={{api_key}}"), "key=secret");
    }

    #[test]
    fn test_dynamic_guid() {
        let engine = VariableEngine::new();
        let result = engine.interpolate("{{$guid}}");
        assert_eq!(result.len(), 36);
        assert_eq!(result.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_dynamic_timestamp() {
        let engine = VariableEngine::new();
        let result = engine.interpolate("{{$timestamp}}");
        let ts: i64 = result.parse().unwrap();
        let now = chrono::Utc::now().timestamp();
        assert!((ts - now).abs() < 2);
    }

    #[test]
    fn test_dynamic_random_int() {
        let engine = VariableEngine::new();
        let result = engine.interpolate("{{$randomInt}}");
        let val: u32 = result.parse().unwrap();
        assert!(val < 10000);
    }

    #[test]
    fn test_interpolate_request_url() {
        let engine = make_engine();
        let mut request = RequestBuilder::new()
            .url("{{base_url}}/users")
            .build()
            .unwrap();
        engine.interpolate_request(&mut request);
        assert_eq!(request.url.as_str(), "https://api.example.com/users");
    }

    #[test]
    fn test_interpolate_request_headers() {
        let engine = make_engine();
        let mut request = RequestBuilder::new()
            .url("https://example.com")
            .header("Authorization", "Bearer {{token}}")
            .build()
            .unwrap();
        engine.interpolate_request(&mut request);
        assert_eq!(
            request.headers.get("Authorization"),
            Some("Bearer abc123")
        );
    }

    #[test]
    fn test_extract_variables() {
        let engine = VariableEngine::new();
        let vars = engine.extract_variables("{{base_url}}/{{path}}?key={{token}}");
        assert_eq!(vars, vec!["base_url", "path", "token"]);
    }

    #[test]
    fn test_extract_variables_skips_dynamic() {
        let engine = VariableEngine::new();
        let vars = engine.extract_variables("{{$guid}}/{{path}}");
        assert_eq!(vars, vec!["path"]);
    }

    #[test]
    fn test_extract_variables_empty() {
        let engine = VariableEngine::new();
        let vars = engine.extract_variables("/no/variables");
        assert!(vars.is_empty());
    }

    #[test]
    fn test_no_environment_returns_input_unchanged() {
        let engine = VariableEngine::new();
        assert_eq!(
            engine.interpolate("{{base_url}}/users"),
            "{{base_url}}/users"
        );
    }

    #[test]
    fn test_disabled_variable_not_interpolated() {
        let mut env = Environment::new("Test".to_string());
        env.add_variable(EnvironmentVariable {
            key: "secret".to_string(),
            value: "should_not_appear".to_string(),
            enabled: false,
        });
        let engine = VariableEngine::new().with_environment(env);
        assert_eq!(engine.interpolate("{{secret}}"), "{{secret}}");
    }

    #[test]
    fn test_environment_switch() {
        let mut engine = VariableEngine::new();
        let mut staging = Environment::new("Staging".to_string());
        staging.add_variable(EnvironmentVariable::new(
            "host".to_string(),
            "staging.example.com".to_string(),
        ));
        let mut prod = Environment::new("Production".to_string());
        prod.add_variable(EnvironmentVariable::new(
            "host".to_string(),
            "api.example.com".to_string(),
        ));

        engine.set_environment(Some(staging));
        assert_eq!(engine.interpolate("{{host}}"), "staging.example.com");

        engine.set_environment(Some(prod));
        assert_eq!(engine.interpolate("{{host}}"), "api.example.com");
    }

    #[test]
    fn test_interpolate_request_body_raw() {
        let engine = make_engine();
        let mut request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://example.com")
            .body(RequestBody::Raw("token={{token}}".to_string()))
            .build()
            .unwrap();
        engine.interpolate_request(&mut request);
        match request.body {
            RequestBody::Raw(s) => assert_eq!(s, "token=abc123"),
            _ => panic!("Expected Raw body"),
        }
    }

    #[test]
    fn test_interpolate_request_body_form() {
        let engine = make_engine();
        let mut request = RequestBuilder::new()
            .method(Method::Post)
            .url("https://example.com")
            .body(RequestBody::Form(vec![(
                "token".to_string(),
                "{{token}}".to_string(),
            )]))
            .build()
            .unwrap();
        engine.interpolate_request(&mut request);
        match request.body {
            RequestBody::Form(ref pairs) => {
                assert_eq!(pairs[0].1, "abc123");
            }
            _ => panic!("Expected Form body"),
        }
    }

    #[test]
    fn test_serde_roundtrip_request_after_interpolation() {
        let engine = make_engine();
        let mut request = RequestBuilder::new()
            .url("{{base_url}}/users")
            .header("X-Token", "{{token}}")
            .build()
            .unwrap();
        engine.interpolate_request(&mut request);
        let json = serde_json::to_string(&request).unwrap();
        let decoded: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.url.as_str(), "https://api.example.com/users");
        assert_eq!(decoded.headers.get("X-Token"), Some("abc123"));
    }

    #[test]
    fn test_get_variable_from_environment() {
        let engine = make_engine();
        assert_eq!(
            engine.get_variable("base_url"),
            Some("https://api.example.com".to_string())
        );
        assert!(engine.get_variable("nonexistent").is_none());
    }

    #[test]
    fn test_get_variable_from_globals() {
        let mut engine = VariableEngine::new();
        engine.set_global("global_key".to_string(), "global_val".to_string());
        assert_eq!(
            engine.get_variable("global_key"),
            Some("global_val".to_string())
        );
    }

    #[test]
    fn test_get_variable_priority() {
        let mut env = Environment::new("Test".to_string());
        env.add_variable(EnvironmentVariable::new(
            "key".to_string(),
            "env_value".to_string(),
        ));
        let mut engine = VariableEngine::new().with_environment(env);
        engine.set_global("key".to_string(), "global_value".to_string());
        assert_eq!(
            engine.get_variable("key"),
            Some("env_value".to_string())
        );
    }

    #[test]
    fn test_remove_global() {
        let mut engine = VariableEngine::new();
        engine.set_global("temp".to_string(), "value".to_string());
        assert!(engine.get_variable("temp").is_some());
        engine.remove_global("temp");
        assert!(engine.get_variable("temp").is_none());
    }

    #[test]
    fn test_extract_variables_no_duplicates() {
        let engine = VariableEngine::new();
        let vars = engine.extract_variables("{{a}} {{b}} {{a}}");
        assert_eq!(vars, vec!["a", "b", "a"]);
    }
}
