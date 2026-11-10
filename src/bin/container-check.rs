//! Developer import probe. Password verification uses verify-check's stdin worker.
fn main() {
    let mut args = std::env::args_os().skip(1);
    let path = std::path::PathBuf::from(args.next().expect("container path"));
    let spec = if let Some(path) = args.next() {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    } else {
        hashhelm::containers::Spec::default()
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = (|| -> Result<serde_json::Value, String> {
        let record = hashhelm::verification::record_container(&path, spec, &cancel)?;
        let container = record.container.as_ref().unwrap();
        let extracted = hashhelm::containers::extract(&path, &container.spec)?;
        Ok(
            serde_json::json!({"extracted":extracted,"modes":container.spec.modes(),"record":record}),
        )
    })();
    match result {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
