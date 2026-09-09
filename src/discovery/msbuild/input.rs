//! Capped file reads shared by candidate and restore input parsers.

use std::io::Read;
use std::path::Path;

/// Read through the inclusive limit without trusting a separate metadata check.
/// Oversized content is configuration failure; operational I/O retains its type.
pub(super) fn bounded_read(path: &Path, limit: u64) -> crate::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(crate::Error::Config(format!(
            "{} exceeds the {limit}-byte input limit",
            path.display()
        )));
    }
    Ok(bytes)
}
