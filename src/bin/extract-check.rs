use hashhelm::import;
fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        std::process::exit(2)
    };
    match import::extract(std::path::Path::new(&path), None) {
        Ok(result) => println!("{}", serde_json::to_string(&result).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
