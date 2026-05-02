use crate::graph::{GraphError, ValidationError, Workflow, WorkflowEdge, WorkflowNode};
use crate::variables::{interpolate, VariableStore};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Notify;
use yinx_core::request::{Request, RequestUrl};
use yinx_core::response::Response;
use yinx_http::client::HttpClient;

#[derive(Error, Debug, PartialEq)]
pub enum ExecutionError {
    #[error("Node '{0}' not found")]
    NodeNotFound(String),
    #[error("HTTP request failed: {0}")]
    HttpRequestFailed(String),
    #[error("Variable extraction failed: {0}")]
    VariableExtractionFailed(String),
    #[error("Condition evaluation failed: {0}")]
    ConditionEvaluationFailed(String),
    #[error("Workflow execution failed at node '{0}': {1}")]
    WorkflowFailed(String, String),
    #[error("Workflow was cancelled")]
    Cancelled,
    #[error("Max retries exceeded for node '{0}'")]
    MaxRetriesExceeded(String),
    #[error("Graph error: {0}")]
    Graph(#[from] GraphError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkflowState {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl Default for WorkflowState {
    fn default() -> Self {
        WorkflowState::Pending
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorStrategy {
    Stop,
    Continue,
    Retry,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 1000,
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// 6.28: Validate retry configuration
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.max_attempts == 0 {
            return Err(ValidationError::InvalidRetryConfig(
                "max_attempts must be greater than 0".to_string(),
            ));
        }
        if self.base_delay_ms == 0 {
            return Err(ValidationError::InvalidRetryConfig(
                "base_delay_ms must be greater than 0".to_string(),
            ));
        }
        if self.backoff_multiplier <= 0.0 {
            return Err(ValidationError::InvalidRetryConfig(
                "backoff_multiplier must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    pub error_strategy: ErrorStrategy,
    pub retry_config: RetryConfig,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            error_strategy: ErrorStrategy::Stop,
            retry_config: RetryConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeExecutionResult {
    pub node_id: String,
    pub response: Option<Response>,
    pub error: Option<String>,
    pub extracted_variables: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowExecutionResult {
    pub state: WorkflowState,
    pub node_results: HashMap<String, NodeExecutionResult>,
    pub error: Option<String>,
}

pub struct WorkflowExecutor {
    http_client: Arc<HttpClient>,
    variables: Arc<RwLock<VariableStore>>,
    cancel_token: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
}

impl WorkflowExecutor {
    pub fn new(http_client: HttpClient) -> Self {
        Self {
            http_client: Arc::new(http_client),
            variables: Arc::new(RwLock::new(VariableStore::new())),
            cancel_token: Arc::new(Notify::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_variables(mut self, variables: VariableStore) -> Self {
        self.variables = Arc::new(RwLock::new(variables));
        self
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.cancel_token.notify_one();
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn interpolate_request(&self, request: &Request) -> Request {
        let store = self.variables.read();
        let url = interpolate(request.url.as_str(), &store);

        let mut new_request = request.clone();
        new_request.url = RequestUrl::new(&url).unwrap_or_else(|_| request.url.clone());

        let mut new_headers = yinx_core::request::Headers::new();
        for (name, value) in request.headers.to_pairs() {
            let interpolated_value = interpolate(&value, &store);
            let _ = new_headers.set(&name, &interpolated_value);
        }
        new_request.headers = new_headers;

        if let yinx_core::request::RequestBody::Raw(ref s) = request.body {
            let interpolated_body = interpolate(s, &store);
            new_request.body = yinx_core::request::RequestBody::Raw(interpolated_body);
        }

        new_request
    }

    pub async fn execute_node(
        &self,
        node: &WorkflowNode,
        options: &ExecutionOptions,
    ) -> NodeExecutionResult {
        let mut last_error = None;

        for attempt in 0..options.retry_config.max_attempts {
            if self.is_cancelled() {
                return NodeExecutionResult {
                    node_id: node.id.clone(),
                    response: None,
                    error: Some("Cancelled".to_string()),
                    extracted_variables: HashMap::new(),
                };
            }

            if attempt > 0 {
                let delay_ms = (options.retry_config.base_delay_ms as f64
                    * options
                        .retry_config
                        .backoff_multiplier
                        .powi(attempt as i32 - 1)) as u64;
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            let interpolated_request = self.interpolate_request(&node.request);

            let send_result = tokio::select! {
                result = self.http_client.send_request(interpolated_request.clone()) => result,
                _ = self.cancel_token.notified() => {
                    self.cancelled.store(true, Ordering::SeqCst);
                    return NodeExecutionResult {
                        node_id: node.id.clone(),
                        response: None,
                        error: Some("Cancelled".to_string()),
                        extracted_variables: HashMap::new(),
                    };
                }
            };

            match send_result {
                Ok(response) => {
                    if response.status.is_error()
                        && matches!(options.error_strategy, ErrorStrategy::Retry)
                    {
                        last_error = Some(format!("HTTP error: {}", response.status));
                        continue;
                    }

                    let status_is_error = response.status.is_error();
                    let status_str = format!("{}", response.status);
                    let mut extracted = HashMap::new();
                    self.extract_variables_from_response(&response, &mut extracted);

                    if status_is_error {
                        return NodeExecutionResult {
                            node_id: node.id.clone(),
                            response: Some(response),
                            error: Some(format!("HTTP error: {}", status_str)),
                            extracted_variables: extracted,
                        };
                    } else {
                        return NodeExecutionResult {
                            node_id: node.id.clone(),
                            response: Some(response),
                            error: None,
                            extracted_variables: extracted,
                        };
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                    if !matches!(options.error_strategy, ErrorStrategy::Retry) {
                        break;
                    }
                }
            }
        }

        NodeExecutionResult {
            node_id: node.id.clone(),
            response: None,
            error: last_error,
            extracted_variables: HashMap::new(),
        }
    }

    fn extract_variables_from_response(
        &self,
        response: &Response,
        extracted: &mut HashMap<String, serde_json::Value>,
    ) {
        let mut store = self.variables.write();

        if let Some(body) = response.body.as_text() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                extracted.insert("response_body".to_string(), json.clone());
                store.set(
                    "response_body".to_string(),
                    json,
                    crate::variables::VariableScope::Local,
                );
            } else {
                extracted.insert(
                    "response_body".to_string(),
                    serde_json::Value::String(body.clone()),
                );
            }
        } else if let yinx_core::response::ResponseBody::Json(ref json) = response.body {
            extracted.insert("response_body".to_string(), json.clone());
            store.set(
                "response_body".to_string(),
                json.clone(),
                crate::variables::VariableScope::Local,
            );
        }

        let status_val =
            serde_json::Value::Number(serde_json::Number::from(response.status.code()));
        extracted.insert("status_code".to_string(), status_val.clone());
        store.set(
            "status_code".to_string(),
            status_val,
            crate::variables::VariableScope::Local,
        );

        let mut headers_map = HashMap::new();
        for (name, value) in response.headers.to_pairs() {
            headers_map.insert(name, value);
        }
        let headers_val = serde_json::to_value(&headers_map).unwrap_or(serde_json::Value::Null);
        extracted.insert("response_headers".to_string(), headers_val.clone());
    }

    pub fn evaluate_edge_condition(
        &self,
        edge: &WorkflowEdge,
        _node_result: &NodeExecutionResult,
    ) -> Result<bool, ExecutionError> {
        let condition = match &edge.condition {
            Some(c) => c,
            None => return Ok(true),
        };

        let store = self.variables.read();

        if condition == "true" {
            return Ok(true);
        }
        if condition == "false" {
            return Ok(false);
        }

        if condition.starts_with("status == ") {
            let expected = condition[10..].trim().parse::<u16>().map_err(|_| {
                ExecutionError::ConditionEvaluationFailed(format!(
                    "Invalid status code in condition: {}",
                    condition
                ))
            })?;
            if let Some(serde_json::Value::Number(n)) = store.get("status_code") {
                if let Some(code) = n.as_u64() {
                    return Ok(code as u16 == expected);
                }
            }
            return Ok(false);
        }

        if condition.starts_with("status != ") {
            let expected = condition[10..].trim().parse::<u16>().map_err(|_| {
                ExecutionError::ConditionEvaluationFailed(format!(
                    "Invalid status code in condition: {}",
                    condition
                ))
            })?;
            if let Some(serde_json::Value::Number(n)) = store.get("status_code") {
                if let Some(code) = n.as_u64() {
                    return Ok(code as u16 != expected);
                }
            }
            return Ok(false);
        }

        if condition.starts_with("status >= ") {
            let expected = condition[10..].trim().parse::<u16>().map_err(|_| {
                ExecutionError::ConditionEvaluationFailed(format!(
                    "Invalid status code in condition: {}",
                    condition
                ))
            })?;
            if let Some(serde_json::Value::Number(n)) = store.get("status_code") {
                if let Some(code) = n.as_u64() {
                    return Ok(code as u16 >= expected);
                }
            }
            return Ok(false);
        }

        if condition.starts_with("status < ") {
            let expected = condition[8..].trim().parse::<u16>().map_err(|_| {
                ExecutionError::ConditionEvaluationFailed(format!(
                    "Invalid status code in condition: {}",
                    condition
                ))
            })?;
            if let Some(serde_json::Value::Number(n)) = store.get("status_code") {
                if let Some(code) = n.as_u64() {
                    return Ok((code as u16) < expected);
                }
            }
            return Ok(false);
        }

        if condition.starts_with("has_var(") && condition.ends_with(")") {
            let var_name = &condition[8..condition.len() - 1];
            return Ok(store.get(var_name.trim()).is_some());
        }

        if condition.contains("==") {
            let parts: Vec<&str> = condition.split("==").collect();
            if parts.len() == 2 {
                let left = parts[0].trim();
                let right = parts[1].trim().trim_matches('"');
                if let Some(val) = store.get(left) {
                    return Ok(val.as_str() == Some(right));
                }
                return Ok(false);
            }
        }

        Err(ExecutionError::ConditionEvaluationFailed(format!(
            "Unsupported condition: {}",
            condition
        )))
    }

    pub async fn execute_sequential(
        &self,
        workflow: &Workflow,
        options: &ExecutionOptions,
    ) -> Result<WorkflowExecutionResult, ExecutionError> {
        let mut result = WorkflowExecutionResult {
            state: WorkflowState::Running,
            node_results: HashMap::new(),
            error: None,
        };

        let sorted_nodes = workflow.topological_sort()?;

        for node_id in &sorted_nodes {
            if self.is_cancelled() {
                result.state = WorkflowState::Cancelled;
                result.error = Some("Workflow was cancelled".to_string());
                return Ok(result);
            }

            let node = workflow
                .nodes
                .get(node_id)
                .ok_or_else(|| ExecutionError::NodeNotFound(node_id.clone()))?;

            let node_result = self.execute_node(node, options).await;

            if let Some(ref error) = node_result.error {
                if error == "Cancelled" {
                    result.state = WorkflowState::Cancelled;
                    result.error = Some("Workflow was cancelled".to_string());
                    result.node_results.insert(node_id.clone(), node_result);
                    return Ok(result);
                }
                if matches!(options.error_strategy, ErrorStrategy::Stop) {
                    result.state = WorkflowState::Failed;
                    result.error = Some(format!("Node '{}' failed: {}", node_id, error));
                    result.node_results.insert(node_id.clone(), node_result);
                    return Ok(result);
                }
            }

            result.node_results.insert(node_id.clone(), node_result);
        }

        result.state = WorkflowState::Done;
        Ok(result)
    }

    pub async fn execute_parallel(
        &self,
        workflow: &Workflow,
        options: &ExecutionOptions,
    ) -> Result<WorkflowExecutionResult, ExecutionError> {
        let mut result = WorkflowExecutionResult {
            state: WorkflowState::Running,
            node_results: HashMap::new(),
            error: None,
        };

        let sorted_nodes = workflow.topological_sort()?;
        let mut completed = std::collections::HashSet::new();

        while completed.len() < sorted_nodes.len() {
            if self.is_cancelled() {
                result.state = WorkflowState::Cancelled;
                result.error = Some("Workflow was cancelled".to_string());
                return Ok(result);
            }

            let ready_nodes: Vec<&String> = sorted_nodes
                .iter()
                .filter(|n| !completed.contains(*n))
                .filter(|n| {
                    workflow.edges.iter().all(|e| {
                        if e.to == **n {
                            completed.contains(&e.from)
                        } else {
                            true
                        }
                    })
                })
                .collect();

            if ready_nodes.is_empty() {
                break;
            }

            let mut handles = Vec::new();
            for node_id in &ready_nodes {
                let node = workflow
                    .nodes
                    .get(*node_id)
                    .ok_or_else(|| ExecutionError::NodeNotFound((*node_id).clone()))?;
                let executor = Self {
                    http_client: self.http_client.clone(),
                    variables: self.variables.clone(),
                    cancel_token: self.cancel_token.clone(),
                    cancelled: self.cancelled.clone(),
                };
                let node_clone = WorkflowNode {
                    id: node.id.clone(),
                    request: node.request.clone(),
                    metadata: node.metadata.clone(),
                };
                let options_clone = options.clone();

                let handle = tokio::spawn(async move {
                    executor.execute_node(&node_clone, &options_clone).await
                });
                handles.push((*node_id, handle));
            }

            for (node_id, handle) in handles {
                match handle.await {
                    Ok(node_result) => {
                        if let Some(ref error) = node_result.error {
                            if matches!(options.error_strategy, ErrorStrategy::Stop) {
                                result.state = WorkflowState::Failed;
                                result.error =
                                    Some(format!("Node '{}' failed: {}", node_id, error));
                                result.node_results.insert(node_id.clone(), node_result);
                                return Ok(result);
                            }
                        }
                        result.node_results.insert(node_id.clone(), node_result);
                        completed.insert(node_id.clone());
                    }
                    Err(e) => {
                        result.state = WorkflowState::Failed;
                        result.error = Some(format!("Task join error: {}", e));
                        return Ok(result);
                    }
                }
            }
        }

        result.state = WorkflowState::Done;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variables::VariableScope;
    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use yinx_core::request::{Method, RequestBuilder};

    fn make_request(url: &str) -> Request {
        RequestBuilder::new()
            .method(Method::Get)
            .url(url)
            .build()
            .unwrap()
    }

    fn make_workflow() -> Workflow {
        Workflow::new("Test Workflow")
    }

    // 6.12: Single node executor
    #[tokio::test]
    async fn test_6_12_single_node_executor() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"message": "ok"})))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let node = WorkflowNode::new(make_request(mock_server.uri().as_str()));
        let options = ExecutionOptions::default();

        let result = executor.execute_node(&node, &options).await;
        assert!(result.response.is_some());
        assert!(result.error.is_none());
        assert_eq!(result.node_id, node.id);
    }

    // 6.13: Variable extraction post-execution
    #[tokio::test]
    async fn test_6_13_variable_extraction_post_execution() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("X-Request-Id", "abc-123")
                    .set_body_json(json!({"data": {"id": 42}})),
            )
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let node = WorkflowNode::new(make_request(mock_server.uri().as_str()));
        let options = ExecutionOptions::default();

        let result = executor.execute_node(&node, &options).await;
        assert!(result.response.is_some());
        assert!(result.extracted_variables.contains_key("response_body"));
        assert!(result.extracted_variables.contains_key("status_code"));
        assert!(result.extracted_variables.contains_key("response_headers"));
    }

    // 6.14: Edge condition evaluation
    #[tokio::test]
    async fn test_6_14_edge_condition_evaluation_true() {
        let http_client = HttpClient::new().unwrap();
        let mut variables = VariableStore::new();
        variables.set("status_code", json!(200), VariableScope::Local);
        let executor = WorkflowExecutor::new(http_client).with_variables(variables);

        let edge = WorkflowEdge::new("node-1", "node-2").with_condition("status == 200");
        let node_result = NodeExecutionResult {
            node_id: "node-1".to_string(),
            response: Some(Response::builder().status(200).build()),
            error: None,
            extracted_variables: HashMap::new(),
        };

        let result = executor
            .evaluate_edge_condition(&edge, &node_result)
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_6_14_edge_condition_evaluation_false() {
        let http_client = HttpClient::new().unwrap();
        let mut variables = VariableStore::new();
        variables.set("status_code", json!(200), VariableScope::Local);
        let executor = WorkflowExecutor::new(http_client).with_variables(variables);

        let edge = WorkflowEdge::new("node-1", "node-2").with_condition("status == 404");
        let node_result = NodeExecutionResult {
            node_id: "node-1".to_string(),
            response: Some(Response::builder().status(200).build()),
            error: None,
            extracted_variables: HashMap::new(),
        };

        let result = executor
            .evaluate_edge_condition(&edge, &node_result)
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_6_14_edge_condition_no_condition_always_true() {
        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let edge = WorkflowEdge::new("node-1", "node-2");
        let node_result = NodeExecutionResult {
            node_id: "node-1".to_string(),
            response: Some(Response::builder().status(200).build()),
            error: None,
            extracted_variables: HashMap::new(),
        };

        let result = executor
            .evaluate_edge_condition(&edge, &node_result)
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_6_14_edge_condition_has_var() {
        let http_client = HttpClient::new().unwrap();
        let mut variables = VariableStore::new();
        variables.set("my_var", json!("exists"), VariableScope::Local);
        let executor = WorkflowExecutor::new(http_client).with_variables(variables);

        let edge = WorkflowEdge::new("node-1", "node-2").with_condition("has_var(my_var)");
        let node_result = NodeExecutionResult {
            node_id: "node-1".to_string(),
            response: None,
            error: None,
            extracted_variables: HashMap::new(),
        };

        let result = executor
            .evaluate_edge_condition(&edge, &node_result)
            .unwrap();
        assert!(result);
    }

    // 6.15: Sequential workflow executor
    #[tokio::test]
    async fn test_6_15_sequential_workflow_executor() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"step": 1})))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let mut workflow = make_workflow();
        let node1 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-1");
        let node2 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-2");
        workflow.add_node(node1).unwrap();
        workflow.add_node(node2).unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();

        let options = ExecutionOptions::default();
        let result = executor
            .execute_sequential(&workflow, &options)
            .await
            .unwrap();

        assert_eq!(result.state, WorkflowState::Done);
        assert_eq!(result.node_results.len(), 2);
        assert!(result.node_results.contains_key("node-1"));
        assert!(result.node_results.contains_key("node-2"));
    }

    // 6.16: Parallel execution
    #[tokio::test]
    async fn test_6_16_parallel_execution() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let mut workflow = make_workflow();
        let node1 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-1");
        let node2 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-2");
        workflow.add_node(node1).unwrap();
        workflow.add_node(node2).unwrap();

        let options = ExecutionOptions::default();
        let result = executor
            .execute_parallel(&workflow, &options)
            .await
            .unwrap();

        assert_eq!(result.state, WorkflowState::Done);
        assert_eq!(result.node_results.len(), 2);
    }

    // 6.17: Error handling strategies - Stop
    #[tokio::test]
    async fn test_6_17_error_handling_stop() {
        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let mut workflow = make_workflow();
        let node1 = WorkflowNode::new(
            RequestBuilder::new()
                .method(Method::Get)
                .url("http://invalid-url-that-will-fail.example")
                .build()
                .unwrap(),
        )
        .with_id("node-1");
        workflow.add_node(node1).unwrap();

        let options = ExecutionOptions {
            error_strategy: ErrorStrategy::Stop,
            ..Default::default()
        };
        let result = executor
            .execute_sequential(&workflow, &options)
            .await
            .unwrap();

        assert_eq!(result.state, WorkflowState::Failed);
        assert!(result.error.is_some());
    }

    // 6.18: Retry logic with backoff
    #[tokio::test]
    async fn test_6_18_retry_logic_with_backoff() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let node = WorkflowNode::new(make_request(mock_server.uri().as_str()));
        let options = ExecutionOptions {
            error_strategy: ErrorStrategy::Retry,
            retry_config: RetryConfig {
                max_attempts: 3,
                base_delay_ms: 10,
                backoff_multiplier: 2.0,
            },
        };

        let start = std::time::Instant::now();
        let result = executor.execute_node(&node, &options).await;
        let elapsed = start.elapsed();

        assert!(result.error.is_some());
        assert!(elapsed >= Duration::from_millis(10));
    }

    // 6.19: Workflow state machine
    #[tokio::test]
    async fn test_6_19_workflow_state_machine_done() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let mut workflow = make_workflow();
        let node1 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-1");
        workflow.add_node(node1).unwrap();

        let options = ExecutionOptions::default();
        let result = executor
            .execute_sequential(&workflow, &options)
            .await
            .unwrap();

        assert_eq!(result.state, WorkflowState::Done);
    }

    // 6.20: Workflow cancellation
    #[tokio::test]
    async fn test_6_20_workflow_cancellation() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(10)))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let executor = WorkflowExecutor::new(http_client);

        let mut workflow = make_workflow();
        let node1 = WorkflowNode::new(make_request(mock_server.uri().as_str())).with_id("node-1");
        workflow.add_node(node1).unwrap();

        let options = ExecutionOptions::default();

        let executor_clone = WorkflowExecutor {
            http_client: executor.http_client.clone(),
            variables: executor.variables.clone(),
            cancel_token: executor.cancel_token.clone(),
            cancelled: executor.cancelled.clone(),
        };

        let handle =
            tokio::spawn(
                async move { executor_clone.execute_sequential(&workflow, &options).await },
            );

        tokio::time::sleep(Duration::from_millis(100)).await;
        executor.cancel();

        let result = handle.await.unwrap().unwrap();
        assert_eq!(result.state, WorkflowState::Cancelled);
    }

    #[tokio::test]
    async fn test_variable_interpolation_in_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let http_client = HttpClient::new().unwrap();
        let mut variables = VariableStore::new();
        variables.set("base_url", json!(mock_server.uri()), VariableScope::Global);
        let executor = WorkflowExecutor::new(http_client).with_variables(variables);

        let request = RequestBuilder::new()
            .method(Method::Get)
            .url("${base_url}")
            .build()
            .unwrap();

        let node = WorkflowNode::new(request);
        let options = ExecutionOptions::default();

        let result = executor.execute_node(&node, &options).await;
        assert!(result.response.is_some());
    }

    // 6.28: Validate retry configuration
    #[test]
    fn test_6_28_retry_config_zero_max_attempts() {
        let config = RetryConfig {
            max_attempts: 0,
            base_delay_ms: 100,
            backoff_multiplier: 2.0,
        };
        assert!(config.validate().is_err());
        if let Err(ValidationError::InvalidRetryConfig(msg)) = config.validate() {
            assert!(msg.contains("max_attempts"));
        } else {
            panic!("Expected InvalidRetryConfig error");
        }
    }

    #[test]
    fn test_6_28_retry_config_zero_base_delay() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 0,
            backoff_multiplier: 2.0,
        };
        assert!(config.validate().is_err());
        if let Err(ValidationError::InvalidRetryConfig(msg)) = config.validate() {
            assert!(msg.contains("base_delay_ms"));
        } else {
            panic!("Expected InvalidRetryConfig error");
        }
    }

    #[test]
    fn test_6_28_retry_config_zero_backoff() {
        let config = RetryConfig {
            max_attempts: 3,
            base_delay_ms: 100,
            backoff_multiplier: 0.0,
        };
        assert!(config.validate().is_err());
        if let Err(ValidationError::InvalidRetryConfig(msg)) = config.validate() {
            assert!(msg.contains("backoff_multiplier"));
        } else {
            panic!("Expected InvalidRetryConfig error");
        }
    }

    #[test]
    fn test_6_28_valid_retry_config() {
        let config = RetryConfig::default();
        assert!(config.validate().is_ok());
    }
}
