//! Argumenty wiersza poleceń: `mailto:` i gotowy szkic wiadomości.
//!
//! Kontrakt jest celowo mały i zgodny z freedesktop:
//!
//! ```text
//! luna-email 'mailto:luna@mijsys.pl?subject=...&body=...'
//! luna-email --to a@b.c --subject "Temat" --body "Treść" \
//!            --attach raport.txt --cc kopia@b.c
//! luna-email --to a@b.c --body-file /tmp/treść.txt
//! ```
//!
//! Dzięki temu `xdg-email`, `gio open` i Ustawienia (diagnostyka) trafiają
//! do tego samego, wypełnionego formularza — bez osobnej ścieżki w aplikacji.

use std::path::PathBuf;

use crate::model::parse_addresses;

/// Gotowy szkic do otwarcia w edytorze.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeRequest {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<PathBuf>,
}

impl ComposeRequest {
    /// Czy jest co pokazać (samo `--compose` też otwiera pusty formularz).
    pub fn is_empty(&self) -> bool {
        self.to.is_empty()
            && self.cc.is_empty()
            && self.subject.is_empty()
            && self.body.is_empty()
            && self.attachments.is_empty()
    }
}

/// Parsuje argumenty po nazwie binarki. `None` = zwykłe uruchomienie aplikacji.
pub fn parse<I>(args: I) -> Option<ComposeRequest>
where
    I: IntoIterator<Item = String>,
{
    let mut request = ComposeRequest::default();
    let mut requested = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--compose" => requested = true,
            "--to" => {
                if let Some(value) = args.next() {
                    request.to.extend(parse_addresses(&value));
                    requested = true;
                }
            }
            "--cc" => {
                if let Some(value) = args.next() {
                    request.cc.extend(parse_addresses(&value));
                    requested = true;
                }
            }
            "--subject" => {
                if let Some(value) = args.next() {
                    request.subject = value;
                    requested = true;
                }
            }
            "--body" => {
                if let Some(value) = args.next() {
                    request.body = value;
                    requested = true;
                }
            }
            "--body-file" => {
                if let Some(value) = args.next() {
                    match std::fs::read_to_string(&value) {
                        Ok(text) => request.body = text,
                        Err(error) => {
                            eprintln!("luna-email: nie można odczytać {value}: {error}");
                        }
                    }
                    requested = true;
                }
            }
            "--attach" => {
                if let Some(value) = args.next() {
                    if !value.is_empty() {
                        request.attachments.push(PathBuf::from(value));
                        requested = true;
                    }
                }
            }
            other if other.starts_with("mailto:") => {
                fill_from_mailto(other, &mut request);
                requested = true;
            }
            _ => {}
        }
    }

    requested.then_some(request)
}

/// Uzupełnia szkic z URI `mailto:` (RFC 6068).
fn fill_from_mailto(uri: &str, request: &mut ComposeRequest) {
    let rest = uri.strip_prefix("mailto:").unwrap_or(uri);
    let (addresses, query) = rest.split_once('?').unwrap_or((rest, ""));
    for address in addresses.split(',') {
        let address = percent_decode(address, false);
        let address = address.trim();
        if !address.is_empty() {
            request.to.push(address.to_owned());
        }
    }
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value, true);
        match key {
            "subject" => request.subject = value,
            "body" => request.body = value,
            "cc" => request.cc.extend(parse_addresses(&value)),
            "to" => request.to.extend(parse_addresses(&value)),
            _ => {}
        }
    }
}

/// Dekoduje `%XX`. W zapytaniu `+` oznacza spację (tak robi większość klientów);
/// w adresie `+` zostaje znakiem plus (RFC 6068).
fn percent_decode(value: &str, plus_as_space: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                match (high, low) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        index += 3;
                    }
                    _ => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' if plus_as_space => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn no_arguments_means_regular_start() {
        assert_eq!(parse(Vec::new()), None);
    }

    #[test]
    fn mailto_fills_recipients_subject_and_body() {
        let request = parse(args(&[
            "mailto:ala@example.org,bob@example.org?subject=Raport%20Luna&body=Opis%20problemu",
        ]))
        .expect("szkic");
        assert_eq!(request.to, ["ala@example.org", "bob@example.org"]);
        assert_eq!(request.subject, "Raport Luna");
        assert_eq!(request.body, "Opis problemu");
    }

    #[test]
    fn mailto_keeps_non_ascii_and_decodes_plus_as_space_in_query() {
        let request = parse(args(&[
            "mailto:ala@example.org?subject=Za%C5%BC%C3%B3%C5%82%C4%87&body=a+b",
        ]))
        .expect("szkic");
        assert_eq!(request.subject, "Zażółć");
        assert_eq!(request.body, "a b");
    }

    #[test]
    fn plus_in_an_address_is_not_a_space() {
        let request = parse(args(&["mailto:ala+tag@example.org"])).expect("szkic");
        assert_eq!(request.to, ["ala+tag@example.org"]);
    }

    #[test]
    fn explicit_flags_build_the_same_request() {
        let request = parse(args(&[
            "--to",
            "Ania <ania@example.org>, bob@example.org",
            "--cc",
            "kopia@example.org",
            "--subject",
            "Temat",
            "--body",
            "Treść",
            "--attach",
            "/tmp/raport.txt",
        ]))
        .expect("szkic");
        assert_eq!(request.to, ["ania@example.org", "bob@example.org"]);
        assert_eq!(request.cc, ["kopia@example.org"]);
        assert_eq!(request.subject, "Temat");
        assert_eq!(request.body, "Treść");
        assert_eq!(request.attachments, [PathBuf::from("/tmp/raport.txt")]);
    }

    #[test]
    fn body_file_is_read() {
        let dir = std::env::temp_dir().join(format!("luna-email-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("katalog");
        let path = dir.join("raport.txt");
        std::fs::write(&path, "Treść raportu").expect("zapis");

        let request = parse(vec![
            "--to".to_owned(),
            "ala@example.org".to_owned(),
            "--body-file".to_owned(),
            path.display().to_string(),
        ])
        .expect("szkic");
        assert_eq!(request.body, "Treść raportu");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compose_alone_opens_an_empty_form() {
        let request = parse(args(&["--compose"])).expect("szkic");
        assert!(request.is_empty());
    }

    #[test]
    fn percent_decoder_handles_broken_sequences() {
        assert_eq!(percent_decode("a%2", false), "a%2");
        assert_eq!(percent_decode("a%ZZb", false), "a%ZZb");
        assert_eq!(percent_decode("%C5%81", false), "Ł");
    }
}
