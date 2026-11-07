//! Detekcja GBM potrzebnego do zero-copy na NVIDIA.
//!
//! `nvidia-drm` odrzuca modyfikator bufora wyeksportowany z EGL (`EINVAL`), więc
//! zero-copy może powstać wyłącznie w alokatorze NVIDII — przez GBM. Potrzebne są:
//!
//! - `libgbm.so.1` (Mesa, część systemu),
//! - `/usr/lib/gbm/nvidia-drm_gbm.so` (backend alokatora NVIDIA),
//! - `libnvidia-egl-gbm.so.*` (platforma EGL dla buforów GBM).
//!
//! Ten moduł **tylko sprawdza dostępność** i nie zmienia ścieżki renderowania.
//! Właściwa alokacja `gbm_bo` → `EGL_LINUX_DMA_BUF_EXT` → `AddFB2WithModifiers`
//! jest osobnym etapem (zob. `docs/gpu.md`, ETAP 4.3).

use std::path::{Path, PathBuf};

/// Katalogi, w których dystrybucje kładą biblioteki.
const LIB_DIRS: &[&str] = &[
    "/usr/lib",
    "/usr/lib64",
    "/usr/local/lib",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
];

/// Pierwszy istniejący plik `{dir}/{name}` lub `{dir}/{name}.*` (soname).
fn find_lib(dir: &str, name: &str) -> Option<PathBuf> {
    let direct = Path::new(dir).join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let prefix = format!("{name}.");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(&prefix))
                && p.is_file()
        })
}

/// Ścieżka do `libgbm.so.1` (soname lub wersjonowana).
pub fn libgbm_path() -> Option<PathBuf> {
    LIB_DIRS.iter().find_map(|d| find_lib(d, "libgbm.so"))
}

/// Ścieżka do `libnvidia-egl-gbm.so.*`.
pub fn egl_platform_path() -> Option<PathBuf> {
    LIB_DIRS
        .iter()
        .find_map(|d| find_lib(d, "libnvidia-egl-gbm.so"))
}

/// Ścieżka do backendu GBM alokatora NVIDII (`gbm/nvidia-drm_gbm.so`).
pub fn backend_path() -> Option<PathBuf> {
    LIB_DIRS.iter().find_map(|d| {
        let p = Path::new(d).join("gbm").join("nvidia-drm_gbm.so");
        p.is_file().then_some(p)
    })
}

/// Czy wszystkie trzy elementy są dostępne?
pub fn available() -> bool {
    libgbm_path().is_some() && egl_platform_path().is_some() && backend_path().is_some()
}

/// Krótki opis do logów — mówi **czego** brakuje, zamiast tylko „nie”.
pub fn describe() -> String {
    let gbm = libgbm_path();
    let egl = egl_platform_path();
    let backend = backend_path();
    if gbm.is_some() && egl.is_some() && backend.is_some() {
        return format!(
            "tak ({} + {} + {})",
            gbm.as_ref().map_or("-", |p| p.to_str().unwrap_or("?")),
            egl.as_ref().map_or("-", |p| p.to_str().unwrap_or("?")),
            backend.as_ref().map_or("-", |p| p.to_str().unwrap_or("?")),
        );
    }
    let mut missing = Vec::new();
    if gbm.is_none() {
        missing.push("libgbm.so.1");
    }
    if egl.is_none() {
        missing.push("libnvidia-egl-gbm.so");
    }
    if backend.is_none() {
        missing.push("gbm/nvidia-drm_gbm.so");
    }
    format!("nie (brak: {})", missing.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detekcja nie może panikować ani wymagać GPU (na CI nic nie ma).
    #[test]
    fn describe_is_total() {
        let s = describe();
        assert!(!s.is_empty());
        assert_eq!(
            available(),
            libgbm_path().is_some() && egl_platform_path().is_some() && backend_path().is_some()
        );
    }
}
