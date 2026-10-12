#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use api::desktop::{Command, Service, Snapshot};
use std::sync::Arc;
use tauri::{Manager, State};

#[tauri::command]
async fn discover_agent(
    request: api::desktop::DiscoveryRequest,
) -> Result<api::desktop::AgentDiscovery, String> {
    api::desktop::discover_agent(request).await
}

#[tauri::command]
async fn snapshot(service: State<'_, Arc<Service>>) -> Result<Snapshot, String> {
    Ok(service.snapshot().await)
}
#[tauri::command]
async fn command(service: State<'_, Arc<Service>>, command: Command) -> Result<(), String> {
    service.command(command).await
}
#[tauri::command]
async fn export_run(service: State<'_, Arc<Service>>, run_id: String) -> Result<String, String> {
    service.export_run(&run_id).await
}
#[tauri::command]
fn compare_schedulers() -> Vec<orchestrator::prototype::evaluation::Evaluation> {
    orchestrator::prototype::evaluation::compare()
}
#[tauri::command]
async fn export_evaluation(service: State<'_, Arc<Service>>) -> Result<String, String> {
    service.export_evaluation().await
}
#[tauri::command]
async fn compare_experiment(
    config: orchestrator::prototype::experiments::Config,
) -> Result<orchestrator::prototype::experiments::Report, String> {
    api::desktop::compare_experiment(config).await
}
#[tauri::command]
async fn export_experiment(
    service: State<'_, Arc<Service>>,
    config: orchestrator::prototype::experiments::Config,
) -> Result<String, String> {
    service.export_experiment(config).await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = std::env::var_os("ORCHESTRATION_LAB_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            let service = Service::open(&directory).map_err(std::io::Error::other)?;
            app.manage(service.clone());
            tauri::async_runtime::spawn(service.drive());
            if std::env::var_os("ORCHESTRATION_LAB_TEST_HIDDEN").is_none() {
                app.get_webview_window("main")
                    .ok_or("Missing main window")?
                    .show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            discover_agent,
            command,
            export_run,
            compare_schedulers,
            export_evaluation,
            compare_experiment,
            export_experiment
        ])
        .run(tauri::generate_context!())
        .expect("The desktop app could not start. Check WebView2 and the workspace checkpoint.");
}
