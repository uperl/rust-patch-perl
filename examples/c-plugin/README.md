# C example plugin

A `patch-perl` plugin written in plain C against
[`include/patch_perl_plugin.h`](../../include/patch_perl_plugin.h). It behaves
exactly like the Rust [`patch-perl-plugin-example`](../../crates/patch-perl-plugin-example):
it logs, writes a `PATCHPERL_PLUGIN_RAN` marker, patches `plugin-target.txt` if
present, and fails on a `PLUGIN_SHOULD_FAIL` marker.

## Build

```sh
make
# -> libpatch_perl_plugin_cexample.so   (Linux)
#    libpatch_perl_plugin_cexample.bundle (macOS)
```

Windows (MSVC):

```bat
cl /O2 /LD /I ..\..\include plugin.c /Fe:patch_perl_plugin_cexample.dll
```

## Use

```sh
export PERL5_PATCHPERL_PLUGIN="$PWD/libpatch_perl_plugin_cexample.so"
# or, by name, if it is on the search path:
export PATCH_PERL_PLUGIN_PATH="$PWD"
export PERL5_PATCHPERL_PLUGIN=cexample
```

Then run a patch, e.g. with the bundled example runner:

```sh
cargo run --example patchtree -- 5.10.1 /path/to/perl-5.10.1
```

`crates/patch-perl/tests/c_plugin.rs` compiles and exercises this file
automatically when a C compiler is available (skipped otherwise).
