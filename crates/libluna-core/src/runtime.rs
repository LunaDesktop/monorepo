//! Katalog runtime (`XDG_RUNTIME_DIR`) — wspólny dla kompozytora i klientów.
//!
//! Gniazdo Waylanda leży w `XDG_RUNTIME_DIR`. Menedżer sesji (logind/`pam_systemd`)
//! zwykle ustawia tę zmienną, ale gdy tego nie zrobi (np. `pam_systemd` nie założył
//! sesji), i kompozytor, i klienci muszą wybrać **ten sam** katalog — inaczej klient
//! nie znajdzie gniazda i zgłosi `NoCompositor`. Ten moduł daje jedną, spójną logikę.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

/// Bieżący uid — właściciel `/proc/self` (bez zależności od libc/rustix).
fn current_uid() -> Option<u32> {
    fs::metadata("/proc/self").ok().map(|m| m.uid())
}

/// Czy `path` to istniejący, bezwzględny katalog należący do `uid`, tryb `0700`
/// (bez dostępu dla grupy i innych)?
pub fn is_private_dir_owned_by(path: &Path, uid: u32) -> bool {
    path.is_absolute()
        && fs::metadata(path)
            .map(|m| m.is_dir() && m.uid() == uid && m.mode() & 0o077 == 0)
            .unwrap_or(false)
}

/// Zapewnia poprawny `XDG_RUNTIME_DIR`.
///
/// Jeśli zmienna jest już ustawiona i wskazuje prywatny katalog naszego użytkownika —
/// nic nie zmieniamy. Inaczej wybieramy `/run/user/<uid>`, a gdy ono nie istnieje —
/// tworzymy prywatny katalog `0700` pod `$TMPDIR`.
///
/// Zwraca `Some(katalog)`, gdy zmienną trzeba było ustawić (przydatne do diagnostyki),
/// `None`, gdy była już poprawna albo gdy nie dało się ustalić uid.
pub fn ensure_runtime_dir() -> std::io::Result<Option<PathBuf>> {
    let Some(uid) = current_uid() else {
        return Ok(None);
    };

    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if is_private_dir_owned_by(Path::new(&dir), uid) {
            return Ok(None);
        }
    }

    let run_user = PathBuf::from(format!("/run/user/{uid}"));
    let dir = if is_private_dir_owned_by(&run_user, uid) {
        run_user
    } else {
        let base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let dir = base.join(format!("luna-runtime-{uid}"));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        match builder.create(&dir) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        if !is_private_dir_owned_by(&dir, uid) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "{} nie jest prywatnym katalogiem uid {uid} (oczekiwano 0700)",
                    dir.display()
                ),
            ));
        }
        dir
    };

    std::env::set_var("XDG_RUNTIME_DIR", &dir);
    Ok(Some(dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_dir_of_our_uid_is_accepted() {
        let uid = current_uid().expect("uid na Linuksie");
        let dir = std::env::temp_dir().join(format!("luna-runtime-test-{}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700);
        builder.create(&dir).unwrap();

        assert!(is_private_dir_owned_by(&dir, uid));
        // Obcy uid.
        assert!(!is_private_dir_owned_by(&dir, uid.wrapping_add(1)));
        // Zbyt otwarte uprawnienia.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!is_private_dir_owned_by(&dir, uid));
        // Nieistniejąca ścieżka i ścieżka względna.
        assert!(!is_private_dir_owned_by(&dir.join("missing"), uid));
        assert!(!is_private_dir_owned_by(Path::new("relative/dir"), uid));

        fs::remove_dir_all(&dir).ok();
    }
}
