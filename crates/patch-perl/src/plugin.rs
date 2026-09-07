//! Native shared-object plugin support.
//!
//! `Devel::PatchPerl` loads a Perl module named by the `PERL5_PATCHPERL_PLUGIN`
//! environment variable and calls its `patchperl` class method once the source
//! tree has been patched. This port keeps the same hook and the same
//! environment variable, but a plugin is a **native shared library** exposing a
//! small C ABI, so it can be written in C, C++ or Rust.
//!
//! The ABI is defined in `include/patch_perl_plugin.h`; the structs below are
//! its `#[repr(C)]` mirror. A plugin must export:
//!
//! ```c
//! uint32_t patch_perl_plugin_abi_version(void);
//! int32_t  patch_perl_plugin_run(const PatchPerlContext *ctx, const char **error_out);
//! ```
//!
//! As upstream, only a single plugin is honoured, a load/ABI failure is fatal
//! (mirroring Perl's `die` on a failed `require`), and a failure *inside* the
//! plugin is only logged.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};

use crate::{Error, Result};

/// ABI version implemented by this crate. A plugin's
/// `patch_perl_plugin_abi_version()` must return this exact value.
pub const ABI_VERSION: u32 = 1;

/// Log levels passed to the host `log` callback (`PatchPerlLogLevel`).
pub const LOG_ERROR: i32 = 0;
pub const LOG_WARN: i32 = 1;
pub const LOG_INFO: i32 = 2;

/// Callbacks the host lends to the plugin for the duration of one call.
#[repr(C)]
pub struct PatchPerlHost {
    pub host_data: *mut c_void,
    pub apply_patch: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> i32>,
    pub write_file:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> i32>,
    pub log: Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char)>,
}

/// The context handed to `patch_perl_plugin_run` (`PatchPerlContext`).
#[repr(C)]
pub struct PatchPerlContext {
    pub abi_version: u32,
    pub version: *const c_char,
    pub source: *const c_char,
    pub patchexe: *const c_char,
    pub host: *const PatchPerlHost,
}

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type RunFn = unsafe extern "C" fn(*const PatchPerlContext, *mut *const c_char) -> c_int;

/// State pointed at by `PatchPerlHost::host_data`.
struct HostState {
    root: PathBuf,
}

unsafe extern "C" fn cb_apply_patch(host_data: *mut c_void, diff: *const c_char) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if host_data.is_null() || diff.is_null() {
            return -1;
        }
        let state = &*(host_data as *const HostState);
        let Ok(text) = CStr::from_ptr(diff).to_str() else {
            return -1;
        };
        match crate::diff::apply_in(&state.root, text) {
            Ok(()) => 0,
            Err(e) => {
                log::warn!("plugin apply_patch failed: {e}");
                1
            }
        }
    });
    result.unwrap_or(-1)
}

unsafe extern "C" fn cb_write_file(
    host_data: *mut c_void,
    path: *const c_char,
    bytes: *const c_char,
    len: usize,
) -> i32 {
    let result = std::panic::catch_unwind(|| {
        if host_data.is_null() || path.is_null() || (bytes.is_null() && len != 0) {
            return -1;
        }
        let state = &*(host_data as *const HostState);
        let Ok(rel) = CStr::from_ptr(path).to_str() else {
            return -1;
        };
        let data = if len == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(bytes as *const u8, len)
        };
        let target = state.root.join(rel);
        if let Some(parent) = target.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return -1;
            }
        }
        match std::fs::write(&target, data) {
            Ok(()) => 0,
            Err(e) => {
                log::warn!("plugin write_file({rel}) failed: {e}");
                -1
            }
        }
    });
    result.unwrap_or(-1)
}

unsafe extern "C" fn cb_log(_host_data: *mut c_void, level: i32, msg: *const c_char) {
    let _ = std::panic::catch_unwind(|| {
        if msg.is_null() {
            return;
        }
        let text = CStr::from_ptr(msg).to_string_lossy();
        match level {
            LOG_ERROR => log::error!("[plugin] {text}"),
            LOG_INFO => log::info!("[plugin] {text}"),
            _ => log::warn!("[plugin] {text}"),
        }
    });
}

/// Library-file extensions worth trying, most-specific first.
#[cfg(target_os = "macos")]
const LIB_EXTS: &[&str] = &["bundle", "dylib", "so"];
#[cfg(all(unix, not(target_os = "macos")))]
const LIB_EXTS: &[&str] = &["so"];
#[cfg(windows)]
const LIB_EXTS: &[&str] = &["dll"];

fn looks_like_path(spec: &str) -> bool {
    spec.contains('/')
        || spec.contains('\\')
        || LIB_EXTS.iter().any(|e| spec.ends_with(&format!(".{e}")))
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(raw) = std::env::var_os("PATCH_PERL_PLUGIN_PATH") {
        dirs.extend(std::env::split_paths(&raw));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
            // Cargo puts examples/cdylibs in target/<profile>/ alongside deps/.
            dirs.push(parent.join("deps"));
        }
    }
    dirs
}

/// Resolve a `PERL5_PATCHPERL_PLUGIN` value to a shared-library path.
fn resolve_plugin(spec: &str) -> Option<PathBuf> {
    if looks_like_path(spec) {
        let p = PathBuf::from(spec);
        return p.exists().then_some(p);
    }

    let mut candidates: Vec<String> = Vec::new();
    for ext in LIB_EXTS {
        candidates.push(format!("libpatch_perl_plugin_{spec}.{ext}"));
        candidates.push(format!("patch_perl_plugin_{spec}.{ext}"));
        candidates.push(format!("lib{spec}.{ext}"));
        candidates.push(format!("{spec}.{ext}"));
    }

    for dir in search_dirs() {
        for name in &candidates {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        // Last resort: any file whose stem ends with the requested name,
        // mirroring upstream's `/\Q$possible\E$/` suffix match.
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname = fname.to_string_lossy();
                let is_lib = LIB_EXTS.iter().any(|e| fname.ends_with(&format!(".{e}")));
                if is_lib && stem_ends_with(&fname, spec) {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

fn stem_ends_with(fname: &str, spec: &str) -> bool {
    let stem = fname.split('.').next().unwrap_or(fname);
    let stem = stem.strip_prefix("lib").unwrap_or(stem);
    stem == spec || stem.ends_with(&format!("_{spec}")) || stem.ends_with(spec)
}

fn find_patch_exe() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for exe in ["gpatch", "patch"] {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// Port of `Devel::PatchPerl::_process_plugin`.
///
/// `plugin_override` takes precedence over `$PERL5_PATCHPERL_PLUGIN`. `source`
/// must be absolute; the plugin is called with the process current directory
/// set to it.
pub(crate) fn process_plugin(
    plugin_override: Option<&str>,
    version: &str,
    source: &Path,
) -> Result<()> {
    let spec = match plugin_override
        .map(str::to_string)
        .or_else(|| std::env::var("PERL5_PATCHPERL_PLUGIN").ok())
    {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(()),
    };

    let Some(lib_path) = resolve_plugin(&spec) else {
        log::warn!(
            "You specified a plugin '{spec}' that isn't installed, \
             just thought you might be interested."
        );
        return Ok(());
    };

    // SAFETY: loading an arbitrary shared library is inherently unsafe; the user
    // asked for this specific one via the environment.
    let lib = unsafe { Library::new(&lib_path) }
        .map_err(|e| Error::Plugin(format!("could not load '{}': {e}", lib_path.display())))?;

    unsafe {
        let abi: Symbol<AbiVersionFn> = lib
            .get(b"patch_perl_plugin_abi_version\0")
            .map_err(|e| Error::Plugin(format!("missing patch_perl_plugin_abi_version: {e}")))?;
        let got = abi();
        if got != ABI_VERSION {
            return Err(Error::Plugin(format!(
                "plugin '{}' reports ABI version {got}, this build speaks {ABI_VERSION}",
                lib_path.display()
            )));
        }

        let run: Symbol<RunFn> = lib
            .get(b"patch_perl_plugin_run\0")
            .map_err(|e| Error::Plugin(format!("missing patch_perl_plugin_run: {e}")))?;

        let c_version = CString::new(version).unwrap();
        let c_source = CString::new(source.to_string_lossy().as_bytes()).unwrap();
        let c_patchexe = find_patch_exe().and_then(|s| CString::new(s).ok());

        let mut state = HostState {
            root: source.to_path_buf(),
        };
        let host = PatchPerlHost {
            host_data: &mut state as *mut _ as *mut c_void,
            apply_patch: Some(cb_apply_patch),
            write_file: Some(cb_write_file),
            log: Some(cb_log),
        };
        let ctx = PatchPerlContext {
            abi_version: ABI_VERSION,
            version: c_version.as_ptr(),
            source: c_source.as_ptr(),
            patchexe: c_patchexe.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            host: &host,
        };

        let saved_cwd = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(source);

        let mut err_ptr: *const c_char = std::ptr::null();
        let rc = run(&ctx, &mut err_ptr);

        if let Some(cwd) = saved_cwd {
            let _ = std::env::set_current_dir(cwd);
        }

        if rc != 0 {
            let detail = if err_ptr.is_null() {
                format!("plugin returned {rc}")
            } else {
                CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
            };
            log::warn!("Warnings from the plugin: '{detail}'");
        }
    }

    drop(lib);
    Ok(())
}
