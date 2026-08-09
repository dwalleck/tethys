//! Probe for tethys-7a6a (unified directional reachability).
//!
//! Exercises the CURRENT public reachability surface —
//! `Tethys::get_forward_reachable` / `Tethys::get_backward_reachable` — on a
//! byte-copy of the production self-index and dumps every result entry
//! (target id, depth, legacy `is_test` projection, discovery path) so an
//! independent raw-SQLite oracle can be compared item by item.
//!
//! Usage: probe <workspace> <qualified_symbol> <max_depth>
//! Slices mirror the oracle: fwd@N, bwd@N, fwd@0 (validates, empty), fwd@1.

use std::path::Path;
use tethys::{ReachabilityDirection, ReachabilityResult, Tethys};

fn dump(t: &Tethys, symbol: &str, max_depth: usize, forward: bool, tag: &str) {
    let result: ReachabilityResult = if forward {
        t.get_forward_reachable(symbol, Some(max_depth))
    } else {
        t.get_backward_reachable(symbol, Some(max_depth))
    }
    .unwrap_or_else(|e| {
        eprintln!("ERR {tag} symbol={symbol} max_depth={max_depth}: {e}");
        std::process::exit(2);
    });
    let dir = match result.direction {
        ReachabilityDirection::Forward => "forward",
        ReachabilityDirection::Backward => "backward",
    };
    println!(
        "META {tag} symbol={symbol} source_id={} max_depth={} count={} dir={}",
        result.source.id,
        result.max_depth,
        result.reachable.len(),
        dir
    );
    for (i, entry) in result.reachable.iter().enumerate() {
        let path: Vec<String> = entry.path.iter().map(|s| s.id.to_string()).collect();
        println!(
            "ENTRY {tag} seq={i} id={} depth={} is_test={} qn={} path=[{}]",
            entry.target.id,
            entry.depth,
            entry.target.is_test,
            entry.target.qualified_name,
            path.join(",")
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let workspace = Path::new(&args[1]);
    let symbol = &args[2];
    let depth: usize = args[3].parse().expect("max depth");
    let t = Tethys::new(workspace).unwrap_or_else(|e| {
        eprintln!("ERR open {}: {e}", workspace.display());
        std::process::exit(2);
    });
    dump(&t, symbol, depth, true, "fwd");
    dump(&t, symbol, depth, false, "bwd");
    dump(&t, symbol, 0, true, "fwd0");
    dump(&t, symbol, 1, true, "fwd1");
}
