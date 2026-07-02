//! idxperf probe harness: rebuild the index in STREAMING mode.
//!
//! Streaming mode has no CLI flag, so the canonical-dump determinism and
//! equivalence checks for the streaming write path drive it through the
//! library API. See `.idxperf/spec.md`.
//!
//! Usage: `cargo run --release --example idxperf_stream -- <workspace>`

fn main() {
    let ws = std::env::args()
        .nth(1)
        .expect("usage: idxperf_stream <workspace>");
    let mut tethys = tethys::Tethys::new(std::path::Path::new(&ws)).expect("Tethys::new");
    let stats = tethys
        .rebuild_with_options(tethys::IndexOptions::with_streaming())
        .expect("rebuild_with_options(streaming)");
    println!(
        "streaming: {} files, {} symbols, {} refs in {:?}",
        stats.files_indexed, stats.symbols_found, stats.references_found, stats.duration
    );
}
