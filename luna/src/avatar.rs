//! Awatary użytkownika: generowanie proceduralne, walidacja i zapis.
//!
//! Bezpieczeństwo: przyjmujemy **tylko PNG**, ograniczamy rozmiar pliku i wymiary
//! (przed dekodowaniem, żeby nie zrobić bomby pamięciowej), a wynik zawsze
//! **re-kodujemy** do nowego PNG. Dzięki temu do AccountsService trafia świeży
//! obraz bez metadanych, dodatkowych chunków i dowolnego „ogonka” po `IEND`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect};
use libluna_i18n::tr;
use libluna_render::wallpaper::{cover, decode_png};
use libluna_render::{ImageData, Scene, SoftwareRenderer, TextEngine, TextRun};
use zbus::blocking::Connection;
use zbus::zvariant::OwnedObjectPath;

/// Rozmiar zapisywanego awatara (greeter i panel skalują go do swojej średnicy).
pub const AVATAR_SIZE: u32 = 256;
/// Maksymalny rozmiar pliku źródłowego (8 MiB).
pub const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
/// Maksymalny bok obrazu źródłowego.
pub const MAX_DIMENSION: u32 = 4096;
/// Maksymalna liczba pikseli źródła (ochrona przed dekompresją-olbrzymem).
pub const MAX_PIXELS: u64 = 16_000_000;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
const ACCOUNTS_DEST: &str = "org.freedesktop.Accounts";
const ACCOUNTS_PATH: &str = "/org/freedesktop/Accounts";
const ACCOUNTS_IFACE: &str = "org.freedesktop.Accounts";
const USER_IFACE: &str = "org.freedesktop.Accounts.User";

const PALETTE: [(u8, u8, u8); 10] = [
    (0x35, 0x84, 0xe4),
    (0x2e, 0xa0, 0x6a),
    (0xe0, 0x7b, 0x2f),
    (0x8e, 0x4f, 0xd0),
    (0xc0, 0x4a, 0x4a),
    (0x1f, 0x9e, 0xa8),
    (0x4a, 0x5c, 0xc8),
    (0x9a, 0x6b, 0x3f),
    (0xd0, 0x4f, 0x8c),
    (0x4f, 0x9e, 0x35),
];

/// Gotowy awatar: PNG do zapisu i obraz do podglądu.
#[derive(Debug, Clone)]
pub struct AvatarImage {
    pub png: Vec<u8>,
    pub image: Arc<ImageData>,
}

/// Wynik zapisu: plik lokalny zawsze, AccountsService — jeśli dostępny.
#[derive(Debug, Clone)]
pub struct SaveResult {
    pub local_path: PathBuf,
    pub accounts_service: Result<(), String>,
}

/// Inicjały z nazwy: „Ada Lovelace” → `AL`, „ala” → `AL`, brak nazwy → `?`.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut out = String::new();
    match words.as_slice() {
        [] => out.push('?'),
        [one] => out.extend(one.chars().take(2)),
        [first, .., last] => {
            out.extend(first.chars().take(1));
            out.extend(last.chars().take(1));
        }
    }
    out.to_uppercase()
}

/// Awatar proceduralny: deterministyczne kolory i wzór z nazwy + inicjały.
pub fn generated_avatar(name: &str) -> Result<AvatarImage, String> {
    let image = generated_image(name, AVATAR_SIZE);
    let png = encode_image(&image)?;
    Ok(AvatarImage {
        png,
        image: Arc::new(image),
    })
}

/// Wczytuje i waliduje źródłowy PNG (bez zapisu).
pub fn load_source(path: &Path) -> Result<ImageData, String> {
    let meta = fs::symlink_metadata(path).map_err(|_| tr("Could not read the image"))?;
    if !meta.file_type().is_file() {
        return Err(tr("Avatar must be a regular file"));
    }
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(tr("Image is too large"));
    }
    let bytes = fs::read(path).map_err(|_| tr("Could not read the image"))?;
    let source = sanitize_bytes(&bytes)?;
    Ok(source)
}

/// Wczytuje obraz i od razu skaluje go do awatara.
pub fn custom_avatar(path: &Path, zoom: f64) -> Result<AvatarImage, String> {
    let source = load_source(path)?;
    custom_from_source(&source, zoom)
}

/// Skaluje i centruje już zwalidowane źródło; wynik jest ponownie kodowany.
pub fn custom_from_source(source: &ImageData, zoom: f64) -> Result<AvatarImage, String> {
    let zoom = zoom.clamp(1.0, 2.0);
    let big = (AVATAR_SIZE as f64 * zoom).round() as u32;
    let covered = cover(source, big, big);
    let image = crop_center(&covered, AVATAR_SIZE);
    let png = encode_image(&image)?;
    Ok(AvatarImage {
        png,
        image: Arc::new(image),
    })
}

fn sanitize_bytes(bytes: &[u8]) -> Result<ImageData, String> {
    validate_png(bytes)?;
    decode_png(bytes).ok_or_else(|| tr("Invalid PNG image"))
}

fn validate_png(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 33 || bytes[..8] != PNG_SIGNATURE {
        return Err(tr("Only PNG images are supported"));
    }
    let chunk_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap_or([0; 4]));
    if chunk_len < 13 || &bytes[12..16] != b"IHDR" {
        return Err(tr("Invalid PNG image"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap_or([0; 4]));
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap_or([0; 4]));
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || width as u64 * height as u64 > MAX_PIXELS
    {
        return Err(tr("Image dimensions are too large"));
    }
    Ok((width, height))
}

fn crop_center(source: &ImageData, size: u32) -> ImageData {
    let size = size.min(source.width).min(source.height);
    let x = (source.width - size) / 2;
    let y = (source.height - size) / 2;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        let start = (((y + row) * source.width + x) * 4) as usize;
        let end = start + (size * 4) as usize;
        rgba.extend_from_slice(&source.rgba[start..end]);
    }
    ImageData::new(size, size, rgba).with_opaque(true)
}

fn encode_image(image: &ImageData) -> Result<Vec<u8>, String> {
    let mut scene = Scene::new();
    scene.image(
        Rect::from_xywh(0.0, 0.0, image.width as f64, image.height as f64),
        Arc::new(image.clone()),
    );
    let mut renderer = SoftwareRenderer::new(image.width.max(1), image.height.max(1));
    renderer.draw(&scene, 1.0, Color::TRANSPARENT);
    renderer
        .encode_png()
        .ok_or_else(|| tr("Could not encode the avatar"))
}

fn generated_image(name: &str, size: u32) -> ImageData {
    let size = size.max(16);
    let f = size as f64;
    let hash = name_hash(name);
    let (br, bg, bb) = PALETTE[(hash % PALETTE.len() as u64) as usize];
    let (ar, ag, ab) = PALETTE[((hash / 7 + 3) % PALETTE.len() as u64) as usize];
    let base = Color::from_rgba8(br, bg, bb, 0xff);
    let accent = Color::from_rgba8(ar, ag, ab, 0xff);

    let mut scene = Scene::new();
    scene.fill_rect(Rect::from_xywh(0.0, 0.0, f, f), base);

    match (hash >> 8) % 4 {
        0 => {
            for i in 0..3 {
                let radius = f * (0.18 + 0.07 * i as f64);
                scene.fill_circle(
                    Point::new(f * (0.2 + 0.28 * i as f64), f * 0.78),
                    radius,
                    accent.with_alpha(0.18),
                );
            }
        }
        1 => {
            for i in 0..4 {
                if i % 2 == 0 {
                    scene.fill_rect(
                        Rect::from_xywh(0.0, i as f64 * f / 4.0, f, f / 8.0),
                        accent.with_alpha(0.18),
                    );
                }
            }
        }
        2 => {
            for row in 0..4 {
                for col in 0..4 {
                    if (row + col) % 2 == 0 {
                        scene.fill_rect(
                            Rect::from_xywh(
                                col as f64 * f / 4.0,
                                row as f64 * f / 4.0,
                                f / 4.0,
                                f / 4.0,
                            ),
                            accent.with_alpha(0.12),
                        );
                    }
                }
            }
        }
        _ => {
            scene.fill_circle(
                Point::new(f * 0.76, f * 0.24),
                f * 0.35,
                accent.with_alpha(0.20),
            );
        }
    }

    let text = TextEngine::system();
    let initials = initials(name);
    let font_size = f * 0.36;
    let width = text.measure(&initials, font_size, weight::BOLD).width;
    scene.text(
        TextRun::new(
            Point::new((f - width) / 2.0, f * 0.5 + font_size * 0.35),
            &initials,
            font_size,
        )
        .with_weight(weight::BOLD)
        .with_color(Color::WHITE),
    );

    let mut renderer = SoftwareRenderer::new(size, size);
    renderer.draw(&scene, 1.0, Color::TRANSPARENT);
    ImageData::new(size, size, renderer.data().to_vec()).with_opaque(true)
}

fn name_hash(name: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Zapisuje awatar w katalogu użytkownika (zawsze działa bez uprawnień roota).
pub fn write_local_avatar(png: &[u8]) -> Result<PathBuf, String> {
    let path = local_avatar_path().ok_or_else(|| tr("No data directory"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|err| format!("{}: {err}", tr("Could not create avatar directory")))?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o644);
    let mut file = options
        .open(&path)
        .map_err(|err| format!("{}: {err}", tr("Could not write avatar")))?;
    file.write_all(png)
        .map_err(|err| format!("{}: {err}", tr("Could not write avatar")))?;
    file.flush()
        .map_err(|err| format!("{}: {err}", tr("Could not write avatar")))?;
    // Awatar i tak jest publiczny (greeter pokazuje go przed logowaniem), a
    // greeter działa jako inny użytkownik — 0600 uniemożliwiłoby odczyt kopii.
    #[cfg(unix)]
    {
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o644));
    }
    Ok(path)
}

/// Zapisuje lokalnie i próbuje ustawić ikonę w AccountsService.
pub fn save_avatar(login: &str, png: &[u8]) -> Result<SaveResult, String> {
    let local_path = write_local_avatar(png)?;
    let accounts_service = set_accounts_avatar(login, png);
    Ok(SaveResult {
        local_path,
        accounts_service,
    })
}

fn set_accounts_avatar(login: &str, png: &[u8]) -> Result<(), String> {
    let temp = std::env::temp_dir().join(format!(
        "luna-avatar-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    write_private(&temp, png)?;

    let result = (|| {
        let conn = Connection::system().map_err(|err| format!("D-Bus: {err}"))?;
        let reply = conn
            .call_method(
                Some(ACCOUNTS_DEST),
                ACCOUNTS_PATH,
                Some(ACCOUNTS_IFACE),
                "FindUserByName",
                &(login,),
            )
            .map_err(|err| format!("AccountsService: {err}"))?;
        let path: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|err| format!("AccountsService: {err}"))?;
        conn.call_method(
            Some(ACCOUNTS_DEST),
            path.as_str(),
            Some(USER_IFACE),
            "SetIconFile",
            &(temp.display().to_string(),),
        )
        .map_err(|err| format!("AccountsService: {err}"))?;
        Ok(())
    })();

    let _ = fs::remove_file(&temp);
    result
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|err| format!("{}: {err}", tr("Could not write temporary avatar")))?;
    file.write_all(bytes)
        .map_err(|err| format!("{}: {err}", tr("Could not write temporary avatar")))
}

/// Ścieżka ikony z AccountsService: `Icon=` z profilu, potem `icons/<login>`.
pub fn accounts_icon_path(login: &str) -> Option<PathBuf> {
    icon_path_from(Path::new("/var/lib/AccountsService"), login)
}

fn icon_path_from(base: &Path, login: &str) -> Option<PathBuf> {
    let configured = fs::read_to_string(base.join("users").join(login))
        .ok()
        .and_then(|text| configured_icon(&text));
    if let Some(path) = configured {
        let path = if path.is_absolute() {
            path
        } else {
            base.join(path)
        };
        if path.is_file() {
            return Some(path);
        }
    }
    let fallback = base.join("icons").join(login);
    fallback.is_file().then_some(fallback)
}

fn configured_icon(text: &str) -> Option<PathBuf> {
    let mut in_user = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_user = line == "[User]";
            continue;
        }
        if in_user {
            if let Some(value) = line.strip_prefix("Icon=") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(PathBuf::from(value));
                }
            }
        }
    }
    None
}

/// Wczytuje aktualny awatar użytkownika (AccountsService, potem kopia lokalna).
pub fn load_user_avatar(login: &str) -> Option<Arc<ImageData>> {
    if let Some(path) = accounts_icon_path(login) {
        if let Some(image) = read_icon(&path) {
            return Some(Arc::new(image));
        }
    }
    let path = local_avatar_path()?;
    read_icon(&path).map(Arc::new)
}

/// Znacznik czasu plików, z których bierzemy awatar (panel odświeża go w locie).
///
/// `None` = brak awatara. Zmiana pliku w Ustawieniach zmienia znacznik, więc
/// powłoka wczytuje nowy obraz bez restartu.
/// Ścieżka pliku, z którego pochodzi bieżący awatar (AccountsService albo kopia lokalna).
///
/// Używane m.in. przez Ustawienia, żeby pokazać, skąd pochodzi zdjęcie konta.
pub fn current_avatar_path(login: &str) -> Option<PathBuf> {
    if let Some(path) = accounts_icon_path(login) {
        if path.is_file() {
            return Some(path);
        }
    }
    local_avatar_path().filter(|path| path.is_file())
}

pub fn avatar_stamp(login: &str) -> Option<std::time::SystemTime> {
    let mut newest: Option<std::time::SystemTime> = None;
    for path in [accounts_icon_path(login), local_avatar_path()]
        .into_iter()
        .flatten()
    {
        let Ok(meta) = fs::metadata(path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        newest = Some(newest.map_or(modified, |current| current.max(modified)));
    }
    newest
}

fn read_icon(path: &Path) -> Option<ImageData> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    sanitize_bytes(&bytes).ok()
}

fn local_avatar_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(base.join("luna").join("avatar").join("current.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png(width: u32, height: u32) -> Vec<u8> {
        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, width as f64, height as f64),
            Color::from_rgba8(20, 120, 220, 255),
        );
        let mut renderer = SoftwareRenderer::new(width, height);
        renderer.draw(&scene, 1.0, Color::TRANSPARENT);
        renderer.encode_png().expect("PNG")
    }

    #[test]
    fn initials_follow_the_name() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("ala"), "AL");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn generated_avatar_is_deterministic_and_decodable() {
        let first = generated_avatar("Patryk Szkudlarek").unwrap();
        let second = generated_avatar("Patryk Szkudlarek").unwrap();
        assert_eq!(first.png, second.png);
        assert_eq!(first.image.width, AVATAR_SIZE);
        let decoded = decode_png(&first.png).unwrap();
        assert_eq!(decoded.width, AVATAR_SIZE);
        assert!(decode_png(&generated_avatar("Ala").unwrap().png).is_some());
    }

    #[test]
    fn rejects_non_png_and_oversized_dimensions() {
        assert!(sanitize_bytes(b"not a png").is_err());
        let mut header = Vec::new();
        header.extend_from_slice(&PNG_SIGNATURE);
        header.extend_from_slice(&13u32.to_be_bytes());
        header.extend_from_slice(b"IHDR");
        header.extend_from_slice(&100_000u32.to_be_bytes());
        header.extend_from_slice(&100_000u32.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert!(sanitize_bytes(&header).is_err());
    }

    #[test]
    fn sanitizing_strips_payload_after_iend() {
        let mut png = sample_png(8, 8);
        png.extend_from_slice(b"MALWARE-TRAILER");
        let path = std::env::temp_dir().join(format!(
            "luna-avatar-test-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::write(&path, &png).unwrap();
        let avatar = custom_avatar(&path, 1.0).unwrap();
        assert_eq!(avatar.image.width, AVATAR_SIZE);
        assert!(!avatar
            .png
            .windows(b"MALWARE-TRAILER".len())
            .any(|w| w == b"MALWARE-TRAILER"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn icon_path_uses_profile_then_fallback() {
        let base = std::env::temp_dir().join(format!(
            "luna-avatar-icons-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(base.join("users")).unwrap();
        fs::create_dir_all(base.join("icons")).unwrap();
        let png = sample_png(2, 2);
        fs::write(base.join("user.png"), &png).unwrap();
        fs::write(base.join("icons/fallback"), &png).unwrap();
        fs::write(
            base.join("users/custom"),
            format!("[User]\nIcon={}\n", base.join("user.png").display()),
        )
        .unwrap();
        assert_eq!(icon_path_from(&base, "custom"), Some(base.join("user.png")));
        assert_eq!(
            icon_path_from(&base, "fallback"),
            Some(base.join("icons/fallback"))
        );
        let _ = fs::remove_dir_all(base);
    }
}
