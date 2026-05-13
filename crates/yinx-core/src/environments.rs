use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl EnvironmentVariable {
    pub fn new(key: String, value: String) -> Self {
        Self {
            key,
            value,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    pub variables: Vec<EnvironmentVariable>,
}

impl Environment {
    pub fn new(name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            variables: Vec::new(),
        }
    }

    pub fn add_variable(&mut self, var: EnvironmentVariable) {
        self.variables.push(var);
    }

    pub fn get_variable(&self, key: &str) -> Option<&EnvironmentVariable> {
        self.variables.iter().find(|v| v.key == key && v.enabled)
    }

    pub fn remove_variable(&mut self, key: &str) -> Option<EnvironmentVariable> {
        let idx = self.variables.iter().position(|v| v.key == key)?;
        Some(self.variables.remove(idx))
    }

    pub fn resolve(&self, input: &str) -> String {
        let mut result = input.to_string();
        for var in &self.variables {
            if var.enabled {
                result = result.replace(&format!("{{{{{}}}}}", var.key), &var.value);
            }
        }
        result
    }

    pub fn enabled_variables(&self) -> impl Iterator<Item = &EnvironmentVariable> {
        self.variables.iter().filter(|v| v.enabled)
    }

    pub fn variable_count(&self) -> usize {
        self.enabled_variables().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_env(name: &str) -> Environment {
        let mut env = Environment::new(name.to_string());
        env.add_variable(EnvironmentVariable::new(
            "base_url".to_string(),
            "https://api.example.com".to_string(),
        ));
        env.add_variable(EnvironmentVariable::new(
            "token".to_string(),
            "abc123".to_string(),
        ));
        env
    }

    #[test]
    fn test_environment_new() {
        let env = Environment::new("Staging".to_string());
        assert_eq!(env.name, "Staging");
        assert!(env.variables.is_empty());
        assert!(!env.id.is_empty());
    }

    #[test]
    fn test_environment_add_variable() {
        let mut env = Environment::new("Staging".to_string());
        env.add_variable(EnvironmentVariable::new(
            "host".to_string(),
            "localhost".to_string(),
        ));
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn test_environment_get_variable() {
        let env = make_env("Staging");
        let var = env.get_variable("base_url");
        assert!(var.is_some());
        assert_eq!(var.unwrap().value, "https://api.example.com");
    }

    #[test]
    fn test_environment_get_variable_not_found() {
        let env = make_env("Staging");
        assert!(env.get_variable("nonexistent").is_none());
    }

    #[test]
    fn test_environment_get_variable_disabled() {
        let mut env = make_env("Staging");
        env.variables[0].enabled = false;
        assert!(env.get_variable("base_url").is_none());
    }

    #[test]
    fn test_environment_remove_variable() {
        let mut env = make_env("Staging");
        let removed = env.remove_variable("base_url");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().key, "base_url");
        assert_eq!(env.variables.len(), 1);
    }

    #[test]
    fn test_environment_remove_variable_not_found() {
        let mut env = make_env("Staging");
        assert!(env.remove_variable("nonexistent").is_none());
    }

    #[test]
    fn test_environment_resolve() {
        let env = make_env("Staging");
        let result = env.resolve("{{base_url}}/users");
        assert_eq!(result, "https://api.example.com/users");
    }

    #[test]
    fn test_environment_resolve_no_variables() {
        let env = make_env("Staging");
        let result = env.resolve("/users");
        assert_eq!(result, "/users");
    }

    #[test]
    fn test_environment_resolve_multiple() {
        let env = make_env("Staging");
        let result = env.resolve("{{base_url}}/users?token={{token}}");
        assert_eq!(result, "https://api.example.com/users?token=abc123");
    }

    #[test]
    fn test_environment_resolve_with_disabled_variable() {
        let mut env = make_env("Staging");
        env.variables[0].enabled = false;
        let result = env.resolve("{{base_url}}/users");
        assert_eq!(result, "{{base_url}}/users");
    }

    #[test]
    fn test_environment_enabled_variables() {
        let mut env = make_env("Staging");
        env.add_variable(EnvironmentVariable {
            key: "disabled_var".to_string(),
            value: "val".to_string(),
            enabled: false,
        });
        let enabled: Vec<_> = env.enabled_variables().collect();
        assert_eq!(enabled.len(), 2);
    }

    #[test]
    fn test_environment_variable_count() {
        let mut env = make_env("Staging");
        env.add_variable(EnvironmentVariable {
            key: "disabled_var".to_string(),
            value: "val".to_string(),
            enabled: false,
        });
        assert_eq!(env.variable_count(), 2);
    }

    #[test]
    fn test_environment_serde_roundtrip() {
        let env = make_env("Staging");
        let json = serde_json::to_string(&env).unwrap();
        let decoded: Environment = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, env.name);
        assert_eq!(decoded.variables.len(), env.variables.len());
    }

    #[test]
    fn test_environment_variable_new() {
        let var = EnvironmentVariable::new("key".to_string(), "value".to_string());
        assert_eq!(var.key, "key");
        assert_eq!(var.value, "value");
        assert!(var.enabled);
    }

    #[test]
    fn test_environment_variable_serde() {
        let var = EnvironmentVariable::new("key".to_string(), "value".to_string());
        let json = serde_json::to_string(&var).unwrap();
        let decoded: EnvironmentVariable = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.key, var.key);
        assert_eq!(decoded.value, var.value);
    }
}
