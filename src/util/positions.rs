#![allow(dead_code)]

use tower_lsp_server::ls_types::{Position, PositionEncodingKind, Range};

// ── Byte offset → LSP position ────────────────────────────────────────────────

/// Convert a UTF-8 byte offset into the LSP `Position` (line/character) using
/// the negotiated encoding. UTF-8 means byte column; UTF-16 means UTF-16
/// code-unit column (matching what most editors historically expect).
pub fn offset_to_position(source: &str, byte_offset: usize, encoding: &PositionEncodingKind) -> Position {
    let clamped = byte_offset.min(source.len());
    let prefix = &source[..clamped];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_text = &prefix[line_start..];
    let character = if *encoding == PositionEncodingKind::UTF8 {
        line_text.len() as u32
    } else {
        line_text.chars().map(|c| c.len_utf16() as u32).sum()
    };
    Position { line, character }
}

// ── LSP position → byte offset ────────────────────────────────────────────────

/// Convert an LSP `Position` back to a UTF-8 byte offset in `source`.
/// Returns `source.len()` on out-of-range inputs.
pub fn position_to_offset(source: &str, pos: Position, encoding: &PositionEncodingKind) -> usize {
    // Find the byte offset of the start of `pos.line`
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, b) in source.bytes().enumerate() {
        if line == pos.line {
            break;
        }
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    if line < pos.line {
        return source.len();
    }
    let line_text = &source[line_start..];
    let char_offset = character_to_byte(line_text, pos.character, encoding);
    (line_start + char_offset).min(source.len())
}

fn character_to_byte(line: &str, character: u32, encoding: &PositionEncodingKind) -> usize {
    if *encoding == PositionEncodingKind::UTF8 {
        (character as usize).min(line.len())
    } else {
        // UTF-16 code-unit count
        let mut units = 0u32;
        let mut byte_offset = 0usize;
        for c in line.chars() {
            if units >= character {
                break;
            }
            units += c.len_utf16() as u32;
            byte_offset += c.len_utf8();
        }
        byte_offset
    }
}

// ── LSP Range → byte range ────────────────────────────────────────────────────

pub fn range_to_byte_range(source: &str, range: Range, encoding: &PositionEncodingKind) -> (usize, usize) {
    let start = position_to_offset(source, range.start, encoding);
    let end = position_to_offset(source, range.end, encoding);
    (start, end.max(start))
}

// ── Apply incremental text changes ───────────────────────────────────────────

/// Apply a list of LSP `TextDocumentContentChangeEvent` changes to `source`.
/// Changes with `range: None` replace the whole document.
/// Changes are applied in the order given — the caller must ensure they are
/// non-overlapping and in reverse-source order when incremental.
pub fn apply_changes(
    source: &str,
    changes: &[tower_lsp_server::ls_types::TextDocumentContentChangeEvent],
    encoding: &PositionEncodingKind,
) -> String {
    let mut text = source.to_string();
    for change in changes {
        match change.range {
            None => {
                text = change.text.clone();
            }
            Some(range) => {
                let (start, end) = range_to_byte_range(&text, range, encoding);
                text.replace_range(start..end, &change.text);
            }
        }
    }
    text
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const UTF8: PositionEncodingKind = PositionEncodingKind::UTF8;
    const UTF16: PositionEncodingKind = PositionEncodingKind::UTF16;

    #[test]
    fn offset_to_pos_ascii() {
        let src = "hello\nworld\n";
        assert_eq!(offset_to_position(src, 0, &UTF8), Position { line: 0, character: 0 });
        assert_eq!(offset_to_position(src, 5, &UTF8), Position { line: 0, character: 5 });
        assert_eq!(offset_to_position(src, 6, &UTF8), Position { line: 1, character: 0 });
        assert_eq!(offset_to_position(src, 11, &UTF8), Position { line: 1, character: 5 });
    }

    #[test]
    fn offset_to_pos_multibyte_utf16() {
        // "é" is U+00E9, 2 bytes in UTF-8, 1 code unit in UTF-16
        let src = "caféé\n";
        // 'é' at byte 3, 4 (2 bytes each)
        let pos = offset_to_position(src, src.len() - 1 /* before \n */, &UTF16);
        // "caféé" has 5 chars, all 1 UTF-16 unit → character = 5
        assert_eq!(pos, Position { line: 0, character: 5 });
    }

    #[test]
    fn offset_to_pos_surrogate_utf16() {
        // "𝕳" is U+1D573 (Mathematical Fraktur), 4 bytes UTF-8, 2 UTF-16 code units
        let src = "a𝕳b\n";
        // byte 0 = 'a', bytes 1-4 = "𝕳", byte 5 = 'b'
        let pos_b = offset_to_position(src, 5, &UTF16);
        assert_eq!(pos_b, Position { line: 0, character: 3 }); // a=1 + 𝕳=2 = 3
    }

    #[test]
    fn round_trip_utf8() {
        let src = "line1\nline2\nline3\n";
        let enc = &UTF8;
        for offset in [0, 5, 6, 11, 12] {
            let pos = offset_to_position(src, offset, enc);
            let back = position_to_offset(src, pos, enc);
            assert_eq!(back, offset, "round-trip failed at offset {offset}");
        }
    }

    #[test]
    fn round_trip_utf16_multibyte() {
        let src = "caféé\n";
        let enc = &UTF16;
        // 'é' is at bytes 3 and 5 respectively
        for byte_offset in [0, 1, 2, 3, 5, 7] {
            // Only test at char boundaries
            if !src.is_char_boundary(byte_offset) {
                continue;
            }
            let pos = offset_to_position(src, byte_offset, enc);
            let back = position_to_offset(src, pos, enc);
            assert_eq!(back, byte_offset, "round-trip failed at byte {byte_offset}");
        }
    }
}
