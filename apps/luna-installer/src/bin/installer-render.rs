//! Renderuje kroki Luna Installer do PNG — podgląd bez Waylanda.
//!
//! Użycie:
//!   `cargo run --bin installer-render -- [--dark] [--lang CODE] \
//!      [--step welcome|language|region|network|requirements|disk|account|summary|progress|done] \
//!      [--error] [wyjście.png]`

use std::error::Error;
use std::path::PathBuf;

use libluna_core::{Accent, Size, ThemePreference};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_installer::app::{App, PickerKind, Step};
use luna_installer::keyboard::{Keyboard, XkbLists, XkbOption};
use luna_installer::live::LiveEnvironment;
use luna_installer::network::{NetworkState, Wifi};
use luna_installer::plan::{FileSystem, PartitionMode, RootMode};
use luna_installer::translations::LanguageOption;
use luna_installer::{ui, Disk, Partition};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut dark = false;
    let mut lang = "pl".to_owned();
    let mut step = Step::Welcome;
    let mut error = false;
    let mut picker: Option<PickerKind> = None;
    let mut out: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dark" => {
                dark = true;
                index += 1;
            }
            "--error" => {
                error = true;
                index += 1;
            }
            "--lang" => {
                if let Some(value) = args.get(index + 1) {
                    lang = value.clone();
                }
                index += 2;
            }
            "--step" => {
                step = match args.get(index + 1).map(String::as_str) {
                    Some("language") => Step::Language,
                    Some("region") => Step::Region,
                    Some("network") => Step::Network,
                    Some("requirements") => Step::Requirements,
                    Some("disk") => Step::Disk,
                    Some("account") => Step::Account,
                    Some("summary") => Step::Summary,
                    Some("progress") => Step::Progress,
                    Some("done") => Step::Done,
                    _ => Step::Welcome,
                };
                index += 2;
            }
            "--picker" => {
                picker = match args.get(index + 1).map(String::as_str) {
                    Some("language") => Some(PickerKind::Language),
                    Some("layout") => Some(PickerKind::Layout),
                    Some("variant") => Some(PickerKind::Variant),
                    Some("model") => Some(PickerKind::Model),
                    Some("timezone") => Some(PickerKind::Timezone),
                    Some("disk") => Some(PickerKind::Disk),
                    Some("mode") => Some(PickerKind::Mode),
                    Some("root-partition") => Some(PickerKind::RootPartition),
                    Some("efi-partition") => Some(PickerKind::EfiPartition),
                    Some("swap-partition") => Some(PickerKind::SwapPartition),
                    Some("filesystem") => Some(PickerKind::Filesystem),
                    Some("swap") => Some(PickerKind::SwapSize),
                    Some("bootloader") => Some(PickerKind::Bootloader),
                    Some("root-mode") => Some(PickerKind::RootMode),
                    _ => None,
                };
                index += 2;
            }
            other => {
                out = Some(other.to_owned());
                index += 1;
            }
        }
    }

    let out = out.unwrap_or_else(|| {
        format!(
            "luna-installer-{}{}.png",
            step_name(step),
            if dark { "-dark" } else { "" }
        )
    });

    set_locale(Locale::new(&lang));
    libluna_i18n::load_catalogs_from_dirs();

    let languages = vec![
        LanguageOption {
            code: "pl".to_owned(),
            name: "Polski".to_owned(),
            locale: "pl_PL.UTF-8".to_owned(),
            coverage: 98,
        },
        LanguageOption {
            code: "en".to_owned(),
            name: "English".to_owned(),
            locale: "en_US.UTF-8".to_owned(),
            coverage: 100,
        },
        LanguageOption {
            code: "de".to_owned(),
            name: "Deutsch".to_owned(),
            locale: "de_DE.UTF-8".to_owned(),
            coverage: 96,
        },
        LanguageOption {
            code: "fr".to_owned(),
            name: "Français".to_owned(),
            locale: "fr_FR.UTF-8".to_owned(),
            coverage: 0,
        },
    ];
    let xkb = XkbLists {
        layouts: vec![
            XkbOption {
                code: "pl".to_owned(),
                name: "Polish".to_owned(),
            },
            XkbOption {
                code: "us".to_owned(),
                name: "English (US)".to_owned(),
            },
            XkbOption {
                code: "de".to_owned(),
                name: "German".to_owned(),
            },
            XkbOption {
                code: "fr".to_owned(),
                name: "French".to_owned(),
            },
        ],
        variants: vec![],
        models: vec![
            XkbOption {
                code: "pc105".to_owned(),
                name: "Generic 105-key PC".to_owned(),
            },
            XkbOption {
                code: "pc104".to_owned(),
                name: "Generic 104-key PC".to_owned(),
            },
        ],
    };
    let timezones = vec![
        "Europe/Warsaw".to_owned(),
        "Europe/Berlin".to_owned(),
        "Europe/London".to_owned(),
        "UTC".to_owned(),
    ];
    let disks = vec![
        Disk {
            name: "nvme0n1".to_owned(),
            path: "/dev/nvme0n1".to_owned(),
            size_bytes: 512_000_000_000,
            model: "Luna SSD".to_owned(),
            removable: false,
            free_bytes: 0,
            protection: None,
        },
        Disk {
            name: "sdb".to_owned(),
            path: "/dev/sdb".to_owned(),
            size_bytes: 16_000_000_000,
            model: "USB Live".to_owned(),
            removable: true,
            free_bytes: 0,
            protection: Some("Contains the running live image".to_owned()),
        },
    ];
    let network = NetworkState {
        available: true,
        connected: true,
        internet: true,
        interface: Some("wlan0".to_owned()),
        wifi: vec![
            Wifi {
                ssid: "Luna-Net".to_owned(),
                signal: 88,
                security: "WPA2".to_owned(),
                active: true,
            },
            Wifi {
                ssid: "Sąsiad".to_owned(),
                signal: 47,
                security: "WPA2".to_owned(),
                active: false,
            },
        ],
        error: None,
    };

    let mut app = App::with_data(
        LiveEnvironment {
            live: true,
            medium: None,
            reason: String::new(),
        },
        languages,
        xkb,
        timezones,
        disks,
        network,
        Some(PathBuf::from("/usr/libexec/luna/luna-install-backend")),
    );
    app.step = step;
    app.keyboard = Keyboard {
        layout: "pl".to_owned(),
        variant: String::new(),
        model: "pc105".to_owned(),
    };
    app.locale = "pl_PL.UTF-8".to_owned();
    app.timezone = "Europe/Warsaw".to_owned();
    app.form.full_name.set_text("Luna Tester");
    app.form.username.set_text("luna");
    app.form.hostname.set_text("luna-pc");
    app.form.password.set_text("dobre-haslo");
    app.form.confirm.set_text("dobre-haslo");
    app.form.luks_password.set_text("haslo-luks");
    app.form.luks_confirm.set_text("haslo-luks");
    app.partitions = vec![
        Partition {
            name: "nvme0n1p1".to_owned(),
            path: "/dev/nvme0n1p1".to_owned(),
            size_bytes: 512_000_000,
            fstype: "vfat".to_owned(),
            part_type: "c12a7328-f81f-11d2-ba4b-00a0c93ec93b".to_owned(),
            mount: None,
        },
        Partition {
            name: "nvme0n1p2".to_owned(),
            path: "/dev/nvme0n1p2".to_owned(),
            size_bytes: 120_000_000_000,
            fstype: "ext4".to_owned(),
            part_type: String::new(),
            mount: None,
        },
        Partition {
            name: "nvme0n1p3".to_owned(),
            path: "/dev/nvme0n1p3".to_owned(),
            size_bytes: 390_000_000_000,
            fstype: "ext4".to_owned(),
            part_type: String::new(),
            mount: None,
        },
    ];
    app.options.uefi = true;
    app.options.mode = if step == Step::Disk {
        PartitionMode::Manual
    } else {
        PartitionMode::EraseDisk
    };
    app.options.filesystem = FileSystem::Btrfs;
    app.options.encrypt = matches!(step, Step::Summary | Step::Disk);
    app.options.root_partition = Some("/dev/nvme0n1p3".to_owned());
    app.options.efi_partition = Some("/dev/nvme0n1p1".to_owned());
    app.autologin = matches!(step, Step::Summary | Step::Account);
    app.root_mode = if step == Step::Account {
        RootMode::Custom
    } else {
        RootMode::Locked
    };
    app.form.root_password.set_text("root-haslo");
    app.form.root_confirm.set_text("root-haslo");
    app.set_focus(if step == Step::Language {
        luna_installer::Field::LayoutSearch
    } else if step == Step::Account {
        luna_installer::Field::FullName
    } else {
        luna_installer::Field::FullName
    });
    if step == Step::Summary {
        app.confirm_erase = true;
    }
    if step == Step::Progress {
        app.log = vec![
            "PROGRESS: 10".to_owned(),
            "STEP: Partitioning /dev/nvme0n1 (GPT, sgdisk)".to_owned(),
            "STEP: Creating LUKS2 container".to_owned(),
            "STEP: Creating btrfs filesystem".to_owned(),
            "STEP: Copying the live system (this can take a while)".to_owned(),
        ];
        app.progress = 0.45;
        app.spinner = 0.35;
    }
    if step == Step::Done {
        app.finished = Some(if error {
            Err("The installer finished with an error (exit code 1).".to_owned())
        } else {
            Ok(())
        });
        app.progress = 1.0;
    }
    app.refresh_checks();
    if let Some(kind) = picker {
        app.open_picker(kind);
    }

    let preference = if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    };
    let theme = preference.resolve(false, Accent::Blue);
    let size = Size::new(1080.0, 780.0);
    let text = TextEngine::system();
    let mut scene = Scene::new();
    ui::paint(&app, &mut scene, size, &theme, &text);

    let mut renderer = SoftwareRenderer::new(1080, 780);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("failed to encode PNG")?;
    std::fs::write(&out, png)?;
    println!(
        "saved {out} (1080x780, lang={lang}, step={})",
        step_name(step)
    );
    Ok(())
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Welcome => "welcome",
        Step::Language => "language",
        Step::Region => "region",
        Step::Network => "network",
        Step::Requirements => "requirements",
        Step::Disk => "disk",
        Step::Account => "account",
        Step::Summary => "summary",
        Step::Progress => "progress",
        Step::Done => "done",
    }
}
