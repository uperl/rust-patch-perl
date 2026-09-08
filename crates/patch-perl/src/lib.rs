//! A Rust port of the classic Perl module [`Devel::PatchPerl`][dpp].
//!
//! `Devel::PatchPerl` patches a Perl source tree so that an old version of Perl
//! still builds on a modern toolchain. It is what `perlbrew` and `Perl::Build`
//! use under the hood. This crate is a faithful port of the **library** (not the
//! `patchperl` command-line tool), with two deliberate differences:
//!
//! * unified diffs are applied by a small pure-Rust engine ([`mod@diff`]) rather
//!   than by shelling out to GNU `patch`;
//! * the `PERL5_PATCHPERL_PLUGIN` hook loads a **native shared library** with a
//!   C ABI ([`mod@plugin`]) instead of a Perl module, so plugins can be written
//!   in C, C++ or Rust.
//!
//! # Example
//!
//! ```no_run
//! // Patch an unpacked perl-5.10.1 source tree in place.
//! patch_perl::patch_source(Some("5.10.1"), "/tmp/perl-5.10.1")?;
//! # Ok::<(), patch_perl::Error>(())
//! ```
//!
//! When the version is `None` it is read from `patchlevel.h`:
//!
//! ```no_run
//! let v = patch_perl::determine_version("/tmp/perl-5.10.1");
//! assert_eq!(v.as_deref(), Some("5.10.1"));
//! ```
//!
//! Anything at or above Perl 5.34 (`CERTIFIED`) is left untouched by the patch
//! set; replacement `hints` files stop at Perl 5.42 (`HINTSCERT`). Plugins run
//! regardless of version.
//!
//! # Upstream version
//!
//! Ported from [`Devel::PatchPerl` 2.14][rel] (released 2025-08-30 by Chris
//! Williams / BINGOS). The `@patch` dispatch table, the `CERTIFIED` (`5.33.2`)
//! and `HINTSCERT` (`5.41.12`) gates, the replacement `hints/*.sh` files and the
//! embedded diffs are all taken from that release; see the README for details.
//!
//! [dpp]: https://metacpan.org/pod/Devel::PatchPerl
//! [rel]: https://metacpan.org/release/BINGOS/Devel-PatchPerl-2.14

// Version-range guards below are transcribed from upstream `return unless
// $num < X` statements; the `!(a < b)` form keeps that mapping obvious.
#![allow(clippy::nonminimal_bool)]

use std::path::{Path, PathBuf};

pub mod diff;
pub mod hints;
pub mod plugin;
pub mod version;

mod dispatch;
mod fsutil;
mod osname;
mod patches;
mod patchlevel;

pub use version::{norm_ver, NormVer, CERTIFIED, HINTSCERT};

/// The crate error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// `path` does not look like an unpacked Perl source tree.
    #[error("`{0}` is not a Perl source tree (no patchlevel.h)")]
    NotASourceTree(PathBuf),
    /// No version was supplied and none could be read from `patchlevel.h`.
    #[error("could not determine the Perl version; supply one explicitly")]
    VersionUndetermined,
    /// An I/O error while reading or writing the source tree.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A diff did not apply.
    #[error("{0}")]
    Patch(#[from] diff::DiffError),
    /// A plugin could not be loaded or spoke the wrong ABI. A failure *inside* a
    /// successfully loaded plugin is only logged, never returned (matching
    /// upstream `_process_plugin`).
    #[error("plugin error: {0}")]
    Plugin(String),
}

/// `Result` with this crate's [`Error`] as the default error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Determine the Perl version of the source tree at `source` by parsing
/// `patchlevel.h`. Returns `None` if `source` is not a Perl source tree.
///
/// Port of `Devel::PatchPerl::determine_version`.
pub fn determine_version(source: impl AsRef<Path>) -> Option<String> {
    version::determine_version(source.as_ref())
}

/// Patch the Perl source tree at `source` for the given `version` (or the
/// version read from `patchlevel.h` when `version` is `None`).
///
/// Port of `Devel::PatchPerl::patch_source`. Equivalent to
/// [`PatchPerl::new().source(source).run()`][PatchPerl], optionally with
/// [`version`][PatchPerl::version] set.
pub fn patch_source(version: Option<&str>, source: impl AsRef<Path>) -> Result<()> {
    let mut b = PatchPerl::new().source(source.as_ref());
    if let Some(v) = version {
        b = b.version(v);
    }
    b.run()
}

/// Builder for a patch run.
#[derive(Debug, Clone)]
pub struct PatchPerl {
    version: Option<String>,
    source: PathBuf,
    plugin: Option<String>,
    run_plugins: bool,
}

impl Default for PatchPerl {
    fn default() -> Self {
        Self::new()
    }
}

impl PatchPerl {
    /// A new builder targeting the current directory, with plugin processing on.
    pub fn new() -> Self {
        PatchPerl {
            version: None,
            source: PathBuf::from("."),
            plugin: None,
            run_plugins: true,
        }
    }

    /// Set the Perl version explicitly (e.g. `"5.10.1"`, `"5.8.9"`,
    /// `"5.005_03"`). When unset it is auto-detected from `patchlevel.h`.
    pub fn version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    /// Set the root of the unpacked Perl source tree (default: `.`).
    pub fn source(mut self, p: impl Into<PathBuf>) -> Self {
        self.source = p.into();
        self
    }

    /// Force a specific plugin, overriding `$PERL5_PATCHPERL_PLUGIN`. Accepts a
    /// bare name (resolved on the plugin search path) or a path to a shared
    /// library. See [`mod@plugin`].
    pub fn plugin(mut self, name_or_path: impl Into<String>) -> Self {
        self.plugin = Some(name_or_path.into());
        self
    }

    /// Enable or disable the `PERL5_PATCHPERL_PLUGIN` hook (default: enabled).
    pub fn run_plugins(mut self, yes: bool) -> Self {
        self.run_plugins = yes;
        self
    }

    /// Run the patch process.
    pub fn run(&self) -> Result<()> {
        if !self.source.exists() {
            return Err(Error::NotASourceTree(self.source.clone()));
        }
        // Absolute but not canonicalised: closer to upstream's `rel2abs`, and it
        // does not resolve symlinks out from under the caller.
        let source = if self.source.is_absolute() {
            self.source.clone()
        } else {
            std::env::current_dir()?.join(&self.source)
        };

        let version = match &self.version {
            Some(v) => v.clone(),
            None => match version::determine_version(&source) {
                Some(v) => {
                    log::warn!("Auto-guessed '{v}'");
                    v
                }
                None => return Err(Error::VersionUndetermined),
            },
        };

        let normver = version::norm_ver(&version);

        if normver < version::HINTSCERT {
            hints::patch_hints(&source)?;
        }

        if normver < version::CERTIFIED {
            let cx = patches::Ctx {
                version: &version,
                normver,
                root: &source,
            };
            dispatch::run(&cx)?;
        }

        if self.run_plugins {
            plugin::process_plugin(self.plugin.as_deref(), &version, &source)?;
        }

        Ok(())
    }
}
