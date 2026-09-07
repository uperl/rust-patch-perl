//! A minimal `patch-perl` plugin, built as a `cdylib`.
//!
//! It exercises every host callback so the integration tests can assert the ABI
//! round-trips:
//!
//! * logs an info line,
//! * writes a marker file `PATCHPERL_PLUGIN_RAN` containing the Perl version,
//! * if `plugin-target.txt` exists in the source tree, patches its single line
//!   from `before` to `after`,
//! * if a file named `PLUGIN_SHOULD_FAIL` exists in the source tree, returns an
//!   error (which the host must treat as a non-fatal warning).

use patch_perl_plugin::{export_plugin, Context, LogLevel, Plugin};

struct Example;

impl Plugin for Example {
    fn run(ctx: &Context) -> Result<(), String> {
        ctx.log(
            LogLevel::Info,
            &format!("example plugin: patching perl {}", ctx.version()),
        );

        ctx.write_file("PATCHPERL_PLUGIN_RAN", ctx.version().as_bytes())?;

        if ctx.source().join("plugin-target.txt").exists() {
            ctx.apply_patch(
                "--- plugin-target.txt\n\
                 +++ plugin-target.txt\n\
                 @@ -1 +1 @@\n\
                 -before\n\
                 +after\n",
            )?;
        }

        if ctx.source().join("PLUGIN_SHOULD_FAIL").exists() {
            return Err("PLUGIN_SHOULD_FAIL marker present".into());
        }

        Ok(())
    }
}

export_plugin!(Example);
