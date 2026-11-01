//! Where exported videos go and what they are called.

use std::path::{Path, PathBuf};

const MAX_NAME_CHARS: usize = 150;

pub fn folder() -> PathBuf {
    dirs::video_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_default()
        .join("Rigelcam")
}

fn clock(seconds: u32) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    format!("{hours:02}-{minutes:02}-{seconds:02}")
}

/// A file name without its extension. Camera names come from the recorder, so they can't choose
/// the folder or hide the file.
pub fn file_stem(camera: &str, date: &str, start: u32, end: u32) -> String {
    let safe: String = camera
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let safe: String = safe
        .trim_start_matches('.')
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    format!("{safe} {date} {} {}", clock(start), clock(end))
}

/// Removes videos left unfinished by a process that was killed, which only its `Drop` would clean
/// up. Call it while holding the instance lock, since a running export looks the same.
pub fn remove_unfinished(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name.ends_with(".part") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The first `.mp4` name in a folder that doesn't overwrite anything.
pub fn free_path(folder: &Path, stem: &str) -> PathBuf {
    let path = folder.join(format!("{stem}.mp4"));
    if !path.exists() {
        return path;
    }
    (2u32..)
        .map(|copy| folder.join(format!("{stem} ({copy}).mp4")))
        .find(|path| !path.exists())
        .expect("a folder can't hold every number")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_unfinished_videos() {
        let folder =
            std::env::temp_dir().join(format!("rigelcam-unfinished-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = |name: &str| folder.join(name);
        for name in [
            ".Patio 2026-10-31.123.part",
            "Patio 2026-10-31.mp4",
            "notes.part",
        ] {
            std::fs::write(path(name), b"x").unwrap();
        }
        remove_unfinished(&folder);
        assert!(!path(".Patio 2026-10-31.123.part").exists());
        assert!(path("Patio 2026-10-31.mp4").exists());
        assert!(path("notes.part").exists());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn names_a_file_with_the_camera_and_the_stretch() {
        assert_eq!(
            file_stem("CD Patio1", "2026-10-31", 3600, 3725),
            "CD Patio1 2026-10-31 01-00-00 01-02-05"
        );
    }

    #[test]
    fn a_camera_name_stays_inside_the_folder_and_visible() {
        let stem = file_stem("../../.evil:a/b\n", "2026-10-31", 0, 1);
        assert_eq!(stem, "_.._.evil_a_b_ 2026-10-31 00-00-00 00-00-01");
        assert_eq!(Path::new(&stem).components().count(), 1);
        assert!(!file_stem("...hidden", "2026-10-31", 0, 1).starts_with('.'));
    }

    #[test]
    fn numbers_a_name_already_taken() {
        let folder = std::env::temp_dir().join(format!("rigelcam-clips-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(free_path(&folder, "a"), folder.join("a.mp4"));
        std::fs::write(folder.join("a.mp4"), b"").unwrap();
        std::fs::write(folder.join("a (2).mp4"), b"").unwrap();
        assert_eq!(free_path(&folder, "a"), folder.join("a (3).mp4"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
