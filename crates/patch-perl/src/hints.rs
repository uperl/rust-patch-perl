//! Replacement `hints/*.sh` files.
//!
//! Port of `Devel::PatchPerl::Hints` plus `Devel::PatchPerl::_patch_hints`.
//! Old Perls ship `hints` files that no longer produce a working build on
//! current systems; this module overwrites them with fixed copies.

use std::path::Path;

use crate::osname::osname;
use crate::Result;

macro_rules! hints_table {
    ($( ($os:literal, $file:literal) ),* $(,)?) => {
        /// `(os key, on-disk filename, file contents)` for every bundled hints file.
        pub(crate) const HINTS: &[(&str, &str, &str)] = &[
            $( ($os, $file, include_str!(concat!("../assets/hints/", $file))) ),*
        ];
    };
}

hints_table![
    ("bitrig", "bitrig.sh"),
    ("cygwin", "cygwin.sh"),
    ("darwin", "darwin.sh"),
    ("dragonfly", "dragonfly.sh"),
    ("freebsd", "freebsd.sh"),
    ("gnu", "gnu.sh"),
    ("gnukfreebsd", "gnukfreebsd.sh"),
    ("hpux", "hpux.sh"),
    ("linux", "linux.sh"),
    ("midnightbsd", "midnightbsd.sh"),
    ("netbsd", "netbsd.sh"),
    ("openbsd", "openbsd.sh"),
    ("solaris", "solaris_2.sh"),
];

/// Return `(filename, contents)` of the replacement hints file for `os`
/// (a `$^O`-style name), or `None` if there is no bundled replacement.
///
/// Port of `Devel::PatchPerl::Hints::hint_file`.
pub fn hint_file(os: &str) -> Option<(&'static str, &'static str)> {
    HINTS
        .iter()
        .find(|(key, _, _)| *key == os)
        .map(|(_, file, content)| (*file, *content))
}

/// The list of OS names for which a replacement hints file exists, sorted.
///
/// Port of `Devel::PatchPerl::Hints::hints`.
pub fn hints() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = HINTS.iter().map(|(k, _, _)| *k).collect();
    v.sort_unstable();
    v
}

/// Port of `Devel::PatchPerl::_patch_hints`: overwrite `<root>/hints/<file>.sh`
/// for the current OS (and `linux` too, when running on `gnukfreebsd`).
pub(crate) fn patch_hints(root: &Path) -> Result<()> {
    let mut oses = vec![osname()];
    if osname() == "gnukfreebsd" {
        oses.push("linux".to_string());
    }

    for os in oses {
        let Some((file, data)) = hint_file(&os) else {
            return Ok(()); // matches upstream `return unless ...`
        };
        let path = root.join("hints").join(file);
        log::warn!("Patching '{}'", path.display());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        #[cfg(unix)]
        if path.exists() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        }
        std::fs::write(&path, data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hint_file_is_non_empty() {
        for (os, file, content) in HINTS {
            assert!(!content.is_empty(), "{os} ({file}) is empty");
        }
    }

    #[test]
    fn solaris_maps_to_solaris_2_sh() {
        let (file, content) = hint_file("solaris").unwrap();
        assert_eq!(file, "solaris_2.sh");
        assert!(content.contains("solaris"));
    }

    #[test]
    fn hints_list_is_sorted_and_complete() {
        let h = hints();
        assert_eq!(h.len(), 13);
        assert!(h.windows(2).all(|w| w[0] < w[1]));
        assert!(h.contains(&"linux"));
    }

    #[test]
    fn unknown_os_has_no_hint() {
        assert!(hint_file("plan9").is_none());
    }
}
