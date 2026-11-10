//! Explicit, headless smoke check of the production public update endpoint.
fn main() {
    let data = tempfile::tempdir().expect("temporary update cache");
    let options = hashhelm::updater::Options::new("stable", "zh").unwrap();
    for _ in 0..2 {
        match hashhelm::updater::check(data.path(), &options) {
            Ok(report) => println!("{}", serde_json::to_string(&report).unwrap()),
            Err(code) => {
                eprintln!("Update check failed: {code}");
                std::process::exit(1);
            }
        }
    }
}
