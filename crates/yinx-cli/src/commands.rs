use clap::{Parser, Subcommand};
use std::process;
use yinx_core::collections::Collection;
use yinx_core::request::{Method, RequestBody, RequestBuilder};
use yinx_http::client::HttpClient;

#[derive(Parser, Debug)]
#[command(
    name = "yinx",
    version,
    about = "A terminal HTTP client with streaming and workflow support"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,

    /// Only output status code (for pipe-friendly usage)
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Include headers and timing in output
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run a single request or execute a workflow file
    Run {
        /// URL or path to workflow file
        target: String,

        /// HTTP method (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
        #[arg(short = 'X', long, default_value = "GET")]
        method: String,

        /// Request body (for POST/PUT/PATCH)
        #[arg(short, long)]
        data: Option<String>,

        /// Request body as JSON
        #[arg(long)]
        json_data: Option<String>,

        /// Header in 'Name: Value' format (can be repeated)
        #[arg(short = 'H', long)]
        header: Vec<String>,

        /// Timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
    },

    /// Import requests from a file (Postman, Insomnia, OpenAPI, or curl)
    Import {
        /// Path to import file
        file: String,

        /// Output file for imported requests (optional)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Stream a response (raw streaming to stdout)
    Stream {
        /// URL to stream from
        url: String,

        /// HTTP method
        #[arg(short, long, default_value = "GET")]
        method: String,

        /// Header in 'Name: Value' format (can be repeated)
        #[arg(short = 'H', long)]
        header: Vec<String>,
    },

    /// Alias for 'run'
    Exec {
        /// URL or path to workflow file
        target: String,

        /// HTTP method (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
        #[arg(short = 'X', long, default_value = "GET")]
        method: String,

        /// Request body (for POST/PUT/PATCH)
        #[arg(short, long)]
        data: Option<String>,

        /// Request body as JSON
        #[arg(long)]
        json_data: Option<String>,

        /// Header in 'Name: Value' format (can be repeated)
        #[arg(short = 'H', long)]
        header: Vec<String>,

        /// Timeout in seconds
        #[arg(long, default_value = "30")]
        timeout: u64,
    },
}

pub async fn run() {
    let cli = Cli::parse();

    let cmd = match cli.command {
        None => {
            if let Err(e) = yinx_tui::run_tui().await {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
            return;
        }
        Some(cmd) => cmd,
    };

    let result = match cmd {
        Commands::Run {
            target,
            method,
            data,
            json_data,
            header,
            timeout,
        } => {
            run_request_or_workflow(
                &target,
                &method,
                data,
                json_data,
                header,
                timeout,
                cli.json,
                cli.quiet,
                cli.verbose,
            )
            .await
        }
        Commands::Exec {
            target,
            method,
            data,
            json_data,
            header,
            timeout,
        } => {
            run_request_or_workflow(
                &target,
                &method,
                data,
                json_data,
                header,
                timeout,
                cli.json,
                cli.quiet,
                cli.verbose,
            )
            .await
        }
        Commands::Import { file, output } => import_file(&file, output).await,
        Commands::Stream {
            url,
            method,
            header,
        } => stream_response(&url, &method, header).await,
    };

    match result {
        Ok(exit_code) => process::exit(exit_code),
        Err(e) => {
            if cli.json {
                let error_output = serde_json::json!({
                    "error": e.to_string(),
                    "status": null
                });
                println!("{}", serde_json::to_string_pretty(&error_output).unwrap());
            } else {
                eprintln!("Error: {}", e);
            }
            process::exit(1);
        }
    }
}

async fn run_request_or_workflow(
    target: &str,
    method: &str,
    data: Option<String>,
    json_data: Option<String>,
    headers: Vec<String>,
    timeout: u64,
    json_output: bool,
    quiet: bool,
    verbose: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let is_workflow =
        (target.ends_with(".yaml") || target.ends_with(".yml") || target.ends_with(".json"))
            && std::path::Path::new(target).exists();

    if is_workflow {
        run_workflow(target, json_output, quiet, verbose).await
    } else {
        run_single_request(
            target,
            method,
            data,
            json_data,
            headers,
            timeout,
            json_output,
            quiet,
            verbose,
        )
        .await
    }
}

async fn run_single_request(
    url: &str,
    method: &str,
    data: Option<String>,
    json_data: Option<String>,
    headers: Vec<String>,
    timeout: u64,
    json_output: bool,
    quiet: bool,
    verbose: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let method: Method = method.parse()?;

    let mut builder = RequestBuilder::new()
        .method(method)
        .url(url.to_string())
        .timeout_secs(timeout);

    for header_str in headers {
        let parts: Vec<&str> = header_str.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid header format: '{}'. Expected 'Name: Value'",
                header_str
            )
            .into());
        }
        builder = builder.header(parts[0].trim(), parts[1].trim());
    }

    if let Some(json_str) = json_data {
        let json: serde_json::Value = serde_json::from_str(&json_str)?;
        builder = builder.body(RequestBody::Json(json));
    } else if let Some(body_str) = data {
        builder = builder.body(RequestBody::Raw(body_str));
    }

    let request = builder.build()?;
    let client = HttpClient::new()?;

    let response = client.send_request(request).await?;

    let exit_code = if response.status.is_success() {
        0
    } else if response.status.is_client_error() {
        4
    } else if response.status.is_server_error() {
        5
    } else {
        1
    };

    if quiet {
        println!("{}", response.status.code());
    } else if json_output {
        let body_json = match &response.body {
            yinx_core::response::ResponseBody::Json(v) => v.clone(),
            yinx_core::response::ResponseBody::Text(s) => {
                serde_json::from_str::<serde_json::Value>(s)
                    .unwrap_or_else(|_| serde_json::Value::String(s.clone()))
            }
            _ => serde_json::Value::Null,
        };

        let mut headers_map = serde_json::Map::new();
        for (name, value) in response.headers.to_pairs() {
            headers_map.insert(
                name.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

        let output = serde_json::json!({
            "status": response.status.code(),
            "status_text": response.status.phrase(),
            "headers": headers_map,
            "body": body_json,
            "timing_ms": response.timing_ms,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("HTTP/1.1 {}", response.status);

        if verbose {
            println!();
            for (name, value) in response.headers.to_pairs() {
                println!("{}: {}", name, value);
            }
        }

        println!();

        if let Some(text) = response.body.as_text() {
            println!("{}", text);
        }
    }

    Ok(exit_code)
}

async fn run_workflow(
    file_path: &str,
    json_output: bool,
    quiet: bool,
    verbose: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file_path)?;

    let workflow: yinx_workflow::graph::Workflow = if file_path.ends_with(".json") {
        serde_json::from_str(&content)?
    } else {
        serde_yaml::from_str(&content)?
    };

    let client = HttpClient::new()?;
    let executor = yinx_workflow::engine::WorkflowExecutor::new(client);

    let options = yinx_workflow::engine::ExecutionOptions::default();
    let result = executor.execute_sequential(&workflow, &options).await?;

    if quiet {
        let exit_code = match result.state {
            yinx_workflow::engine::WorkflowState::Done => 0,
            yinx_workflow::engine::WorkflowState::Failed => 1,
            yinx_workflow::engine::WorkflowState::Cancelled => 2,
            _ => 3,
        };
        println!("{}", exit_code);
        Ok(exit_code)
    } else if json_output {
        let output = serde_json::json!({
            "state": format!("{:?}", result.state).to_lowercase(),
            "node_results": result.node_results,
            "error": result.error,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        let exit_code = match result.state {
            yinx_workflow::engine::WorkflowState::Done => 0,
            _ => 1,
        };
        Ok(exit_code)
    } else {
        println!("Workflow state: {:?}", result.state);
        if let Some(error) = &result.error {
            println!("Error: {}", error);
        }
        for (node_id, node_result) in &result.node_results {
            println!("\nNode: {}", node_id);
            if let Some(response) = &node_result.response {
                println!("  Status: {}", response.status);
                if verbose {
                    for (name, value) in response.headers.to_pairs() {
                        println!("  {}: {}", name, value);
                    }
                }
            }
            if let Some(error) = &node_result.error {
                println!("  Error: {}", error);
            }
        }
        let exit_code = match result.state {
            yinx_workflow::engine::WorkflowState::Done => 0,
            _ => 1,
        };
        Ok(exit_code)
    }
}

async fn import_file(
    file: &str,
    output: Option<String>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file)
        .map_err(|e| format!("Failed to read '{}': {}", file, e))?;

    let path = std::path::Path::new(file);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let (collection, _source_name) = if ext == "yaml" || ext == "yml" {
        // OpenAPI / Swagger YAML
        let requests = yinx_import::openapi::parse_openapi(&content)
            .map_err(|e| format!("Failed to parse OpenAPI spec: {}", e))?;
        let mut c = Collection::new(format!("OpenAPI: {}", path.file_stem().unwrap_or_default().to_string_lossy()));
        for (i, req) in requests.into_iter().enumerate() {
            let saved = yinx_core::state::SavedRequest {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("{} {}", req.method, req.url.as_str()),
                request: req,
                tags: Vec::new(),
            };
            if i == 0 { println!("  {} {}", saved.request.method, saved.request.url.as_str()); }
            c.add_item(yinx_core::collections::CollectionItem::Request(Box::new(saved)));
        }
        println!("Imported {} request(s) from OpenAPI spec", c.item_count());
        (c, "OpenAPI".to_string())
    } else if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
        if json.get("__export_format").is_some() || json.get("resources").is_some() {
            // Insomnia
            let requests = yinx_import::insomnia::parse_insomnia_export(&content)
                .map_err(|e| format!("Failed to parse Insomnia export: {}", e))?;
            let mut c = Collection::new(format!("Insomnia: {}", path.file_stem().unwrap_or_default().to_string_lossy()));
            for (i, req) in requests.into_iter().enumerate() {
                let saved = yinx_core::state::SavedRequest {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: format!("Request {}", i),
                    request: req,
                    tags: Vec::new(),
                };
                if i == 0 { println!("  {} {}", saved.request.method, saved.request.url.as_str()); }
                c.add_item(yinx_core::collections::CollectionItem::Request(Box::new(saved)));
            }
            println!("Imported {} request(s) from Insomnia", c.item_count());
            (c, "Insomnia".to_string())
        } else if json.get("info").and_then(|i| i.get("schema")).is_some() {
            // Postman
            let (collection, warnings) = yinx_import::postman::parse_collection_to_collection(&content)
                .map_err(|e| format!("Failed to parse Postman collection: {}", e))?;
            for w in &warnings {
                eprintln!("Warning: {}", w);
            }
            for sr in collection.flatten_requests() {
                println!("  {} {} — {}", sr.request.method, sr.request.url.as_str(), sr.name);
            }
            let count = collection.item_count();
            println!("Imported {} request(s) from Postman collection '{}'", count, collection.name);
            (collection, "Postman".to_string())
        } else {
            return Err(format!("Unrecognized JSON format in '{}'. Expected a Postman collection, Insomnia export, or OpenAPI spec.", file).into());
        }
    } else {
        // Try as curl command
        let request = yinx_import::curl::parse_curl(&content)
            .map_err(|e| format!("Failed to parse curl command: {:?}", e))?;
        let mut c = Collection::new(format!("curl: {}", path.file_stem().unwrap_or_default().to_string_lossy()));
        let saved = yinx_core::state::SavedRequest {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("{} {}", request.method, request.url.as_str()),
            request,
            tags: Vec::new(),
        };
        println!("  {} {}", saved.request.method, saved.request.url.as_str());
        c.add_item(yinx_core::collections::CollectionItem::Request(Box::new(saved)));
        println!("Imported 1 request from curl command");
        (c, "curl".to_string())
    };

    if let Some(output_path) = output {
        let json = serde_json::to_string_pretty(&collection)?;
        std::fs::write(&output_path, json)
            .map_err(|e| format!("Failed to write output to '{}': {}", output_path, e))?;
        println!("Saved collection to {}", output_path);
    } else {
        let default_dir = std::path::PathBuf::from(
            &std::env::var("HOME").unwrap_or_else(|_| ".".to_string()),
        )
        .join(".local")
        .join("share")
        .join("yinx")
        .join("collections");
        std::fs::create_dir_all(&default_dir)?;
        let output_path = default_dir.join(format!("{}.json", collection.id));
        let json = serde_json::to_string_pretty(&collection)?;
        std::fs::write(&output_path, json)
            .map_err(|e| format!("Failed to save collection: {}", e))?;
        println!("Saved collection to {}", output_path.display());
    }

    Ok(0)
}

async fn stream_response(
    url: &str,
    method: &str,
    headers: Vec<String>,
) -> Result<i32, Box<dyn std::error::Error>> {
    println!("Streaming from: {}", url);

    let method: Method = method.parse()?;
    let mut builder = RequestBuilder::new().method(method).url(url.to_string());

    for header_str in headers {
        let parts: Vec<&str> = header_str.splitn(2, ':').collect();
        if parts.len() == 2 {
            builder = builder.header(parts[0].trim(), parts[1].trim());
        }
    }

    let request = builder.build()?;
    let client = HttpClient::new()?;

    let response = client.send_request(request).await?;

    if let Some(text) = response.body.as_text() {
        println!("{}", text);
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // 12.1: CLI argument parser
    #[test]
    fn test_12_1_args_parsed_correctly() {
        let args = vec!["yinx", "run", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Run { target, .. }) => assert_eq!(target, "https://example.com"),
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_12_1_import_subcommand() {
        let args = vec!["yinx", "import", "collection.json"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Import { file, .. }) => assert_eq!(file, "collection.json"),
            _ => panic!("Expected Import command"),
        }
    }

    #[test]
    fn test_12_1_stream_subcommand() {
        let args = vec!["yinx", "stream", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Stream { url, .. }) => assert_eq!(url, "https://example.com"),
            _ => panic!("Expected Stream command"),
        }
    }

    // 12.7: exec alias for run
    #[test]
    fn test_12_7_exec_alias_for_run() {
        let args = vec!["yinx", "exec", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Some(Commands::Exec { target, .. }) => assert_eq!(target, "https://example.com"),
            _ => panic!("Expected Exec command"),
        }
    }

    // 12.4: JSON output mode
    #[test]
    fn test_12_4_json_flag() {
        let args = vec!["yinx", "--json", "run", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.json);
    }

    // 12.8: Quiet mode
    #[test]
    fn test_12_8_quiet_flag() {
        let args = vec!["yinx", "--quiet", "run", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.quiet);
    }

    // 12.9: Verbose mode
    #[test]
    fn test_12_9_verbose_flag() {
        let args = vec!["yinx", "--verbose", "run", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.verbose);
    }

    // 12.1: Default method is GET
    #[test]
    fn test_12_1_default_method_is_get() {
        let args = vec!["yinx", "run", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        if let Some(Commands::Run { method, .. }) = cli.command {
            assert_eq!(method, "GET");
        } else {
            panic!("Expected Run command");
        }
    }

    // 12.1: Custom method flag
    #[test]
    fn test_12_1_custom_method_flag() {
        let args = vec!["yinx", "run", "-X", "POST", "https://example.com"];
        let cli = Cli::try_parse_from(args).unwrap();
        if let Some(Commands::Run { method, .. }) = cli.command {
            assert_eq!(method, "POST");
        } else {
            panic!("Expected Run command");
        }
    }
}
