/*
 * Example patch-perl plugin written in C.
 *
 * Mirrors crates/patch-perl-plugin-example (the Rust one) so it can be driven
 * by the same test:
 *
 *   - logs an info line,
 *   - writes a marker file PATCHPERL_PLUGIN_RAN containing the Perl version,
 *   - if plugin-target.txt exists in the source tree, patches its single line
 *     from "before" to "after",
 *   - if a file named PLUGIN_SHOULD_FAIL exists, returns an error (which the
 *     host must treat as a non-fatal warning).
 *
 * Build (see Makefile):
 *   cc -O2 -Wall -fPIC -I../../include -shared -o libpatch_perl_plugin_cexample.so plugin.c
 *
 * License: Artistic-1.0 OR GPL-1.0-or-later (same terms as Perl).
 */

#include "patch_perl_plugin.h"

#include <stdio.h>
#include <string.h>
#include <sys/stat.h>

static char g_error[256];

static int file_exists(const char *path)
{
    struct stat st;
    return stat(path, &st) == 0;
}

uint32_t patch_perl_plugin_abi_version(void)
{
    return PATCH_PERL_PLUGIN_ABI_VERSION;
}

int32_t patch_perl_plugin_run(const PatchPerlContext *ctx, const char **error_out)
{
    if (ctx == NULL || ctx->abi_version != PATCH_PERL_PLUGIN_ABI_VERSION) {
        if (error_out) *error_out = "unexpected ABI version";
        return 2;
    }

    const PatchPerlHost *host = ctx->host;
    const char *version = ctx->version ? ctx->version : "";

    if (host && host->log) {
        char msg[192];
        snprintf(msg, sizeof msg, "c example plugin: patching perl %s", version);
        host->log(host->host_data, PATCH_PERL_LOG_INFO, msg);
    }

    /* write_file callback: a marker relative to the source tree */
    if (host && host->write_file) {
        if (host->write_file(host->host_data, "PATCHPERL_PLUGIN_RAN",
                             version, strlen(version)) != 0) {
            if (error_out) {
                snprintf(g_error, sizeof g_error, "write_file(PATCHPERL_PLUGIN_RAN) failed");
                *error_out = g_error;
            }
            return 1;
        }
    }

    /* apply_patch callback: patch a file if the host set one up. The plugin is
     * called with the current directory set to the source tree. */
    if (host && host->apply_patch && file_exists("plugin-target.txt")) {
        static const char diff[] =
            "--- plugin-target.txt\n"
            "+++ plugin-target.txt\n"
            "@@ -1 +1 @@\n"
            "-before\n"
            "+after\n";
        if (host->apply_patch(host->host_data, diff) != 0) {
            if (error_out) {
                snprintf(g_error, sizeof g_error, "apply_patch(plugin-target.txt) failed");
                *error_out = g_error;
            }
            return 1;
        }
    }

    if (file_exists("PLUGIN_SHOULD_FAIL")) {
        if (error_out) *error_out = "PLUGIN_SHOULD_FAIL marker present";
        return 1;
    }

    return 0;
}
