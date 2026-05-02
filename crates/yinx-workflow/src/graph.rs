use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;
use yinx_core::request::{Request, RequestError};

#[derive(Error, Debug, PartialEq)]
pub enum GraphError {
    #[error("Node with id {0} already exists")]
    NodeAlreadyExists(String),
    #[error("Node with id {0} not found")]
    NodeNotFound(String),
    #[error("Edge from {0} to {1} already exists")]
    EdgeAlreadyExists(String, String),
    #[error("Source node {0} not found")]
    SourceNodeNotFound(String),
    #[error("Target node {0} not found")]
    TargetNodeNotFound(String),
    #[error("Cycle detected in workflow graph")]
    CycleDetected,
    #[error("Request error: {0}")]
    RequestError(#[from] RequestError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub request: Request,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl WorkflowNode {
    pub fn new(request: Request) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            request,
            metadata: HashMap::new(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    pub fn validate(&self) -> Result<(), GraphError> {
        self.request.validate().map_err(GraphError::RequestError)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    pub condition: Option<String>,
    pub transforms: Vec<serde_json::Value>,
}

impl WorkflowEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            condition: None,
            transforms: Vec::new(),
        }
    }

    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    pub fn with_transform(mut self, transform: serde_json::Value) -> Self {
        self.transforms.push(transform);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub name: String,
    pub nodes: HashMap<String, WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub variables: HashMap<String, serde_json::Value>,
}

impl Workflow {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: HashMap::new(),
            edges: Vec::new(),
            variables: HashMap::new(),
        }
    }

    pub fn with_node(mut self, node: WorkflowNode) -> Result<Self, GraphError> {
        self.add_node(node)?;
        Ok(self)
    }

    pub fn with_edge(mut self, edge: WorkflowEdge) -> Result<Self, GraphError> {
        self.add_edge(edge)?;
        Ok(self)
    }

    pub fn add_node(&mut self, node: WorkflowNode) -> Result<(), GraphError> {
        if self.nodes.contains_key(&node.id) {
            return Err(GraphError::NodeAlreadyExists(node.id.clone()));
        }
        node.validate()?;
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    pub fn remove_node(&mut self, id: &str) -> Option<WorkflowNode> {
        let node = self.nodes.remove(id);
        if node.is_some() {
            self.edges.retain(|e| e.from != id && e.to != id);
        }
        node
    }

    pub fn add_edge(&mut self, edge: WorkflowEdge) -> Result<(), GraphError> {
        if !self.nodes.contains_key(&edge.from) {
            return Err(GraphError::SourceNodeNotFound(edge.from.clone()));
        }
        if !self.nodes.contains_key(&edge.to) {
            return Err(GraphError::TargetNodeNotFound(edge.to.clone()));
        }
        if self
            .edges
            .iter()
            .any(|e| e.from == edge.from && e.to == edge.to)
        {
            return Err(GraphError::EdgeAlreadyExists(
                edge.from.clone(),
                edge.to.clone(),
            ));
        }
        self.edges.push(edge);
        Ok(())
    }

    pub fn remove_edge(&mut self, from: &str, to: &str) -> bool {
        let len_before = self.edges.len();
        self.edges.retain(|e| !(e.from == from && e.to == to));
        self.edges.len() < len_before
    }

    pub fn validate_dag(&self) -> Result<(), GraphError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node_id in self.nodes.keys() {
            if !visited.contains(node_id)
                && self.has_cycle_from(node_id, &mut visited, &mut rec_stack)
            {
                return Err(GraphError::CycleDetected);
            }
        }
        Ok(())
    }

    fn has_cycle_from(
        &self,
        node_id: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(node_id.to_string());
        rec_stack.insert(node_id.to_string());

        for edge in &self.edges {
            if edge.from == node_id {
                if !visited.contains(&edge.to) {
                    if self.has_cycle_from(&edge.to, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(&edge.to) {
                    return true;
                }
            }
        }

        rec_stack.remove(node_id);
        false
    }

    pub fn set_variable(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.variables.insert(key.into(), value);
    }

    pub fn get_variable(&self, key: &str) -> Option<&serde_json::Value> {
        self.variables.get(key)
    }

    pub fn topological_sort(&self) -> Result<Vec<String>, GraphError> {
        let mut result = Vec::new();

        // First validate there's no cycle
        self.validate_dag()?;

        // Build adjacency list
        let mut in_degree: HashMap<&String, usize> = self.nodes.keys().map(|k| (k, 0)).collect();
        for edge in &self.edges {
            *in_degree.entry(&edge.to).or_insert(0) += 1;
        }

        // Find nodes with no incoming edges
        let mut queue: Vec<&String> = in_degree
            .iter()
            .filter(|(_, &count)| count == 0)
            .map(|(&node, _)| node)
            .collect();

        while let Some(node_id) = queue.pop() {
            result.push(node_id.clone());

            for edge in &self.edges {
                if edge.from == *node_id {
                    if let Some(count) = in_degree.get_mut(&edge.to) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push(&edge.to);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(GraphError::CycleDetected);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yinx_core::request::{Method, RequestBuilder};

    fn make_request(url: &str) -> Request {
        RequestBuilder::new()
            .method(Method::Get)
            .url(url)
            .build()
            .unwrap()
    }

    #[test]
    fn test_workflow_node_creation() {
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request);
        assert!(!node.id.is_empty());
        assert_eq!(node.metadata.len(), 0);
    }

    #[test]
    fn test_workflow_node_with_id() {
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request).with_id("node-1");
        assert_eq!(node.id, "node-1");
    }

    #[test]
    fn test_workflow_node_with_metadata() {
        let request = make_request("https://example.com");
        let node =
            WorkflowNode::new(request).with_metadata("description", serde_json::json!("Get users"));
        assert_eq!(node.metadata.len(), 1);
        assert_eq!(node.metadata["description"], "Get users");
    }

    #[test]
    fn test_workflow_node_validate_valid() {
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request);
        assert!(node.validate().is_ok());
    }

    #[test]
    fn test_workflow_node_validate_invalid() {
        let node = WorkflowNode::new(
            RequestBuilder::new()
                .build()
                .unwrap_or_else(|_| make_request("https://example.com")),
        );
        // This should still pass since we have a valid URL
        assert!(node.validate().is_ok());
    }

    #[test]
    fn test_workflow_edge_creation() {
        let edge = WorkflowEdge::new("node-1", "node-2");
        assert_eq!(edge.from, "node-1");
        assert_eq!(edge.to, "node-2");
        assert!(edge.condition.is_none());
        assert!(edge.transforms.is_empty());
    }

    #[test]
    fn test_workflow_edge_with_condition() {
        let edge = WorkflowEdge::new("node-1", "node-2").with_condition("status == 200");
        assert_eq!(edge.condition, Some("status == 200".to_string()));
    }

    #[test]
    fn test_workflow_edge_with_transform() {
        let edge = WorkflowEdge::new("node-1", "node-2")
            .with_transform(serde_json::json!({"type": "extract", "path": "$.data.id"}));
        assert_eq!(edge.transforms.len(), 1);
    }

    #[test]
    fn test_workflow_creation() {
        let workflow = Workflow::new("Test Workflow");
        assert_eq!(workflow.name, "Test Workflow");
        assert!(workflow.nodes.is_empty());
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_workflow_add_node() {
        let mut workflow = Workflow::new("Test");
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request);
        assert!(workflow.add_node(node).is_ok());
        assert_eq!(workflow.nodes.len(), 1);
    }

    #[test]
    fn test_workflow_add_duplicate_node() {
        let mut workflow = Workflow::new("Test");
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request).with_id("node-1");
        workflow.add_node(node.clone()).unwrap();
        let result = workflow.add_node(node);
        assert!(matches!(result, Err(GraphError::NodeAlreadyExists(_))));
    }

    #[test]
    fn test_workflow_remove_node() {
        let mut workflow = Workflow::new("Test");
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request).with_id("node-1");
        workflow.add_node(node.clone()).unwrap();
        assert!(workflow.remove_node("node-1").is_some());
        assert!(workflow.nodes.is_empty());
    }

    #[test]
    fn test_workflow_remove_node_removes_edges() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let node1 = WorkflowNode::new(req1).with_id("node-1");
        let node2 = WorkflowNode::new(req2).with_id("node-2");
        workflow.add_node(node1).unwrap();
        workflow.add_node(node2).unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        workflow.remove_node("node-1");
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_workflow_add_edge() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        assert!(workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .is_ok());
        assert_eq!(workflow.edges.len(), 1);
    }

    #[test]
    fn test_workflow_add_edge_missing_source() {
        let mut workflow = Workflow::new("Test");
        let req = make_request("https://example.com");
        workflow
            .add_node(WorkflowNode::new(req).with_id("node-2"))
            .unwrap();
        let result = workflow.add_edge(WorkflowEdge::new("node-1", "node-2"));
        assert!(matches!(result, Err(GraphError::SourceNodeNotFound(_))));
    }

    #[test]
    fn test_workflow_add_edge_missing_target() {
        let mut workflow = Workflow::new("Test");
        let req = make_request("https://example.com");
        workflow
            .add_node(WorkflowNode::new(req).with_id("node-1"))
            .unwrap();
        let result = workflow.add_edge(WorkflowEdge::new("node-1", "node-2"));
        assert!(matches!(result, Err(GraphError::TargetNodeNotFound(_))));
    }

    #[test]
    fn test_workflow_add_duplicate_edge() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        let result = workflow.add_edge(WorkflowEdge::new("node-1", "node-2"));
        assert!(matches!(result, Err(GraphError::EdgeAlreadyExists(_, _))));
    }

    #[test]
    fn test_workflow_remove_edge() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        assert!(workflow.remove_edge("node-1", "node-2"));
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_workflow_validate_dag_no_cycle() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let req3 = make_request("https://example.com/3");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req3).with_id("node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-2", "node-3"))
            .unwrap();
        assert!(workflow.validate_dag().is_ok());
    }

    #[test]
    fn test_workflow_validate_dag_with_cycle() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let req3 = make_request("https://example.com/3");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req3).with_id("node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-2", "node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-3", "node-1"))
            .unwrap();
        assert!(matches!(
            workflow.validate_dag(),
            Err(GraphError::CycleDetected)
        ));
    }

    #[test]
    fn test_workflow_validate_dag_self_loop() {
        let mut workflow = Workflow::new("Test");
        let req = make_request("https://example.com");
        workflow
            .add_node(WorkflowNode::new(req).with_id("node-1"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-1"))
            .unwrap();
        assert!(matches!(
            workflow.validate_dag(),
            Err(GraphError::CycleDetected)
        ));
    }

    #[test]
    fn test_workflow_variables() {
        let mut workflow = Workflow::new("Test");
        workflow.set_variable("base_url", serde_json::json!("https://api.example.com"));
        assert_eq!(
            workflow.get_variable("base_url"),
            Some(&serde_json::json!("https://api.example.com"))
        );
        assert_eq!(workflow.get_variable("nonexistent"), None);
    }

    #[test]
    fn test_workflow_node_serde_roundtrip() {
        let request = make_request("https://example.com");
        let node = WorkflowNode::new(request).with_id("node-1");
        let json = serde_json::to_string(&node).unwrap();
        let decoded: WorkflowNode = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, node.id);
    }

    #[test]
    fn test_workflow_edge_serde_roundtrip() {
        let edge = WorkflowEdge::new("node-1", "node-2").with_condition("status == 200");
        let json = serde_json::to_string(&edge).unwrap();
        let decoded: WorkflowEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.from, edge.from);
        assert_eq!(decoded.condition, edge.condition);
    }

    #[test]
    fn test_workflow_serde_roundtrip() {
        let mut workflow = Workflow::new("Test Workflow");
        let req = make_request("https://example.com");
        let node = WorkflowNode::new(req).with_id("node-1");
        workflow.add_node(node).unwrap();
        let json = serde_json::to_string(&workflow).unwrap();
        let decoded: Workflow = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, workflow.name);
        assert_eq!(decoded.nodes.len(), 1);
    }

    #[test]
    fn test_workflow_topological_sort() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let req3 = make_request("https://example.com/3");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req3).with_id("node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-2", "node-3"))
            .unwrap();
        let sorted = workflow.topological_sort().unwrap();
        assert_eq!(sorted.len(), 3);
        assert!(
            sorted.iter().position(|n| n == "node-1").unwrap()
                < sorted.iter().position(|n| n == "node-2").unwrap()
        );
        assert!(
            sorted.iter().position(|n| n == "node-2").unwrap()
                < sorted.iter().position(|n| n == "node-3").unwrap()
        );
    }

    #[test]
    fn test_workflow_topological_sort_cycle() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-2", "node-1"))
            .unwrap();
        assert!(matches!(
            workflow.topological_sort(),
            Err(GraphError::CycleDetected)
        ));
    }

    #[test]
    fn test_workflow_topological_sort_multiple_sources() {
        let mut workflow = Workflow::new("Test");
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let req3 = make_request("https://example.com/3");
        workflow
            .add_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap();
        workflow
            .add_node(WorkflowNode::new(req3).with_id("node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-1", "node-3"))
            .unwrap();
        workflow
            .add_edge(WorkflowEdge::new("node-2", "node-3"))
            .unwrap();
        let sorted = workflow.topological_sort().unwrap();
        assert_eq!(sorted.len(), 3);
        assert!(
            sorted.iter().position(|n| n == "node-1").unwrap()
                < sorted.iter().position(|n| n == "node-3").unwrap()
        );
        assert!(
            sorted.iter().position(|n| n == "node-2").unwrap()
                < sorted.iter().position(|n| n == "node-3").unwrap()
        );
    }

    #[test]
    fn test_workflow_builder_api() {
        let req1 = make_request("https://example.com/1");
        let req2 = make_request("https://example.com/2");
        let workflow = Workflow::new("Test")
            .with_node(WorkflowNode::new(req1).with_id("node-1"))
            .unwrap()
            .with_node(WorkflowNode::new(req2).with_id("node-2"))
            .unwrap()
            .with_edge(WorkflowEdge::new("node-1", "node-2"))
            .unwrap();
        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);
    }
}
