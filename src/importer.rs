use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::config::Config;
use crate::scanner::Photoset;

const JPEG_EXTENSIONS: &[&str] = &["jpg", "jpeg"];

fn subfolder_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase());
    match ext.as_deref() {
        Some(e) if JPEG_EXTENSIONS.contains(&e) => "jpeg",
        _ => "raw",
    }
}

pub struct ImportResult {
    pub copied: usize,
    pub skipped: usize,
    pub dest_dir: PathBuf,
}

/// Copies all files in a photoset to the configured destination directory.
/// RAW files go into a `raw/` subfolder; JPEGs go into a `jpeg/` subfolder.
/// Files that already exist with matching sizes are skipped.
pub fn import_photoset(photoset: &Photoset, config: &Config) -> Result<ImportResult> {
    let dest_dir = config.photoset_path(&photoset.date);

    let total = photoset.files.len();
    let mut copied = 0usize;
    let mut skipped = 0usize;

    for (i, src) in photoset.files.iter().enumerate() {
        let filename = src
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("file has no name: {}", src.display()))?;
        let sub = dest_dir.join(subfolder_for(src));
        std::fs::create_dir_all(&sub)?;
        let dest = sub.join(filename);

        // Skip if already present with matching size
        if dest.exists() {
            let src_size = std::fs::metadata(src)?.len();
            let dest_size = std::fs::metadata(&dest)?.len();
            if src_size == dest_size {
                skipped += 1;
                continue;
            }
        }

        print!(
            "  [{}/{}] {} ...",
            i + 1,
            total,
            filename.to_string_lossy()
        );
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::fs::copy(src, &dest)?;
        println!(" done");
        copied += 1;
    }

    Ok(ImportResult {
        copied,
        skipped,
        dest_dir,
    })
}

/// Verifies every source file is present at the destination with a matching size.
pub fn verify_import(photoset: &Photoset, config: &Config) -> Result<Vec<PathBuf>> {
    let dest_dir = config.photoset_path(&photoset.date);
    let mut missing = vec![];

    for src in &photoset.files {
        let filename = match src.file_name() {
            Some(n) => n,
            None => continue,
        };
        let dest = dest_dir.join(subfolder_for(src)).join(filename);

        let ok = dest.exists() && {
            let src_size = std::fs::metadata(src)?.len();
            let dest_size = std::fs::metadata(&dest)?.len();
            src_size == dest_size
        };

        if !ok {
            missing.push(src.clone());
        }
    }

    Ok(missing)
}

/// Deletes all source files in the photoset from the card.
pub fn delete_from_card(photoset: &Photoset) -> Result<usize> {
    let mut deleted = 0usize;
    for src in &photoset.files {
        std::fs::remove_file(src)?;
        deleted += 1;
    }
    Ok(deleted)
}
