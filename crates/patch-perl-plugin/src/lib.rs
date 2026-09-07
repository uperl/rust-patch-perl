//! SDK for writing [`patch-perl`] plugins in Rust.
//!
//! A plugin is a `cdylib` that exports the C ABI described in
//! `include/patch_perl_plugin.h`. Instead of writing the `extern "C"` shims by
//! hand, implement [`Plugin`] and invoke [`export_plugin!`]:
//!
//! ```no_run
//! use patch_perl_plugin::{export_plugin, Context, LogLevel, Plugin};
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn run(ctx: &Context) -> Result<(), String> {
//!         ctx.log(LogLevel::Info, &format!("patching perl {}", ctx.version()));
//!         if ctx.source().join("some-file").exists() {
//!             ctx.apply_patch("--- some-file\n+++ some-file\n@@ -1 +1 @@\n-a\n+b\n")?;
//!         }
//!         ctx.write_file("MY_PLUGIN_RAN", b"ok")?;
//!         Ok(())
//!     }
//! }
//!
//! export_plugin!(MyPlugin);
//! ```
//!
//! Build with `crate-type = ["cdylib"]`; the resulting `lib<name>.so` /
//! `.dylib` / `.bundle` / `<name>.dll` is what `PERL5_PATCHPERL_PLUGIN` points
//! at.
//!
//! [`patch-perl`]: https://docs.rs/patch-perl

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;

/// ABI version implemented by this SDK. Must equal the host's.
pub const ABI_VERSION: u32 = 1;

/// `PatchPerlHost` from the C header.
#[repr(C)]
pub struct RawHost {
    pub host_data: *mut c_void,
    pub apply_patch: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> i32>,
    pub write_file:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> i32>,
    pub log: Option<unsafe extern "C" fn(*mut c_void, i32, *const c_char)>,
}

/// `PatchPerlContext` from the C header.
#[repr(C)]
pub struct RawContext {
    pub abi_version: u32,
    pub version: *const c_char,
    pub source: *const c_char,
    pub patchexe: *const c_char,
    pub host: *const RawHost,
}

/// Severity passed to [`Context::log`] (`PatchPerlLogLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
}

/// A safe view over the [`RawContext`] the host passes to a plugin.
pub struct Context<'a> {
    raw: &'a RawContext,
}

impl<'a> Context<'a> {
    /// # Safety
    ///
    /// `raw` must be a valid `PatchPerlContext` from the host, with all string
    /// pointers NUL-terminated and its `host` vtable valid for the call.
    pub unsafe fn from_raw(raw: &'a RawContext) -> Self {
        Context { raw }
    }

    fn str_at(&self, p: *const c_char) -> &'a str {
        if p.is_null() {
            return "";
        }
        // SAFETY: contract of `from_raw`.
        unsafe { CStr::from_ptr(p) }.to_str().unwrap_or("")
    }

    /// The Perl version being patched, e.g. `"5.10.1"`.
    pub fn version(&self) -> &'a str {
        self.str_at(self.raw.version)
    }

    /// The absolute path to the Perl source tree (also the process cwd).
    pub fn source(&self) -> &'a Path {
        Path::new(self.str_at(self.raw.source))
    }

    /// Path to a usable `patch` utility, if the host found one.
    pub fn patchexe(&self) -> Option<&'a Path> {
        let s = self.str_at(self.raw.patchexe);
        (!s.is_empty()).then(|| Path::new(s))
    }

    fn host(&self) -> Result<&'a RawHost, String> {
        if self.raw.host.is_null() {
            return Err("host vtable is null".into());
        }
        // SAFETY: contract of `from_raw`.
        Ok(unsafe { &*self.raw.host })
    }

    /// Apply a unified diff (`-p0`) to the source tree via the host's engine.
    pub fn apply_patch(&self, diff: &str) -> Result<(), String> {
        let host = self.host()?;
        let f = host.apply_patch.ok_or("host provides no apply_patch")?;
        let c = CString::new(diff).map_err(|_| "diff contains a NUL byte".to_string())?;
        // SAFETY: `f` and `host_data` come from the host; `c` outlives the call.
        let rc = unsafe { f(host.host_data, c.as_ptr()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("apply_patch failed (rc = {rc})"))
        }
    }

    /// Write `bytes` to `rel` (relative to the source tree) via the host.
    pub fn write_file(&self, rel: &str, bytes: &[u8]) -> Result<(), String> {
        let host = self.host()?;
        let f = host.write_file.ok_or("host provides no write_file")?;
        let c = CString::new(rel).map_err(|_| "path contains a NUL byte".to_string())?;
        let ptr = if bytes.is_empty() {
            std::ptr::null()
        } else {
            bytes.as_ptr() as *const c_char
        };
        // SAFETY: pointers valid for the duration of the call.
        let rc = unsafe { f(host.host_data, c.as_ptr(), ptr, bytes.len()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("write_file({rel}) failed (rc = {rc})"))
        }
    }

    /// Emit a log line through the host's logger.
    pub fn log(&self, level: LogLevel, msg: &str) {
        let Ok(host) = self.host() else { return };
        let Some(f) = host.log else { return };
        let Ok(c) = CString::new(msg) else { return };
        // SAFETY: pointer valid for the call.
        unsafe { f(host.host_data, level as i32, c.as_ptr()) };
    }
}

/// Implement this for your plugin type, then pass it to [`export_plugin!`].
pub trait Plugin {
    /// Do the work. Return `Err(message)` to have the host log a warning
    /// (a plugin failure is never fatal to the patch run).
    fn run(ctx: &Context) -> Result<(), String>;
}

/// Generate the required `extern "C"` entry points for a [`Plugin`] type.
#[macro_export]
macro_rules! export_plugin {
    ($t:ty) => {
        /// C ABI: report the ABI version this plugin was built against.
        #[no_mangle]
        pub extern "C" fn patch_perl_plugin_abi_version() -> u32 {
            $crate::ABI_VERSION
        }

        /// C ABI: run the plugin.
        ///
        /// # Safety
        ///
        /// `ctx` must be a valid `PatchPerlContext *` and `error_out` either
        /// null or a valid `const char **`.
        #[no_mangle]
        pub unsafe extern "C" fn patch_perl_plugin_run(
            ctx: *const $crate::RawContext,
            error_out: *mut *const ::std::os::raw::c_char,
        ) -> ::std::os::raw::c_int {
            // SAFETY: forwarding the caller's own contract on `ctx`/`error_out`.
            unsafe { $crate::__run_impl::<$t>(ctx, error_out) }
        }
    };
}

/// Implementation detail of [`export_plugin!`]. Not part of the stable API.
///
/// # Safety
///
/// `ctx` must be null or a valid `PatchPerlContext *`; `error_out` must be null
/// or a valid `const char **`.
#[doc(hidden)]
pub unsafe fn __run_impl<T: Plugin>(
    ctx: *const RawContext,
    error_out: *mut *const c_char,
) -> c_int {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if ctx.is_null() {
            return 2;
        }
        // SAFETY: null checked; validity is the caller's contract.
        let raw = unsafe { &*ctx };
        if raw.abi_version != ABI_VERSION {
            return 2;
        }
        let cx = unsafe { Context::from_raw(raw) };
        match T::run(&cx) {
            Ok(()) => 0,
            Err(msg) => {
                if !error_out.is_null() {
                    if let Ok(c) = CString::new(msg) {
                        // Leaked on purpose: the host reads it right after we
                        // return and never frees it. One small leak per failed
                        // plugin run is acceptable.
                        unsafe { *error_out = c.into_raw() };
                    }
                }
                1
            }
        }
    }));
    outcome.unwrap_or(3)
}
