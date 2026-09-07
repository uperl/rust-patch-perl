//! Behavioural tests that don't need a real Perl tarball.
//!
//! Applying the full patch set for an old Perl requires a *complete* source
//! tree (a routine that targets a missing file is a hard error, exactly as
//! upstream `patch -f` would be). End-to-end fidelity against upstream
//! `Devel::PatchPerl` is therefore covered by `tests/differential.rs`; here we
//! check the pieces that stand alone.

use std::fs;
use std::path::Path;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

#[test]
fn determine_version_modern_patchlevel_h() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "patchlevel.h",
        "#define PERL_REVISION   5\n#define PERL_VERSION    10\n#define PERL_SUBVERSION 1\n",
    );
    assert_eq!(
        patch_perl::determine_version(d.path()).as_deref(),
        Some("5.10.1")
    );
}

#[test]
fn determine_version_legacy_patchlevel_h() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "patchlevel.h",
        "#define PATCHLEVEL 5\n#define SUBVERSION 3\n",
    );
    assert_eq!(
        patch_perl::determine_version(d.path()).as_deref(),
        Some("5.005_03")
    );
}

#[test]
fn determine_version_none_when_absent() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(patch_perl::determine_version(d.path()), None);
}

#[test]
fn patch_source_errors_without_a_version() {
    let d = tempfile::tempdir().unwrap();
    let err = patch_perl::patch_source(None, d.path()).unwrap_err();
    assert!(
        matches!(err, patch_perl::Error::VersionUndetermined),
        "{err:?}"
    );
}

#[test]
fn patch_source_errors_on_missing_source() {
    let err = patch_perl::patch_source(Some("5.10.1"), "/no/such/dir/anywhere").unwrap_err();
    assert!(
        matches!(err, patch_perl::Error::NotASourceTree(_)),
        "{err:?}"
    );
}

#[test]
fn certified_versions_touch_nothing() {
    // >= 5.34 (CERTIFIED) and >= 5.42 (HINTSCERT): no routines, no hints.
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "patchlevel.h",
        "#define PERL_REVISION 5\n#define PERL_VERSION 42\n#define PERL_SUBVERSION 0\n",
    );
    let before = fs::read_to_string(d.path().join("patchlevel.h")).unwrap();
    patch_perl::PatchPerl::new()
        .source(d.path())
        .run_plugins(false)
        .run()
        .unwrap();
    assert_eq!(
        fs::read_to_string(d.path().join("patchlevel.h")).unwrap(),
        before
    );
    assert!(!d.path().join("hints").exists());
}

#[test]
fn hints_between_certified_and_hintscert_still_replaced() {
    // 5.34 <= v < 5.42: no patch routines, but hints are still replaced.
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "patchlevel.h",
        "#define PERL_REVISION 5\n#define PERL_VERSION 40\n#define PERL_SUBVERSION 0\n",
    );
    write(d.path(), "hints/linux.sh", "old contents\n");
    patch_perl::PatchPerl::new()
        .source(d.path())
        .run_plugins(false)
        .run()
        .unwrap();
    let linux_sh = fs::read_to_string(d.path().join("hints/linux.sh")).unwrap();
    assert_ne!(linux_sh, "old contents\n");
    assert!(linux_sh.contains("hints/linux.sh"));
    // patchlevel.h (a patch-routine target) is untouched at this version.
    assert!(!fs::read_to_string(d.path().join("patchlevel.h"))
        .unwrap()
        .contains("patch-perl"));
}

#[test]
fn hint_file_and_hints_list() {
    assert!(patch_perl::hints::hint_file("linux").is_some());
    assert_eq!(
        patch_perl::hints::hint_file("solaris").unwrap().0,
        "solaris_2.sh"
    );
    assert!(patch_perl::hints::hint_file("tos").is_none());
    assert_eq!(patch_perl::hints::hints().len(), 13);
}

#[test]
fn diff_engine_applies_a_multi_file_patch() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.c", "int a = 1;\nint b = 2;\nint c = 3;\n");
    write(d.path(), "sub/b.c", "x\ny\nz\n");
    let diff = "\
--- a.c
+++ a.c
@@ -1,3 +1,3 @@
 int a = 1;
-int b = 2;
+int b = 22;
 int c = 3;
--- sub/b.c
+++ sub/b.c
@@ -1,3 +1,3 @@
 x
-y
+Y
 z
";
    patch_perl::diff::apply_in(d.path(), diff).unwrap();
    assert_eq!(
        fs::read_to_string(d.path().join("a.c")).unwrap(),
        "int a = 1;\nint b = 22;\nint c = 3;\n"
    );
    assert_eq!(
        fs::read_to_string(d.path().join("sub/b.c")).unwrap(),
        "x\nY\nz\n"
    );
}

#[test]
fn diff_engine_reports_a_failed_hunk() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.c", "totally different\n");
    let diff = "--- a.c\n+++ a.c\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
    let err = patch_perl::diff::apply_in(d.path(), diff).unwrap_err();
    assert_eq!(err.hunk, Some(1));
}
