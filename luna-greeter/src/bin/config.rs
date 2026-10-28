//! `luna-greeter-config` — bezpieczny pomocnik zapisu ustawień ekranu logowania.
//!
//! Uruchamiany przez Ustawienia Luny przez `pkexec` (albo ręcznie przez roota):
//!
//! ```text
//! pkexec luna-greeter-config write   # konfiguracja `klucz=wartość` na stdin
//! luna-greeter-config path           # wypisuje ścieżkę pliku systemowego
//! ```
//!
//! Narzędzie **tylko parsuje znane klucze** i zapisuje `/etc/luna/greeter.conf`.
//! Nie wykonuje żadnych poleceń i nie interpretuje powłoki. Gdy wybrana tapeta
//! nie jest czytelna dla użytkownika `greeter` (np. leży w katalogu domowym
//! z prawami 0700), kopiuje ją do `/var/lib/luna/wallpapers/` i zapisuje w
//! konfiguracji ścieżkę kopii.

use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use libluna_core::greeter::{GreeterSettings, SYSTEM_PATH};

/// Maksymalny rozmiar wejścia (konfiguracja jest malutka).
const MAX_INPUT: usize = 64 * 1024;
/// Maksymalny rozmiar tapety, jaką przyjmujemy.
const MAX_WALLPAPER: u64 = 32 * 1024 * 1024;
/// Katalog kopii tapet czytelnych dla konta `greeter`.
const WALLPAPER_DIR: &str = "/var/lib/luna/wallpapers";
/// Sygnatura pliku PNG.
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn main() -> Result<(), Box<dyn Error>> {
    let command = std::env::args().nth(1).unwrap_or_default();
    match command.as_str() {
        "write" => write_from_stdin()?,
        "path" => println!("{}", GreeterSettings::config_path().display()),
        "-h" | "--help" | "help" => help(),
        other => {
            eprintln!("luna-greeter-config: nieznane polecenie {other:?}");
            help();
            std::process::exit(2);
        }
    }
    Ok(())
}

fn help() {
    println!(
        "luna-greeter-config — zapis ustawień ekranu logowania Luny\n\n\
         Użycie:\n  \
           luna-greeter-config write   zapisz konfigurację ze standardowego wejścia\n  \
           luna-greeter-config path    wypisz ścieżkę pliku ({SYSTEM_PATH})\n\n\
         Format wejścia (nieznane klucze są ignorowane):\n  \
           theme=light|dark|system\n  \
           accent=blue|teal|green|yellow|orange|red|pink|purple|slate\n  \
           show_clock=true|false\n  \
           wallpaper=/bezwzględna/ścieżka.png\n"
    );
}

fn write_from_stdin() -> Result<(), Box<dyn Error>> {
    let mut input = String::new();
    std::io::stdin()
        .take(MAX_INPUT as u64 + 1)
        .read_to_string(&mut input)?;
    if input.len() > MAX_INPUT {
        return Err("dane wejściowe są za duże".into());
    }

    let mut settings = GreeterSettings::parse(&input);
    if let Some(wallpaper) = &settings.wallpaper {
        let path = ensure_greeter_readable(wallpaper)?;
        settings.wallpaper = Some(path);
    }
    // `pkexec` czyści środowisko, więc produkcyjnie to zawsze `SYSTEM_PATH`.
    // `LUNA_GREETER_CONFIG` służy do testów i uruchomień bez roota.
    let path = GreeterSettings::config_path();
    settings.save_local(&path)?;
    eprintln!("luna-greeter-config: zapisano {}", path.display());
    Ok(())
}

/// Sprawdza tapetę i — jeśli konto `greeter` jej nie odczyta — kopiuje ją do
/// [`WALLPAPER_DIR`]. Zwraca ścieżkę, którą należy zapisać w konfiguracji.
fn ensure_greeter_readable(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let meta = std::fs::metadata(path)
        .map_err(|err| format!("nie można odczytać tapety {}: {err}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("tapeta nie jest plikiem: {}", path.display()).into());
    }
    if meta.len() > MAX_WALLPAPER {
        return Err(format!(
            "tapeta jest za duża ({} B, limit {MAX_WALLPAPER} B)",
            meta.len()
        )
        .into());
    }
    let mut header = [0u8; 8];
    File::open(path)?.read_exact(&mut header)?;
    if header != PNG_MAGIC {
        return Err("tapeta ekranu logowania musi być plikiem PNG".into());
    }
    if world_readable(path) {
        return Ok(path.to_owned());
    }

    let dir = Path::new(WALLPAPER_DIR);
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))?;
    let target = dir.join(copy_name(path));
    std::fs::copy(path, &target)?;
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))?;
    eprintln!(
        "luna-greeter-config: skopiowano tapetę do {} (czytelna dla greetera)",
        target.display()
    );
    Ok(target)
}

/// Czy plik i wszystkie katalogi nadrzędne są czytelne dla „innych” (konto `greeter`).
fn world_readable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.permissions().mode() & 0o004 == 0 {
        return false;
    }
    for ancestor in path.ancestors().skip(1) {
        if ancestor == Path::new("/") {
            break;
        }
        let Ok(dir) = std::fs::metadata(ancestor) else {
            return false;
        };
        if dir.permissions().mode() & 0o001 == 0 {
            return false;
        }
    }
    true
}

/// Stabilna nazwa kopii: `nazwa-<fnv1a ścieżki>.png` (bez nadpisywania obcych plików).
fn copy_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("wallpaper");
    let clean: String = stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    format!("{clean}-{:08x}.png", fnv1a(path.as_os_str().as_bytes()))
}

/// FNV-1a 32-bit — deterministyczna nazwa kopii między uruchomieniami.
fn fnv1a(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("luna-greeter-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("katalog");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("prawa");
        dir
    }

    #[test]
    fn copy_name_is_stable_and_safe() {
        let name = copy_name(Path::new("/home/patryk/Moje Obrazy/tło letnie.png"));
        assert!(name.starts_with("t-o-letnie-"));
        assert!(name.ends_with(".png"));
        assert_eq!(
            name,
            copy_name(Path::new("/home/patryk/Moje Obrazy/tło letnie.png"))
        );
        assert_ne!(name, copy_name(Path::new("/home/inny/tło letnie.png")));
    }

    #[test]
    fn accepts_png_and_rejects_other_formats() {
        let dir = temp_dir("png");
        let png = dir.join("ok.png");
        std::fs::write(&png, PNG_MAGIC).unwrap();
        assert!(ensure_greeter_readable(&png).is_ok());

        let jpg = dir.join("zle.jpg");
        std::fs::write(&jpg, b"\xff\xd8\xff\xe0not-a-png").unwrap();
        assert!(ensure_greeter_readable(&jpg).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn detects_unreadable_parent_directories() {
        let dir = temp_dir("private");
        let file = dir.join("tlo.png");
        std::fs::write(&file, PNG_MAGIC).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        // Katalog 0700 nie jest dostępny dla konta `greeter`.
        assert!(!world_readable(&file));
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(world_readable(&file));
        let _ = std::fs::remove_dir_all(dir);
    }
}
