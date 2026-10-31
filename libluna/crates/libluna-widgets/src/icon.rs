//! Współdzielone ikony rysowane wektorowo (bez zależności od fontu).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use libluna_core::icons::{
    active_icon_set, active_icon_theme, find_icon_set, icon_search_dirs, IconSet,
};
use libluna_core::{Color, IconTheme, Point, Rect};
use libluna_render::wallpaper::decode_png;
use libluna_render::{decode_svg, ImageData, Scene};

use crate::ctx::PaintCtx;

/// Strzałka w dół (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_down(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size, center.y - size * 0.5),
        Point::new(center.x, center.y + size * 0.5),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x, center.y + size * 0.5),
        Point::new(center.x + size, center.y - size * 0.5),
        width,
        color,
    );
}

/// Strzałka w górę (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_up(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size, center.y + size * 0.5),
        Point::new(center.x, center.y - size * 0.5),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x, center.y - size * 0.5),
        Point::new(center.x + size, center.y + size * 0.5),
        width,
        color,
    );
}

/// Strzałka w prawo (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_right(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.45, center.y - size),
        Point::new(center.x + size * 0.45, center.y),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x + size * 0.45, center.y),
        Point::new(center.x - size * 0.45, center.y + size),
        width,
        color,
    );
}

/// Strzałka w lewo (chevron); środek w `center`, rozpiętość `size`, grubość `width`.
pub fn chevron_left(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    paint.scene.stroke_line(
        Point::new(center.x + size * 0.45, center.y - size),
        Point::new(center.x - size * 0.45, center.y),
        width,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.45, center.y),
        Point::new(center.x + size * 0.45, center.y + size),
        width,
        color,
    );
}

/// Lupka (wyszukiwanie); środek w `center`, bok `size`.
pub fn search(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    // Lupka: okrąg + rączka pod 45°. Wcześniejsza wersja rysowała okrąg
    // jako zaokrąglony prostokąt i zaczynała rączkę wewnątrz koła — ikona
    // wyglądała jak klucz i była przesunięta w lewo/górę.
    let r = size * 0.30;
    let c = Point::new(center.x - size * 0.06, center.y - size * 0.06);
    paint.scene.stroke_circle(c, r, 1.7, color);

    let d = std::f64::consts::FRAC_1_SQRT_2;
    let start = Point::new(c.x + r * d, c.y + r * d);
    let end = Point::new(c.x + (r + size * 0.22) * d, c.y + (r + size * 0.22) * d);
    // `stroke_polyline` ma zaokrąglone końce — rączka nie wygląda na uciętą.
    paint.scene.stroke_polyline(&[start, end], 1.7, color);
}

/// Ikona katalogu (folder) w prostokącie `box_`.
///
/// W stylu pastelowym folder jest maślany, z jaśniejszym paskiem u góry;
/// w trybie monochromatycznym zachowuje kolor przekazany przez wywołującego.
pub fn folder(paint: &mut PaintCtx, color: Color, box_: Rect) {
    // Zestaw zewnętrzny (np. Papirus albo instalowany Luna-Icons) ma własną
    // ikonę katalogu; dopiero gdy jej brak, rysujemy wektorowy folder Luny.
    if paint_named_icon(paint, Some("folder"), box_) {
        return;
    }
    let tab_h = box_.height() * 0.24;
    let tab_w = box_.width() * 0.46;
    let (fill, light) = if active_icon_theme().is_monochrome() {
        (color, color)
    } else {
        (PASTEL_BUTTER.base, PASTEL_BUTTER.soft)
    };
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(box_.x(), box_.y(), tab_w, tab_h * 1.6),
        tab_h * 0.4,
        fill,
    );
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(
            box_.x(),
            box_.y() + tab_h * 0.9,
            box_.width(),
            box_.height() - tab_h * 0.9,
        ),
        box_.width() * 0.1,
        fill,
    );
    if !active_icon_theme().is_monochrome() {
        // Delikatny rozświetlony pasek nad krawędzią katalogu.
        paint.scene.fill_rounded_rect(
            Rect::from_xywh(
                box_.x() + box_.width() * 0.06,
                box_.y() + box_.height() * 0.36,
                box_.width() * 0.5,
                (box_.height() * 0.07).max(1.0),
            ),
            box_.width() * 0.035,
            light,
        );
    }
}

/// Ikona pliku (kartka z zagiętym rogiem) w prostokącie `box_`.
pub fn file(paint: &mut PaintCtx, color: Color, box_: Rect) {
    // Ten sam mechanizm co dla katalogów: zewnętrzny zestaw może dostarczyć
    // ikonę ogólnego pliku (`text-x-generic`).
    if paint_named_icon(paint, Some("text-x-generic"), box_) {
        return;
    }
    let w = box_.width();
    let h = box_.height();
    let fold = w * 0.34;
    if active_icon_theme().is_monochrome() {
        paint.scene.stroke_rounded_rect(box_, w * 0.12, 1.8, color);
        paint.scene.stroke_line(
            Point::new(box_.right() - fold, box_.y()),
            Point::new(box_.right() - fold, box_.y() + fold),
            1.8,
            color,
        );
        paint.scene.stroke_line(
            Point::new(box_.right() - fold, box_.y() + fold),
            Point::new(box_.right(), box_.y() + fold),
            1.8,
            color,
        );
        return;
    }

    // Pastelowa kartka: jasny papier, błękitne zagięcie i cienki kontur.
    let page = [
        Point::new(box_.x(), box_.y()),
        Point::new(box_.right() - fold, box_.y()),
        Point::new(box_.right(), box_.y() + fold),
        Point::new(box_.right(), box_.bottom()),
        Point::new(box_.x(), box_.bottom()),
    ];
    paint.scene.fill_polygon(&page, PASTEL_PAPER.soft);
    paint.scene.fill_polygon(
        &[
            Point::new(box_.right() - fold, box_.y()),
            Point::new(box_.right(), box_.y() + fold),
            Point::new(box_.right() - fold, box_.y() + fold),
        ],
        PASTEL_SKY.base,
    );
    let mut outline = page.to_vec();
    outline.push(page[0]);
    paint
        .scene
        .stroke_polyline(&outline, (w * 0.09).max(1.0), PASTEL_PAPER.ink);
    paint.scene.stroke_line(
        Point::new(box_.right() - fold, box_.y() + fold),
        Point::new(box_.right() - fold, box_.y()),
        (w * 0.07).max(1.0),
        PASTEL_PAPER.ink,
    );
    // Dwie linie tekstu — kartka ma być rozpoznawalna także w 16 px.
    for t in [0.58, 0.74] {
        paint.scene.stroke_line(
            Point::new(box_.x() + w * 0.22, box_.y() + h * t),
            Point::new(box_.right() - w * 0.22, box_.y() + h * t),
            (h * 0.06).max(1.0),
            PASTEL_SKY.ink.scale_alpha(0.75),
        );
    }
}

/// Krzyżyk (zamknięcie); środek w `box_`, ramię `arm`.
pub fn close(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    paint.scene.stroke_line(
        Point::new(c.x - arm, c.y - arm),
        Point::new(c.x + arm, c.y + arm),
        2.0,
        color,
    );
    paint.scene.stroke_line(
        Point::new(c.x + arm, c.y - arm),
        Point::new(c.x - arm, c.y + arm),
        2.0,
        color,
    );
}

/// Pozioma kreska (minimalizacja).
pub fn minimize(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    paint.scene.stroke_line(
        Point::new(c.x - arm, c.y),
        Point::new(c.x + arm, c.y),
        2.0,
        color,
    );
}

/// Kwadrat (maksymalizacja).
pub fn maximize(paint: &mut PaintCtx, color: Color, box_: Rect, half: f64) {
    let c = box_.center();
    paint.scene.stroke_rounded_rect(
        Rect::from_xywh(c.x - half, c.y - half, half * 2.0, half * 2.0),
        2.0,
        2.0,
        color,
    );
}

/// Oko (podgląd hasła); środek w `center`, półszerokość `size`.
pub fn eye(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    use std::f64::consts::PI;
    let w = size;
    let h = size * 0.62;
    let n = 12;
    let mut prev = Point::new(center.x - w, center.y);
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let p = Point::new(center.x - w + 2.0 * w * t, center.y - h * (PI * t).sin());
        paint.scene.stroke_line(prev, p, 1.6, color);
        prev = p;
    }
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let p = Point::new(center.x - w + 2.0 * w * t, center.y + h * (PI * t).sin());
        paint.scene.stroke_line(prev, p, 1.6, color);
        prev = p;
    }
    let r = size * 0.22;
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(center.x - r, center.y - r, r * 2.0, r * 2.0),
        r,
        color,
    );
}

/// Oko przekreślone (hasło ukryte).
pub fn eye_off(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    eye(paint, color, center, size);
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.9, center.y + size * 0.9),
        Point::new(center.x + size * 0.9, center.y - size * 0.9),
        1.6,
        color,
    );
}

/// Kłódka (pole hasła); środek w `center`, bok `size`.
pub fn lock(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    use std::f64::consts::PI;
    let body = Rect::from_xywh(
        center.x - size * 0.42,
        center.y - size * 0.02,
        size * 0.84,
        size * 0.62,
    );
    paint
        .scene
        .stroke_rounded_rect(body, size * 0.10, 1.6, color);
    // Pałąk: górna półkola + krótkie pionowe odcinki do korpusu.
    let r = size * 0.28;
    let top = center.y - size * 0.18;
    let mut prev = Point::new(center.x - r, top + r * 0.20);
    for i in 1..=16 {
        let t = i as f64 / 16.0;
        let a = PI + PI * t;
        let p = Point::new(center.x + r * a.cos(), top + r * a.sin());
        paint.scene.stroke_line(prev, p, 1.6, color);
        prev = p;
    }
    paint.scene.stroke_line(
        Point::new(center.x - r, top + r * 0.20),
        Point::new(center.x - r, top + r * 0.55),
        1.6,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x + r, top + r * 0.20),
        Point::new(center.x + r, top + r * 0.55),
        1.6,
        color,
    );
    paint.scene.fill_rounded_rect(
        Rect::from_xywh(
            center.x - size * 0.07,
            center.y + size * 0.18,
            size * 0.14,
            size * 0.22,
        ),
        size * 0.07,
        color,
    );
}

/// Koło zębate (ustawienia); środek w `center`, bok `size`.
pub fn gear(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    use std::f64::consts::TAU;
    // Proporcje dobrane do siatki 24×24: całe koło mieści się w bezpiecznym
    // obszarze ~2..22, tak jak pozostałe ikony zestawu.
    let ring = size * 0.26;
    let tooth_in = size * 0.23;
    let tooth_out = size * 0.42;
    for k in 0..8 {
        let a = k as f64 * TAU / 8.0;
        let (c, s) = (a.cos(), a.sin());
        let (px, py) = (-s, c);
        let (base, tip) = (size * 0.105, size * 0.065);
        paint.scene.fill_polygon(
            &[
                Point::new(
                    center.x + tooth_in * c - base * px,
                    center.y + tooth_in * s - base * py,
                ),
                Point::new(
                    center.x + tooth_out * c - tip * px,
                    center.y + tooth_out * s - tip * py,
                ),
                Point::new(
                    center.x + tooth_out * c + tip * px,
                    center.y + tooth_out * s + tip * py,
                ),
                Point::new(
                    center.x + tooth_in * c + base * px,
                    center.y + tooth_in * s + base * py,
                ),
            ],
            color,
        );
    }
    paint.scene.stroke_circle(center, ring, size * 0.105, color);
}

/// Restart (strzałka w kółko); środek w `center`, bok `size`.
pub fn restart(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    use std::f64::consts::PI;
    let r = size * 0.50;
    let start = -0.55 * PI;
    let end = 1.10 * PI;
    let n = 28;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let a = start + (end - start) * t;
        pts.push(Point::new(center.x + r * a.cos(), center.y + r * a.sin()));
    }
    paint.scene.stroke_polyline(&pts, 1.8, color);
    // Grocik na końcu łuku.
    let a = end;
    let tip = Point::new(center.x + r * a.cos(), center.y + r * a.sin());
    let tangent = Point::new(-a.sin(), a.cos());
    let normal = Point::new(a.cos(), a.sin());
    let back = Point::new(
        tip.x - tangent.x * size * 0.20,
        tip.y - tangent.y * size * 0.20,
    );
    paint.scene.stroke_line(
        tip,
        Point::new(
            back.x + normal.x * size * 0.14,
            back.y + normal.y * size * 0.14,
        ),
        1.8,
        color,
    );
    paint.scene.stroke_line(
        tip,
        Point::new(
            back.x - normal.x * size * 0.14,
            back.y - normal.y * size * 0.14,
        ),
        1.8,
        color,
    );
}

/// Tarcza (tryb awaryjny); środek w `center`, bok `size`.
pub fn shield(paint: &mut PaintCtx, color: Color, center: Point, size: f64) {
    let left = center.x - size * 0.42;
    let right = center.x + size * 0.42;
    let top = center.y - size * 0.50;
    let mid = center.y + size * 0.05;
    let bottom = center.y + size * 0.50;
    let pts = [
        Point::new(center.x, top),
        Point::new(right, top + size * 0.16),
        Point::new(right, mid),
        Point::new(center.x, bottom),
        Point::new(left, mid),
        Point::new(left, top + size * 0.16),
        Point::new(center.x, top),
    ];
    paint.scene.stroke_polyline(&pts, 1.6, color);
    // Mały „check” w środku tarczy.
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.16, center.y + size * 0.02),
        Point::new(center.x - size * 0.03, center.y + size * 0.15),
        1.6,
        color,
    );
    paint.scene.stroke_line(
        Point::new(center.x - size * 0.03, center.y + size * 0.15),
        Point::new(center.x + size * 0.20, center.y - size * 0.14),
        1.6,
        color,
    );
}

/// Trzy poziome kreski (menu „hamburger”); środek w `center`, szerokość `size`, grubość `width`.
pub fn burger(paint: &mut PaintCtx, color: Color, center: Point, size: f64, width: f64) {
    for i in -1..=1 {
        let y = center.y + i as f64 * size * 0.42;
        paint.scene.stroke_line(
            Point::new(center.x - size, y),
            Point::new(center.x + size, y),
            width,
            color,
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Nazwany pakiet ikon symbolicznych — rysowany wektorowo, w stylu Luny.
// ─────────────────────────────────────────────────────────────────────────────

/// Nazwa ikony z pakietu Luny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconName {
    Menu,
    Search,
    Files,
    Settings,
    Terminal,
    Music,
    Star,
    Clock,
    Store,
    Edit,
    Wifi,
    Volume,
    Battery,
    Power,
    Grid,
    List,
    Carousel,
    ChevronLeft,
    ChevronRight,
    Close,
    Minimize,
    Maximize,
    Check,
    Plus,
    Home,
    User,
    Info,
    App,
    Brightness,
    // Ikony aplikacji (taskbar/launcher) — spójny zestaw wektorowy Luny.
    Globe,
    Mail,
    Chat,
    Code,
    Image,
    Video,
    Document,
    Calculator,
    Game,
    Archive,
    Printer,
    Bluetooth,
    Monitor,
    Screenshot,
    Sliders,
    Play,
    Pause,
    /// Okrągła strzałka — odświeżanie / ponowne wczytanie.
    Refresh,
    Moon,
    /// Tryb automatyczny (systemowy) — pół słońca, pół księżyca.
    Auto,
    /// Pinezka — „przypnij do launchera”.
    Pin,
    /// Mikrofon — wskaźnik bezpieczeństwa (aplikacja używa mikrofonu).
    Microphone,
    /// Aparat — wskaźnik bezpieczeństwa (aplikacja używa kamery).
    Camera,
}

impl IconName {
    /// Wszystkie ikony z pakietu (kolejność = kolejność w galerii).
    pub const ALL: [IconName; 52] = [
        IconName::Menu,
        IconName::Search,
        IconName::Files,
        IconName::Settings,
        IconName::Terminal,
        IconName::Music,
        IconName::Star,
        IconName::Clock,
        IconName::Store,
        IconName::Edit,
        IconName::Wifi,
        IconName::Volume,
        IconName::Battery,
        IconName::Power,
        IconName::Grid,
        IconName::List,
        IconName::Carousel,
        IconName::ChevronLeft,
        IconName::ChevronRight,
        IconName::Close,
        IconName::Minimize,
        IconName::Maximize,
        IconName::Check,
        IconName::Plus,
        IconName::Home,
        IconName::User,
        IconName::Info,
        IconName::App,
        IconName::Brightness,
        IconName::Globe,
        IconName::Mail,
        IconName::Chat,
        IconName::Code,
        IconName::Image,
        IconName::Video,
        IconName::Document,
        IconName::Calculator,
        IconName::Game,
        IconName::Archive,
        IconName::Printer,
        IconName::Bluetooth,
        IconName::Monitor,
        IconName::Screenshot,
        IconName::Sliders,
        IconName::Play,
        IconName::Pause,
        IconName::Refresh,
        IconName::Moon,
        IconName::Auto,
        IconName::Pin,
        IconName::Microphone,
        IconName::Camera,
    ];

    /// Nazwa ikony (do galerii/diagnostyki).
    pub fn name(self) -> &'static str {
        match self {
            IconName::Menu => "Menu",
            IconName::Search => "Search",
            IconName::Files => "Files",
            IconName::Settings => "Settings",
            IconName::Terminal => "Terminal",
            IconName::Music => "Music",
            IconName::Star => "Star",
            IconName::Clock => "Clock",
            IconName::Store => "Store",
            IconName::Edit => "Edit",
            IconName::Wifi => "Wifi",
            IconName::Volume => "Volume",
            IconName::Battery => "Battery",
            IconName::Power => "Power",
            IconName::Grid => "Grid",
            IconName::List => "List",
            IconName::Carousel => "Carousel",
            IconName::ChevronLeft => "ChevronLeft",
            IconName::ChevronRight => "ChevronRight",
            IconName::Close => "Close",
            IconName::Minimize => "Minimize",
            IconName::Maximize => "Maximize",
            IconName::Check => "Check",
            IconName::Plus => "Plus",
            IconName::Home => "Home",
            IconName::User => "User",
            IconName::Info => "Info",
            IconName::App => "App",
            IconName::Brightness => "Brightness",
            IconName::Globe => "Globe",
            IconName::Mail => "Mail",
            IconName::Chat => "Chat",
            IconName::Code => "Code",
            IconName::Image => "Image",
            IconName::Video => "Video",
            IconName::Document => "Document",
            IconName::Calculator => "Calculator",
            IconName::Game => "Game",
            IconName::Archive => "Archive",
            IconName::Printer => "Printer",
            IconName::Bluetooth => "Bluetooth",
            IconName::Monitor => "Monitor",
            IconName::Screenshot => "Screenshot",
            IconName::Sliders => "Sliders",
            IconName::Play => "Play",
            IconName::Pause => "Pause",
            IconName::Refresh => "Refresh",
            IconName::Moon => "Moon",
            IconName::Auto => "Auto",
            IconName::Pin => "Pin",
            IconName::Microphone => "Microphone",
            IconName::Camera => "Camera",
        }
    }
}

/// Współrzędne w siatce 24×24 mapowane na prostokąt docelowy.
struct Grid {
    ox: f64,
    oy: f64,
    s: f64,
}

impl Grid {
    fn new(rect: Rect) -> Self {
        let side = rect.width().min(rect.height()).max(1.0);
        let s = side / 24.0;
        Self {
            ox: rect.x() + (rect.width() - 24.0 * s) / 2.0,
            oy: rect.y() + (rect.height() - 24.0 * s) / 2.0,
            s,
        }
    }

    fn p(&self, x: f64, y: f64) -> Point {
        Point::new(self.ox + x * self.s, self.oy + y * self.s)
    }

    fn d(&self, v: f64) -> f64 {
        v * self.s
    }
}

/// Punkty łuku (środek `c`, promień `r`, kąty w stopniach, y w dół).
fn arc(c: Point, r: f64, a0: f64, a1: f64, segs: usize) -> Vec<Point> {
    (0..=segs)
        .map(|i| {
            let t = a0 + (a1 - a0) * i as f64 / segs as f64;
            let a = t.to_radians();
            Point::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// Grocik strzałki odświeżania na końcu łuku (współrzędne siatki 24×24).
///
/// `angle` to kąt końca łuku w stopniach; `forward` to długość grocika wzdłuż
/// kierunku obrotu, a `wing` — połowa rozpiętości jego podstawy. Zwraca trzy
/// punkty trójkąta: skrzydło–tip–skrzydło. Grocik leży **za** końcem łuku, dzięki
/// czemu czyta się jak strzałka, a nie jak zadzior.
fn refresh_arrowhead(
    cx: f64,
    cy: f64,
    r: f64,
    angle: f64,
    forward: f64,
    wing: f64,
) -> [(f64, f64); 3] {
    let a = angle.to_radians();
    let (sin, cos) = (a.sin(), a.cos());
    let base = (cx + r * cos, cy + r * sin);
    let tangent = (-sin, cos);
    let normal = (cos, sin);
    let tip = (base.0 + tangent.0 * forward, base.1 + tangent.1 * forward);
    [
        (base.0 + normal.0 * wing, base.1 + normal.1 * wing),
        tip,
        (base.0 - normal.0 * wing, base.1 - normal.1 * wing),
    ]
}

/// Rysuje ikonę z pakietu w prostokącie `rect` w zadanym kolorze (styl symboliczny).
/// Nazwy freedesktop dla ikon, które mają sens w zewnętrznym zestawie.
///
/// Świadomie pomijamy kontrolki UI (menu, strzałki, „×”, „+”), żeby zmiana zestawu
/// nie psuła spójności paska i przycisków — te pozostają wektorowe.
fn external_names(name: IconName) -> &'static [&'static str] {
    match name {
        IconName::Files => &["system-file-manager", "folder", "user-home"],
        IconName::Settings => &["preferences-system", "preferences-desktop"],
        IconName::Terminal => &["utilities-terminal", "terminal"],
        IconName::Music => &["audio-x-generic", "multimedia-audio-player"],
        IconName::Star => &["emblem-favorite", "starred"],
        IconName::Clock => &["preferences-system-time", "appointment-new", "clock"],
        IconName::Store => &["system-software-install", "store"],
        IconName::Edit => &["accessories-text-editor", "text-editor"],
        IconName::Wifi => &["network-wireless", "network-wireless-signal-excellent"],
        IconName::Volume => &["audio-volume-high", "audio-speakers"],
        IconName::Battery => &["battery-good", "battery-full"],
        IconName::Brightness => &["display-brightness", "weather-clear", "brightness"],
        IconName::Power => &["system-shutdown", "system-log-out"],
        IconName::Home => &["user-home", "folder-home"],
        IconName::User => &["system-users", "user-info"],
        IconName::Info => &["help-about", "dialog-information"],
        IconName::Globe => &["applications-internet", "web-browser"],
        IconName::Mail => &["internet-mail", "mail-message-new"],
        IconName::Chat => &["internet-chat", "user-available"],
        IconName::Code => &["applications-development", "text-x-script"],
        IconName::Image => &["image-x-generic", "image-viewer"],
        IconName::Video => &["video-x-generic", "video-player"],
        IconName::Document => &["text-x-generic", "x-office-document"],
        IconName::Calculator => &["accessories-calculator"],
        IconName::Game => &["applications-games", "input-gaming"],
        IconName::Archive => &["package-x-generic"],
        IconName::Printer => &["printer"],
        IconName::Bluetooth => &["bluetooth", "bluetooth-active"],
        IconName::Monitor => &["video-display", "computer"],
        IconName::Screenshot => &["applets-screenshooter", "camera-photo"],
        // Quick Panel ma własną ikonę suwaków — nie może wyglądać jak Ustawienia.
        IconName::Sliders => &["preferences-other", "applications-system"],
        IconName::Play => &["media-playback-start"],
        IconName::Pause => &["media-playback-pause"],
        IconName::Moon => &["weather-clear-night", "night-light"],
        _ => &[],
    }
}

static SET_CACHE: OnceLock<Mutex<HashMap<String, Option<IconSet>>>> = OnceLock::new();
static NAMED_PATH_CACHE: OnceLock<Mutex<HashMap<(String, String, u32), Option<PathBuf>>>> =
    OnceLock::new();
static NAMED_SVG_CACHE: OnceLock<Mutex<HashMap<(String, String), Option<PathBuf>>>> =
    OnceLock::new();
static FALLBACK_PATH_CACHE: OnceLock<Mutex<HashMap<(String, u32), Option<PathBuf>>>> =
    OnceLock::new();
static FALLBACK_SVG_CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
static IMAGE_CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<Arc<ImageData>>>>> = OnceLock::new();
static SVG_IMAGE_CACHE: OnceLock<Mutex<HashMap<(PathBuf, u32), Option<Arc<ImageData>>>>> =
    OnceLock::new();

fn cached_set(name: &str) -> Option<IconSet> {
    let cache = SET_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(name) {
            return hit.clone();
        }
    }
    let set = find_icon_set(name);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(name.to_owned(), set.clone());
    }
    set
}

fn cached_image(path: &std::path::Path) -> Option<Arc<ImageData>> {
    let cache = IMAGE_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(path) {
            return hit.clone();
        }
    }
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| decode_png(&bytes))
        .map(Arc::new);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(path.to_path_buf(), image.clone());
    }
    image
}

/// Ikona z zewnętrznego zestawu (PNG albo SVG) dopasowana do `rect`
/// z zachowaniem proporcji.
fn external_icon(name: IconName, rect: Rect) -> Option<(Arc<ImageData>, Rect)> {
    let names = external_names(name);
    if names.is_empty() || active_icon_set().is_none() {
        return None;
    }
    let target = rect.width().min(rect.height()).max(1.0).ceil() as u32;
    let image = named_image_multi(names, target)?;
    let (w, h) = (image.width as f64, image.height as f64);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let side = rect.width().min(rect.height());
    let scale = side / w.max(h);
    let target = Rect::from_xywh(
        rect.center().x - w * scale / 2.0,
        rect.center().y - h * scale / 2.0,
        w * scale,
        h * scale,
    );
    Some((image, target))
}

/// Rysuje ikonę z aktywnego zestawu zewnętrznego, jeśli istnieje.
fn paint_external(paint: &mut PaintCtx, name: IconName, rect: Rect) -> bool {
    let Some((image, target)) = external_icon(name, rect) else {
        return false;
    };
    paint.scene.image(target, image);
    true
}

/// Szuka pliku PNG dla dowolnej nazwy freedesktop (np. `firefox`, `org.gnome.Nautilus`).
/// Usuwa rozszerzenie z `Icon=` — zestawy freedesktop przechowują samą nazwę.
fn icon_base_name(name: &str) -> Option<&str> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(
        name.strip_suffix(".png")
            .or_else(|| name.strip_suffix(".svg"))
            .or_else(|| name.strip_suffix(".xpm"))
            .unwrap_or(name),
    )
}

fn cached_named_path(set: &IconSet, name: &str, target: u32) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let direct = std::path::Path::new(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let Some(base) = icon_base_name(name) else {
        return None;
    };
    let key = (set.name.clone(), base.to_owned(), target);
    let cache = NAMED_PATH_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let path = set.find_png_sized(&[base], target as f64);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, path.clone());
    }
    path
}

/// Plik SVG z aktywnego zestawu (np. Papirus trzyma SVG w katalogach rozmiarów).
fn cached_named_svg(set: &IconSet, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let direct = std::path::Path::new(name);
    if direct.is_absolute() && direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let Some(base) = icon_base_name(name) else {
        return None;
    };
    let key = (set.name.clone(), base.to_owned());
    let cache = NAMED_SVG_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let path = set.find_svg(&[base]);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, path.clone());
    }
    path
}

/// Kolejność systemowych zestawów przy szukaniu oryginalnej ikony aplikacji.
///
/// `hicolor` jest bazą, do której aplikacje instalują swoje ikony, więc ma
/// pierwszeństwo. Motywy o wysokim kontraście pomijamy — ich czarno-białe
/// wersje wyglądają w kolorowym launcherze gorzej niż wektorowa ikona Luny.
fn fallback_rank(path: &std::path::Path) -> (u8, String) {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let rank = match name.as_str() {
        "hicolor" => 0,
        "adwaita" => 1,
        _ => 2,
    };
    (rank, name)
}

fn is_high_contrast(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().starts_with("highcontrast"))
}

/// Katalogi z ikonami „luźnymi” (poza strukturą motywu).
fn pixmap_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/pixmaps"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    for dir in data_dirs {
        dirs.push(dir.join("pixmaps"));
    }
    dirs
}

/// Szuka ikony w **innych** zestawach niż aktywny (oryginalne ikony aplikacji).
///
/// To jest świadomy fallback: gdy `Luna-Icons` nie ma wpisu dla np. `gparted`,
/// bierzemy grafikę dostarczoną przez samą aplikację do `hicolor` (albo innego
/// zainstalowanego motywu), zamiast rysować ogólną ikonę wektorową.
fn fallback_sets() -> Vec<PathBuf> {
    let mut sets: Vec<PathBuf> = Vec::new();
    for root in icon_search_dirs() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !is_high_contrast(&path) {
                sets.push(path);
            }
        }
    }
    sets
}

fn find_fallback_path(name: &str, target: f64) -> Option<PathBuf> {
    find_fallback_path_in(fallback_sets(), &pixmap_dirs(), name, target)
}

/// Jak [`find_fallback_path`], ale dla SVG (Adwaita Demo, GTK Demo, Papirus…).
fn find_fallback_svg(name: &str) -> Option<PathBuf> {
    find_fallback_svg_in(fallback_sets(), &pixmap_dirs(), name)
}

fn find_fallback_svg_in(
    mut sets: Vec<PathBuf>,
    pixmaps: &[PathBuf],
    name: &str,
) -> Option<PathBuf> {
    sets.retain(|path| !is_high_contrast(path));
    sets.sort_by_cached_key(|path| fallback_rank(path));
    sets.dedup();

    for path in &sets {
        let set = IconSet {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            path: path.clone(),
            comment: None,
        };
        if let Some(found) = set.find_svg(&[name]) {
            return Some(found);
        }
    }

    for dir in pixmaps {
        let path = dir.join(format!("{name}.svg"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Czysta część wyszukiwania — łatwa do testowania bez systemowych katalogów.
fn find_fallback_path_in(
    mut sets: Vec<PathBuf>,
    pixmaps: &[PathBuf],
    name: &str,
    target: f64,
) -> Option<PathBuf> {
    sets.retain(|path| !is_high_contrast(path));
    sets.sort_by_cached_key(|path| fallback_rank(path));
    sets.dedup();

    for path in &sets {
        let set = IconSet {
            name: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned(),
            path: path.clone(),
            comment: None,
        };
        if let Some(found) = set.find_png_sized(&[name], target) {
            return Some(found);
        }
    }

    for dir in pixmaps {
        let path = dir.join(format!("{name}.png"));
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Jak [`find_fallback_path`], ale z pamięcią podręczną (rysowanie panelu
/// nie może skanować katalogów ikon w każdej klatce).
fn cached_fallback_path(name: &str, target: u32) -> Option<PathBuf> {
    let Some(base) = icon_base_name(name) else {
        return None;
    };
    let key = (base.to_owned(), target);
    let cache = FALLBACK_PATH_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let path = find_fallback_path(base, target as f64);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, path.clone());
    }
    path
}

fn cached_fallback_svg(name: &str) -> Option<PathBuf> {
    let Some(base) = icon_base_name(name) else {
        return None;
    };
    let key = base.to_owned();
    let cache = FALLBACK_SVG_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let path = find_fallback_svg(base);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, path.clone());
    }
    path
}

/// Rasteryzuje SVG raz i trzyma gotowy obraz w pamięci.
///
/// Renderujemy w rozmiarze co najmniej 64 px (nawet dla ikony 16 px), żeby
/// krawędzie były gładkie po zmniejszeniu. Cache jest wspólny dla wszystkich
/// rozmiarów wyświetlania.
fn cached_svg_image(path: &std::path::Path, target: u32) -> Option<Arc<ImageData>> {
    let render = target.clamp(64, 256);
    let key = (path.to_path_buf(), render);
    let cache = SVG_IMAGE_CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock() {
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
    }
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| decode_svg(&bytes, render, render))
        .map(Arc::new);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, image.clone());
    }
    image
}

fn image_at_path(path: &std::path::Path, target: u32) -> Option<Arc<ImageData>> {
    let is_svg = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"));
    if is_svg {
        cached_svg_image(path, target)
    } else {
        cached_image(path)
    }
}

/// Obraz ikony: aktywny zestaw (PNG → SVG), potem systemowe motywy (PNG → SVG).
fn named_image_multi(names: &[&str], target: u32) -> Option<Arc<ImageData>> {
    for name in names {
        let direct = std::path::Path::new(name);
        if direct.is_absolute() && direct.is_file() {
            if let Some(image) = image_at_path(direct, target) {
                return Some(image);
            }
        }
    }

    if let Some(set) = active_icon_set().and_then(|name| cached_set(&name)) {
        for name in names {
            if let Some(path) = cached_named_path(&set, name, target) {
                if let Some(image) = cached_image(&path) {
                    return Some(image);
                }
            }
        }
        for name in names {
            if let Some(path) = cached_named_svg(&set, name) {
                if let Some(image) = cached_svg_image(&path, target) {
                    return Some(image);
                }
            }
        }
    }

    for name in names {
        if let Some(path) = cached_fallback_path(name, target) {
            if let Some(image) = cached_image(&path) {
                return Some(image);
            }
        }
    }
    for name in names {
        if let Some(path) = cached_fallback_svg(name) {
            if let Some(image) = cached_svg_image(&path, target) {
                return Some(image);
            }
        }
    }
    None
}

fn named_image(name: &str, target: u32) -> Option<Arc<ImageData>> {
    named_image_multi(&[name], target)
}

/// Rysuje ikonę aplikacji po jej nazwie z `.desktop` (zestaw zewnętrzny).
///
/// Najpierw szuka w aktywnym zestawie (np. `Luna-Icons`), a gdy go tam nie ma —
/// w oryginalnych ikonach systemowych (`hicolor`, inne motywy, `pixmaps`).
/// Obsługuje PNG i SVG (to drugie rasteryzuje `libluna-render`).
/// Zwraca `false`, gdy brak aktywnego zestawu lub grafiki — wywołujący powinien
/// wtedy narysować ikonę wektorową Luny.
pub fn paint_named_icon(paint: &mut PaintCtx, name: Option<&str>, rect: Rect) -> bool {
    // Tryb monochromatyczny rysuje cały interfejs jednym kolorem — nie mieszamy
    // wtedy kolorowych PNG z zewnętrznych zestawów.
    if active_icon_theme().is_monochrome() {
        return false;
    }
    let Some(name) = name else {
        return false;
    };
    // Brak aktywnego zestawu = świadomy tryb „tylko wektorowe” (icon_set = none).
    if active_icon_set().is_none() {
        return false;
    }
    let target = rect.width().min(rect.height()).max(1.0).ceil() as u32;
    // Najpierw wybrany zestaw (np. Luna-Icons), potem oryginalna ikona aplikacji
    // z systemowych motywów (PNG albo SVG) — dzięki temu nieznane aplikacje
    // nie dostają „łatki”.
    let Some(image) = named_image(name, target) else {
        return false;
    };
    let (w, h) = (image.width as f64, image.height as f64);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let side = rect.width().min(rect.height());
    let scale = side / w.max(h);
    let target = Rect::from_xywh(
        rect.center().x - w * scale / 2.0,
        rect.center().y - h * scale / 2.0,
        w * scale,
        h * scale,
    );
    paint.scene.image(target, image);
    true
}

pub fn paint_icon(paint: &mut PaintCtx, name: IconName, rect: Rect, color: Color) {
    paint_icon_themed(paint, name, rect, color, active_icon_theme());
}

/// Jak [`paint_icon`], ale ze wskazanym stylem ikon (pastelowy / monochromatyczny).
pub fn paint_icon_themed(
    paint: &mut PaintCtx,
    name: IconName,
    rect: Rect,
    color: Color,
    theme: IconTheme,
) {
    // Zestaw zewnętrzny (np. Papirus) jest globalny — ma pierwszeństwo dla ikon
    // aplikacji i miejsc; kontrolki UI pozostają wektorowe. W trybie
    // monochromatycznym świadomie go pomijamy, żeby cały interfejs był jednokolorowy.
    if !theme.is_monochrome() && paint_external(paint, name, rect) {
        return;
    }
    let g = Grid::new(rect);
    if !theme.is_monochrome() && has_pastel_art(name) {
        paint_pastel(paint, name, &g);
        return;
    }
    // Zębate koło ustawień rysujemy wspólnym helperem — dokładnie ten sam
    // kształt trafia do pastelowego pakietu ikon i na pasek.
    if name == IconName::Settings {
        gear(paint, color, g.p(12.0, 12.0), g.d(24.0));
        return;
    }
    let w = g.d(2.0);
    let scene = &mut *paint.scene;

    match name {
        // Obsłużone wcześniej wspólnym helperem `gear` (ten sam kształt co pasek).
        IconName::Settings => {}
        IconName::Menu => {
            for y in [6.5, 12.0, 17.5] {
                scene.stroke_line(g.p(4.0, y), g.p(20.0, y), w, color);
            }
        }
        IconName::Search => {
            scene.stroke_circle(g.p(10.5, 10.5), g.d(6.5), w, color);
            scene.stroke_line(g.p(15.3, 15.3), g.p(20.5, 20.5), w, color);
        }
        IconName::Files => {
            scene.fill_polygon(
                &[
                    g.p(3.0, 6.5),
                    g.p(9.0, 6.5),
                    g.p(11.0, 8.8),
                    g.p(21.0, 8.8),
                    g.p(21.0, 19.5),
                    g.p(3.0, 19.5),
                ],
                color,
            );
        }
        IconName::Terminal => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 4.5).x, g.p(3.0, 4.5).y, g.d(18.0), g.d(15.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.stroke_polyline(&[g.p(7.0, 10.0), g.p(9.5, 12.0), g.p(7.0, 14.0)], w, color);
            scene.stroke_line(g.p(12.0, 15.0), g.p(16.0, 15.0), w, color);
        }
        IconName::Music => {
            scene.stroke_circle(g.p(9.5, 17.0), g.d(2.6), w, color);
            scene.stroke_polyline(&[g.p(12.0, 17.0), g.p(12.0, 6.5), g.p(17.0, 8.0)], w, color);
        }
        IconName::Star => {
            let pts: Vec<Point> = (0..10)
                .map(|i| {
                    let a = (-90.0 + i as f64 * 36.0).to_radians();
                    let r = if i % 2 == 0 { 9.0 } else { 3.8 };
                    g.p(12.0 + r * a.cos(), 12.5 + r * a.sin())
                })
                .collect();
            scene.fill_polygon(&pts, color);
        }
        IconName::Clock => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(8.0), w, color);
            scene.stroke_polyline(&[g.p(12.0, 12.0), g.p(12.0, 7.5)], w, color);
            scene.stroke_polyline(&[g.p(12.0, 12.0), g.p(16.0, 13.8)], w, color);
        }
        IconName::Store => {
            scene.fill_polygon(
                &[
                    g.p(5.5, 8.0),
                    g.p(18.5, 8.0),
                    g.p(17.5, 20.0),
                    g.p(6.5, 20.0),
                ],
                color,
            );
            scene.stroke_polyline(
                &[g.p(9.0, 8.0), g.p(9.0, 6.0), g.p(15.0, 6.0), g.p(15.0, 8.0)],
                g.d(1.8),
                color,
            );
        }
        IconName::Edit => {
            scene.fill_polygon(
                &[
                    g.p(15.5, 3.5),
                    g.p(20.5, 8.5),
                    g.p(9.0, 20.0),
                    g.p(4.0, 21.0),
                    g.p(5.0, 16.0),
                ],
                color,
            );
        }
        IconName::Wifi => {
            scene.fill_circle(g.p(12.0, 18.0), g.d(1.4), color);
            for r in [4.0, 7.0, 10.2] {
                scene.stroke_polyline(
                    &arc(g.p(12.0, 18.0), g.d(r), 200.0, 340.0, 10),
                    g.d(1.9),
                    color,
                );
            }
        }
        IconName::Volume => {
            scene.fill_polygon(
                &[
                    g.p(3.0, 9.5),
                    g.p(7.0, 9.5),
                    g.p(11.0, 5.0),
                    g.p(11.0, 19.0),
                    g.p(7.0, 14.5),
                    g.p(3.0, 14.5),
                ],
                color,
            );
            for r in [5.0, 8.0] {
                scene.stroke_polyline(
                    &arc(g.p(11.0, 12.0), g.d(r), -50.0, 50.0, 8),
                    g.d(1.9),
                    color,
                );
            }
        }
        IconName::Battery => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 7.0).x, g.p(3.0, 7.0).y, g.d(16.0), g.d(10.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(19.5, 10.5).x, g.p(19.5, 10.5).y, g.d(2.5), g.d(3.0)),
                g.d(1.0),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(5.0, 9.5).x, g.p(5.0, 9.5).y, g.d(9.0), g.d(5.0)),
                g.d(1.2),
                color,
            );
        }
        IconName::Power => {
            scene.stroke_polyline(&arc(g.p(12.0, 12.0), g.d(7.5), -60.0, 240.0, 28), w, color);
            scene.stroke_line(g.p(12.0, 3.5), g.p(12.0, 12.0), w, color);
        }
        IconName::Grid => {
            for (x, y) in [(4.0, 4.0), (13.5, 4.0), (4.0, 13.5), (13.5, 13.5)] {
                scene.fill_rounded_rect(
                    Rect::from_xywh(g.p(x, y).x, g.p(x, y).y, g.d(6.5), g.d(6.5)),
                    g.d(1.6),
                    color,
                );
            }
        }
        IconName::List => {
            for y in [6.5, 12.0, 17.5] {
                scene.fill_circle(g.p(5.5, y), g.d(1.4), color);
                scene.stroke_line(g.p(9.5, y), g.p(19.5, y), g.d(1.8), color);
            }
        }
        IconName::Carousel => {
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(3.0, 8.5).x, g.p(3.0, 8.5).y, g.d(3.2), g.d(7.0)),
                g.d(1.2),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(17.8, 8.5).x, g.p(17.8, 8.5).y, g.d(3.2), g.d(7.0)),
                g.d(1.2),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(8.0, 6.0).x, g.p(8.0, 6.0).y, g.d(8.0), g.d(12.0)),
                g.d(1.6),
                color,
            );
        }
        IconName::ChevronLeft => {
            scene.stroke_polyline(
                &[g.p(14.5, 6.0), g.p(8.5, 12.0), g.p(14.5, 18.0)],
                g.d(2.4),
                color,
            );
        }
        IconName::ChevronRight => {
            scene.stroke_polyline(
                &[g.p(9.5, 6.0), g.p(15.5, 12.0), g.p(9.5, 18.0)],
                g.d(2.4),
                color,
            );
        }
        IconName::Close => {
            scene.stroke_line(g.p(6.5, 6.5), g.p(17.5, 17.5), w, color);
            scene.stroke_line(g.p(17.5, 6.5), g.p(6.5, 17.5), w, color);
        }
        IconName::Minimize => {
            scene.stroke_line(g.p(5.0, 12.0), g.p(19.0, 12.0), w, color);
        }
        IconName::Maximize => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(5.0, 5.0).x, g.p(5.0, 5.0).y, g.d(14.0), g.d(14.0)),
                g.d(1.5),
                w,
                color,
            );
        }
        IconName::Check => {
            scene.stroke_polyline(
                &[g.p(5.0, 12.5), g.p(10.0, 17.5), g.p(19.0, 6.5)],
                g.d(2.2),
                color,
            );
        }
        IconName::Plus => {
            scene.stroke_line(g.p(12.0, 5.0), g.p(12.0, 19.0), w, color);
            scene.stroke_line(g.p(5.0, 12.0), g.p(19.0, 12.0), w, color);
        }
        IconName::Home => {
            scene.fill_polygon(
                &[
                    g.p(12.0, 3.5),
                    g.p(21.0, 11.5),
                    g.p(18.5, 11.5),
                    g.p(18.5, 20.0),
                    g.p(13.5, 20.0),
                    g.p(13.5, 15.0),
                    g.p(10.5, 15.0),
                    g.p(10.5, 20.0),
                    g.p(5.5, 20.0),
                    g.p(5.5, 11.5),
                    g.p(3.0, 11.5),
                ],
                color,
            );
        }
        IconName::User => {
            scene.stroke_circle(g.p(12.0, 9.0), g.d(4.0), w, color);
            scene.stroke_polyline(
                &[
                    g.p(5.5, 20.5),
                    g.p(5.5, 18.5),
                    g.p(8.5, 15.5),
                    g.p(15.5, 15.5),
                    g.p(18.5, 18.5),
                    g.p(18.5, 20.5),
                ],
                w,
                color,
            );
        }
        IconName::Info => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(8.0), w, color);
            scene.fill_circle(g.p(12.0, 7.8), g.d(1.3), color);
            scene.stroke_line(g.p(12.0, 11.0), g.p(12.0, 16.5), w, color);
        }
        IconName::App => {
            // Ogólne okno aplikacji (pasek tytułu + kropki), a nie „kwadrat w kwadracie”.
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.5, 4.5).x, g.p(3.5, 4.5).y, g.d(17.0), g.d(15.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.stroke_line(g.p(3.5, 9.0), g.p(20.5, 9.0), w, color);
            for x in [6.4, 9.2, 12.0] {
                scene.fill_circle(g.p(x, 6.8), g.d(0.8), color);
            }
        }
        IconName::Brightness => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(4.0), w, color);
            for k in 0..8 {
                let a = (k as f64 * 45.0).to_radians();
                let (c, s) = (a.cos(), a.sin());
                scene.stroke_line(
                    g.p(12.0 + 6.2 * c, 12.0 + 6.2 * s),
                    g.p(12.0 + 8.9 * c, 12.0 + 8.9 * s),
                    g.d(2.0),
                    color,
                );
            }
        }
        // ── Ikony aplikacji (taskbar/launcher) — ten sam grid i grubość kreski ──
        IconName::Globe => {
            scene.stroke_circle(g.p(12.0, 12.0), g.d(8.0), w, color);
            scene.stroke_line(g.p(4.0, 12.0), g.p(20.0, 12.0), g.d(1.6), color);
            scene.stroke_line(g.p(12.0, 4.0), g.p(12.0, 20.0), g.d(1.6), color);
            for sign in [-1.0_f64, 1.0] {
                let pts: Vec<Point> = (0..=8)
                    .map(|i| {
                        let a =
                            -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 8.0;
                        g.p(12.0 + sign * 4.0 * a.cos(), 12.0 + 8.0 * a.sin())
                    })
                    .collect();
                scene.stroke_polyline(&pts, g.d(1.4), color);
            }
        }
        IconName::Mail => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 5.0).x, g.p(3.0, 5.0).y, g.d(18.0), g.d(14.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.stroke_polyline(
                &[g.p(4.5, 7.0), g.p(12.0, 12.5), g.p(19.5, 7.0)],
                g.d(1.6),
                color,
            );
        }
        IconName::Chat => {
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(3.0, 4.0).x, g.p(3.0, 4.0).y, g.d(18.0), g.d(13.0)),
                g.d(4.0),
                color,
            );
            scene.fill_polygon(&[g.p(7.5, 16.5), g.p(6.0, 21.0), g.p(11.5, 16.5)], color);
        }
        IconName::Code => {
            scene.stroke_polyline(&[g.p(9.0, 7.0), g.p(4.0, 12.0), g.p(9.0, 17.0)], w, color);
            scene.stroke_polyline(
                &[g.p(15.0, 7.0), g.p(20.0, 12.0), g.p(15.0, 17.0)],
                w,
                color,
            );
            scene.stroke_line(g.p(13.5, 5.0), g.p(10.5, 19.0), g.d(1.6), color);
        }
        IconName::Image => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 4.5).x, g.p(3.0, 4.5).y, g.d(18.0), g.d(15.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.fill_circle(g.p(8.5, 9.5), g.d(1.8), color);
            scene.fill_polygon(
                &[
                    g.p(5.0, 17.5),
                    g.p(9.5, 11.5),
                    g.p(13.0, 15.0),
                    g.p(15.5, 12.5),
                    g.p(19.0, 17.5),
                ],
                color,
            );
        }
        IconName::Video => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 5.5).x, g.p(3.0, 5.5).y, g.d(18.0), g.d(13.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.fill_polygon(&[g.p(10.0, 9.0), g.p(16.0, 12.0), g.p(10.0, 15.0)], color);
        }
        IconName::Document => {
            scene.stroke_polyline(
                &[
                    g.p(6.0, 3.0),
                    g.p(14.0, 3.0),
                    g.p(19.0, 8.0),
                    g.p(19.0, 21.0),
                    g.p(6.0, 21.0),
                    g.p(6.0, 3.0),
                ],
                w,
                color,
            );
            scene.stroke_polyline(
                &[g.p(14.0, 3.0), g.p(14.0, 8.0), g.p(19.0, 8.0)],
                g.d(1.6),
                color,
            );
            for y in [12.0, 16.0] {
                scene.stroke_line(g.p(9.0, y), g.p(16.0, y), g.d(1.6), color);
            }
        }
        IconName::Calculator => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(5.0, 3.0).x, g.p(5.0, 3.0).y, g.d(14.0), g.d(18.0)),
                g.d(2.5),
                w,
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(7.0, 5.5).x, g.p(7.0, 5.5).y, g.d(10.0), g.d(4.0)),
                g.d(1.0),
                color,
            );
            for (x, y) in [
                (8.5, 13.0),
                (12.0, 13.0),
                (15.5, 13.0),
                (8.5, 16.5),
                (12.0, 16.5),
                (15.5, 16.5),
            ] {
                scene.fill_circle(g.p(x, y), g.d(1.1), color);
            }
        }
        IconName::Game => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(2.5, 8.0).x, g.p(2.5, 8.0).y, g.d(19.0), g.d(10.5)),
                g.d(5.0),
                w,
                color,
            );
            scene.stroke_line(g.p(7.0, 11.5), g.p(7.0, 15.5), g.d(1.8), color);
            scene.stroke_line(g.p(5.0, 13.5), g.p(9.0, 13.5), g.d(1.8), color);
            scene.fill_circle(g.p(16.5, 12.5), g.d(1.3), color);
            scene.fill_circle(g.p(18.8, 14.8), g.d(1.3), color);
        }
        IconName::Archive => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 5.0).x, g.p(3.0, 5.0).y, g.d(18.0), g.d(15.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.stroke_line(g.p(3.0, 9.5), g.p(21.0, 9.5), g.d(1.6), color);
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(9.0, 3.0).x, g.p(9.0, 3.0).y, g.d(6.0), g.d(4.5)),
                g.d(1.0),
                color,
            );
        }
        IconName::Printer => {
            scene.fill_rect(
                Rect::from_xywh(g.p(7.0, 4.0).x, g.p(7.0, 4.0).y, g.d(10.0), g.d(5.0)),
                color,
            );
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 9.0).x, g.p(3.0, 9.0).y, g.d(18.0), g.d(9.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.fill_rect(
                Rect::from_xywh(g.p(7.0, 15.0).x, g.p(7.0, 15.0).y, g.d(10.0), g.d(5.0)),
                color,
            );
        }
        IconName::Bluetooth => {
            scene.stroke_polyline(
                &[
                    g.p(8.0, 7.5),
                    g.p(16.0, 16.5),
                    g.p(12.0, 21.0),
                    g.p(12.0, 3.0),
                    g.p(16.0, 7.5),
                    g.p(8.0, 16.5),
                ],
                g.d(1.8),
                color,
            );
        }
        IconName::Monitor => {
            // Czysty ekran (ikona `video-display`) — bez linii EKG, która myliła
            // ją z monitorem aktywności.
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 4.5).x, g.p(3.0, 4.5).y, g.d(18.0), g.d(12.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.stroke_line(g.p(12.0, 16.5), g.p(12.0, 19.0), g.d(1.8), color);
            scene.stroke_line(g.p(8.0, 20.0), g.p(16.0, 20.0), g.d(1.8), color);
        }
        IconName::Screenshot => {
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.0, 7.0).x, g.p(3.0, 7.0).y, g.d(18.0), g.d(12.0)),
                g.d(2.0),
                w,
                color,
            );
            scene.stroke_circle(g.p(12.0, 13.0), g.d(3.4), g.d(1.8), color);
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(9.0, 4.5).x, g.p(9.0, 4.5).y, g.d(6.0), g.d(3.5)),
                g.d(1.2),
                color,
            );
        }
        IconName::Sliders => {
            for (y, knob_x) in [(7.0, 15.0), (12.0, 9.0), (17.0, 13.5)] {
                scene.stroke_line(g.p(4.0, y), g.p(20.0, y), g.d(1.8), color);
                scene.fill_circle(g.p(knob_x, y), g.d(2.4), color);
            }
        }
        IconName::Play => {
            scene.fill_polygon(&[g.p(7.6, 4.6), g.p(19.2, 12.0), g.p(7.6, 19.4)], color);
        }
        IconName::Pause => {
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(7.2, 4.8).x, g.p(7.2, 4.8).y, g.d(3.4), g.d(14.4)),
                g.d(1.0),
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(13.4, 4.8).x, g.p(13.4, 4.8).y, g.d(3.4), g.d(14.4)),
                g.d(1.0),
                color,
            );
        }
        IconName::Refresh => {
            // Okrągła strzałka z wyraźnym grocikiem na końcu łuku.
            scene.stroke_polyline(&arc(g.p(12.0, 12.0), g.d(7.2), 45.0, 315.0, 28), w, color);
            let [a, b, c] = refresh_arrowhead(12.0, 12.0, 7.2, 315.0, 4.6, 3.0);
            scene.fill_polygon(&[g.p(a.0, a.1), g.p(b.0, b.1), g.p(c.0, c.1)], color);
        }
        IconName::Moon => {
            // Ten sam sierp co w wariancie pastelowym — wcześniej monochromatyczna
            // wersja była pustym kołem i nie czytała się jako tryb ciemny.
            let outer = arc(g.p(12.0, 12.0), g.d(8.6), 271.4, 27.0, 18);
            let inner = arc(g.p(16.2, 9.5), g.d(7.3), 62.0, 236.0, 16);
            let mut pts = outer;
            pts.extend(inner.iter().skip(1).copied());
            let first = pts[0];
            pts.push(first);
            scene.stroke_polyline(&pts, w, color);
        }
        IconName::Auto => {
            // Pół jasnego, pół ciemnego koła — czytelny „tryb automatyczny”.
            scene.stroke_circle(g.p(12.0, 12.0), g.d(7.8), w, color);
            let mut half = vec![g.p(12.0, 12.0)];
            half.extend(arc(g.p(12.0, 12.0), g.d(7.8), -90.0, 90.0, 18));
            scene.fill_polygon(&half, color);
        }
        IconName::Pin => {
            // Pinezka „mapowa”: zaokrąglona główka i zwężenie do ostrza.
            scene.fill_circle(g.p(12.0, 9.0), g.d(5.6), color);
            scene.fill_polygon(&[g.p(7.6, 11.2), g.p(16.4, 11.2), g.p(12.0, 21.0)], color);
        }
        IconName::Microphone => {
            // Kapsuła mikrofonu + pałąk i podstawa (wskaźnik „mikrofon aktywny”).
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(8.6, 3.8).x, g.p(8.6, 3.8).y, g.d(6.8), g.d(10.4)),
                g.d(3.4),
                w,
                color,
            );
            scene.stroke_polyline(
                &[
                    g.p(6.2, 10.8),
                    g.p(6.6, 13.2),
                    g.p(8.2, 15.8),
                    g.p(12.0, 17.0),
                    g.p(15.8, 15.8),
                    g.p(17.4, 13.2),
                    g.p(17.8, 10.8),
                ],
                w,
                color,
            );
            scene.stroke_line(g.p(12.0, 17.0), g.p(12.0, 20.2), w, color);
            scene.stroke_line(g.p(8.8, 20.2), g.p(15.2, 20.2), w, color);
        }
        IconName::Camera => {
            // Korpus kamery, „garb” z tyłu i obiektyw (wskaźnik „aparat aktywny”).
            scene.stroke_rounded_rect(
                Rect::from_xywh(g.p(3.6, 7.2).x, g.p(3.6, 7.2).y, g.d(16.8), g.d(11.6)),
                g.d(2.2),
                w,
                color,
            );
            scene.fill_rounded_rect(
                Rect::from_xywh(g.p(9.2, 5.0).x, g.p(9.2, 5.0).y, g.d(5.6), g.d(3.4)),
                g.d(1.0),
                color,
            );
            scene.stroke_circle(g.p(12.0, 13.2), g.d(4.2), w, color);
            scene.fill_circle(g.p(12.0, 13.2), g.d(1.3), color);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pastelowy zestaw ikon Luny — wypełnione, wielokolorowe ilustracje.
// ─────────────────────────────────────────────────────────────────────────────

/// Trzy odcienie jednej pastelowej rodziny: wypełnienie, rozjaśnienie i kontur.
#[derive(Clone, Copy)]
struct Pastel {
    base: Color,
    soft: Color,
    ink: Color,
}

const fn pastel_rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

const PASTEL_BUTTER: Pastel = Pastel {
    base: pastel_rgb(0xF3, 0xD2, 0x7C),
    soft: pastel_rgb(0xFB, 0xF0, 0xC8),
    ink: pastel_rgb(0xA4, 0x78, 0x2C),
};
const PASTEL_PEACH: Pastel = Pastel {
    base: pastel_rgb(0xF5, 0xC2, 0xA3),
    soft: pastel_rgb(0xFC, 0xE7, 0xD9),
    ink: pastel_rgb(0xB8, 0x73, 0x4A),
};
const PASTEL_ROSE: Pastel = Pastel {
    base: pastel_rgb(0xF2, 0xB7, 0xC6),
    soft: pastel_rgb(0xFB, 0xE4, 0xEA),
    ink: pastel_rgb(0xB8, 0x69, 0x7F),
};
const PASTEL_LILAC: Pastel = Pastel {
    base: pastel_rgb(0xCB, 0xBE, 0xE8),
    soft: pastel_rgb(0xED, 0xE7, 0xF8),
    ink: pastel_rgb(0x7B, 0x67, 0xAB),
};
const PASTEL_SKY: Pastel = Pastel {
    base: pastel_rgb(0xB5, 0xD5, 0xF1),
    soft: pastel_rgb(0xE4, 0xF0, 0xFB),
    ink: pastel_rgb(0x52, 0x7F, 0xAE),
};
const PASTEL_MINT: Pastel = Pastel {
    base: pastel_rgb(0xB2, 0xE1, 0xC9),
    soft: pastel_rgb(0xE2, 0xF5, 0xEC),
    ink: pastel_rgb(0x4C, 0x92, 0x74),
};
const PASTEL_PAPER: Pastel = Pastel {
    base: pastel_rgb(0xF1, 0xED, 0xE3),
    soft: pastel_rgb(0xFB, 0xF9, 0xF3),
    ink: pastel_rgb(0x8B, 0x87, 0x94),
};
const PASTEL_PERIWINKLE: Pastel = Pastel {
    base: pastel_rgb(0xC7, 0xD2, 0xEB),
    soft: pastel_rgb(0xE9, 0xEE, 0xF9),
    ink: pastel_rgb(0x65, 0x74, 0x9C),
};

/// Czy ikona ma pastelową ilustrację.
///
/// Kontrolki interfejsu (menu, lupka, strzałki, „×”, „+”…) celowo zostają
/// jednokolorowe — zmiana odcienia nie może rozstroić paska i przycisków.
fn has_pastel_art(name: IconName) -> bool {
    !matches!(
        name,
        IconName::Menu
            | IconName::Search
            | IconName::Grid
            | IconName::List
            | IconName::Carousel
            | IconName::ChevronLeft
            | IconName::ChevronRight
            | IconName::Close
            | IconName::Minimize
            | IconName::Maximize
            | IconName::Check
            | IconName::Plus
            | IconName::Pin
            | IconName::Microphone
            | IconName::Camera
    )
}

/// Rysowanie w siatce 24×24 z wygodnymi skrótami (wszystko skaluje się do `rect`).
struct PastelPainter<'a, 'b> {
    scene: &'a mut Scene,
    g: &'b Grid,
}

impl PastelPainter<'_, '_> {
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64, color: Color) {
        let p = self.g.p(x, y);
        self.scene.fill_rounded_rect(
            Rect::from_xywh(p.x, p.y, self.g.d(w), self.g.d(h)),
            self.g.d(r),
            color,
        );
    }

    fn outline(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64, width: f64, color: Color) {
        let p = self.g.p(x, y);
        self.scene.stroke_rounded_rect(
            Rect::from_xywh(p.x, p.y, self.g.d(w), self.g.d(h)),
            self.g.d(r),
            self.g.d(width),
            color,
        );
    }

    fn circle(&mut self, x: f64, y: f64, r: f64, color: Color) {
        self.scene.fill_circle(self.g.p(x, y), self.g.d(r), color);
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, width: f64, color: Color) {
        self.scene
            .stroke_line(self.g.p(x0, y0), self.g.p(x1, y1), self.g.d(width), color);
    }

    fn poly(&mut self, points: &[(f64, f64)], color: Color) {
        let points: Vec<Point> = points.iter().map(|&(x, y)| self.g.p(x, y)).collect();
        self.scene.fill_polygon(&points, color);
    }

    fn stroke(&mut self, points: &[(f64, f64)], width: f64, color: Color) {
        let points: Vec<Point> = points.iter().map(|&(x, y)| self.g.p(x, y)).collect();
        self.scene.stroke_polyline(&points, self.g.d(width), color);
    }

    fn arc(
        &mut self,
        x: f64,
        y: f64,
        r: f64,
        a0: f64,
        a1: f64,
        segs: usize,
        width: f64,
        color: Color,
    ) {
        let points = arc(self.g.p(x, y), self.g.d(r), a0, a1, segs);
        self.scene.stroke_polyline(&points, self.g.d(width), color);
    }

    fn star(&mut self, x: f64, y: f64, outer: f64, inner: f64, color: Color) {
        let points: Vec<Point> = (0..10)
            .map(|i| {
                let a = (-90.0 + i as f64 * 36.0).to_radians();
                let r = if i % 2 == 0 { outer } else { inner };
                self.g.p(x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.scene.fill_polygon(&points, color);
    }
}

/// Rysuje pastelową ilustrację ikony w siatce `g`.
fn paint_pastel(paint: &mut PaintCtx, name: IconName, g: &Grid) {
    // Ustawienia: to samo zębate koło co na pasku, tylko w pastelowym liliu.
    if name == IconName::Settings {
        gear(paint, PASTEL_PERIWINKLE.base, g.p(12.0, 12.0), g.d(24.0));
        return;
    }
    let mut p = PastelPainter {
        scene: &mut *paint.scene,
        g,
    };

    match name {
        IconName::Files => {
            // Folder-menedżer z wystającą kartką.
            p.rect(3.0, 6.0, 7.5, 3.0, 1.2, PASTEL_BUTTER.base);
            p.rect(3.0, 7.8, 18.0, 11.7, 2.0, PASTEL_BUTTER.base);
            p.rect(10.5, 9.4, 8.5, 9.0, 1.2, PASTEL_PAPER.soft);
            p.poly(&[(15.6, 9.4), (19.0, 12.8), (15.6, 12.8)], PASTEL_SKY.base);
            for y in [14.2, 16.0, 17.8] {
                p.line(12.3, y, 17.3, y, 1.1, PASTEL_PAPER.ink);
            }
            p.rect(4.2, 8.9, 6.0, 1.0, 0.5, PASTEL_BUTTER.soft);
        }
        IconName::Terminal => {
            p.rect(3.0, 4.5, 18.0, 15.0, 2.6, PASTEL_MINT.base);
            p.rect(4.6, 8.2, 14.8, 9.8, 1.6, PASTEL_MINT.soft);
            for x in [5.6, 7.0, 8.4] {
                p.circle(x, 6.3, 0.55, PASTEL_MINT.soft);
            }
            p.stroke(
                &[(7.0, 10.6), (9.4, 12.6), (7.0, 14.6)],
                1.6,
                PASTEL_MINT.ink,
            );
            p.line(11.4, 14.8, 15.2, 14.8, 1.6, PASTEL_MINT.ink);
        }
        IconName::Music => {
            p.circle(9.0, 17.2, 2.4, PASTEL_ROSE.base);
            p.line(11.2, 17.0, 11.2, 7.0, 1.7, PASTEL_ROSE.ink);
            p.poly(
                &[(11.2, 7.0), (17.0, 8.7), (17.0, 11.2), (11.2, 9.5)],
                PASTEL_ROSE.base,
            );
        }
        IconName::Star => {
            p.star(12.0, 12.5, 9.0, 3.9, PASTEL_BUTTER.base);
            p.star(11.0, 11.3, 4.2, 1.8, PASTEL_BUTTER.soft);
        }
        IconName::Clock => {
            p.circle(12.0, 12.0, 8.6, PASTEL_LILAC.base);
            p.circle(12.0, 12.0, 6.6, PASTEL_LILAC.soft);
            p.line(12.0, 12.0, 12.0, 7.7, 1.5, PASTEL_LILAC.ink);
            p.line(12.0, 12.0, 15.4, 13.6, 1.5, PASTEL_LILAC.ink);
            p.circle(12.0, 12.0, 0.9, PASTEL_LILAC.ink);
        }
        IconName::Store => {
            p.poly(
                &[(5.0, 8.0), (19.0, 8.0), (17.8, 20.0), (6.2, 20.0)],
                PASTEL_PEACH.base,
            );
            p.stroke(
                &[
                    (9.0, 8.2),
                    (9.0, 6.2),
                    (12.0, 5.6),
                    (15.0, 6.2),
                    (15.0, 8.2),
                ],
                1.7,
                PASTEL_PEACH.ink,
            );
            p.rect(7.0, 10.5, 2.0, 7.0, 1.0, PASTEL_PEACH.soft);
        }
        IconName::Edit => {
            p.poly(
                &[
                    (15.8, 3.5),
                    (20.5, 8.2),
                    (9.2, 19.5),
                    (4.2, 20.8),
                    (5.4, 15.8),
                ],
                PASTEL_PEACH.base,
            );
            p.poly(
                &[(14.8, 4.7), (18.7, 8.6), (16.9, 10.3), (13.0, 6.4)],
                PASTEL_PEACH.soft,
            );
            p.poly(&[(4.2, 20.8), (5.4, 15.8), (8.6, 17.8)], PASTEL_PEACH.ink);
        }
        IconName::Wifi => {
            for r in [4.0, 7.0, 10.2] {
                p.arc(12.0, 18.0, r, 200.0, 340.0, 10, 1.9, PASTEL_MINT.ink);
            }
            p.circle(12.0, 18.0, 1.4, PASTEL_MINT.ink);
        }
        IconName::Volume => {
            p.poly(
                &[
                    (3.0, 9.5),
                    (7.0, 9.5),
                    (11.0, 5.0),
                    (11.0, 19.0),
                    (7.0, 14.5),
                    (3.0, 14.5),
                ],
                PASTEL_LILAC.base,
            );
            p.rect(4.0, 10.6, 2.2, 1.4, 0.5, PASTEL_LILAC.soft);
            for r in [4.6, 7.4] {
                p.arc(11.0, 12.0, r, -50.0, 50.0, 8, 1.8, PASTEL_LILAC.ink);
            }
        }
        IconName::Battery => {
            p.rect(3.0, 7.0, 16.0, 10.0, 2.6, PASTEL_MINT.soft);
            p.outline(3.0, 7.0, 16.0, 10.0, 2.6, 1.4, PASTEL_MINT.ink);
            p.rect(19.6, 10.5, 2.4, 3.0, 1.0, PASTEL_MINT.ink);
            p.rect(5.0, 9.6, 9.0, 4.8, 1.2, PASTEL_MINT.base);
        }
        IconName::Power => {
            p.arc(12.0, 12.0, 7.5, -60.0, 240.0, 28, 2.4, PASTEL_ROSE.ink);
            p.line(12.0, 3.4, 12.0, 11.5, 2.2, PASTEL_ROSE.ink);
        }
        IconName::Home => {
            p.poly(&[(12.0, 3.2), (21.2, 11.6), (2.8, 11.6)], PASTEL_PEACH.base);
            p.rect(5.2, 11.0, 13.6, 9.2, 1.6, PASTEL_BUTTER.base);
            p.rect(13.8, 12.4, 3.4, 3.2, 0.8, PASTEL_SKY.base);
            p.rect(9.8, 15.4, 4.4, 4.8, 0.8, PASTEL_PAPER.soft);
        }
        IconName::User => {
            p.circle(12.0, 9.0, 4.2, PASTEL_PERIWINKLE.base);
            p.poly(
                &[
                    (4.8, 20.6),
                    (5.6, 17.6),
                    (8.4, 15.2),
                    (15.6, 15.2),
                    (18.4, 17.6),
                    (19.2, 20.6),
                ],
                PASTEL_PERIWINKLE.base,
            );
            p.rect(9.4, 16.4, 5.2, 1.2, 0.6, PASTEL_PERIWINKLE.soft);
        }
        IconName::Info => {
            p.circle(12.0, 12.0, 8.6, PASTEL_SKY.base);
            p.circle(12.0, 7.9, 1.35, PASTEL_PAPER.soft);
            p.rect(11.1, 10.6, 1.8, 6.2, 0.9, PASTEL_PAPER.soft);
        }
        IconName::App => {
            // Pastelowe okno: liliowy pasek tytułu i jasna treść.
            p.rect(3.5, 4.5, 17.0, 15.0, 2.8, PASTEL_LILAC.base);
            p.rect(5.1, 8.6, 13.8, 9.2, 1.8, PASTEL_PAPER.soft);
            for x in [6.6, 9.3, 12.0] {
                p.circle(x, 6.6, 0.75, PASTEL_PAPER.soft);
            }
        }
        IconName::Brightness => {
            for k in 0..8 {
                let a = (k as f64 * 45.0).to_radians();
                let (c, s) = (a.cos(), a.sin());
                p.line(
                    12.0 + 6.2 * c,
                    12.0 + 6.2 * s,
                    12.0 + 8.9 * c,
                    12.0 + 8.9 * s,
                    2.1,
                    PASTEL_BUTTER.base,
                );
            }
            p.circle(12.0, 12.0, 4.4, PASTEL_BUTTER.base);
            p.circle(12.0, 12.0, 1.9, PASTEL_BUTTER.soft);
        }
        IconName::Globe => {
            p.circle(12.0, 12.0, 8.6, PASTEL_SKY.base);
            p.poly(
                &[
                    (5.2, 8.8),
                    (8.4, 7.4),
                    (11.2, 8.6),
                    (10.2, 11.2),
                    (6.8, 11.6),
                    (5.0, 10.3),
                ],
                PASTEL_MINT.base,
            );
            p.poly(
                &[
                    (12.6, 13.2),
                    (15.8, 11.8),
                    (18.4, 13.0),
                    (18.2, 16.4),
                    (15.0, 18.4),
                    (12.8, 16.8),
                ],
                PASTEL_MINT.base,
            );
            p.line(12.0, 3.6, 12.0, 20.4, 1.1, PASTEL_SKY.ink);
        }
        IconName::Mail => {
            p.rect(3.0, 5.0, 18.0, 14.0, 2.2, PASTEL_PEACH.base);
            p.poly(&[(3.8, 6.2), (12.0, 13.0), (20.2, 6.2)], PASTEL_PEACH.soft);
            p.stroke(
                &[(4.2, 6.4), (12.0, 12.8), (19.8, 6.4)],
                1.2,
                PASTEL_PEACH.ink,
            );
        }
        IconName::Chat => {
            p.rect(3.0, 4.0, 18.0, 13.0, 4.2, PASTEL_LILAC.base);
            p.poly(&[(7.4, 16.6), (5.8, 21.0), (11.6, 16.6)], PASTEL_LILAC.base);
            for x in [8.4, 12.0, 15.6] {
                p.circle(x, 10.6, 1.35, PASTEL_LILAC.soft);
            }
        }
        IconName::Code => {
            p.stroke(&[(9.2, 6.8), (3.8, 12.0), (9.2, 17.2)], 2.2, PASTEL_SKY.ink);
            p.stroke(
                &[(14.8, 6.8), (20.2, 12.0), (14.8, 17.2)],
                2.2,
                PASTEL_SKY.ink,
            );
            p.line(13.4, 4.8, 10.6, 19.2, 2.0, PASTEL_ROSE.ink);
        }
        IconName::Image => {
            p.rect(3.0, 4.5, 18.0, 15.0, 2.2, PASTEL_PAPER.soft);
            p.rect(4.6, 6.1, 14.8, 11.8, 1.4, PASTEL_SKY.base);
            p.circle(8.6, 10.0, 1.9, PASTEL_BUTTER.base);
            p.poly(
                &[
                    (4.6, 17.4),
                    (9.6, 11.2),
                    (13.0, 14.8),
                    (15.4, 12.2),
                    (19.4, 17.4),
                ],
                PASTEL_MINT.base,
            );
            p.outline(3.0, 4.5, 18.0, 15.0, 2.2, 1.1, PASTEL_PAPER.ink);
        }
        IconName::Video => {
            p.rect(3.0, 5.5, 18.0, 13.0, 2.2, PASTEL_LILAC.base);
            p.poly(&[(9.8, 8.8), (16.2, 12.0), (9.8, 15.2)], PASTEL_PAPER.soft);
        }
        IconName::Document => {
            p.rect(6.0, 3.0, 13.0, 18.0, 1.8, PASTEL_PAPER.soft);
            p.outline(6.0, 3.0, 13.0, 18.0, 1.8, 1.1, PASTEL_PAPER.ink);
            p.poly(&[(14.2, 3.0), (19.0, 7.8), (14.2, 7.8)], PASTEL_SKY.base);
            p.stroke(
                &[(14.2, 3.2), (14.2, 7.8), (18.8, 7.8)],
                1.0,
                PASTEL_PAPER.ink,
            );
            for y in [11.0, 14.0, 17.0] {
                p.line(8.4, y, 16.6, y, 1.2, PASTEL_SKY.ink);
            }
        }
        IconName::Calculator => {
            p.rect(5.0, 3.0, 14.0, 18.0, 2.6, PASTEL_MINT.base);
            p.rect(7.0, 5.4, 10.0, 3.8, 1.0, PASTEL_MINT.soft);
            p.line(8.4, 7.3, 15.6, 7.3, 0.9, PASTEL_MINT.ink);
            for (x, y) in [
                (8.5, 12.5),
                (12.0, 12.5),
                (15.5, 12.5),
                (8.5, 16.5),
                (12.0, 16.5),
                (15.5, 16.5),
            ] {
                p.circle(x, y, 1.15, PASTEL_PAPER.soft);
            }
            p.circle(15.5, 16.5, 1.15, PASTEL_ROSE.base);
        }
        IconName::Game => {
            p.rect(2.5, 8.0, 19.0, 10.5, 5.2, PASTEL_ROSE.base);
            p.line(7.0, 11.6, 7.0, 15.4, 1.8, PASTEL_PAPER.soft);
            p.line(5.1, 13.5, 8.9, 13.5, 1.8, PASTEL_PAPER.soft);
            p.circle(16.4, 12.4, 1.35, PASTEL_PAPER.soft);
            p.circle(18.6, 14.7, 1.35, PASTEL_PAPER.soft);
        }
        IconName::Archive => {
            p.rect(3.0, 9.0, 18.0, 11.0, 1.8, PASTEL_PEACH.base);
            p.rect(3.0, 5.0, 18.0, 5.0, 1.8, PASTEL_PEACH.soft);
            p.rect(9.0, 3.0, 6.0, 4.7, 1.1, PASTEL_PEACH.ink);
            p.line(3.6, 9.2, 20.4, 9.2, 1.0, PASTEL_PEACH.ink);
        }
        IconName::Printer => {
            p.rect(7.0, 3.2, 10.0, 5.0, 0.9, PASTEL_PAPER.soft);
            p.rect(3.0, 8.4, 18.0, 8.4, 2.0, PASTEL_SKY.base);
            p.rect(7.0, 13.8, 10.0, 6.4, 0.9, PASTEL_PAPER.soft);
            p.line(8.4, 16.0, 15.6, 16.0, 1.0, PASTEL_PAPER.ink);
            p.line(8.4, 17.6, 15.6, 17.6, 1.0, PASTEL_PAPER.ink);
            p.circle(6.0, 11.0, 0.85, PASTEL_PAPER.soft);
        }
        IconName::Bluetooth => {
            p.stroke(
                &[
                    (8.0, 7.4),
                    (16.2, 16.6),
                    (12.0, 21.0),
                    (12.0, 3.0),
                    (16.2, 7.4),
                    (8.0, 16.6),
                ],
                1.9,
                PASTEL_SKY.ink,
            );
        }
        IconName::Monitor => {
            p.rect(3.0, 4.5, 18.0, 12.0, 2.2, PASTEL_SKY.base);
            p.rect(4.6, 6.1, 14.8, 8.8, 1.2, PASTEL_SKY.soft);
            p.line(12.0, 16.5, 12.0, 19.0, 1.8, PASTEL_SKY.ink);
            p.line(8.4, 19.8, 15.6, 19.8, 1.8, PASTEL_SKY.ink);
        }
        IconName::Screenshot => {
            p.rect(3.0, 7.0, 18.0, 12.0, 2.2, PASTEL_SKY.base);
            p.rect(9.0, 4.5, 6.0, 3.4, 1.0, PASTEL_SKY.base);
            p.circle(12.0, 13.0, 3.7, PASTEL_PAPER.soft);
            p.circle(12.0, 13.0, 2.2, PASTEL_SKY.ink);
            p.circle(13.1, 11.9, 0.6, PASTEL_PAPER.soft);
            p.circle(18.0, 9.6, 1.1, PASTEL_BUTTER.base);
        }
        IconName::Sliders => {
            for (y, knob_x, color) in [
                (7.0, 14.8, PASTEL_ROSE.base),
                (12.0, 8.8, PASTEL_SKY.base),
                (17.0, 13.4, PASTEL_MINT.base),
            ] {
                p.line(4.0, y, 20.0, y, 1.8, PASTEL_PERIWINKLE.base);
                p.circle(knob_x, y, 2.3, color);
                p.circle(knob_x, y, 0.9, PASTEL_PAPER.soft);
            }
        }
        IconName::Play => {
            p.poly(&[(7.6, 4.6), (19.2, 12.0), (7.6, 19.4)], PASTEL_MINT.base);
        }
        IconName::Pause => {
            p.rect(7.2, 4.8, 3.4, 14.4, 1.0, PASTEL_MINT.base);
            p.rect(13.4, 4.8, 3.4, 14.4, 1.0, PASTEL_MINT.base);
        }
        IconName::Refresh => {
            p.arc(12.0, 12.0, 7.2, 45.0, 315.0, 28, 2.4, PASTEL_SKY.base);
            let [a, b, c] = refresh_arrowhead(12.0, 12.0, 7.2, 315.0, 4.6, 3.0);
            p.poly(&[a, b, c], PASTEL_SKY.base);
        }
        IconName::Moon => {
            let outer = arc(Point::new(12.0, 12.0), 8.6, 271.4, 27.0, 18);
            let inner = arc(Point::new(16.2, 9.5), 7.3, 62.0, 236.0, 16);
            let mut points: Vec<(f64, f64)> = outer.iter().map(|p| (p.x, p.y)).collect();
            points.extend(inner.iter().skip(1).map(|p| (p.x, p.y)));
            p.poly(&points, PASTEL_LILAC.base);
            p.circle(8.2, 8.8, 1.0, PASTEL_LILAC.soft);
        }
        IconName::Auto => {
            // Pół słońca (jasny), pół księżyca (ciemny) — jak na stronie i w pasku.
            p.circle(12.0, 12.0, 8.2, PASTEL_BUTTER.base);
            let mut right: Vec<(f64, f64)> = vec![(12.0, 12.0)];
            right.extend(
                arc(Point::new(12.0, 12.0), 8.2, -90.0, 90.0, 18)
                    .iter()
                    .map(|pt| (pt.x, pt.y)),
            );
            p.poly(&right, PASTEL_LILAC.base);
            p.line(12.0, 3.9, 12.0, 20.1, 1.0, PASTEL_PERIWINKLE.ink);
            p.outline(3.8, 3.8, 16.4, 16.4, 8.2, 1.2, PASTEL_PERIWINKLE.ink);
        }
        _ => {}
    }
}

#[cfg(test)]
mod icon_tests {
    use super::*;
    use libluna_core::Theme;
    use libluna_render::{Scene, SoftwareRenderer, TextEngine};

    #[test]
    fn fallback_prefers_hicolor_and_skips_high_contrast() {
        let root = std::env::temp_dir().join(format!(
            "luna-fallback-icons-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let hicolor = root.join("hicolor/48x48/apps");
        let adwaita = root.join("Adwaita/48x48/apps");
        let contrast = root.join("HighContrast/48x48/apps");
        for dir in [&hicolor, &adwaita, &contrast] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("gparted.png"), b"").unwrap();
        }
        let sets = vec![
            root.join("Adwaita"),
            root.join("HighContrast"),
            root.join("hicolor"),
        ];
        let found = find_fallback_path_in(sets, &[], "gparted", 48.0).unwrap();
        assert_eq!(found, hicolor.join("gparted.png"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn fallback_svg_prefers_hicolor_and_skips_high_contrast() {
        let root = std::env::temp_dir().join(format!(
            "luna-fallback-svg-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let hicolor = root.join("hicolor/scalable/apps");
        let adwaita = root.join("Adwaita/scalable/apps");
        let contrast = root.join("HighContrast/scalable/apps");
        for dir in [&hicolor, &adwaita, &contrast] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("org.gnome.Demo.svg"), b"").unwrap();
        }
        let sets = vec![
            root.join("Adwaita"),
            root.join("HighContrast"),
            root.join("hicolor"),
        ];
        let found = find_fallback_svg_in(sets, &[], "org.gnome.Demo").unwrap();
        assert_eq!(found, hicolor.join("org.gnome.Demo.svg"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn every_icon_draws_without_panic() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        for style in libluna_core::IconTheme::ALL {
            for name in IconName::ALL {
                let mut scene = Scene::new();
                let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
                paint_icon_themed(
                    &mut ctx,
                    name,
                    Rect::from_xywh(0.0, 0.0, 24.0, 24.0),
                    theme.colors.text,
                    style,
                );
                assert!(
                    !scene.is_empty(),
                    "ikona {name:?} nic nie rysuje (styl {})",
                    style.name()
                );
            }
        }
    }

    /// Zwraca widoczne granice ikony w jednostkach siatki 24×24.
    fn painted_bounds(name: IconName, style: IconTheme) -> (f64, f64, f64, f64) {
        const PX: u32 = 120;
        let theme = Theme::light();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        {
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            paint_icon_themed(
                &mut ctx,
                name,
                Rect::from_xywh(0.0, 0.0, PX as f64, PX as f64),
                theme.colors.text,
                style,
            );
        }
        let mut renderer = SoftwareRenderer::new(PX, PX);
        renderer.draw(&scene, 1.0, Color::TRANSPARENT);
        let data = renderer.data();
        let (mut x0, mut y0, mut x1, mut y1) = (PX, PX, 0u32, 0u32);
        for y in 0..PX {
            for x in 0..PX {
                if data[((y * PX + x) * 4 + 3) as usize] > 8 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        let s = 24.0 / PX as f64;
        (
            x0 as f64 * s,
            y0 as f64 * s,
            (x1 + 1) as f64 * s,
            (y1 + 1) as f64 * s,
        )
    }

    #[test]
    fn every_icon_keeps_the_same_safe_margin() {
        // Wszystkie ikony — pastelowe i monochromatyczne — muszą trzymać ten sam
        // margines w siatce 24×24. Pilnuje to spójności optycznej po zmianach
        // geometrii (np. zbyt długich promieni słońca albo zębów koła zębatego).
        for style in IconTheme::ALL {
            for name in IconName::ALL {
                let (x0, y0, x1, y1) = painted_bounds(name, style);
                assert!(
                    x1 > x0 && y1 > y0,
                    "ikona {name:?}/{} jest pusta",
                    style.name()
                );
                assert!(
                    x0 >= 1.2 && y0 >= 1.2 && x1 <= 22.8 && y1 <= 22.8,
                    "ikona {name:?}/{} wychodzi poza margines: \
                     {x0:.1},{y0:.1}..{x1:.1},{y1:.1} (dozwolone 1.2..22.8)",
                    style.name()
                );
            }
        }
    }
}
