//! Lokalni użytkownicy dla greetera.
//!
//! greetd nie udostępnia listy kont przez IPC (to celowo: backend nie wie, skąd
//! pochodzą użytkownicy). Greeter czyta więc standardowy, światowy `/etc/passwd`
//! i pokazuje konta „ludzkie” (UID w zakresie zwykłych użytkowników, poprawna
//! powłoka i katalog domowy). Nadal można wpisać dowolną nazwę ręcznie — dzięki
//! temu działają też konta LDAP/SSSD, których nie ma w lokalnym pliku.

/// Konto pokazywane na ekranie logowania.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    /// Nazwa logowania (`login`).
    pub name: String,
    /// Czytelna nazwa z pola GECOS (albo nazwa logowania).
    pub display: String,
    /// UID (do sortowania i diagnostyki).
    pub uid: u32,
}

impl User {
    /// Inicjały do awatara (maks. dwa znaki).
    pub fn initials(&self) -> String {
        initials(&self.display)
    }
}

/// Czyta lokalnych użytkowników z `/etc/passwd`.
pub fn local_users() -> Vec<User> {
    let text = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    parse_passwd(&text)
}

/// Parsuje zawartość `/etc/passwd` (wydzielone, żeby dało się testować bez systemu).
pub fn parse_passwd(text: &str) -> Vec<User> {
    let mut users = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(user) = parse_line(line) {
            users.push(user);
        }
    }
    users.sort_by_cached_key(|u| u.display.to_lowercase());
    users
}

fn parse_line(line: &str) -> Option<User> {
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 7 {
        return None;
    }
    let name = parts[0].trim();
    let uid: u32 = parts[2].parse().ok()?;
    let gecos = parts[4];
    let home = parts[5];
    let shell = parts[6].rsplit('/').next().unwrap_or(parts[6]);

    // Zwykłe konta: UID 1000..70000 (obejmuje też systemd-homed), prawdziwa
    // powłoka i sensowny katalog domowy.
    if !(1000..70000).contains(&uid) {
        return None;
    }
    if matches!(name, "nobody" | "greeter" | "sddm" | "lightdm") {
        return None;
    }
    if matches!(
        shell,
        "nologin" | "false" | "sync" | "shutdown" | "halt" | "git-shell"
    ) {
        return None;
    }
    if home.is_empty() || matches!(home, "/nonexistent" | "/var/empty" | "/dev/null") {
        return None;
    }

    let display = gecos
        .split(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(name)
        .to_owned();
    Some(User {
        name: name.to_owned(),
        display,
        uid,
    })
}

/// Pierwsze litery dwóch pierwszych słów nazwy (albo dwie litery jednego słowa).
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut out = String::new();
    match words.as_slice() {
        [] => out.push('?'),
        [one] => {
            for ch in one.chars().filter(|c| c.is_alphanumeric()).take(2) {
                out.extend(ch.to_uppercase());
            }
        }
        [first, .., last] => {
            if let Some(ch) = first.chars().find(|c| c.is_alphanumeric()) {
                out.extend(ch.to_uppercase());
            }
            if let Some(ch) = last.chars().find(|c| c.is_alphanumeric()) {
                out.extend(ch.to_uppercase());
            }
        }
    }
    if out.is_empty() {
        out.push('?');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
greeter:x:990:990:Greeter:/var/lib/greetd:/usr/sbin/nologin
patryk:x:1000:1000:Patryk Szkudlarek:/home/patryk:/bin/bash
student:x:1001:1001::/home/student:/bin/zsh
service:x:1002:1002:Service:/nonexistent:/usr/sbin/nologin
";

    #[test]
    fn keeps_only_human_accounts() {
        let users = parse_passwd(PASSWD);
        let names: Vec<&str> = users.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["patryk", "student"]);
        assert_eq!(users[0].display, "Patryk Szkudlarek");
        assert_eq!(users[1].display, "student");
    }

    #[test]
    fn initials_for_one_and_two_words() {
        assert_eq!(initials("Patryk Szkudlarek"), "PS");
        assert_eq!(initials("Luna"), "LU");
        assert_eq!(initials(""), "?");
    }

    #[test]
    fn skips_malformed_lines() {
        assert!(parse_passwd("nope\nbroken:x:abc").is_empty());
    }
}
