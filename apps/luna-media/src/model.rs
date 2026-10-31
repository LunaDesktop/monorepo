//! Model biblioteki multimediów: wykrywanie formatów, skanowanie katalogu,
//! sortowanie naturalne i formatowanie wartości.
//!
//! Warstwa niezależna od UI — testy nie potrzebują okna ani Waylanda.

use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use libluna_i18n::tr;
use libluna_widgets::IconName;

/// Rodzaj pliku obsługiwany przez przeglądarkę.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    /// Dokument PDF (strony renderowane przez poppler).
    Pdf,
}

impl MediaKind {
    pub fn label(self) -> String {
        tr(match self {
            MediaKind::Image => "Image",
            MediaKind::Video => "Video",
            MediaKind::Pdf => "PDF document",
        })
    }

    pub fn icon(self) -> IconName {
        match self {
            MediaKind::Image => IconName::Image,
            MediaKind::Video => IconName::Video,
            MediaKind::Pdf => IconName::Document,
        }
    }
}

/// Rozszerzenia obrazów dekodowanych natywnie albo przez ffmpeg.
pub const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "avif", "svg", "ico", "jxl", "heic",
    "heif",
];

/// Aparatowe pliki RAW — pokazujemy osadzony podgląd JPEG (albo `dcraw_emu`).
pub const RAW_EXTS: &[&str] = &[
    "cr2", "cr3", "crw", "nef", "nrw", "arw", "sr2", "srf", "dng", "orf", "rw2", "rwl", "raf",
    "pef", "srw", "x3f", "3fr", "erf", "mrw", "dcr", "kdc", "mef", "mos", "iiq",
];

/// Rozszerzenia filmów (dekodowanie wyłącznie przez ffmpeg).
pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "m4v", "mkv", "webm", "avi", "mov", "ogv", "flv", "wmv", "mpg", "mpeg", "ts", "m2ts",
    "3gp",
];

/// Czy rozszerzenie jest obsługiwanym obrazem (bez względu na wielkość liter).
pub fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
        || RAW_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
}

/// Czy rozszerzenie to aparatowy RAW.
pub fn is_raw_ext(ext: &str) -> bool {
    RAW_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
}

/// Czy ścieżka jest plikiem RAW aparatu.
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(is_raw_ext)
}

/// Czy rozszerzenie jest obsługiwanym filmem.
pub fn is_video_ext(ext: &str) -> bool {
    VIDEO_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
}

/// Rodzaj pliku po rozszerzeniu; `None` = plik nieobsługiwany.
pub fn kind_of(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("pdf") {
        Some(MediaKind::Pdf)
    } else if is_image_ext(ext) {
        Some(MediaKind::Image)
    } else if is_video_ext(ext) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

/// Pojedynczy plik multimedialny w katalogu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub size: u64,
    /// Czas modyfikacji (sekundy od epoki).
    pub modified: Option<i64>,
}

impl MediaItem {
    pub fn title(&self) -> &str {
        &self.name
    }

    /// Etykieta formatu do plakietki na kafelku (np. „JPG”, „MP4”).
    pub fn format_badge(&self) -> String {
        self.path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_uppercase())
            .unwrap_or_default()
    }
}

/// Wczytuje obsługiwane pliki z katalogu (bez podkatalogów i plików ukrytych).
pub fn scan(dir: &Path) -> io::Result<Vec<MediaItem>> {
    let mut items = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // `is_file` idzie za symlinkami — dowiązanie do zdjęcia ma działać.
        if !path.is_file() {
            continue;
        }
        let Some(kind) = kind_of(&path) else {
            continue;
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        items.push(MediaItem {
            path,
            name,
            kind,
            size: metadata.len(),
            modified,
        });
    }
    items.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    Ok(items)
}

/// Podkatalogi do paska folderów (bez ukrytych), posortowane naturalnie.
pub fn scan_folders(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut folders = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return folders;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            folders.push((name, path));
        }
    }
    folders.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    folders
}

/// Porównanie „naturalne”: `IMG_2` przed `IMG_10` (cyfry porównywane jako liczby).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.char_indices().peekable();
    let mut bi = b.char_indices().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((ia, _)), Some((ib, _))) => {
                let (ac, bc) = (
                    a[ia..].chars().next().unwrap(),
                    b[ib..].chars().next().unwrap(),
                );
                if ac.is_ascii_digit() && bc.is_ascii_digit() {
                    let (na, la) = take_number(&a[ia..]);
                    let (nb, lb) = take_number(&b[ib..]);
                    match na.cmp(&nb) {
                        Ordering::Equal => {
                            // Ta sama wartość — krótszy zapis (bez zer wiodących) pierwszy.
                            match la.cmp(&lb) {
                                Ordering::Equal => {}
                                other => return other,
                            }
                        }
                        other => return other,
                    }
                    for _ in 0..la {
                        ai.next();
                    }
                    for _ in 0..lb {
                        bi.next();
                    }
                } else {
                    let (al, bl) = (fold_char(ac), fold_char(bc));
                    match al.cmp(&bl) {
                        Ordering::Equal => {
                            ai.next();
                            bi.next();
                        }
                        other => return other,
                    }
                }
            }
        }
    }
}

fn take_number(s: &str) -> (u64, usize) {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    // Bardzo długie liczby traktujemy jako nasycone — kolejność i tak jest czytelna.
    let value = digits.parse::<u64>().unwrap_or(u64::MAX);
    (value, digits.len())
}

fn fold_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Rozmiar w czytelnej postaci (SI, jak w menedżerze plików).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Czas trwania jako `m:ss` albo `h:mm:ss`.
pub fn format_duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "0:00".to_owned();
    }
    let total = seconds.round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Czas modyfikacji w postaci `YYYY-MM-DD HH:MM` (czas lokalny).
pub fn format_mtime(secs: Option<i64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let (y, m, d, h, mi, _) = local_time(secs);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Bieżący czas w sekundach od epoki.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn local_time(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    // SAFETY: `localtime_r` wypełnia strukturę `tm` i nie alokuje; przekazujemy
    // poprawny wskaźnik na `time_t`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = secs as libc::time_t;
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        return (1970, 1, 1, 0, 0, 0);
    }
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
    )
}

/// Nazwa kodeka w postaci przyjaznej dla użytkownika.
pub fn codec_label(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "h264" => "H.264".to_owned(),
        "hevc" | "h265" => "H.265".to_owned(),
        "vp8" => "VP8".to_owned(),
        "vp9" => "VP9".to_owned(),
        "av1" => "AV1".to_owned(),
        "mpeg4" => "MPEG-4".to_owned(),
        "mpeg2video" => "MPEG-2".to_owned(),
        "theora" => "Theora".to_owned(),
        "prores" => "ProRes".to_owned(),
        "" => "—".to_owned(),
        other => other.to_ascii_uppercase(),
    }
}

/// Liczba elementów wg rodzaju (obrazy, filmy, dokumenty PDF).
pub fn count_kinds(items: &[MediaItem]) -> (usize, usize, usize) {
    let images = items.iter().filter(|i| i.kind == MediaKind::Image).count();
    let videos = items.iter().filter(|i| i.kind == MediaKind::Video).count();
    (images, videos, items.len() - images - videos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_kinds_by_extension() {
        assert_eq!(kind_of(Path::new("a.PNG")), Some(MediaKind::Image));
        assert_eq!(kind_of(Path::new("a.jpeg")), Some(MediaKind::Image));
        assert_eq!(kind_of(Path::new("a.MP4")), Some(MediaKind::Video));
        assert_eq!(kind_of(Path::new("a.webm")), Some(MediaKind::Video));
        assert_eq!(kind_of(Path::new("skan.PDF")), Some(MediaKind::Pdf));
        assert_eq!(kind_of(Path::new("foto.CR3")), Some(MediaKind::Image));
        assert_eq!(kind_of(Path::new("foto.nef")), Some(MediaKind::Image));
        assert_eq!(kind_of(Path::new("a.txt")), None);
        assert_eq!(kind_of(Path::new("bez-rozszerzenia")), None);
    }

    #[test]
    fn natural_order_puts_numbers_numerically() {
        let mut names = vec!["IMG_10.png", "IMG_2.png", "IMG_1.png", "obraz.png"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec!["IMG_1.png", "IMG_2.png", "IMG_10.png", "obraz.png"]
        );
    }

    #[test]
    fn natural_order_is_case_insensitive() {
        assert_eq!(natural_cmp("aaa", "BBB"), Ordering::Less);
        assert_eq!(natural_cmp("Plik2", "plik02"), Ordering::Less);
    }

    #[test]
    fn formats_sizes_and_durations() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1_500), "1.5 KB");
        assert_eq!(human_size(12_500_000), "12.5 MB");
        assert_eq!(human_size(150_000_000_000), "150 GB");
        assert_eq!(format_duration(0.0), "0:00");
        assert_eq!(format_duration(62.4), "1:02");
        assert_eq!(format_duration(3_723.0), "1:02:03");
        assert_eq!(format_duration(f64::NAN), "0:00");
    }

    #[test]
    fn scans_only_supported_and_visible_files() {
        let dir = std::env::temp_dir().join(format!(
            "luna-media-scan-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.png"), b"x").unwrap();
        fs::write(dir.join("a.mp4"), b"x").unwrap();
        fs::write(dir.join("notatka.txt"), b"x").unwrap();
        fs::write(dir.join(".ukryty.jpg"), b"x").unwrap();
        fs::create_dir_all(dir.join("folder.jpg")).unwrap();
        let items = scan(&dir).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "a.mp4");
        assert_eq!(items[0].kind, MediaKind::Video);
        assert_eq!(items[1].name, "b.png");
        assert_eq!(items[1].kind, MediaKind::Image);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_subfolders_sorted_and_skips_hidden() {
        let dir = std::env::temp_dir().join(format!(
            "luna-media-folders-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Album 10")).unwrap();
        fs::create_dir_all(dir.join("Album 2")).unwrap();
        fs::create_dir_all(dir.join(".ukryty")).unwrap();
        fs::write(dir.join("plik.png"), b"x").unwrap();
        let folders = scan_folders(&dir);
        let names: Vec<&str> = folders.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, vec!["Album 2", "Album 10"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn codec_labels_are_human_readable() {
        assert_eq!(codec_label("h264"), "H.264");
        assert_eq!(codec_label("hevc"), "H.265");
        assert_eq!(codec_label("vp9"), "VP9");
        assert_eq!(codec_label(""), "—");
    }
}
