//! Exercises the C-ABI plugin hook end to end using the example plugin.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build `patch-perl-plugin-example` and return the path to its cdylib.
fn build_example_plugin() -> PathBuf {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "patch-perl-plugin-example"])
        .status()
        .expect("run cargo build");
    assert!(status.success(), "building the example plugin failed");

    // `CARGO_MANIFEST_DIR` is .../crates/patch-perl; the workspace target dir is
    // two levels up unless CARGO_TARGET_DIR says otherwise.
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .to_path_buf()
        });

    for sub in ["debug", "debug/deps"] {
        let dir = target_dir.join(sub);
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                let is_dylib = name.ends_with(".so")
                    || name.ends_with(".dylib")
                    || name.ends_with(".bundle")
                    || name.ends_with(".dll");
                if is_dylib && name.contains("patch_perl_plugin_example") {
                    return e.path();
                }
            }
        }
    }
    panic!(
        "could not find the built example plugin under {}",
        target_dir.display()
    );
}

fn fake_tree() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("patchlevel.h"),
        "#define PERL_REVISION 5\n#define PERL_VERSION 42\n#define PERL_SUBVERSION 0\n",
    )
    .unwrap();
    // A target for the plugin's apply_patch call.
    fs::write(d.path().join("plugin-target.txt"), "before\n").unwrap();
    d
}

#[test]
fn plugin_runs_and_host_callbacks_work() {
    let plugin = build_example_plugin();
    let d = fake_tree();

    patch_perl::PatchPerl::new()
        .version("5.42.0") // CERTIFIED: only the plugin runs
        .source(d.path())
        .plugin(plugin.to_str().unwrap())
        .run()
        .expect("patch_source with plugin");

    // write_file callback
    let marker = fs::read_to_string(d.path().join("PATCHPERL_PLUGIN_RAN")).unwrap();
    assert_eq!(marker, "5.42.0");
    // apply_patch callback
    assert_eq!(
        fs::read_to_string(d.path().join("plugin-target.txt")).unwrap(),
        "after\n"
    );
}

#[test]
fn failing_plugin_is_not_fatal() {
    let plugin = build_example_plugin();
    let d = fake_tree();

    fs::write(d.path().join("PLUGIN_SHOULD_FAIL"), "").unwrap();
    let result = patch_perl::PatchPerl::new()
        .version("5.42.0")
        .source(d.path())
        .plugin(plugin.to_str().unwrap())
        .run();

    // The plugin returned an error, but the run still succeeds (upstream only
    // `warn`s). Side effects up to the failure point still happened.
    result.expect("a failing plugin must not fail the run");
    assert!(d.path().join("PATCHPERL_PLUGIN_RAN").exists());
}

#[test]
fn unknown_plugin_is_not_fatal() {
    let d = fake_tree();
    patch_perl::PatchPerl::new()
        .version("5.42.0")
        .source(d.path())
        .plugin("definitely-not-installed-anywhere")
        .run()
        .expect("an unresolvable plugin only warns");
}
