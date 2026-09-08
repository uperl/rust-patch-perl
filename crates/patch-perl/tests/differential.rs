//! Differential test against upstream `Devel::PatchPerl`.
//!
//! Ignored by default: it needs the network (to fetch Perl tarballs), `tar`,
//! and a Perl with `Devel::PatchPerl` installed. Run it explicitly:
//!
//! ```text
//! cargo test -p patch-perl --test differential -- --ignored --nocapture
//! ```
//!
//! For each version it unpacks the tarball twice, patches one copy with upstream
//! `Devel::PatchPerl->patch_source` and the other with this crate, then asserts
//! the two trees are byte-identical (ignoring GNU-`patch` / `perl -i` backup
//! artefacts). `PATCH_PERL_VERSION` / `PATCH_PERL_PATCHLEVEL_LABEL` make the two
//! "patched by" strings match.
//!
//! This crate is a port of `Devel::PatchPerl` 2.14 (2025-08-30); the comparison
//! is only meaningful against that upstream release. It reads
//! `$Devel::PatchPerl::VERSION` from the installed module but does not assert on
//! it, so a mismatch shows up as tree divergence rather than a clear message.

use std::fs;
use std::path::Path;
use std::process::Command;

const VERSIONS: &[&str] = &["5.8.9", "5.10.1", "5.16.3", "5.30.3", "5.40.0"];

fn have(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
#[ignore = "needs network + tar + perl with Devel::PatchPerl"]
fn matches_upstream_byte_for_byte() {
    if !have("perl", &["-MDevel::PatchPerl", "-e1"]) {
        eprintln!("skip: perl with Devel::PatchPerl not available");
        return;
    }
    if !have("tar", &["--version"]) {
        eprintln!("skip: tar not available");
        return;
    }

    let work = std::env::temp_dir().join("patch-perl-difftest");
    let cache = work.join("cache");
    fs::create_dir_all(&cache).unwrap();

    let mut failures = Vec::new();
    for v in VERSIONS {
        let file = if v.contains('_') {
            format!("perl{v}.tar.gz")
        } else {
            format!("perl-{v}.tar.gz")
        };
        let tarball = cache.join(&file);
        if !tarball.exists() {
            let url = format!("https://www.cpan.org/src/5.0/{file}");
            let ok = have(
                "curl",
                &[
                    "-sfL",
                    "--max-time",
                    "180",
                    "-o",
                    tarball.to_str().unwrap(),
                    &url,
                ],
            ) || have("wget", &["-q", "-O", tarball.to_str().unwrap(), &url]);
            if !ok || !tarball.exists() {
                eprintln!("skip {v}: could not download {url}");
                continue;
            }
        }

        let base = work.join(v);
        let _ = fs::remove_dir_all(&base);
        let rust_dir = base.join("rust");
        let perl_dir = base.join("perl");
        fs::create_dir_all(&rust_dir).unwrap();
        fs::create_dir_all(&perl_dir).unwrap();
        untar(&tarball, &rust_dir);
        untar(&tarball, &perl_dir);

        std::env::set_var("PATCH_PERL_VERSION", upstream_version());
        std::env::set_var(
            "PATCH_PERL_PATCHLEVEL_LABEL",
            format!("Devel::PatchPerl {}", upstream_version()),
        );

        let perl_ok = Command::new("perl")
            .args([
                "-MDevel::PatchPerl",
                "-e",
                "Devel::PatchPerl->patch_source($ARGV[0], $ARGV[1])",
                v,
                perl_dir.to_str().unwrap(),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !perl_ok {
            eprintln!("skip {v}: upstream Devel::PatchPerl could not patch this tree here");
            continue;
        }

        patch_perl::patch_source(Some(v), &rust_dir).unwrap_or_else(|e| panic!("{v}: {e}"));

        if let Some(diff) = tree_diff(&perl_dir, &rust_dir) {
            eprintln!("{v}: trees differ:\n{diff}");
            failures.push((*v).to_string());
        } else {
            eprintln!("{v}: OK (byte-identical to upstream)");
        }
    }

    assert!(
        failures.is_empty(),
        "diverged from upstream for: {failures:?}"
    );
}

fn upstream_version() -> String {
    let out = Command::new("perl")
        .args([
            "-MDevel::PatchPerl",
            "-e",
            "print $Devel::PatchPerl::VERSION",
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn untar(tarball: &Path, into: &Path) {
    let ok = Command::new("tar")
        .args([
            "xzf",
            tarball.to_str().unwrap(),
            "-C",
            into.to_str().unwrap(),
            "--strip-components=1",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "tar failed for {}", tarball.display());
}

/// `diff -rq` ignoring backup / reject artefacts. Returns `Some(report)` on any
/// content difference or missing/extra file.
fn tree_diff(a: &Path, b: &Path) -> Option<String> {
    let out = Command::new("diff")
        .args([
            "-rq",
            "-x",
            "*.orig",
            "-x",
            "*.rej",
            "-x",
            "*.bak",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ])
        .output()
        .expect("run diff");
    if out.status.success() {
        None
    } else {
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
