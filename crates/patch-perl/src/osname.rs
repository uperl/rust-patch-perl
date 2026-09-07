//! Perl-style operating-system name (`$^O`).
//!
//! Upstream `Devel::PatchPerl` gates many patches on the value of Perl's `$^O`
//! special variable.  Rust's [`std::env::consts::OS`] uses slightly different
//! spellings (`macos` vs `darwin`, `windows` vs `MSWin32`), so this module maps
//! back to the `$^O` spelling that the ported guards compare against.
//!
//! The `PATCH_PERL_FAKE_OS` environment variable overrides the detected value.
//! It exists so the OS-gated patches can be exercised in tests on any host; it
//! is not part of upstream and should not be relied on in production.

/// Returns the current operating system spelled the way Perl's `$^O` would.
///
/// Honours the `PATCH_PERL_FAKE_OS` override when set and non-empty.
pub fn osname() -> String {
    if let Ok(fake) = std::env::var("PATCH_PERL_FAKE_OS") {
        if !fake.is_empty() {
            return fake;
        }
    }
    map(std::env::consts::OS)
}

fn map(rust_os: &str) -> String {
    match rust_os {
        "macos" => "darwin",
        "windows" => "MSWin32",
        // linux, freebsd, netbsd, openbsd, dragonfly, solaris, ... already match $^O.
        other => other,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_rust_spellings_to_perl() {
        assert_eq!(map("macos"), "darwin");
        assert_eq!(map("windows"), "MSWin32");
        assert_eq!(map("linux"), "linux");
        assert_eq!(map("freebsd"), "freebsd");
        assert_eq!(map("solaris"), "solaris");
    }
}
