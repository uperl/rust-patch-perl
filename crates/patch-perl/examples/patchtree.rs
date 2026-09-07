//! Patch a Perl source tree, the way the `patchperl` CLI would.
//!
//! ```text
//! cargo run --example patchtree -- <version> <path/to/perl-source>
//! cargo run --example patchtree -- <path/to/perl-source>   # auto-detect version
//! ```
//!
//! This is a convenience for manual testing, not a supported CLI.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger_lite();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (version, source) = match args.as_slice() {
        [source] => (None, source.clone()),
        [version, source] => (Some(version.clone()), source.clone()),
        _ => {
            eprintln!("usage: patchtree [<version>] <path/to/perl-source>");
            std::process::exit(2);
        }
    };

    patch_perl::patch_source(version.as_deref(), &source)?;
    eprintln!("patched {source}");
    Ok(())
}

/// Tiny stderr logger so `log::warn!` output is visible without a dependency.
fn env_logger_lite() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }
    static LOGGER: L = L;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}
