//! Standalone production-scale streaming driver for revision_smoke.py.
use std::path::Path;
use tethys::{IndexOptions, Tethys};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    let root = Path::new(arguments.get(1).ok_or("workspace argument required")?);
    let options = IndexOptions::with_streaming_batch_size(32);
    let stats = if arguments.get(2).is_some_and(|arg| arg == "--rebuild") {
        Tethys::rebuild_workspace(root, options)?
    } else {
        Tethys::new(root)?.index_with_options(options)?
    };
    println!(
        "files={} errors={}",
        stats.files_indexed,
        stats.errors.len()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
