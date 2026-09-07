/*
 * patch_perl_plugin.h - C ABI for patch-perl plugins.
 *
 * A plugin is a native shared library (`.so` / `.dylib` / `.bundle` / `.dll`)
 * named by the PERL5_PATCHPERL_PLUGIN environment variable.  After patch-perl
 * has finished patching a Perl source tree it loads the plugin and calls
 * `patch_perl_plugin_run` with the process current directory set to the source
 * tree.  A plugin may be written in C, C++ or Rust.
 *
 * This mirrors the behaviour of Devel::PatchPerl's Perl plugin hook, but the
 * plugin is compiled code rather than a Perl module.
 *
 * License: Artistic-1.0 OR GPL-1.0-or-later (same terms as Perl itself).
 */

#ifndef PATCH_PERL_PLUGIN_H
#define PATCH_PERL_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Bump when the layout of the structs below, or the entry-point contract,
 * changes incompatibly.  `patch_perl_plugin_abi_version()` must return this. */
#define PATCH_PERL_PLUGIN_ABI_VERSION 1u

typedef enum {
    PATCH_PERL_LOG_ERROR = 0,
    PATCH_PERL_LOG_WARN  = 1,
    PATCH_PERL_LOG_INFO  = 2
} PatchPerlLogLevel;

/*
 * Callbacks lent to the plugin by the host.  Every pointer here is valid only
 * for the duration of the `patch_perl_plugin_run` call.  `host_data` must be
 * passed back as the first argument of each callback.
 */
typedef struct PatchPerlHost {
    void *host_data;

    /* Apply a unified diff (GNU `patch -p0` semantics) to the source tree.
     * Returns 0 on success, non-zero on failure. */
    int32_t (*apply_patch)(void *host_data, const char *unified_diff);

    /* Create or truncate `path` (relative to the source tree) and write `len`
     * bytes from `bytes`.  Returns 0 on success, non-zero on failure. */
    int32_t (*write_file)(void *host_data, const char *path,
                          const char *bytes, size_t len);

    /* Emit a diagnostic through the host's logger.  `level` is a
     * PatchPerlLogLevel. */
    void (*log)(void *host_data, int32_t level, const char *message);
} PatchPerlHost;

/*
 * The context passed to `patch_perl_plugin_run`.  Borrowed for the call only;
 * copy anything you need to keep.
 */
typedef struct PatchPerlContext {
    uint32_t abi_version;      /* == PATCH_PERL_PLUGIN_ABI_VERSION */
    const char *version;       /* Perl version string, e.g. "5.10.1" */
    const char *source;        /* absolute path to the Perl source tree (also cwd) */
    const char *patchexe;      /* path to a `patch` utility, or NULL if none found */
    const PatchPerlHost *host; /* callback vtable */
} PatchPerlContext;

/* ------------------------------------------------------------------------- *
 * Exports every plugin must provide.
 * ------------------------------------------------------------------------- */

/* Return PATCH_PERL_PLUGIN_ABI_VERSION.  The host refuses to run the plugin if
 * this does not match its own. */
uint32_t patch_perl_plugin_abi_version(void);

/*
 * Run the plugin.  Return 0 on success.
 *
 * On failure, return non-zero and optionally set `*error_out` to a
 * NUL-terminated UTF-8 string describing the problem.  The string must remain
 * valid after the call returns (use static storage or a deliberate leak); the
 * host reads it and never frees it.  A non-zero return is logged by the host as
 * a warning and does not abort the overall patch run.
 */
int32_t patch_perl_plugin_run(const PatchPerlContext *ctx, const char **error_out);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* PATCH_PERL_PLUGIN_H */
