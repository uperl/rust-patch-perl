//! Compiles the C example plugin (`examples/c-plugin/plugin.c`) and drives it
//! through the same host as `tests/plugin.rs`. Skipped when no C compiler is
//! available.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cc() -> Option<String> {
    if let Ok(c) = std::env::var("CC") {
        if !c.is_empty() {
            return Some(c);
        }
    }
    for c in ["cc", "clang", "gcc"] {
        let ok = Command::new(c)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(c.to_string());
        }
    }
    None
}

/// Compile the plugin into `out_dir`, returning the shared-library path.
fn build_c_plugin(cc: &str, out_dir: &Path) -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let src = repo_root.join("examples/c-plugin/plugin.c");
    let include = repo_root.join("include");

    let (ext, mut link_flags): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("bundle", vec!["-bundle", "-undefined", "dynamic_lookup"])
    } else {
        ("so", vec!["-shared"])
    };
    link_flags.push("-fPIC");

    let out = out_dir.join(format!("libpatch_perl_plugin_cexample.{ext}"));
    let status = Command::new(cc)
        .args(["-O2", "-Wall"])
        .args(&link_flags)
        .arg("-I")
        .arg(&include)
        .arg("-o")
        .arg(&out)
        .arg(&src)
        .status()
        .expect("run C compiler");
    assert!(status.success(), "compiling the C example plugin failed");
    out
}

fn fake_tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("patchlevel.h"),
        "#define PERL_REVISION 5\n#define PERL_VERSION 42\n#define PERL_SUBVERSION 0\n",
    )
    .unwrap();
    fs::write(d.path().join("plugin-target.txt"), "before\n").unwrap();
    d
}

#[test]
fn c_plugin_round_trips_through_the_c_abi() {
    let Some(cc) = cc() else {
        eprintln!("skip: no C compiler found");
        return;
    };
    let build_dir = tempfile::tempdir().unwrap();
    let plugin = build_c_plugin(&cc, build_dir.path());

    let tree = fake_tree();
    patch_perl::PatchPerl::new()
        .version("5.42.0")
        .source(tree.path())
        .plugin(plugin.to_str().unwrap())
        .run()
        .expect("patch_source with the C plugin");

    assert_eq!(
        fs::read_to_string(tree.path().join("PATCHPERL_PLUGIN_RAN")).unwrap(),
        "5.42.0"
    );
    assert_eq!(
        fs::read_to_string(tree.path().join("plugin-target.txt")).unwrap(),
        "after\n"
    );
}

#[test]
fn c_plugin_failure_is_not_fatal() {
    let Some(cc) = cc() else {
        eprintln!("skip: no C compiler found");
        return;
    };
    let build_dir = tempfile::tempdir().unwrap();
    let plugin = build_c_plugin(&cc, build_dir.path());

    let tree = fake_tree();
    fs::write(tree.path().join("PLUGIN_SHOULD_FAIL"), "").unwrap();

    patch_perl::PatchPerl::new()
        .version("5.42.0")
        .source(tree.path())
        .plugin(plugin.to_str().unwrap())
        .run()
        .expect("a failing C plugin must not fail the run");

    // side effects before the failure still happened
    assert!(tree.path().join("PATCHPERL_PLUGIN_RAN").exists());
}
