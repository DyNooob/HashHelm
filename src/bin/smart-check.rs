use hashhelm::profile;
use std::{io::Read, path::Path};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|v| v == "--pinyin-list") {
        let text = std::fs::read_to_string(&args[2]).unwrap();
        let mut seen = std::collections::HashSet::new();
        let mut words = Vec::new();
        for word in text.lines() {
            let (py, _, _) = profile::transliterate(word);
            if !py.is_empty() && seen.insert(py.clone()) {
                words.push(py)
            }
        }
        std::fs::write(&args[3], words.join("\n") + "\n").unwrap();
        return;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let p = serde_json::from_str::<profile::Profile>(&input).unwrap();
    match profile::save(
        &p,
        Path::new(
            args.get(1)
                .map(String::as_str)
                .unwrap_or("tests/artifacts/profile"),
        ),
    ) {
        Ok(g) => println!("{}", serde_json::to_string(&g).unwrap()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    }
}
