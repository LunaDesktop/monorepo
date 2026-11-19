//! Headless-podglądy `luna-tasks` do dokumentacji (bez kompozytora).
//!
//! Uruchom: `cargo run --bin preview`
//! → `docs/images/tasks-{list,search,confirm,menu,about}.png`.

use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use libluna_core::{Rect, Size, Theme};
use libluna_i18n::{set_locale, Locale};
use libluna_render::{Scene, SoftwareRenderer, TextEngine};
use luna_tasks::app::{App, Confirm};
use luna_tasks::prefs::Prefs;
use luna_tasks::procs::{Process, Snapshot, SystemInfo};
use luna_tasks::ui;

const SIZE: Size = Size {
    width: 980.0,
    height: 700.0,
};

fn main() -> Result<(), Box<dyn Error>> {
    set_locale(Locale::new("pl"));

    let text = TextEngine::system();
    let dark = Theme::dark();
    let light = Theme::light();

    let mut list = demo_app();
    list.selected = Some(4210);
    save(&mut list, &dark, &text, "tasks-list.png")?;

    let mut search = demo_app();
    search.activate_search();
    search.search.set_text("web");
    search.recompute();
    search.selected = Some(4312);
    save(&mut search, &light, &text, "tasks-search.png")?;

    let mut charts = demo_app();
    charts.show_charts = true;
    charts.selected = Some(4210);
    seed_charts(&mut charts);
    save(&mut charts, &dark, &text, "tasks-charts.png")?;

    let mut menu = demo_app();
    menu.open_menu(ui::menu_anchor(SIZE, &dark, &text));
    save(&mut menu, &dark, &text, "tasks-menu.png")?;

    let mut context = demo_app();
    context.open_context(4210, Rect::from_xywh(300.0, 170.0, 1.0, 1.0));
    save(&mut context, &dark, &text, "tasks-context.png")?;

    let mut confirm = demo_app();
    confirm.selected = Some(4210);
    confirm.confirm = Some(Confirm {
        pid: 4210,
        start_jiffies: 900,
        name: "firefox".to_owned(),
        force: true,
    });
    save(&mut confirm, &dark, &text, "tasks-confirm.png")?;

    let mut about = demo_app();
    about.show_about();
    save(&mut about, &light, &text, "tasks-about.png")?;

    Ok(())
}

fn process(
    pid: u32,
    name: &str,
    user: &str,
    uid: u32,
    cpu: f32,
    rss_kib: u64,
    threads: u32,
) -> Process {
    Process {
        pid,
        ppid: 1,
        name: name.to_owned(),
        cmdline: format!("/usr/bin/{name} --session luna"),
        state: if cpu > 1.0 { 'R' } else { 'S' },
        uid,
        user: user.to_owned(),
        cpu_jiffies: 0,
        start_jiffies: 900,
        rss_kib,
        threads,
        cpu_percent: cpu,
    }
}

fn demo_app() -> App {
    let processes = vec![
        process(4210, "firefox", "patryk", 1000, 12.4, 3_600_000, 48),
        process(4312, "Web Content", "patryk", 1000, 6.8, 512_000, 22),
        process(4410, "luna-tasks", "patryk", 1000, 2.1, 48_000, 6),
        process(4477, "luna-shell", "patryk", 1000, 1.4, 96_000, 12),
        process(4021, "selene", "patryk", 1000, 3.6, 148_000, 14),
        process(1201, "pipewire", "patryk", 1000, 0.4, 12_000, 5),
        process(980, "NetworkManager", "root", 0, 0.3, 18_000, 3),
        process(760, "systemd-journald", "root", 0, 0.2, 24_000, 2),
        process(1, "systemd", "root", 0, 0.1, 14_000, 1),
    ];
    let snapshot = Snapshot {
        processes,
        system: SystemInfo {
            cpu_percent: 23.0,
            cpu_count: 8,
            memory_total_kib: 15_980_000,
            memory_available_kib: 9_640_000,
            uptime: Duration::from_secs(38_520),
            load: [0.82, 0.67, 0.54],
        },
        ..Snapshot::default()
    };
    let mut app = App::with_snapshot(snapshot, Prefs::default());
    // Długa ścieżka jak w realnym edytorze — podgląd pokazuje łamanie i clip.
    if let Some(firefox) = app
        .snapshot
        .processes
        .iter_mut()
        .find(|process| process.pid == 4210)
    {
        firefox.cmdline = "/home/patryks/.local/zed.app/libexec/zed-editor --existing zed-cli:///tmp/.tmp8y5mUY/socket".to_owned();
    }
    app.uid = 1000;
    app
}

fn seed_charts(app: &mut App) {
    for i in 0..60 {
        let t = i as f32;
        app.charts
            .system_cpu
            .push(0.30 + 0.16 * (t * 0.22).sin() + 0.05 * (t * 0.57).cos());
        app.charts
            .system_memory
            .push(0.36 + 0.03 * (t * 0.11).sin());
        app.charts
            .selected_cpu
            .push(0.10 + 0.07 * (t * 0.31).sin() + 0.03 * (t * 0.9).cos());
    }
    app.charts.pid = Some(4210);
}

fn save(app: &mut App, theme: &Theme, text: &TextEngine, name: &str) -> Result<(), Box<dyn Error>> {
    let mut scene = Scene::new();
    ui::paint(app, &mut scene, SIZE, theme, text);

    let width = SIZE.width.round() as u32;
    let height = SIZE.height.round() as u32;
    let mut renderer = SoftwareRenderer::new(width, height);
    renderer.draw(&scene, 1.0, theme.colors.window_bg);
    let png = renderer.encode_png().ok_or("nie udało się zakodować PNG")?;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/images")
        .join(name);
    std::fs::write(&path, png)?;
    println!("saved {}", path.display());
    Ok(())
}
