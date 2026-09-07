//! Ports of the individual `_patch_*` routines from `Devel::PatchPerl`.
//!
//! Each function mirrors the upstream sub of the same name (minus the leading
//! underscore). The version-range and `$^O` guards are reproduced exactly, so a
//! routine only touches the tree when upstream would have. Embedded diffs and
//! whole-file replacements live under `assets/` and are pulled in with
//! `include_str!`; only the control flow is hand-ported.

use std::path::Path;

use crate::diff;
use crate::osname::osname;
use crate::version::NormVer;
use crate::Result;

pub(crate) use crate::patchlevel::{patch_develpatchperlversion, patch_patchlevel};

/// Everything a patch routine needs: the raw version string (for `eq` / regex
/// comparisons), its normalised form (for range comparisons) and the absolute
/// root of the Perl source tree.
pub(crate) struct Ctx<'a> {
    pub version: &'a str,
    pub normver: NormVer,
    pub root: &'a Path,
}

impl Ctx<'_> {
    fn nv(&self) -> u64 {
        self.normver.micro()
    }
}

/// `include_str!` an embedded diff from `assets/patches/`.
macro_rules! diff_asset {
    ($name:literal) => {
        include_str!(concat!("../../assets/patches/", $name))
    };
}

/// `include_str!` an embedded whole file from `assets/files/`.
macro_rules! file_asset {
    ($name:literal) => {
        include_str!(concat!("../../assets/files/", $name))
    };
}

fn apply(cx: &Ctx, diff: &str) -> Result<()> {
    diff::apply_in(cx.root, diff)?;
    Ok(())
}

/// Write `contents` to `<root>/<rel>` (mirrors `Devel::PatchPerl::_write_or_die`
/// plus the `chmod` guard some callers add for read-only source trees).
fn write_file(cx: &Ctx, rel: &str, contents: &str) -> Result<()> {
    crate::fsutil::overwrite(&cx.root.join(rel), contents.as_bytes())?;
    Ok(())
}

/// In-place literal byte-string replacement across a file, like the `perl -pi -e`
/// one-liners upstream shells out to. Every occurrence is replaced. No-op (not an
/// error) if the file is absent or the pattern does not occur.
fn substitute(cx: &Ctx, rel: &str, from: &str, to: &str) -> Result<()> {
    let path = cx.root.join(rel);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(());
    };
    let (from, to) = (from.as_bytes(), to.as_bytes());
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut hit = false;
    while i < bytes.len() {
        if bytes[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
            hit = true;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    if hit {
        crate::fsutil::overwrite(&path, &out)?;
    }
    Ok(())
}

fn matches(re: &str, s: &str) -> bool {
    regex::Regex::new(re).unwrap().is_match(s)
}

// ---------------------------------------------------------------------------
// 5.005 family
// ---------------------------------------------------------------------------

pub(crate) fn patch_5_005(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_5_005.01.diff"))
}
pub(crate) fn patch_5_005_01(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_5_005_01.01.diff"))
}
pub(crate) fn patch_5_005_02(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_5_005_02.01.diff"))
}

pub(crate) fn patch_handy(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_handy.01.diff"))
}

// ---------------------------------------------------------------------------
// makedepend
// ---------------------------------------------------------------------------

pub(crate) fn replace_makedepend(cx: &Ctx) -> Result<()> {
    write_file(
        cx,
        "makedepend.SH",
        file_asset!("replace_makedepend.makedepend.SH"),
    )
}

pub(crate) fn patch_makedepend_lc(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_makedepend_lc.01.diff"))
}

pub(crate) fn patch_makedepend_sh(cx: &Ctx) -> Result<()> {
    let asset = match cx.version {
        "5.6.0" => diff_asset!("patch_makedepend_SH.01.diff"),
        "5.6.1" => diff_asset!("patch_makedepend_SH.02.diff"),
        "5.6.2" => diff_asset!("patch_makedepend_SH.03.diff"),
        "5.7.0" => diff_asset!("patch_makedepend_SH.04.diff"),
        "5.7.1" => diff_asset!("patch_makedepend_SH.05.diff"),
        "5.7.2" => diff_asset!("patch_makedepend_SH.06.diff"),
        "5.7.3" => diff_asset!("patch_makedepend_SH.07.diff"),
        "5.8.0" => diff_asset!("patch_makedepend_SH.08.diff"),
        "5.9.4" => diff_asset!("patch_makedepend_SH.09.diff"),
        _ => diff_asset!("patch_makedepend_SH.10.diff"),
    };
    apply(cx, asset)
}

// ---------------------------------------------------------------------------
// DB_File
// ---------------------------------------------------------------------------

fn patch_db(cx: &Ctx, ver: u32) -> Result<()> {
    let repl = format!("<db{ver}/db.h>");
    substitute(cx, "ext/DB_File/DB_File.xs", "<db.h>", &repl)?;
    substitute(cx, "Configure", "<db.h>", &repl)?;
    Ok(())
}
pub(crate) fn patch_db_1(cx: &Ctx) -> Result<()> {
    patch_db(cx, 1)
}
pub(crate) fn patch_db_3(cx: &Ctx) -> Result<()> {
    patch_db(cx, 3)
}

pub(crate) fn patch_dbfile_clang(cx: &Ctx) -> Result<()> {
    if !(osname() == "freebsd" || osname() == "darwin") {
        return Ok(());
    }
    if !(cx.nv() > 5_008_008) {
        return Ok(());
    }
    if !(cx.nv() < 5_010_001) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_dbfile_clang.01.diff"))?;
    apply(cx, diff_asset!("patch_dbfile_clang.02.diff"))?;
    write_file(
        cx,
        "ext/DB_File/Makefile.PL",
        file_asset!("patch_dbfile_clang.Makefile.PL"),
    )?;
    write_file(
        cx,
        "ext/DB_File/config.in",
        file_asset!("patch_dbfile_clang.config.in"),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// misc single-diff routines
// ---------------------------------------------------------------------------

pub(crate) fn patch_doio(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_doio.01.diff"))
}

pub(crate) fn patch_configure(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_configure.01.diff"))
}

pub(crate) fn patch_conf_gconvert(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_conf_gconvert.01.diff"))
}

pub(crate) fn patch_sort_n(cx: &Ctx) -> Result<()> {
    substitute(
        cx,
        "Configure",
        "$sort -n +1",
        "($sort -n -k 2 2>/dev/null || $sort -n +1)",
    )
}

pub(crate) fn patch_makefile_sh_phony(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_makefile_sh_phony.01.diff"))
}

pub(crate) fn patch_odbm_file_hints_linux(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_odbm_file_hints_linux.01.diff"))
}

pub(crate) fn patch_make_ext_pl(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_make_ext_pl.01.diff"))
}

pub(crate) fn patch_589_perlio_c(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_589_perlio_c.01.diff"))
}

pub(crate) fn patch_regmatch_pointer_5180(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_regmatch_pointer_5180.01.diff"))
}

pub(crate) fn patch_cow_speed(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_cow_speed.01.diff"))
}

pub(crate) fn patch_5183_metajson(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_5183_metajson.01.diff"))
}

pub(crate) fn patch_time_hires(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_time_hires.01.diff"))
}

pub(crate) fn patch_useshrplib(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_useshrplib.01.diff"))
}

// ---------------------------------------------------------------------------
// hsplit rehash
// ---------------------------------------------------------------------------

pub(crate) fn patch_hsplit_rehash_58(cx: &Ctx) -> Result<()> {
    let mut d = diff_asset!("patch_hsplit_rehash_58.01.diff").to_string();
    if cx.version == "5.8.8" {
        // Upstream uses `s///` without `/g`: only the *first* occurrence in the
        // diff text is rewritten (the deletion/context line, so it matches the
        // 5.8.8 source's typo; the addition line keeps the correct spelling).
        replace_first(&mut d, "non-pathological", "non-pathalogical");
        replace_first(&mut d, "triggering", "triggerring");
    }
    apply(cx, &d)
}

fn replace_first(s: &mut String, from: &str, to: &str) {
    if let Some(pos) = s.find(from) {
        s.replace_range(pos..pos + from.len(), to);
    }
}

pub(crate) fn patch_hsplit_rehash_510(cx: &Ctx) -> Result<()> {
    apply(cx, diff_asset!("patch_hsplit_rehash_510.01.diff"))
}

// ---------------------------------------------------------------------------
// version-switched multi-diff routines
// ---------------------------------------------------------------------------

pub(crate) fn patch_archive_tar_tests(cx: &Ctx) -> Result<()> {
    if matches(r"^5\.10", cx.version) {
        apply(cx, diff_asset!("patch_archive_tar_tests.01.diff"))
    } else {
        apply(cx, diff_asset!("patch_archive_tar_tests.02.diff"))
    }
}

pub(crate) fn patch_preprocess_options(cx: &Ctx) -> Result<()> {
    if matches(r"^5\.(?:8|10)\.", cx.version) {
        apply(cx, diff_asset!("patch_preprocess_options.01.diff"))
    } else if matches(r"^5\.6\.", cx.version) {
        apply(cx, diff_asset!("patch_preprocess_options.02.diff"))
    } else {
        Ok(())
    }
}

pub(crate) fn patch_fp_class_denorm(cx: &Ctx) -> Result<()> {
    if cx.nv() < 5_025_004 {
        apply(cx, diff_asset!("patch_fp_class_denorm.01.diff"))
    } else {
        apply(cx, diff_asset!("patch_fp_class_denorm.02.diff"))
    }
}

// ---------------------------------------------------------------------------
// sysv
// ---------------------------------------------------------------------------

fn sysv_should_skip() -> bool {
    osname() != "linux" || Path::new("/usr/include/asm/page.h").is_file()
}
pub(crate) fn patch_sysv_old(cx: &Ctx) -> Result<()> {
    if sysv_should_skip() {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_sysv.01.diff"))
}
pub(crate) fn patch_sysv_new(cx: &Ctx) -> Result<()> {
    if sysv_should_skip() {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_sysv.02.diff"))
}

// ---------------------------------------------------------------------------
// $^O-gated routines
// ---------------------------------------------------------------------------

pub(crate) fn patch_conf_solaris(cx: &Ctx) -> Result<()> {
    if osname() != "solaris" {
        return Ok(());
    }
    if !(cx.nv() < 5_018_000) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_conf_solaris.01.diff"))
}

pub(crate) fn patch_bitrig(cx: &Ctx) -> Result<()> {
    if osname() != "bitrig" {
        return Ok(());
    }
    if !(cx.nv() < 5_019_004) {
        return Ok(());
    }
    if !(cx.nv() < 5_008_000) {
        apply(cx, diff_asset!("patch_bitrig.01.diff"))?; // BOOGLE
    }
    if cx.nv() < 5_008_009 {
        apply(cx, diff_asset!("patch_bitrig.02.diff"))?; // BITRIGM1
    } else {
        apply(cx, diff_asset!("patch_bitrig.03.diff"))?; // BITRIGMX
    }
    if cx.nv() < 5_008_001 {
        // nothing
    } else if cx.nv() < 5_008_007 {
        apply(cx, diff_asset!("patch_bitrig.04.diff"))?; // BITRIGC3
    } else if cx.nv() < 5_008_009 {
        apply(cx, diff_asset!("patch_bitrig.05.diff"))?; // BITRIGC2
    } else if cx.nv() < 5_013_000 {
        apply(cx, diff_asset!("patch_bitrig.06.diff"))?; // BITRIGC1
    } else {
        apply(cx, diff_asset!("patch_bitrig.07.diff"))?; // BITRIGCX
    }
    Ok(())
}

pub(crate) fn patch_dynaloader_mac(cx: &Ctx) -> Result<()> {
    if osname() != "darwin" {
        return Ok(());
    }
    if (cx.nv() > 5_032_000 && cx.nv() < 5_033_000) || cx.nv() > 5_033_005 {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_dynaloader_mac.01.diff"))
}

pub(crate) fn patch_eumm_darwin(cx: &Ctx) -> Result<()> {
    if osname() != "darwin" {
        return Ok(());
    }
    if (cx.nv() > 5_032_000 && cx.nv() < 5_033_000) || cx.nv() > 5_033_005 {
        return Ok(());
    }
    if cx.nv() != 5_006_002 && cx.nv() < 5_008_000 {
        return apply(cx, diff_asset!("patch_eumm_darwin.01.diff"));
    }
    if cx.nv() < 5_011_000 {
        return apply(cx, diff_asset!("patch_eumm_darwin.02.diff"));
    }
    if cx.nv() < 5_013_005 {
        return apply(cx, diff_asset!("patch_eumm_darwin.03.diff"));
    }
    if cx.nv() < 5_015_001 {
        return apply(cx, diff_asset!("patch_eumm_darwin.04.diff"));
    }
    apply(cx, diff_asset!("patch_eumm_darwin.05.diff"))
}

pub(crate) fn patch_mmaix_pm(cx: &Ctx) -> Result<()> {
    if osname() != "aix" {
        return Ok(());
    }
    if !(cx.nv() > 5_027_000) {
        return Ok(());
    }
    if !(cx.nv() < 5_031_001) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_mmaix_pm.01.diff"))
}

// ---------------------------------------------------------------------------
// version-range gated routines
// ---------------------------------------------------------------------------

pub(crate) fn patch_conf_fwrapv(cx: &Ctx) -> Result<()> {
    if !(cx.nv() < 5_019_011) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_conf_fwrapv.01.diff"))
}

pub(crate) fn patch_sdbm_file_c(cx: &Ctx) -> Result<()> {
    if !(cx.nv() > 5_010_000) {
        return Ok(());
    }
    if !(cx.nv() < 5_014_004) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_sdbm_file_c.01.diff"))
}

pub(crate) fn patch_pp_c_libc(cx: &Ctx) -> Result<()> {
    if !(cx.nv() > 5_008_000) {
        return Ok(());
    }
    if !(cx.nv() < 5_028_000) {
        return Ok(());
    }
    apply(cx, diff_asset!("patch_pp_c_libc.01.diff"))
}

pub(crate) fn patch_errno_gcc5(cx: &Ctx) -> Result<()> {
    let nv = cx.nv();
    if !(nv < 5_021_009) {
        return Ok(());
    }
    if nv > 5_020_002 && nv < 5_021_000 {
        return Ok(());
    }
    if nv < 5_006_000 {
        log::warn!("The Errno GCC 5 patch only goes back as far as v5.6.0");
        log::warn!("You will have to generate your own patch to go farther back");
        Ok(())
    } else if nv < 5_006_001 {
        apply(cx, diff_asset!("patch_errno_gcc5.01.diff"))
    } else if nv == 5_007_000 {
        apply(cx, diff_asset!("patch_errno_gcc5.02.diff"))
    } else if nv < 5_007_002 {
        apply(cx, diff_asset!("patch_errno_gcc5.03.diff"))
    } else if nv < 5_007_003 {
        apply(cx, diff_asset!("patch_errno_gcc5.04.diff"))
    } else if nv < 5_008_009 {
        apply(cx, diff_asset!("patch_errno_gcc5.05.diff"))
    } else if nv > 5_008_009 && nv < 5_009_003 {
        apply(cx, diff_asset!("patch_errno_gcc5.06.diff"))
    } else {
        apply(cx, diff_asset!("patch_errno_gcc5.07.diff"))
    }
}

pub(crate) fn patch_utils_h2ph(cx: &Ctx) -> Result<()> {
    let nv = cx.nv();
    if !(nv < 5_021_009) {
        return Ok(());
    }
    if nv == 5_020_003 {
        return Ok(());
    }
    if nv < 5_006_001 {
        return apply(cx, diff_asset!("patch_utils_h2ph.01.diff"));
    }
    if nv < 5_007_000 {
        return apply(cx, diff_asset!("patch_utils_h2ph.02.diff"));
    }
    if nv < 5_007_001 {
        apply(cx, diff_asset!("patch_utils_h2ph.03.diff"))?;
    } else if nv < 5_007_002 {
        apply(cx, diff_asset!("patch_utils_h2ph.04.diff"))?;
    } else if nv < 5_007_003 {
        apply(cx, diff_asset!("patch_utils_h2ph.05.diff"))?;
    }
    if nv < 5_008_000 {
        return apply(cx, diff_asset!("patch_utils_h2ph.06.diff"));
    }
    if nv < 5_008_001 {
        return apply(cx, diff_asset!("patch_utils_h2ph.07.diff"));
    }
    if nv < 5_008_009 {
        return apply(cx, diff_asset!("patch_utils_h2ph.08.diff"));
    }
    if nv > 5_008_009 && nv < 5_009_002 {
        apply(cx, diff_asset!("patch_utils_h2ph.09.diff"))?;
    }
    if nv > 5_008_009 && nv < 5_009_003 {
        apply(cx, diff_asset!("patch_utils_h2ph.10.diff"))?;
    }
    if nv > 5_008_009 && nv < 5_009_004 {
        apply(cx, diff_asset!("patch_utils_h2ph.11.diff"))?;
    }
    apply(cx, diff_asset!("patch_utils_h2ph.12.diff"))
}

pub(crate) fn patch_lib_h2ph(cx: &Ctx) -> Result<()> {
    let nv = cx.nv();
    if !(nv < 5_021_010) {
        return Ok(());
    }
    if nv == 5_020_003 {
        return Ok(());
    }
    if nv >= 5_013_005 {
        apply(cx, diff_asset!("patch_lib_h2ph.01.diff"))
    } else if nv >= 5_013_001 {
        apply(cx, diff_asset!("patch_lib_h2ph.02.diff"))
    } else if nv >= 5_010_001 {
        apply(cx, diff_asset!("patch_lib_h2ph.03.diff"))
    } else {
        Ok(())
    }
}

pub(crate) fn patch_time_local_t(cx: &Ctx) -> Result<()> {
    let nv = cx.nv();
    if nv < 5_029_000 && nv > 5_025_003 {
        return apply(cx, diff_asset!("patch_time_local_t.01.diff"));
    }
    if nv < 5_025_004 && nv > 5_013_008 {
        return apply(cx, diff_asset!("patch_time_local_t.02.diff"));
    }
    if nv < 5_013_009 && nv > 5_010_001 {
        return apply(cx, diff_asset!("patch_time_local_t.03.diff"));
    }
    if (nv <= 5_010_001 && nv > 5_009_003) || nv == 5_008_009 {
        return apply(cx, diff_asset!("patch_time_local_t.04.diff"));
    }
    if nv == 5_009_002 || nv == 5_009_003 || nv == 5_008_008 || nv == 5_008_007 {
        return apply(cx, diff_asset!("patch_time_local_t.05.diff"));
    }
    Ok(())
}

#[allow(clippy::manual_range_contains)] // kept as written to mirror the upstream conditions
pub(crate) fn patch_conf_gcc10(cx: &Ctx) -> Result<()> {
    let nv = cx.nv();
    if !(nv < 5_031_006) {
        return Ok(());
    }
    if nv >= 5_030_002 {
        return Ok(());
    }
    if nv <= 5_006_001 || (nv >= 5_007_000 && nv < 5_008_000) {
        return apply(cx, diff_asset!("patch_conf_gcc10.01.diff"));
    }
    if nv <= 5_008_008 || (nv > 5_008_009 && nv < 5_009_004) {
        return apply(cx, diff_asset!("patch_conf_gcc10.02.diff"));
    }
    if nv <= 5_010_000 {
        return apply(cx, diff_asset!("patch_conf_gcc10.03.diff"));
    }
    if nv < 5_021_002 {
        return apply(cx, diff_asset!("patch_conf_gcc10.04.diff"));
    }
    if nv < 5_023_005 && !(nv >= 5_022_002 && nv < 5_023_000) {
        return apply(cx, diff_asset!("patch_conf_gcc10.05.diff"));
    }
    if (nv <= 5_026_000 || (nv >= 5_027_000 && nv < 5_027_003))
        && !(nv >= 5_024_003 && nv < 5_025_000)
    {
        return apply(cx, diff_asset!("patch_conf_gcc10.06.diff"));
    }
    if nv < 5_029_003 {
        return apply(cx, diff_asset!("patch_conf_gcc10.07.diff"));
    }
    apply(cx, diff_asset!("patch_conf_gcc10.08.diff"))
}

// ---------------------------------------------------------------------------
// disabled upstream (`sub { return; ... }`)
// ---------------------------------------------------------------------------

/// Disabled in `Devel::PatchPerl` (the sub `return`s immediately). Kept as a
/// no-op so the dispatch table stays a 1:1 transcription.
pub(crate) fn patch_skip_using_gcc_brace_groups(_cx: &Ctx) -> Result<()> {
    Ok(())
}

/// Disabled in `Devel::PatchPerl` (see [`patch_skip_using_gcc_brace_groups`]).
pub(crate) fn patch_skip_using_gcc_bg_ppport(_cx: &Ctx) -> Result<()> {
    Ok(())
}
