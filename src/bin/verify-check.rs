//! Headless integration entry point. Request JSON (including password hex) uses stdin.
use hashhelm::{platform, verification};
fn main() {
    if let Some(code) = platform::guardian_entry() {
        std::process::exit(code);
    }
    if let Some(code) = verification::worker_entry() {
        std::process::exit(code);
    }
    let value = (|| -> Result<_, String> {
        let request: verification::Request =
            serde_json::from_reader(std::io::stdin()).map_err(|e| e.to_string())?;
        verification::run(request, &std::sync::atomic::AtomicBool::new(false))
    })();
    match value {
        Ok(result) => println!("{}", serde_json::to_string(&result).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
