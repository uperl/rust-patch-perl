//! Filesystem helpers shared by the patch engine.

use std::path::Path;

/// Overwrite `path` with `bytes`, creating parent directories as needed.
///
/// Perl source tarballs (especially pre-5.8) ship files read-only (mode 0444).
/// GNU `patch` and `perl -i` both cope with that; so must we. If the existing
/// file is not writable we grant `u+w` for the write and restore the original
/// mode afterwards, mirroring the `chmod` dance in `Devel::PatchPerl::_patch`.
pub(crate) fn overwrite(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if mode & 0o200 == 0 {
                let mut perms = meta.permissions();
                perms.set_mode(mode | 0o200);
                let _ = std::fs::set_permissions(path, perms);
                let result = std::fs::write(path, bytes);
                if let Ok(m) = std::fs::metadata(path) {
                    let mut back = m.permissions();
                    back.set_mode(mode);
                    let _ = std::fs::set_permissions(path, back);
                }
                return result;
            }
        }
    }

    std::fs::write(path, bytes)
}
