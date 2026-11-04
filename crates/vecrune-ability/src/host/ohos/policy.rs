//! Pure host policies, also tested on the desktop without an Ability or device.

use std::path::{Path, PathBuf};

/// Atomic replacement must stay in a directory the user authorized, including after symlink
/// resolution. A lexical prefix alone would also accept `allowed/../elsewhere`.
pub fn can_save(path: &Path, writable: &[PathBuf]) -> bool {
    let target = match std::fs::canonicalize(path) {
        Ok(target) => target,
        Err(_) if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) => return false,
        Err(_) => path.to_path_buf(),
    };
    let Some(parent) = target.parent().and_then(|parent| std::fs::canonicalize(parent).ok()) else { return false };
    writable.iter().any(|dir| std::fs::canonicalize(dir).is_ok_and(|dir| parent.starts_with(dir)))
}

/// Read a tightly packed RGBA capture at a normalized image position. The last row and column
/// include the image's right/bottom edge; malformed buffers and coordinates return an error.
pub fn sample(width: u32, height: u32, rgba: &[u8], position: [f32; 2]) -> Result<[f32; 3], String> {
    let count = u64::from(width).checked_mul(u64::from(height)).filter(|count| *count > 0 && *count <= 64 * 1024 * 1024);
    if count.and_then(|count| count.checked_mul(4)) != Some(rgba.len() as u64) || !position.iter().all(|v| v.is_finite()) {
        return Err("Invalid screen capture pixels or coordinates".into());
    }
    let x = (position[0].clamp(0.0, 1.0) * width as f32) as usize;
    let y = (position[1].clamp(0.0, 1.0) * height as f32) as usize;
    let width = width as usize;
    let offset = y.min(height as usize - 1).checked_mul(width).and_then(|n| n.checked_add(x.min(width - 1))).and_then(|n| n.checked_mul(4));
    let pixel = offset.and_then(|offset| rgba.get(offset..offset.checked_add(4)?)).ok_or("Screen capture pixel is missing")?;
    let mut rgb = [0.0; 3];
    for (value, byte) in rgb.iter_mut().zip(pixel.iter()) {
        *value = f32::from(*byte) / 255.0;
    }
    Ok(rgb)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_edges_and_scaled_coordinates_select_the_expected_pixel() {
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        assert_eq!(sample(2, 2, &rgba, [0.0, 0.0]).unwrap(), [1.0, 0.0, 0.0]);
        assert_eq!(sample(2, 2, &rgba, [0.75, 0.25]).unwrap(), [0.0, 1.0, 0.0]);
        assert_eq!(sample(2, 2, &rgba, [0.25, 0.75]).unwrap(), [0.0, 0.0, 1.0]);
        assert_eq!(sample(2, 2, &rgba, [1.0, 1.0]).unwrap(), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn malformed_or_oversized_snapshots_and_nonfinite_coordinates_are_rejected() {
        assert!(sample(0, 0, &[], [0.0, 0.0]).is_err());
        assert!(sample(u32::MAX, u32::MAX, &[], [0.0, 0.0]).is_err());
        assert!(sample(1, 1, &[1, 2, 3], [0.0, 0.0]).is_err());
        assert!(sample(1, 1, &[1, 2, 3, 4], [f32::NAN, 0.0]).is_err());
        assert!(sample(1, 1, &[1, 2, 3, 4], [0.0, f32::INFINITY]).is_err());
    }

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "vectorcraft-ohos-policy-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            // Best effort: only this test's temporary files are removed.
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn folder_grants_accept_children_and_reject_parent_traversal_and_sibling_prefixes() {
        let temp = Temp::new();
        let allowed = temp.0.join("art");
        let elsewhere = temp.0.join("art-other");
        std::fs::create_dir_all(allowed.join("exports")).unwrap();
        std::fs::create_dir(&elsewhere).unwrap();
        let roots = [allowed.clone()];
        assert!(can_save(&allowed.join("drawing.vectorcraft"), &roots));
        assert!(can_save(&allowed.join("exports/drawing.svg"), &roots));
        assert!(!can_save(&elsewhere.join("drawing.vectorcraft"), &roots));
        assert!(!can_save(&allowed.join("../art-other/drawing.svg"), &roots));
        assert!(!can_save(&temp.0.join("drawing.svg"), &roots));
        assert!(!can_save(&allowed.join("drawing.svg"), &[]));
    }

    #[test]
    #[cfg(unix)]
    fn folder_grants_do_not_follow_links_outside_the_authorized_folder() {
        let temp = Temp::new();
        let allowed = temp.0.join("art");
        let elsewhere = temp.0.join("other");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("original.svg"), b"original").unwrap();
        std::os::unix::fs::symlink(&elsewhere, allowed.join("exports")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("original.svg"), allowed.join("linked.svg")).unwrap();
        std::os::unix::fs::symlink(elsewhere.join("missing.svg"), allowed.join("broken.svg")).unwrap();
        let roots = [allowed.clone()];
        assert!(!can_save(&allowed.join("exports/drawing.svg"), &roots));
        assert!(!can_save(&allowed.join("linked.svg"), &roots));
        assert!(!can_save(&allowed.join("broken.svg"), &roots));
        assert_eq!(std::fs::read(elsewhere.join("original.svg")).unwrap(), b"original");
    }
}
