//! A small pure-Rust unified-diff applier.
//!
//! `Devel::PatchPerl` shells out to GNU `patch -f -s -p0`.  This crate applies
//! the same embedded diffs in-process instead.  The behaviour aimed for is a
//! subset of GNU `patch`:
//!
//! * `-p0`: the path is taken verbatim from the `+++` header (first whitespace
//!   field), after any `a/` `b/` prefixes -- which the upstream payloads have
//!   already stripped.
//! * multi-file diffs (repeated `--- ` / `+++ ` sections),
//! * per-hunk *offset* search (the hunk need not be at the stated line),
//! * per-hunk *fuzz* up to 2 (leading/trailing context lines may be dropped),
//! * `\ No newline at end of file`.
//!
//! Target files are treated as raw bytes (Perl sources are not always UTF-8);
//! only the diff text itself is required to be UTF-8.  Lines such as
//! `diff --git ...` / `index ...` between sections are ignored.

use std::path::{Path, PathBuf};

/// Maximum number of context lines that may be ignored at *each* end of a hunk,
/// matching GNU `patch`'s default `--fuzz=2` (so up to 2 leading *and* 2
/// trailing context lines may be dropped, independently).
const MAX_FUZZ: usize = 2;

/// Failure to parse or apply a diff.
#[derive(Debug, Clone)]
pub struct DiffError {
    /// The file the failure relates to, when known.
    pub file: Option<PathBuf>,
    /// The 1-based hunk number, when the failure is hunk-specific.
    pub hunk: Option<usize>,
    /// Human-readable explanation.
    pub reason: String,
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.file, self.hunk) {
            (Some(file), Some(h)) => write!(f, "{}: hunk #{h}: {}", file.display(), self.reason),
            (Some(file), None) => write!(f, "{}: {}", file.display(), self.reason),
            _ => write!(f, "{}", self.reason),
        }
    }
}

impl std::error::Error for DiffError {}

fn err(reason: impl Into<String>) -> DiffError {
    DiffError {
        file: None,
        hunk: None,
        reason: reason.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Context,
    Del,
    Add,
}

#[derive(Debug, Clone)]
struct BodyLine {
    kind: Kind,
    text: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Hunk {
    old_start: usize, // 1-based, from the header
    body: Vec<BodyLine>,
    /// The new side ends without a trailing newline (`\ No newline at end of file`).
    new_no_newline: bool,
    /// The old side ends without a trailing newline.
    old_no_newline: bool,
}

impl Hunk {
    fn old_lines(&self) -> Vec<&[u8]> {
        self.body
            .iter()
            .filter(|l| matches!(l.kind, Kind::Context | Kind::Del))
            .map(|l| l.text.as_slice())
            .collect()
    }

    fn new_lines(&self) -> Vec<&[u8]> {
        self.body
            .iter()
            .filter(|l| matches!(l.kind, Kind::Context | Kind::Add))
            .map(|l| l.text.as_slice())
            .collect()
    }

    /// Count of leading / trailing context lines (used for fuzzy matching).
    fn leading_context(&self) -> usize {
        self.body
            .iter()
            .take_while(|l| l.kind == Kind::Context)
            .count()
    }

    fn trailing_context(&self) -> usize {
        self.body
            .iter()
            .rev()
            .take_while(|l| l.kind == Kind::Context)
            .count()
    }
}

#[derive(Debug, Clone)]
struct FilePatch {
    target: String,
    hunks: Vec<Hunk>,
}

/// Parse a unified diff into a list of per-file patches.
fn parse(diff: &str) -> Result<Vec<FilePatch>, DiffError> {
    let lines: Vec<&str> = diff.split('\n').collect();
    let mut i = 0;
    let mut patches: Vec<FilePatch> = Vec::new();

    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("--- ") {
            // Expect a matching `+++ ` on the next line.
            let plus = lines.get(i + 1).copied().unwrap_or("");
            let target = match plus.strip_prefix("+++ ") {
                Some(t) => first_field(t),
                None => {
                    return Err(err(format!(
                        "malformed diff: `--- ` not followed by `+++ ` (line {})",
                        i + 1
                    )))
                }
            };
            i += 2;

            let mut hunks = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if l.starts_with("@@ ") || l.starts_with("@@\t") {
                    let (hunk, next) = parse_hunk(&lines, i)?;
                    hunks.push(hunk);
                    i = next;
                } else if l.starts_with("--- ") || l.starts_with("diff --git ") {
                    break;
                } else {
                    // stray line between hunks / trailing blank line
                    i += 1;
                }
            }

            if hunks.is_empty() {
                return Err(err(format!("no hunks for `{target}`")));
            }
            patches.push(FilePatch {
                target: target.to_string(),
                hunks,
            });
        } else {
            i += 1;
        }
    }

    if patches.is_empty() {
        return Err(err("no file sections found in diff"));
    }
    Ok(patches)
}

fn first_field(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

/// Parse `@@ -a,b +c,d @@` and its body starting at `lines[start]`.
fn parse_hunk(lines: &[&str], start: usize) -> Result<(Hunk, usize), DiffError> {
    let header = lines[start];
    let (old_start, old_count, new_count) = parse_hunk_header(header)
        .ok_or_else(|| err(format!("malformed hunk header: {header:?}")))?;

    let mut body: Vec<BodyLine> = Vec::new();
    let mut old_seen = 0usize;
    let mut new_seen = 0usize;
    let mut old_no_newline = false;
    let mut new_no_newline = false;
    let mut i = start + 1;

    while i < lines.len() {
        let raw = lines[i];

        // "\ No newline at end of file" may trail the counted body; consume it
        // regardless of whether the -a/+b counts are already satisfied.
        if raw.starts_with('\\') {
            match body.last().map(|l| l.kind) {
                Some(Kind::Add) => new_no_newline = true,
                Some(Kind::Del) => old_no_newline = true,
                Some(Kind::Context) => {
                    old_no_newline = true;
                    new_no_newline = true;
                }
                None => {}
            }
            i += 1;
            continue;
        }

        if old_seen >= old_count && new_seen >= new_count {
            break;
        }

        // A totally empty line inside a hunk is a blank context line.
        if raw.is_empty() {
            body.push(BodyLine {
                kind: Kind::Context,
                text: Vec::new(),
            });
            old_seen += 1;
            new_seen += 1;
            i += 1;
            continue;
        }

        let (marker, text) = raw.split_at(1);
        let text = text.as_bytes().to_vec();
        match marker {
            " " => {
                body.push(BodyLine {
                    kind: Kind::Context,
                    text,
                });
                old_seen += 1;
                new_seen += 1;
            }
            "-" => {
                body.push(BodyLine {
                    kind: Kind::Del,
                    text,
                });
                old_seen += 1;
            }
            "+" => {
                body.push(BodyLine {
                    kind: Kind::Add,
                    text,
                });
                new_seen += 1;
            }
            _ => {
                return Err(err(format!(
                    "unexpected line inside hunk {header:?}: {raw:?}"
                )));
            }
        }
        i += 1;
    }

    if old_seen < old_count || new_seen < new_count {
        return Err(err(format!(
            "truncated hunk {header:?} (expected -{old_count} +{new_count}, saw -{old_seen} +{new_seen})"
        )));
    }

    Ok((
        Hunk {
            old_start,
            body,
            new_no_newline,
            old_no_newline,
        },
        i,
    ))
}

fn parse_hunk_header(header: &str) -> Option<(usize, usize, usize)> {
    // @@ -a,b +c,d @@ ...
    let rest = header.strip_prefix("@@")?.trim_start();
    let rest = rest.strip_prefix('-')?;
    let mut it = rest.split_whitespace();
    let old = it.next()?;
    let new = it.next()?.strip_prefix('+')?;

    let parse_pair = |s: &str| -> Option<(usize, usize)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = parse_pair(old)?;
    let (_new_start, new_count) = parse_pair(new)?;
    Some((old_start.max(1), old_count, new_count))
}

/// A file split into lines (without terminators) plus its final-newline state.
struct Doc {
    lines: Vec<Vec<u8>>,
    trailing_newline: bool,
}

impl Doc {
    fn parse(bytes: &[u8]) -> Doc {
        if bytes.is_empty() {
            return Doc {
                lines: Vec::new(),
                trailing_newline: false,
            };
        }
        let trailing_newline = bytes.last() == Some(&b'\n');
        let mut lines: Vec<Vec<u8>> = bytes.split(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
        if trailing_newline {
            lines.pop(); // drop the empty element after the final '\n'
        }
        Doc {
            lines,
            trailing_newline,
        }
    }

    fn render(&self) -> Vec<u8> {
        let mut out = self.lines.join(&b'\n');
        if self.trailing_newline {
            out.push(b'\n');
        }
        out
    }
}

/// Try to locate `needle` inside `hay` near `ideal` (0-based), searching outward
/// from the ideal position in both directions.
fn find_near(hay: &[Vec<u8>], needle: &[&[u8]], ideal: isize) -> Option<usize> {
    if needle.is_empty() {
        return Some(ideal.clamp(0, hay.len() as isize) as usize);
    }
    if needle.len() > hay.len() {
        return None;
    }
    let last = (hay.len() - needle.len()) as isize;
    let matches_at = |p: usize| {
        hay[p..p + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| a.as_slice() == *b)
    };

    let ideal = ideal.clamp(0, last);
    for dist in 0..=last.max(ideal) + 1 {
        for cand in [ideal - dist, ideal + dist] {
            if (0..=last).contains(&cand) && matches_at(cand as usize) {
                return Some(cand as usize);
            }
            if dist == 0 {
                break; // ideal - 0 == ideal + 0, only test once
            }
        }
    }
    None
}

/// Apply a single hunk to `doc`. `offset` is the running line delta from hunks
/// already applied to this file; it is updated in place.
fn apply_hunk(
    doc: &mut Doc,
    hunk: &Hunk,
    index: usize,
    offset: &mut isize,
    target: &Path,
) -> Result<(), DiffError> {
    let old = hunk.old_lines();
    let new = hunk.new_lines();
    let ideal = (hunk.old_start as isize - 1) + *offset;

    let lead = hunk.leading_context();
    let trail = hunk.trailing_context();

    // GNU `patch` semantics: at fuzz level F, up to F context lines may be
    // ignored at the start AND up to F at the end, independently. Try the least
    // amount of fuzz first, preferring to drop from the trailing end.
    let mut combos: Vec<(usize, usize)> = Vec::new();
    for dl in 0..=MAX_FUZZ.min(lead) {
        for dt in 0..=MAX_FUZZ.min(trail) {
            combos.push((dl, dt));
        }
    }
    combos.sort_by_key(|&(dl, dt)| (dl.max(dt), dl + dt, dl));

    for (drop_lead, drop_trail) in combos {
        if drop_lead + drop_trail >= old.len() || drop_lead + drop_trail >= new.len() {
            continue;
        }
        let old_slice = &old[drop_lead..old.len() - drop_trail];
        // The new side shares the same leading/trailing context lines.
        let new_slice = &new[drop_lead..new.len() - drop_trail];
        if old_slice.is_empty() {
            continue;
        }

        let search_ideal = ideal + drop_lead as isize;
        if let Some(pos) = find_near(&doc.lines, old_slice, search_ideal) {
            let end = pos + old_slice.len();
            let replacement: Vec<Vec<u8>> = new_slice.iter().map(|s| s.to_vec()).collect();
            let removed = old_slice.len();
            let added = replacement.len();
            let reached_eof = end == doc.lines.len();
            doc.lines.splice(pos..end, replacement);
            *offset += added as isize - removed as isize;

            // Trailing-newline bookkeeping only matters at end of file.
            if reached_eof {
                if hunk.new_no_newline {
                    doc.trailing_newline = false;
                } else if hunk.old_no_newline {
                    doc.trailing_newline = true;
                }
            }
            if drop_lead + drop_trail > 0 {
                log::debug!(
                    "{}: hunk #{} applied with fuzz (drop {drop_lead} lead / {drop_trail} trail)",
                    target.display(),
                    index + 1,
                );
            }
            return Ok(());
        }
    }

    // Distinguish "already applied" for a friendlier message.
    let reason = if !new.is_empty() && find_near(&doc.lines, &new, ideal).is_some() {
        "context not found (appears to be already applied)".to_string()
    } else {
        "context not found".to_string()
    };
    Err(DiffError {
        file: Some(target.to_path_buf()),
        hunk: Some(index + 1),
        reason,
    })
}

/// Apply a unified diff whose paths are resolved relative to `root`.
pub fn apply_in(root: &Path, diff: &str) -> Result<(), DiffError> {
    let patches = parse(diff)?;
    for fp in &patches {
        let path = root.join(&fp.target);
        log::debug!("applying {} hunk(s) to {}", fp.hunks.len(), fp.target);
        let original = std::fs::read(&path).map_err(|e| DiffError {
            file: Some(path.clone()),
            hunk: None,
            reason: format!("cannot read: {e}"),
        })?;
        let mut doc = Doc::parse(&original);
        let mut offset: isize = 0;
        for (idx, hunk) in fp.hunks.iter().enumerate() {
            apply_hunk(&mut doc, hunk, idx, &mut offset, &path)?;
        }
        let rendered = doc.render();
        if rendered != original {
            crate::fsutil::overwrite(&path, &rendered).map_err(|e| DiffError {
                file: Some(path.clone()),
                hunk: None,
                reason: format!("cannot write: {e}"),
            })?;
        }
    }
    Ok(())
}

/// Apply a unified diff whose paths are resolved relative to the current directory.
pub fn apply(diff: &str) -> Result<(), DiffError> {
    apply_in(Path::new("."), diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn run(root: &Path, files: &[(&str, &str)], diff: &str) -> Result<Vec<Vec<u8>>, DiffError> {
        for (name, body) in files {
            let p = root.join(name);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, body).unwrap();
        }
        apply_in(root, diff)?;
        Ok(files
            .iter()
            .map(|(n, _)| fs::read(root.join(n)).unwrap())
            .collect())
    }

    #[test]
    fn exact_match() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let out = run(d.path(), &[("a.txt", "one\ntwo\nthree\n")], diff).unwrap();
        assert_eq!(out[0], b"one\nTWO\nthree\n");
    }

    #[test]
    fn offset_match() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let out = run(d.path(), &[("a.txt", "pad\npad\none\ntwo\nthree\n")], diff).unwrap();
        assert_eq!(out[0], b"pad\npad\none\nTWO\nthree\n");
    }

    #[test]
    fn fuzz_match() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let out = run(d.path(), &[("a.txt", "one\ntwo\nDIFFERENT\n")], diff).unwrap();
        assert_eq!(out[0], b"one\nTWO\nDIFFERENT\n");
    }

    #[test]
    fn multi_file() {
        let d = tempdir().unwrap();
        let diff = concat!(
            "--- a.txt\n+++ a.txt\n@@ -1 +1 @@\n-a\n+A\n",
            "--- b.txt\n+++ b.txt\n@@ -1 +1 @@\n-b\n+B\n",
        );
        let out = run(d.path(), &[("a.txt", "a\n"), ("b.txt", "b\n")], diff).unwrap();
        assert_eq!(out[0], b"A\n");
        assert_eq!(out[1], b"B\n");
    }

    #[test]
    fn no_trailing_newline() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1 +1 @@\n-a\n+A\n\\ No newline at end of file\n";
        let out = run(d.path(), &[("a.txt", "a\n")], diff).unwrap();
        assert_eq!(out[0], b"A");
    }

    #[test]
    fn blank_context_line() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,3 +1,4 @@\n foo\n\n+bar\n baz\n";
        let out = run(d.path(), &[("a.txt", "foo\n\nbaz\n")], diff).unwrap();
        assert_eq!(out[0], b"foo\n\nbar\nbaz\n");
    }

    #[test]
    fn already_applied_errors() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let e = run(d.path(), &[("a.txt", "one\nTWO\nthree\n")], diff).unwrap_err();
        assert!(e.reason.contains("already applied"), "{e}");
    }

    #[test]
    fn tab_in_context() {
        let d = tempdir().unwrap();
        let diff = "--- a.txt\n+++ a.txt\n@@ -1,2 +1,2 @@\n \tindented\n-x\n+y\n";
        let out = run(d.path(), &[("a.txt", "\tindented\nx\n")], diff).unwrap();
        assert_eq!(out[0], b"\tindented\ny\n");
    }

    #[test]
    fn non_utf8_target_is_ok() {
        let d = tempdir().unwrap();
        fs::write(
            d.path().join("latin1.txt"),
            [0xff, 0xfe, b'\n', b'x', b'\n'],
        )
        .unwrap();
        let diff = "--- latin1.txt\n+++ latin1.txt\n@@ -2 +2 @@\n-x\n+y\n";
        apply_in(d.path(), diff).unwrap();
        assert_eq!(
            fs::read(d.path().join("latin1.txt")).unwrap(),
            vec![0xff, 0xfe, b'\n', b'y', b'\n']
        );
    }
}
