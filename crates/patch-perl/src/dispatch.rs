//! The version-keyed dispatch table.
//!
//! A 1:1 transcription of the `@patch` array in `Devel::PatchPerl`, in the same
//! order. Each entry pairs a list of version specs (exact strings or regexes,
//! matched like upstream `_is`) with the patch routines to run.

use crate::patches::{self, Ctx};
use crate::Result;

type StepFn = fn(&Ctx) -> Result<()>;

/// A single version spec: an exact string (`eq`) or a regex (`=~`).
pub(crate) enum Spec {
    Exact(&'static str),
    Re(&'static str),
}

pub(crate) struct Entry {
    pub specs: &'static [Spec],
    pub steps: &'static [StepFn],
}

impl Entry {
    /// Port of `_is($entry->{perl}, $version)`.
    fn matches(&self, version: &str) -> bool {
        self.specs.iter().any(|spec| match spec {
            Spec::Exact(e) => *e == version,
            Spec::Re(re) => regex::Regex::new(re)
                .expect("dispatch regex is a compile-time constant")
                .is_match(version),
        })
    }
}

use Spec::{Exact as E, Re as R};

pub(crate) static DISPATCH: &[Entry] = &[
    Entry {
        specs: &[E("5.005")],
        steps: &[patches::patch_5_005],
    },
    Entry {
        specs: &[E("5.005_01")],
        steps: &[patches::patch_5_005_01],
    },
    Entry {
        specs: &[E("5.005_02")],
        steps: &[patches::patch_5_005_02],
    },
    Entry {
        specs: &[R(r"^5\.00[2345]"), E("5.001n")],
        steps: &[patches::patch_handy],
    },
    Entry {
        specs: &[
            E("5.005"),
            E("5.005_01"),
            E("5.005_02"),
            E("5.005_03"),
            E("5.005_04"),
        ],
        steps: &[patches::replace_makedepend],
    },
    Entry {
        specs: &[
            R(r"^5\.00[01234]"),
            E("5.005"),
            E("5.005_01"),
            E("5.005_02"),
            E("5.005_03"),
        ],
        steps: &[patches::patch_db_1],
    },
    Entry {
        specs: &[R(r"^5\.6\.[1-2]$"), R(r"^5\.7\.[0-1]$")],
        steps: &[patches::patch_makefile_sh_phony],
    },
    Entry {
        specs: &[
            E("5.6.0"),
            E("5.6.1"),
            E("5.7.0"),
            E("5.7.1"),
            E("5.7.2"),
            E("5.7.3"),
            E("5.8.0"),
        ],
        steps: &[patches::patch_db_3],
    },
    Entry {
        specs: &[R(r"^5\.004_0[1234]$")],
        steps: &[patches::patch_doio],
    },
    Entry {
        specs: &[E("5.005"), E("5.005_01"), E("5.005_02")],
        steps: &[patches::patch_sysv_old],
    },
    Entry {
        specs: &[
            E("5.005_03"),
            E("5.005_04"),
            R(r"^5\.6\.[0-2]$"),
            R(r"^5\.7\.[0-3]$"),
            R(r"^5\.8\.[0-8]$"),
            R(r"^5\.9\.[0-5]$"),
        ],
        steps: &[patches::patch_sysv_new],
    },
    Entry {
        specs: &[
            R(r"^5\.004_05$"),
            R(r"^5\.005(?:_0[1-4])?$"),
            R(r"^5\.6\.[01]$"),
        ],
        steps: &[patches::patch_configure, patches::patch_makedepend_lc],
    },
    Entry {
        specs: &[R(r"^5\.6\.[0-2]$")],
        steps: &[patches::patch_conf_gconvert, patches::patch_sort_n],
    },
    Entry {
        specs: &[E("5.8.0")],
        steps: &[patches::patch_makedepend_lc],
    },
    Entry {
        specs: &[R(r".*")],
        steps: &[
            patches::patch_conf_solaris,
            patches::patch_bitrig,
            patches::patch_patchlevel,
            patches::patch_develpatchperlversion,
            patches::patch_errno_gcc5,
            patches::patch_conf_fwrapv,
            patches::patch_utils_h2ph,
            patches::patch_lib_h2ph,
            patches::patch_sdbm_file_c,
            patches::patch_mmaix_pm,
            patches::patch_time_local_t,
            patches::patch_pp_c_libc,
            patches::patch_conf_gcc10,
            patches::patch_dynaloader_mac,
            patches::patch_eumm_darwin,
            patches::patch_skip_using_gcc_brace_groups,
            patches::patch_skip_using_gcc_bg_ppport,
        ],
    },
    Entry {
        specs: &[
            R(r"^5\.6\.[0-2]$"),
            R(r"^5\.7\.[0-3]$"),
            R(r"^5\.8\.[0-8]$"),
            R(r"^5\.9\.[0-4]$"),
        ],
        steps: &[patches::patch_makedepend_sh],
    },
    Entry {
        specs: &[R(r"^5\.1[0-2]")],
        steps: &[
            patches::patch_archive_tar_tests,
            patches::patch_odbm_file_hints_linux,
        ],
    },
    Entry {
        specs: &[R(r"^5.1([24].\d+|0.1)")],
        steps: &[patches::patch_make_ext_pl],
    },
    Entry {
        specs: &[R(r"^5\.8\.9$")],
        steps: &[patches::patch_589_perlio_c],
    },
    Entry {
        specs: &[R(r"^5\.8\.[89]$")],
        steps: &[patches::patch_hsplit_rehash_58],
    },
    Entry {
        specs: &[R(r"^5\.10\.1$"), R(r"^5\.12\.5$")],
        steps: &[patches::patch_hsplit_rehash_510],
    },
    Entry {
        specs: &[R(r"^5\.18\.0$")],
        steps: &[patches::patch_regmatch_pointer_5180],
    },
    Entry {
        specs: &[R(r"^5\.20\.0$")],
        steps: &[patches::patch_cow_speed],
    },
    Entry {
        specs: &[R(r"^5\.6\.[012]$"), R(r"^5\.8\.[89]$"), R(r"^5\.10\.[01]$")],
        steps: &[patches::patch_preprocess_options],
    },
    Entry {
        specs: &[R(r"^5\.18\.3$")],
        steps: &[patches::patch_5183_metajson],
    },
    Entry {
        specs: &[R(r"^5\.24\.[012]$")],
        steps: &[patches::patch_time_hires],
    },
    Entry {
        specs: &[
            R(r"^5\.24\.3$"),
            R(r"^5\.25\.(?:[4-9]|10)$"),
            R(r"^5\.26\.[01]$"),
            R(r"^5\.27\.[0-4]$"),
        ],
        steps: &[patches::patch_fp_class_denorm],
    },
    Entry {
        specs: &[R(r"^5\.28\.[01]$")],
        steps: &[patches::patch_useshrplib],
    },
    Entry {
        specs: &[R(r".*")],
        steps: &[patches::patch_dbfile_clang],
    },
];

/// Run every dispatch entry whose spec matches `cx.version`, in table order.
pub(crate) fn run(cx: &Ctx) -> Result<()> {
    for entry in DISPATCH {
        if entry.matches(cx.version) {
            for step in entry.steps {
                step(cx)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matching_count(version: &str) -> usize {
        DISPATCH.iter().filter(|e| e.matches(version)).count()
    }

    #[test]
    fn every_regex_compiles() {
        for entry in DISPATCH {
            for spec in entry.specs {
                if let Spec::Re(re) = spec {
                    regex::Regex::new(re).unwrap_or_else(|e| panic!("bad regex {re:?}: {e}"));
                }
            }
        }
    }

    #[test]
    fn catch_all_entries_always_match() {
        // The two `qr/.*/` entries match anything.
        assert!(matching_count("5.10.1") >= 2);
        assert!(matching_count("totally-bogus") >= 2);
    }

    #[test]
    fn known_versions_pick_expected_routines() {
        // 5.6.1: makefile_sh_phony, sysv_new, conf_gconvert+sort_n,
        // both catch-alls, makedepend_sh, preprocess_options.
        assert!(matching_count("5.6.1") >= 6);
        // A modern-ish version only hits the two catch-alls.
        assert_eq!(matching_count("5.30.0"), 2);
    }
}
