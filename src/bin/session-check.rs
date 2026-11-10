//! Headless interoperability probe used by the public-fixture test scripts.
use serde_json::{Value, json};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
fn main() {
    let result = (|| -> Result<(), String> {
        let mut input = String::new();
        std::io::stdin()
            .take(1024 * 1024 + 1)
            .read_to_string(&mut input)
            .map_err(|e| e.to_string())?;
        if input.len() > 1024 * 1024 {
            return Err("Request too large".into());
        }
        let req: Value = serde_json::from_str(&input).map_err(|e| e.to_string())?;
        let path = |key: &str| PathBuf::from(req[key].as_str().unwrap_or(""));
        let cancel = Arc::new(AtomicBool::new(false));
        match req["op"].as_str().unwrap_or("") {
            "inspect" => println!("{}", hashhelm::sessions::inspect(&path("path"))?),
            "run" => hashhelm::engine::run(
                path("engine"),
                hashhelm::engine::read_args(&req["args"])?,
                path("data"),
                cancel,
                |e| println!("{e}"),
            ),
            "probe" => println!(
                "{}",
                json!({"output":hashhelm::engine::plan_probe(&path("engine"),&path("data"),hashhelm::engine::read_args(&req["args"])?,req["action"].as_str().unwrap_or(""),req["password"].as_str().unwrap_or(""),&cancel)?})
            ),
            _ => return Err("Unknown operation".into()),
        }
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
