use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

/// The file that makes a directory a skill package. Shared so the install guard
/// and the frontmatter reader cannot drift apart on the name.
pub const SKILL_FILE: &str = "SKILL.md";

/// How many bytes of `SKILL.md` we are willing to read while hunting for
/// frontmatter. YAML frontmatter is always at the top of the file, so a bounded
/// read keeps scanning cheap for large skills and avoids pulling whole files
/// into memory.
const FRONTMATTER_SCAN_BYTES: usize = 8 * 1024;

/// Read one scalar value out of a skill's `SKILL.md` YAML frontmatter.
///
/// This is deliberately a small hand-rolled reader rather than a YAML
/// dependency. The only field the desktop client needs is `homepage`, it is
/// always a plain `key: value` scalar on a single line, and a full parser would
/// add a crate (and its supply-chain surface) for one string.
pub fn read_frontmatter_value(skill_dir: &Path, key: &str) -> Option<String> {
    let text = read_head(&skill_dir.join(SKILL_FILE), FRONTMATTER_SCAN_BYTES)?;
    extract_value(&text, key)
}

/// Read up to `limit` bytes from `path`, decoded lossily.
///
/// A bounded read can slice a multi-byte character in half; decoding lossily
/// keeps the parse working on an otherwise valid file instead of failing.
fn read_head(path: &Path, limit: usize) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buf = vec![0u8; limit];
    let mut filled = 0usize;

    while filled < limit {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
    }

    buf.truncate(filled);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Pull `key`'s scalar value out of the leading `---` frontmatter block.
///
/// Returns `None` when there is no well-formed frontmatter block, when the key
/// is absent, or when its value is empty.
fn extract_value(text: &str, key: &str) -> Option<String> {
    let mut lines = text.lines();

    // Frontmatter is only frontmatter if the block opens on the very first line.
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }

    for raw in lines {
        // `lines()` already splits on \n; strip the \r of a CRLF file so the
        // closing delimiter and values compare cleanly.
        let line = raw.trim_end_matches('\r');

        if line.trim_end() == "---" {
            return None; // Block closed without the key.
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        // Only top-level keys count (no indentation), so a nested `homepage`
        // inside another mapping is never mistaken for the real one.
        if line.len() != line.trim_start().len() {
            continue;
        }

        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(value) = rest.strip_prefix(':') else {
            continue;
        };

        let value = unquote(strip_inline_comment(value).trim());
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }

    None
}

/// Drop a YAML end-of-line comment, respecting quotes.
///
/// Per YAML, `#` only opens a comment when preceded by whitespace, which is
/// what keeps `https://host/page#anchor` intact.
fn strip_inline_comment(value: &str) -> &str {
    let bytes = value.as_bytes();
    let mut open_quote: Option<u8> = None;

    for (idx, byte) in bytes.iter().enumerate() {
        match open_quote {
            Some(quote) => {
                if *byte == quote {
                    open_quote = None;
                }
            }
            None => {
                if *byte == b'\'' || *byte == b'"' {
                    open_quote = Some(*byte);
                } else if *byte == b'#'
                    && idx > 0
                    && (bytes[idx - 1] == b' ' || bytes[idx - 1] == b'\t')
                {
                    return &value[..idx];
                }
            }
        }
    }

    value
}

/// Strip a matching pair of wrapping single or double quotes.
fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}
