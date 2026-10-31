//! Lokalne konta użytkowników (`/etc/passwd`, `/etc/group`).
//!
//! Centrum ustawień pokazuje **profile** bez uruchamiania procesów: plik passwd
//! ma wszystko, czego potrzebuje lista (login, nazwa, rola, katalog, powłoka).
//! Wynik jest czystą funkcją tekstu plików, więc można go testować bez systemu.

use std::path::Path;

/// Konto lokalne widoczne w Ustawieniach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount {
    pub login: String,
    /// Pełna nazwa z GECOS (pierwsze pole); pusta = brak.
    pub full_name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
    /// Czy należy do grupy administracyjnej (`wheel`/`sudo`/`admin`).
    pub admin: bool,
    /// Czy to konto bieżącego użytkownika.
    pub current: bool,
}

impl UserAccount {
    /// Nazwa do pokazania w UI (pełna nazwa albo login).
    pub fn display_name(&self) -> &str {
        if self.full_name.is_empty() {
            &self.login
        } else {
            &self.full_name
        }
    }

    /// Rola po polsku/angielsku — UI tłumaczy przez `tr()`.
    pub fn is_admin(&self) -> bool {
        self.admin
    }
}

/// Grupy, których członkostwo oznacza konto administratora.
pub const ADMIN_GROUPS: [&str; 3] = ["wheel", "sudo", "admin"];

/// Najniższy UID zwykłego konta (systemowe konta mają < 1000).
pub const FIRST_USER_UID: u32 = 1000;
/// `nobody` i podobne konta techniczne.
pub const LAST_USER_UID: u32 = 60_000;

/// Odczytuje lokalne konta z systemu (bieżący użytkownik z `USER`/`LOGNAME`).
pub fn local_users() -> Vec<UserAccount> {
    let current = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let groups = std::fs::read_to_string("/etc/group").unwrap_or_default();
    parse_accounts(&passwd, &groups, &current)
}

/// Czy plik powłoki wygląda na „konto bez logowania”.
fn is_nologin(shell: &str) -> bool {
    let name = Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(shell);
    matches!(name, "nologin" | "false" | "sync" | "shutdown" | "halt")
}

/// Parsuje `/etc/passwd` i `/etc/group`.
///
/// `current` to login bieżącego użytkownika (pusty = nie oznaczaj nikogo).
pub fn parse_accounts(passwd: &str, groups: &str, current: &str) -> Vec<UserAccount> {
    // gid → nazwa grupy + członkowie grup administracyjnych
    let mut admin_gids: Vec<u32> = Vec::new();
    let mut admin_logins: Vec<String> = Vec::new();
    for line in groups.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 4 {
            continue;
        }
        let name = fields[0].trim();
        if !ADMIN_GROUPS.contains(&name) {
            continue;
        }
        if let Ok(gid) = fields[2].trim().parse::<u32>() {
            admin_gids.push(gid);
        }
        admin_logins.extend(
            fields[3]
                .split(',')
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(str::to_owned),
        );
    }

    let mut users = Vec::new();
    for line in passwd.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.len() < 7 {
            continue;
        }
        let login = fields[0].trim();
        let Ok(uid) = fields[2].trim().parse::<u32>() else {
            continue;
        };
        let Ok(gid) = fields[3].trim().parse::<u32>() else {
            continue;
        };
        let shell = fields[6].trim();
        if login.is_empty() || !(FIRST_USER_UID..LAST_USER_UID).contains(&uid) || is_nologin(shell)
        {
            continue;
        }
        let full_name = fields[4].split(',').next().unwrap_or("").trim().to_owned();
        let admin = admin_gids.contains(&gid) || admin_logins.iter().any(|m| m == login);
        users.push(UserAccount {
            login: login.to_owned(),
            full_name,
            uid,
            gid,
            home: fields[5].trim().to_owned(),
            shell: shell.to_owned(),
            admin,
            current: !current.is_empty() && login == current,
        });
    }
    users.sort_by_key(|user| user.uid);
    users
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/bin:/usr/bin/nologin
patryks:x:1001:1001:Patryk Szkudlarek:/home/patryks:/bin/bash
gosc:x:1002:1002::/home/gosc:/bin/sh
stary:x:1003:1003:Stary,Biuro,123:/home/stary:/usr/bin/nologin
";

    const GROUP: &str = "\
root:x:0:
wheel:x:998:patryks
users:x:1001:
";

    #[test]
    fn keeps_only_real_human_accounts() {
        let users = parse_accounts(PASSWD, GROUP, "patryks");
        let logins: Vec<&str> = users.iter().map(|u| u.login.as_str()).collect();
        assert_eq!(logins, vec!["patryks", "gosc"]);
    }

    #[test]
    fn marks_admin_by_group_and_current_account() {
        let users = parse_accounts(PASSWD, GROUP, "patryks");
        let patryks = &users[0];
        assert!(patryks.admin);
        assert!(patryks.current);
        assert_eq!(patryks.full_name, "Patryk Szkudlarek");
        assert!(!users[1].admin);
        assert!(!users[1].current);
        assert_eq!(users[1].display_name(), "gosc");
    }

    #[test]
    fn primary_gid_in_admin_group_counts_as_admin() {
        let group = "wheel:x:1002:\n";
        let users = parse_accounts(PASSWD, group, "");
        assert!(users[1].admin, "gid 1002 = wheel");
    }

    #[test]
    fn gecos_is_only_the_full_name_part() {
        let users = parse_accounts(PASSWD, GROUP, "");
        assert_eq!(users[0].full_name, "Patryk Szkudlarek");
    }

    #[test]
    fn garbage_lines_are_ignored() {
        let users = parse_accounts("bzdura\nx:y:z\n", "", "");
        assert!(users.is_empty());
    }
}
