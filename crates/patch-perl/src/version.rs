//! Perl version parsing and normalisation.
//!
//! Ports `Devel::PatchPerl::_norm_ver`, `_determine_version` and the two
//! `use constant` gates `CERTIFIED` / `HINTSCERT`, as of `Devel::PatchPerl` 2.14.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

/// A normalised Perl version, stored as `major * 1_000_000 + minor * 1_000 + subversion`.
///
/// This mirrors `Devel::PatchPerl::_norm_ver`, which formats the parts with
/// `sprintf '%d.%03d%03d'` and then compares the result numerically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NormVer(u64);

impl NormVer {
    /// Build from explicit `(major, minor, subversion)` parts.
    pub const fn from_parts(major: u64, minor: u64, sub: u64) -> Self {
        NormVer(major * 1_000_000 + minor * 1_000 + sub)
    }

    /// The packed integer form (`5.010001` becomes `5_010_001`).
    pub const fn micro(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NormVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{:03}{:03}",
            self.0 / 1_000_000,
            (self.0 / 1_000) % 1_000,
            self.0 % 1_000
        )
    }
}

/// Anything at or above this version is left untouched (upstream `CERTIFIED`).
pub const CERTIFIED: NormVer = NormVer::from_parts(5, 33, 2);
/// Replacement `hints` files stop being applied at or above this version
/// (upstream `HINTSCERT`).
pub const HINTSCERT: NormVer = NormVer::from_parts(5, 41, 12);

static SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[._]0*").unwrap());

/// Parse the leading integer of `s` the way Perl's numeric coercion would
/// (`"005"` -> 5, `"1n"` -> 1, `""` -> 0).
fn leading_int(s: &str) -> u64 {
    let digits: String = s
        .trim_start()
        .chars()
        .skip_while(|c| *c == '+')
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap_or(0)
}

/// Port of `Devel::PatchPerl::_norm_ver`.
///
/// ```text
/// _norm_ver("5.005")    -> 5.005000
/// _norm_ver("5.005_03") -> 5.005003
/// _norm_ver("5.6.0")    -> 5.006000
/// _norm_ver("5.10.1")   -> 5.010001
/// ```
pub fn norm_ver(ver: &str) -> NormVer {
    let mut parts = SPLIT_RE.split(ver).map(leading_int);
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let sub = parts.next().unwrap_or(0);
    NormVer::from_parts(major, minor, sub)
}

/// Port of `Devel::PatchPerl::_determine_version`: read `patchlevel.h` from a
/// Perl source tree and return its version string, or `None` if `path` is not a
/// Perl source tree.
pub fn determine_version(source: &Path) -> Option<String> {
    let patchlevel_h = source.join("patchlevel.h");
    let text = std::fs::read_to_string(&patchlevel_h).ok()?;

    let mut defines: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if !line.starts_with("#define") {
            continue;
        }
        let mut it = line.split_whitespace();
        let _ = it.next(); // "#define"
        if let (Some(name), Some(value)) = (it.next(), it.next()) {
            defines.push((name.to_string(), value.to_string()));
        }
    }
    let get = |key: &str| {
        defines
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    let modern: Vec<String> = ["PERL_REVISION", "PERL_VERSION", "PERL_SUBVERSION"]
        .iter()
        .filter_map(|k| get(k))
        .collect();
    if !modern.is_empty() {
        return Some(modern.join("."));
    }

    let legacy: Vec<u64> = ["PATCHLEVEL", "SUBVERSION"]
        .iter()
        .filter_map(|k| get(k))
        .map(|v| leading_int(&v))
        .collect();
    if legacy.len() == 2 {
        return Some(format!("5.{:03}_{:02}", legacy[0], legacy[1]));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_ver_matches_perl() {
        assert_eq!(norm_ver("5.005").micro(), 5_005_000);
        assert_eq!(norm_ver("5.005_03").micro(), 5_005_003);
        assert_eq!(norm_ver("5.005_04").micro(), 5_005_004);
        assert_eq!(norm_ver("5.004_05").micro(), 5_004_005);
        assert_eq!(norm_ver("5.6.0").micro(), 5_006_000);
        assert_eq!(norm_ver("5.6.1").micro(), 5_006_001);
        assert_eq!(norm_ver("5.10.1").micro(), 5_010_001);
        assert_eq!(norm_ver("5.8.9").micro(), 5_008_009);
        assert_eq!(norm_ver("5.32.0").micro(), 5_032_000);
        assert_eq!(norm_ver("5.40.2").micro(), 5_040_002);
    }

    #[test]
    fn certified_gate() {
        assert!(norm_ver("5.32.1") < CERTIFIED);
        assert!(norm_ver("5.33.2") >= CERTIFIED);
        assert!(norm_ver("5.40.0") >= CERTIFIED);
        assert!(norm_ver("5.40.0") < HINTSCERT);
        assert!(norm_ver("5.42.0") >= HINTSCERT);
    }

    #[test]
    fn display_roundtrip() {
        assert_eq!(norm_ver("5.10.1").to_string(), "5.010001");
    }
}
