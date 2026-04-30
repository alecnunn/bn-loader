use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Recursively copy `src` to `dst`. If `dst` already exists, it is removed first.
pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst).context("Failed to remove existing directory")?;
    }

    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create directory {}", dst.display()))?;

    for entry in fs::read_dir(src)
        .with_context(|| format!("Failed to read directory {}", src.display()))?
    {
        let entry = entry.context("Failed to read entry")?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).context("Failed to copy file")?;
        }
    }

    Ok(())
}
