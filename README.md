# patch-perl

A Rust port of the classic Perl module
[`Devel::PatchPerl`](https://metacpan.org/pod/Devel::PatchPerl).

`Devel::PatchPerl` patches an unpacked Perl source tree so that an old version of
Perl still builds on a modern toolchain. It is what `perlbrew` and `Perl::Build`
use under the hood. This crate is a faithful port of the **library** (the
`patchperl` command-line tool is out of scope), verified byte-for-byte against
[`Devel::PatchPerl` 2.14](#upstream-version) for Perl 5.6.0 through 5.40.

Two things are done differently on purpose:

1. **Unified diffs are applied by a small pure-Rust engine**, not by shelling out
   to GNU `patch`. There is no external `patch` dependency.
2. **The `PERL5_PATCHPERL_PLUGIN` hook loads a native shared library** with a C
   ABI, so a plugin can be written in C, C++ or Rust instead of Perl.

## Library usage

```rust
use patch_perl::{patch_source, determine_version, PatchPerl};

// Patch an unpacked perl-5.10.1 tree in place.
patch_source(Some("5.10.1"), "/tmp/perl-5.10.1")?;

// Let the version be read from patchlevel.h.
patch_source(None, "/tmp/perl-5.10.1")?;
let v = determine_version("/tmp/perl-5.10.1"); // Some("5.10.1")

// Builder form, with plugin processing disabled.
PatchPerl::new()
    .version("5.8.9")
    .source("/tmp/perl-5.8.9")
    .run_plugins(false)
    .run()?;
# Ok::<(), patch_perl::Error>(())
```

Version gates match upstream:

| Perl version        | What runs                                  |
|---------------------|--------------------------------------------|
| `< 5.34` (CERTIFIED)| replacement `hints` + the full patch set   |
| `5.34` .. `< 5.42`  | replacement `hints` only                   |
| `>= 5.42` (HINTSCERT)| nothing                                    |

Plugins run at every version.

OS-specific patches (Solaris, Darwin, Linux, AIX, …) only run on the matching
OS, exactly as upstream. `PATCH_PERL_FAKE_OS` overrides the detected OS for
testing.

## Plugins

A plugin is a shared object — `lib<name>.so` (Linux), `lib<name>.dylib` /
`lib<name>.bundle` (macOS) or `<name>.dll` (Windows) — named by the
`PERL5_PATCHPERL_PLUGIN` environment variable (a bare name resolved on a search
path, or a path to the file). It is loaded after the tree has been patched and
called with the process current directory set to the source tree. As upstream,
one plugin is honoured, a load/ABI failure is fatal, and a failure *inside* the
plugin is only logged.

The C ABI lives in [`include/patch_perl_plugin.h`](include/patch_perl_plugin.h):

```c
uint32_t patch_perl_plugin_abi_version(void);            // must return 1
int32_t  patch_perl_plugin_run(const PatchPerlContext *ctx,
                               const char **error_out);
```

`PatchPerlContext` carries the Perl `version`, the absolute `source` path, a
best-effort `patchexe`, and a `host` vtable with `apply_patch`, `write_file` and
`log` callbacks.

### Writing a plugin in Rust

Use the [`patch-perl-plugin`](crates/patch-perl-plugin) SDK crate:

```rust
use patch_perl_plugin::{export_plugin, Context, LogLevel, Plugin};

struct MyPlugin;

impl Plugin for MyPlugin {
    fn run(ctx: &Context) -> Result<(), String> {
        ctx.log(LogLevel::Info, &format!("patching perl {}", ctx.version()));
        ctx.apply_patch("--- some/file\n+++ some/file\n@@ -1 +1 @@\n-a\n+b\n")?;
        ctx.write_file("MY_PLUGIN_RAN", b"ok")?;
        Ok(())
    }
}

export_plugin!(MyPlugin);
```

```toml
[lib]
crate-type = ["cdylib"]
```

A worked example is in [`crates/patch-perl-plugin-example`](crates/patch-perl-plugin-example).

### Writing a plugin in C

Include [`include/patch_perl_plugin.h`](include/patch_perl_plugin.h) and export
`patch_perl_plugin_abi_version` and `patch_perl_plugin_run`. A worked example
with a `Makefile` is in [`examples/c-plugin`](examples/c-plugin); it is compiled
and exercised by `crates/patch-perl/tests/c_plugin.rs` when a C compiler is
present.

### Plugin search path

For a bare name, these directories are searched (in order): each entry of
`PATCH_PERL_PLUGIN_PATH`, the current directory, and the directory of the
running executable (and its `deps/`). Filenames tried:
`libpatch_perl_plugin_<name>.<ext>`, `patch_perl_plugin_<name>.<ext>`,
`lib<name>.<ext>`, `<name>.<ext>`.

## Environment variables

| Variable                       | Effect                                                        |
|--------------------------------|--------------------------------------------------------------|
| `PERL5_PATCHPERL_PLUGIN`       | plugin to load (name or path)                               |
| `PATCH_PERL_PLUGIN_PATH`       | extra plugin search directories                            |
| `PERL5_PATCHPERL_PATCHLEVEL`   | force `patchlevel.h` rewrite even in a git checkout        |
| `PATCH_PERL_FAKE_OS`           | override the detected OS (testing)                         |
| `PATCH_PERL_VERSION`           | version token written by `_patch_develpatchperlversion`    |
| `PATCH_PERL_PATCHLEVEL_LABEL`  | full label written into `patchlevel.h`'s `local_patches[]` |

## Upstream version

This port is based on **`Devel::PatchPerl` 2.14**, released 2025-08-30 by Chris
Williams (BINGOS) — the current release at the time of the port:
<https://metacpan.org/release/BINGOS/Devel-PatchPerl-2.14>.

Taken from that release and reproduced here:

* the `@patch` dispatch table (every `_patch_*` routine, its version ranges, and
  the OS / header-file / `.git` gating), transcribed 1:1;
* the 13 replacement `hints/*.sh` files and every embedded unified diff, embedded
  verbatim under [`crates/patch-perl/assets/`](crates/patch-perl/assets);
* the certification gates `CERTIFIED` (`5.33.2`) and `HINTSCERT` (`5.41.12`);
* two behaviours specific to 2.14: the GCC brace-groups patching is "defanged"
  (not applied), and a `PERL5_PATCHPERL_PLUGIN` plugin runs regardless of
  certification.

Not reproduced: the `patchperl` CLI, and the Perl-module plugin loader (replaced
by the C-ABI loader described above).

To move to a newer upstream release, re-extract `assets/`, reconcile `@patch` and
the gate constants, and re-run the differential test below against a Perl that
has the new `Devel::PatchPerl` installed.

## Verifying against upstream

`tests/differential.rs` (run with `--ignored`) unpacks real Perl tarballs, patches
one copy with upstream `Devel::PatchPerl` and another with this crate, and asserts
the trees are byte-identical:

```sh
cargo test -p patch-perl --test differential -- --ignored --nocapture
```

It needs the network, `tar`, and a Perl with `Devel::PatchPerl` installed; it
skips (does not fail) when those are missing.

## License

Same terms as Perl 5 itself: `Artistic-1.0 OR GPL-1.0-or-later`. See
[`NOTICE`](NOTICE), [`LICENSE-ARTISTIC`](LICENSE-ARTISTIC) and
[`LICENSE-GPL`](LICENSE-GPL). The embedded patch payloads and `hints` files are
derived from `Devel::PatchPerl` and the Perl 5 distribution.
