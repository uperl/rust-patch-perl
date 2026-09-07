//! Ports of `_patch_patchlevel` and `_patch_develpatchperlversion`.
//!
//! Both record, in the built perl, that the tree was patched. Both are skipped
//! when the source tree is a git checkout (upstream `-d '.git'`), because a
//! checkout has its own machinery for this.

use crate::diff;
use crate::patches::Ctx;
use crate::Result;

/// This tool's version token. Upstream uses `$Devel::PatchPerl::VERSION`
/// (e.g. `2.14`); this port uses its own crate version. `PATCH_PERL_VERSION`
/// overrides it (used by the differential test that compares this port against
/// upstream byte-for-byte).
fn pp_version() -> String {
    std::env::var("PATCH_PERL_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// The label inserted into `patchlevel.h`'s `local_patches[]`.
///
/// Upstream writes `"Devel::PatchPerl <version>"`; this port writes its own
/// name so `perl -V` truthfully reports what did the patching.
/// `PATCH_PERL_PATCHLEVEL_LABEL` fully overrides it.
fn patchlevel_label() -> String {
    std::env::var("PATCH_PERL_PATCHLEVEL_LABEL")
        .unwrap_or_else(|_| format!("patch-perl {}", pp_version()))
}

/// Port of `Devel::PatchPerl::_patch_patchlevel`.
///
/// Inserts `,"<label>"` into the `local_patches[]` array in `patchlevel.h`,
/// just before its terminating `,NULL`.
pub(crate) fn patch_patchlevel(cx: &Ctx) -> Result<()> {
    if cx.root.join(".git").is_dir() && std::env::var_os("PERL5_PATCHPERL_PATCHLEVEL").is_none() {
        return Ok(());
    }
    let path = cx.root.join("patchlevel.h");
    let Ok(input) = std::fs::read_to_string(&path) else {
        return Ok(());
    };

    let mut out = String::with_capacity(input.len() + 64);
    let mut seen_array = false;
    let label = patchlevel_label();
    for line in input.split_inclusive('\n') {
        if seen_array && line.contains("\t,NULL") {
            out.push_str(&format!("\t,\"{label}\"\n"));
        }
        if line.contains("local_patches[]") {
            seen_array = true;
        }
        out.push_str(line);
    }

    if out != input {
        let _ = std::fs::rename(&path, cx.root.join("patchlevel.bak"));
        std::fs::write(&path, out)?;
    }
    Ok(())
}

/// Port of `Devel::PatchPerl::_patch_develpatchperlversion`: make `Configure`
/// emit `BuiltWithPatchPerl='<version>'` into `config.sh`. Upstream uses the
/// bare version here (not the `Devel::PatchPerl` label).
pub(crate) fn patch_develpatchperlversion(cx: &Ctx) -> Result<()> {
    if cx.root.join(".git").is_dir() {
        return Ok(());
    }
    let dpv = pp_version();
    // This is the `<<"END"` heredoc from upstream with Perl's double-quote
    // escapes resolved (`\$` -> `$`, `\%` -> `%`, `\\n` -> `\n`, `$dpv` -> version).
    let diff = format!(
        concat!(
            "diff --git a/Configure b/Configure\n",
            "index e12c8bb..1a8088f 100755\n",
            "--- Configure\n",
            "+++ Configure\n",
            "@@ -25151,6 +25151,8 @@ zcat='$zcat'\n",
            " zip='$zip'\n",
            " EOT\n",
            " \n",
            "+echo \"BuiltWithPatchPerl='{dpv}'\" >>config.sh\n",
            "+\n",
            " : add special variables\n",
            " $test -f $src/patchlevel.h && \\\n",
            " awk '/^#define[ \t]+PERL_/ {{printf \"%s=%s\\n\",$2,$3}}' $src/patchlevel.h >>config.sh\n",
        ),
        dpv = dpv
    );
    match diff::apply_in(cx.root, &diff) {
        Ok(()) => Ok(()),
        // Non-fatal: not every Configure has this exact context.
        Err(e) => {
            log::warn!("_patch_develpatchperlversion skipped: {e}");
            Ok(())
        }
    }
}
