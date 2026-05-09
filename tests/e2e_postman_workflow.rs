//! End-to-end test: Import Postman → run workflow → verify
//! Phase 15.1 - Strict TDD: Red → Green → Refactor

#[test]
fn test_postman_import_to_workflow_execution() {
    // Given: A Postman v2.1 collection as JSON string
    let postman_json = r#"{
        "info": {
            "name": "Test API Collection",
            "_postman_id": "abc123",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": [
            {
                "name": "Get Users",
                "request": {
                    "method": "GET",
                    "url": {
                        "raw": "https://api.example.com/users"
                    }
                }
            }
        ]
    }"#;

    // When: Import Postman collection (already implemented)
    let requests = yinx_import::postman::parse_collection(postman_json)
        .expect("Postman import should succeed");
    assert_eq!(requests.len(), 1, "Should import 1 request");

    // Then: Build workflow from requests (need Workflow::from_requests - will fail)
    let workflow = yinx_workflow::Workflow::from_requests(requests, "Test Workflow");
    assert_eq!(workflow.nodes.len(), 1);
    assert!(workflow.validate().is_ok(), "Workflow should be valid");
}

#[test]
fn test_postman_variable_substitution() {
    let postman_json = r#"{
        "info": {
            "name": "Variable Test",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "variable": [
            {"key": "baseUrl", "value": "https://api.example.com"}
        ],
        "item": [
            {
                "name": "Get Items",
                "request": {
                    "method": "GET",
                    "url": "{{baseUrl}}/items"
                }
            }
        ]
    }"#;

    let requests =
        yinx_import::postman::parse_collection(postman_json).expect("Import should succeed");

    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url.as_str(),
        "https://api.example.com/items",
        "Variables should be substituted"
    );
}
