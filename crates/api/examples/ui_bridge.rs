//! Test-only JSON-lines bridge to the actual service. No network listener.
//! Playwright uses this instead of mocking the scheduler or persistence.
use api::desktop::Service;
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};
#[tokio::main]
async fn main() -> Result<(), String> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Provide a test workspace path")?,
    );
    let service = Service::open(&directory)?;
    let mut settings = service.snapshot().await.workspace.settings;
    settings.demo_delay_ms = 450;
    service
        .command(api::desktop::Command::SaveSettings { settings })
        .await?;
    let driver = tokio::spawn(service.clone().drive());
    for line in io::stdin().lock().lines() {
        let request: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let args = &request["args"];
        let result: Result<Value, String> = match request["cmd"].as_str().unwrap_or("") {
            "snapshot" => serde_json::to_value(service.snapshot().await).map_err(|e| e.to_string()),
            "discover_agent" => match serde_json::from_value(args["request"].clone()) {
                Ok(request) => api::desktop::discover_agent(request)
                    .await
                    .and_then(|card| serde_json::to_value(card).map_err(|e| e.to_string())),
                Err(e) => Err(e.to_string()),
            },
            "command" => match serde_json::from_value(args["command"].clone()) {
                Ok(c) => service.command(c).await.map(|_| Value::Null),
                Err(e) => Err(e.to_string()),
            },
            "compare_schedulers" => {
                serde_json::to_value(orchestrator::prototype::evaluation::compare())
                    .map_err(|e| e.to_string())
            }
            "export_run" => service
                .export_run(args["runId"].as_str().unwrap_or(""))
                .await
                .map(Value::String),
            "export_evaluation" => service.export_evaluation().await.map(Value::String),
            "compare_experiment" => match serde_json::from_value(args["config"].clone()) {
                Ok(config) => api::desktop::compare_experiment(config)
                    .await
                    .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string())),
                Err(e) => Err(e.to_string()),
            },
            "export_experiment" => match serde_json::from_value(args["config"].clone()) {
                Ok(config) => service.export_experiment(config).await.map(Value::String),
                Err(e) => Err(e.to_string()),
            },
            _ => Err("Unknown test command".into()),
        };
        let response = match result {
            Ok(value) => json!({"id":request["id"],"value":value}),
            Err(error) => json!({"id":request["id"],"error":error}),
        };
        println!("{}", response);
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    driver.abort();
    Ok(())
}
