//! Probe for tethys-4m9o: shortest dependency-chain behavior on the real index.
//!
//! Usage: `cargo run --example probe_4m9o -- <workspace_root> <from> <to>`
//! Prints the chain (workspace-relative paths), its length, and wall time.

use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("usage: probe_4m9o <workspace_root> <from> <to>");
        std::process::exit(2);
    }
    let tethys = tethys::Tethys::new(Path::new(&args[1])).expect("open workspace index");

    let start = Instant::now();
    let result = tethys.get_dependency_chain(Path::new(&args[2]), Path::new(&args[3]));
    let elapsed = start.elapsed();

    match result {
        Ok(Some(chain)) => {
            println!(
                "CHAIN len={} time_ms={:.1}",
                chain.len(),
                elapsed.as_secs_f64() * 1000.0
            );
            for p in &chain {
                println!("  {}", p.display());
            }
        }
        Ok(None) => println!("NONE time_ms={:.1}", elapsed.as_secs_f64() * 1000.0),
        Err(e) => println!("ERR {e} time_ms={:.1}", elapsed.as_secs_f64() * 1000.0),
    }
}
