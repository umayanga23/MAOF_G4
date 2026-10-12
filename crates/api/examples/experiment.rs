//! Reproducible headless evidence. Optional argument: path to a Config JSON file.
//! Prints the complete report to stdout; no workspace changes or model calls.
fn main() -> Result<(), String> {
    let config = match std::env::args().nth(1) {
        Some(path) => serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?,
        None => orchestrator::prototype::experiments::Config::default(),
    };
    let report = orchestrator::prototype::experiments::compare(config)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}
