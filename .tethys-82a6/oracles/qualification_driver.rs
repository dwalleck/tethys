//! Issue-local public API caller; compiled only by qualification Runtime::prepare.
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::Duration;
use tethys::discovery::{DiscoveryCachePolicy, DiscoveryOptions, EvaluationContext};
use tethys::{IndexOptions, Tethys};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    mode: String,
    operation: String,
    companion: PathBuf,
    host: Option<PathBuf>,
    properties: BTreeMap<String, String>,
    bypass_cache: bool,
    allow_restore: bool,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "-w" {
        return Err("expected -w WORKSPACE and JSON stdin".into());
    }
    let root = PathBuf::from(&args[2]).canonicalize()?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let input: Input = serde_json::from_str(&text)?;
    let options = match input.mode.as_str() {
        "batch" => IndexOptions::default(),
        "stream" => IndexOptions::with_streaming_batch_size(32),
        _ => return Err("mode must be batch or stream".into()),
    };
    let mut tethys = Tethys::new(&root)?;
    let snapshot = match input.operation.as_str() {
        "index" => {
            let discovery = DiscoveryOptions {
                trust_msbuild: true,
                allow_restore: input.allow_restore,
                companion_directory: Some(input.companion.canonicalize()?),
                msbuild_path: input.host.map(|path| path.canonicalize()).transpose()?,
                context: EvaluationContext {
                    global_properties: input.properties,
                    ..Default::default()
                },
                cache_policy: if input.bypass_cache {
                    DiscoveryCachePolicy::Disabled
                } else {
                    DiscoveryCachePolicy::Enabled
                },
                timeout: Duration::from_secs(60),
            };
            tethys
                .index_with_options(options.with_discovery(discovery))?
                .discovery
        }
        "update" => {
            // The public update API has no options and deliberately uses its own defaults.
            if input.mode != "batch"
                || input.host.is_some()
                || !input.properties.is_empty()
                || input.bypass_cache
                || input.allow_restore
            {
                return Err("public update() cannot accept mode/discovery overrides".into());
            }
            tethys.update()?.discovery
        }
        _ => return Err("operation must be index or update".into()),
    };
    println!(
        "{}",
        serde_json::json!({
            "units": snapshot.units, "projects": snapshot.projects,
            "issues": snapshot.issues, "cache_observations": snapshot.cache_observations,
        })
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
