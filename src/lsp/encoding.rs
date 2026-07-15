//! LSP position encoding negotiation and column conversion.
//!
//! Tethys stores tree-sitter columns, which are BYTE offsets into the line
//! (`utf-8` code units). LSP positions default to `utf-16` code units unless
//! a different encoding is negotiated during `initialize` (LSP 3.17
//! `general.positionEncodings`). Tethys advertises `utf-8` first — a server
//! that accepts it (rust-analyzer does) makes conversion an identity no-op —
//! and re-measures outgoing columns when the server insists on `utf-16`.
//!
//! Only OUTGOING positions are converted. Incoming ranges are matched back
//! line-granularly (`find_symbol_at_line`), and line numbers are identical
//! in every position encoding, so no inverse conversion is needed today.

use lsp_types::{InitializeResult, PositionEncodingKind};
use tracing::trace;

/// Position encoding negotiated with an LSP server during `initialize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PositionEncoding {
    /// Byte offsets — identity for tethys's stored tree-sitter columns.
    Utf8,
    /// `utf-16` code units — the LSP spec default; requires conversion.
    Utf16,
}

impl PositionEncoding {
    /// Read the server's chosen encoding from the `initialize` result.
    ///
    /// Falls back to `Utf16` when the server reports none (the LSP spec
    /// default) or reports an encoding tethys doesn't advertise.
    pub(crate) fn from_initialize_result(result: &InitializeResult) -> Self {
        match result.capabilities.position_encoding.as_ref() {
            Some(kind) if *kind == PositionEncodingKind::UTF8 => Self::Utf8,
            _ => Self::Utf16,
        }
    }

    /// Convert a 0-indexed BYTE column on `line_text` into this encoding.
    ///
    /// `Utf8` is an identity no-op. For `Utf16` the byte prefix of the line
    /// is re-measured as `utf-16` code units. A `byte_col` that is out of
    /// range or not on a char boundary falls back to the raw byte offset
    /// rather than dropping the request (correct for ASCII-only prefixes,
    /// best-effort otherwise).
    pub(crate) fn col_from_utf8(self, line_text: &str, byte_col: u32) -> u32 {
        if self == Self::Utf8 {
            return byte_col;
        }
        let prefix = usize::try_from(byte_col)
            .ok()
            .and_then(|col| line_text.get(..col));
        if let Some(prefix) = prefix {
            u32::try_from(prefix.encode_utf16().count()).unwrap_or(byte_col)
        } else {
            trace!(
                byte_col,
                line_len = line_text.len(),
                "byte column out of range or mid-character; sending raw offset"
            );
            byte_col
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::ServerCapabilities;

    fn initialize_result(encoding: Option<PositionEncodingKind>) -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: encoding,
                ..Default::default()
            },
            server_info: None,
        }
    }

    /// A server that accepts the advertised `utf-8` makes conversion identity.
    #[test]
    fn negotiation_picks_utf8_when_server_chooses_it() {
        let result = initialize_result(Some(PositionEncodingKind::UTF8));
        assert_eq!(
            PositionEncoding::from_initialize_result(&result),
            PositionEncoding::Utf8
        );
    }

    /// No reported encoding means the LSP spec default: `utf-16`.
    #[test]
    fn negotiation_defaults_to_utf16_when_absent() {
        let result = initialize_result(None);
        assert_eq!(
            PositionEncoding::from_initialize_result(&result),
            PositionEncoding::Utf16
        );
    }

    /// An unadvertised encoding (e.g. `utf-32`) falls back to `utf-16`.
    #[test]
    fn negotiation_falls_back_to_utf16_for_unadvertised_kind() {
        let result = initialize_result(Some(PositionEncodingKind::UTF32));
        assert_eq!(
            PositionEncoding::from_initialize_result(&result),
            PositionEncoding::Utf16
        );

        let result = initialize_result(Some(PositionEncodingKind::UTF16));
        assert_eq!(
            PositionEncoding::from_initialize_result(&result),
            PositionEncoding::Utf16
        );
    }

    /// ASCII-only prefix: byte and `utf-16` columns coincide (identity).
    #[test]
    fn utf16_col_is_identity_for_ascii_line() {
        let line = "    let x = compute();";
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, 12), 12);
    }

    /// Multibyte text BEFORE the column shrinks it: each CJK char is 3
    /// `utf-8` bytes but 1 `utf-16` unit.
    #[test]
    fn utf16_col_shrinks_after_multibyte_prefix() {
        // "日本語" = 9 bytes, 3 utf-16 units; the identifier starts after
        // `let s = "日本語"; ` → byte 21, utf-16 unit 15.
        let line = "let s = \"\u{65e5}\u{672c}\u{8a9e}\"; foo()";
        let byte_col = u32::try_from(line.find("foo").expect("foo present")).expect("fits");
        assert_eq!(byte_col, 21, "fixture self-check: byte offset of foo");
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, byte_col), 15);
    }

    /// Multibyte text AFTER the column does not affect it (identity).
    #[test]
    fn utf16_col_ignores_multibyte_after_column() {
        let line = "foo(); // \u{65e5}\u{672c}\u{8a9e}";
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, 0), 0);
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, 3), 3);
    }

    /// Emoji are surrogate pairs: 4 `utf-8` bytes = 2 `utf-16` units.
    #[test]
    fn utf16_col_counts_surrogate_pairs_for_emoji() {
        // "😀" (U+1F600) = 4 bytes, 2 utf-16 units.
        let line = "\"\u{1f600}\" bar()";
        let byte_col = u32::try_from(line.find("bar").expect("bar present")).expect("fits");
        assert_eq!(byte_col, 7, "fixture self-check: byte offset of bar");
        // quote(1) + emoji(2) + quote(1) + space(1) = 5 utf-16 units.
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, byte_col), 5);
    }

    /// `utf-8` negotiation is an identity no-op even with multibyte text.
    #[test]
    fn utf8_col_is_identity_even_with_multibyte_prefix() {
        let line = "let s = \"\u{65e5}\u{672c}\u{8a9e}\"; foo()";
        assert_eq!(PositionEncoding::Utf8.col_from_utf8(line, 20), 20);
    }

    /// Out-of-range and mid-character byte columns fall back to the raw
    /// offset rather than panicking or dropping the request.
    #[test]
    fn utf16_col_falls_back_to_raw_offset_when_unsliceable() {
        let line = "\u{65e5}\u{672c}";
        // Past end of line.
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, 99), 99);
        // Mid-character (byte 1 is inside the 3-byte 日).
        assert_eq!(PositionEncoding::Utf16.col_from_utf8(line, 1), 1);
    }
}
