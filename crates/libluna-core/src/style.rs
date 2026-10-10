//! Tokeny wyglądu — odpowiednik „design tokens” w libadwaita.
//!
//! Zamiast arkuszy CSS libLuna zaczyna od typowanych tokenów: odstępów, promieni,
//! skali typograficznej i palety (jasna/ciemna). Warstwa `libluna-widgets` buduje
//! z nich konkretne widgety, dzięki czemu wygląd jest spójny i tani w zmianie.
//!
//! Wartości palety są zbliżone do libadwaita (GNOME 45+) i mogą wymagać kalibracji
//! podczas wdrażania.

use crate::color::Color;

/// Odstępy w pikselach logicznych (siateczka 3/6/12/18/24 — jak w GNOME).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    pub xs: f64,
    pub sm: f64,
    pub md: f64,
    pub lg: f64,
    pub xl: f64,
}

impl Spacing {
    pub const fn standard() -> Self {
        Self {
            xs: 3.0,
            sm: 6.0,
            md: 12.0,
            lg: 18.0,
            xl: 24.0,
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Self::standard()
    }
}

/// Promienie zaokrągleń (piksele logiczne).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Radius {
    pub small: f64,
    pub medium: f64,
    pub large: f64,
    /// Wartość „pigułki” — na tyle duża, by przyciski były w pełni zaokrąglone.
    pub pill: f64,
}

impl Radius {
    pub const fn standard() -> Self {
        Self {
            small: 6.0,
            medium: 9.0,
            large: 12.0,
            pill: 999.0,
        }
    }
}

impl Default for Radius {
    fn default() -> Self {
        Self::standard()
    }
}

/// Grubości czcionek.
pub mod weight {
    pub const NORMAL: u16 = 400;
    pub const MEDIUM: u16 = 500;
    pub const BOLD: u16 = 700;
}

/// Pojedynczy styl czcionki.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontStyle {
    /// Rozmiar (piksele logiczne).
    pub size: f64,
    /// Grubość (patrz [`weight`]).
    pub weight: u16,
    /// Wysokość linii (piksele logiczne).
    pub line_height: f64,
    /// Czy używać kroju o stałej szerokości.
    pub monospace: bool,
}

impl FontStyle {
    const fn new(size: f64, weight: u16, line_height: f64, monospace: bool) -> Self {
        Self {
            size,
            weight,
            line_height,
            monospace,
        }
    }
}

/// Skala typograficzna.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Typography {
    /// Duży tytuł okna / strony.
    pub title: FontStyle,
    /// Nagłówek sekcji.
    pub heading: FontStyle,
    /// Zwykły tekst treści.
    pub body: FontStyle,
    /// Drobny tekst pomocniczy.
    pub caption: FontStyle,
}

impl Typography {
    pub const fn standard() -> Self {
        Self {
            title: FontStyle::new(20.0, weight::BOLD, 26.0, false),
            heading: FontStyle::new(15.0, weight::BOLD, 20.0, false),
            body: FontStyle::new(14.0, weight::NORMAL, 20.0, false),
            caption: FontStyle::new(12.0, weight::NORMAL, 16.0, false),
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::standard()
    }
}

/// Paleta kolorów dla jednego wariantu (jasny lub ciemny).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorScheme {
    /// Tło okna aplikacji.
    pub window_bg: Color,
    /// Tło głównego widoku / listy.
    pub view_bg: Color,
    /// Tło karty / powierzchni podniesionej.
    pub card_bg: Color,
    /// Tło Headera (HeaderBar).
    pub headerbar_bg: Color,
    /// Tło panelu bocznego.
    pub sidebar_bg: Color,
    /// Kolor tekstu podstawowego.
    pub text: Color,
    /// Kolor tekstu drugorzędnego / wyciszonego.
    pub dim_text: Color,
    /// Kolor akcentu (np. sugerowany przycisk, przełącznik wł.).
    pub accent: Color,
    /// Tekst na akcencie.
    pub accent_fg: Color,
    /// Kolor obramowań i separatorów.
    pub border: Color,
    /// Delikatna nakładka (hover/pressed/„shade”).
    pub shade: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
}

impl ColorScheme {
    /// Wariant jasny (przybliżenie libadwaita light).
    pub const fn light() -> Self {
        Self {
            window_bg: Color::from_rgba8(0xfa, 0xfa, 0xfa, 0xff),
            view_bg: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            card_bg: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            headerbar_bg: Color::from_rgba8(0xf2, 0xf2, 0xf2, 0xff),
            sidebar_bg: Color::from_rgba8(0xeb, 0xeb, 0xeb, 0xff),
            text: Color::from_rgba8(0x2e, 0x2e, 0x2e, 0xff),
            dim_text: Color::rgba(0.0, 0.0, 0.0, 0.55),
            accent: Color::from_rgba8(0x35, 0x84, 0xe4, 0xff),
            accent_fg: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            border: Color::rgba(0.0, 0.0, 0.0, 0.10),
            shade: Color::rgba(0.0, 0.0, 0.0, 0.06),
            error: Color::from_rgba8(0xc0, 0x1c, 0x28, 0xff),
            warning: Color::from_rgba8(0xe5, 0xa5, 0x0a, 0xff),
            success: Color::from_rgba8(0x2e, 0xc2, 0x7e, 0xff),
        }
    }

    /// Wariant ciemny (przybliżenie libadwaita dark).
    pub const fn dark() -> Self {
        Self {
            window_bg: Color::from_rgba8(0x24, 0x24, 0x24, 0xff),
            view_bg: Color::from_rgba8(0x1e, 0x1e, 0x1e, 0xff),
            card_bg: Color::from_rgba8(0x30, 0x30, 0x30, 0xff),
            headerbar_bg: Color::from_rgba8(0x30, 0x30, 0x30, 0xff),
            sidebar_bg: Color::from_rgba8(0x2b, 0x2b, 0x2b, 0xff),
            text: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            dim_text: Color::rgba(1.0, 1.0, 1.0, 0.55),
            accent: Color::from_rgba8(0x35, 0x84, 0xe4, 0xff),
            accent_fg: Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            border: Color::rgba(1.0, 1.0, 1.0, 0.12),
            shade: Color::rgba(1.0, 1.0, 1.0, 0.08),
            error: Color::from_rgba8(0xff, 0x7b, 0x63, 0xff),
            warning: Color::from_rgba8(0xf8, 0xe4, 0x5c, 0xff),
            success: Color::from_rgba8(0x8f, 0xf0, 0xa4, 0xff),
        }
    }
}

/// Czy motyw jest jasny czy ciemny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

/// Geometria paska tytułu ([`HeaderBar`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderBarMetrics {
    /// Wysokość paska (piksele logiczne).
    pub height: f64,
    /// Bok kwadratowego obszaru kontrolki okna (`— □ ✕`).
    pub control: f64,
    /// Odstęp między kontrolkami okna (i od krawędzi paska).
    pub control_gap: f64,
}

/// Geometria przycisku.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonMetrics {
    /// Domyślna wysokość.
    pub height: f64,
    /// Wewnętrzny padding poziomy (z obu stron etykiety).
    pub pad_x: f64,
    /// Rozpiętość wskaźnika rozwijania (chevron).
    pub chevron: f64,
    /// Odstęp etykieta–chevron.
    pub chevron_gap: f64,
}

/// Geometria przełącznika.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchMetrics {
    pub width: f64,
    pub height: f64,
}

/// Geometria suwaka.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderMetrics {
    /// Promień uchwytu (także tolerancja trafienia w poziomie).
    pub knob_r: f64,
    /// Pionowa tolerancja trafienia ponad/pod torem.
    pub hit_pad: f64,
    /// Domyślna wysokość toru.
    pub track_height: f64,
}

/// Geometria listy rozwijanej ([`DropDown`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropDownMetrics {
    pub height: f64,
    /// Wewnętrzny padding poziomy.
    pub pad_x: f64,
    /// Rozpiętość wskaźnika rozwijania.
    pub chevron: f64,
    /// Odstęp etykieta–chevron.
    pub chevron_gap: f64,
    /// Minimalna szerokość kontrolki.
    pub min_width: f64,
}

/// Geometria popoveru ([`Menu`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuMetrics {
    /// Wysokość pozycji akcji.
    pub item_h: f64,
    /// Wysokość nagłówka sekcji.
    pub section_h: f64,
    /// Wysokość separatora.
    pub separator_h: f64,
    /// Padding pionowy panelu (góra/dół).
    pub pad_v: f64,
    /// Padding poziomy zawartości.
    pub pad_h: f64,
    /// Minimalny odstęp między etykietą a skrótem.
    pub label_gap: f64,
    /// Minimalna szerokość panelu.
    pub min_width: f64,
    /// Margines od krawędzi okna przy umieszczaniu.
    pub margin: f64,
    /// Odstęp kotwica–panel.
    pub anchor_gap: f64,
}

/// Geometria menu nawigacji ([`NavList`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavMetrics {
    /// Wysokość pozycji.
    pub item_h: f64,
    /// Wysokość nagłówka sekcji.
    pub section_h: f64,
    /// Padding górny/dolny listy.
    pub pad_v: f64,
    /// Padding poziomy pozycji.
    pub pad_x: f64,
}

/// Geometria przełącznika segmentowego ([`ViewSwitcher`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentedMetrics {
    /// O ile tło-„szyna” wystaje poza segmenty.
    pub track_pad: f64,
}

/// Geometria paska zakładek ([`TabBar`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabsMetrics {
    /// Wysokość paska zakładek.
    pub height: f64,
    /// Minimalna szerokość zakładki.
    pub min_width: f64,
    /// Maksymalna szerokość zakładki.
    pub max_width: f64,
    /// Wewnętrzny padding poziomy etykiety.
    pub pad_x: f64,
    /// Odstęp między zakładkami.
    pub gap: f64,
    /// Bok obszaru przycisku zamknięcia zakładki.
    pub close: f64,
}

/// Geometria dolnego panelu ([`BottomSheet`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BottomSheetMetrics {
    /// Domyślna wysokość panelu.
    pub height: f64,
    /// Szerokość „grabbera”.
    pub handle_width: f64,
    /// Wysokość „grabbera”.
    pub handle_height: f64,
    /// Wysokość nagłówka panelu.
    pub header: f64,
    /// Promień górnych narożników panelu.
    pub radius: f64,
    /// Krycie przygaszającego tła (0..1).
    pub scrim: f64,
    /// Przesunięcie w dół (px) po którym puszczenie zamyka panel.
    pub dismiss: f64,
}

/// Geometria karuzeli stron ([`Pager`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagerMetrics {
    /// Średnica kropki-wskaźnika.
    pub dot: f64,
    /// Odstęp między kropkami.
    pub gap: f64,
    /// Odstęp kropek od dolnej krawędzi.
    pub pad: f64,
}

/// Geometria paska ścieżki / okruszków ([`PathBar`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathBarMetrics {
    /// Wysokość paska.
    pub height: f64,
    /// Wewnętrzny padding poziomy pozycji.
    pub pad_x: f64,
    /// Odstęp wokół separatora.
    pub gap: f64,
}

/// Geometria pola tekstowego ([`SearchEntry`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntryMetrics {
    /// Wysokość pola.
    pub height: f64,
    /// Wewnętrzny padding poziomy.
    pub pad_x: f64,
    /// Bok ikony (lupka, „×”).
    pub icon: f64,
}

/// Geometria widoku elementów ([`ItemView`](crate::style)) — siatka i lista.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemViewMetrics {
    /// Szerokość kafelka (tryb siatki).
    pub tile_w: f64,
    /// Wysokość kafelka (tryb siatki).
    pub tile_h: f64,
    /// Odstęp między kafelkami.
    pub gap: f64,
    /// Wysokość wiersza (tryb listy).
    pub row_h: f64,
    /// Rozmiar ikony elementu.
    pub icon: f64,
}

/// Geometria paska przewijania ([`ScrollView`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollViewMetrics {
    /// Grubość paska.
    pub thickness: f64,
    /// Minimalna długość uchwytu („kciuka”).
    pub min_thumb: f64,
    /// Odstęp paska od krawędzi obszaru.
    pub margin: f64,
}

/// Geometria krokowca ([`Stepper`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepperMetrics {
    /// Wysokość kontrolki.
    pub height: f64,
    /// Domyślna szerokość.
    pub width: f64,
    /// Bok przycisku `−`/`+`.
    pub button: f64,
}

/// Geometria pola wyboru ([`CheckButton`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckMetrics {
    /// Bok kwadratu (checkbox).
    pub size: f64,
    /// Odstęp kwadratu od etykiety.
    pub gap: f64,
}

/// Geometria grupy opcji ([`RadioGroup`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadioMetrics {
    /// Średnica kółka.
    pub dot: f64,
    /// Odstęp kółka od etykiety.
    pub gap: f64,
    /// Wysokość wiersza opcji.
    pub row_h: f64,
}

/// Geometria karty ([`Card`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardMetrics {
    /// Wysokość nagłówka karty.
    pub header: f64,
    /// Padding wewnętrzny (nagłówek).
    pub pad: f64,
}

/// Geometria wiersza akcji ([`ActionRow`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionRowMetrics {
    /// Padding poziomy (lewy i prawy).
    pub pad_x: f64,
    /// Odstęp między tytułem a podtytułem.
    pub gap: f64,
}

/// Geometria powiadomienia ([`Toast`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToastMetrics {
    /// Odstęp od dolnej krawędzi okna.
    pub margin_bottom: f64,
    /// Padding poziomy (wewnątrz).
    pub pad_x: f64,
    /// Odstęp między tekstem a akcją.
    pub gap: f64,
    /// Wysokość powiadomienia.
    pub height: f64,
}

/// Geometria strony stanu ([`StatusPage`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusPageMetrics {
    /// Bok ikony.
    pub icon: f64,
    /// Odstęp pionowy między elementami.
    pub gap: f64,
    /// Szerokość przycisku akcji.
    pub button_w: f64,
    /// Wysokość przycisku akcji.
    pub button_h: f64,
}

/// Geometria wskaźnika zajętości ([`Spinner`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpinnerMetrics {
    /// Domyślny bok.
    pub size: f64,
    /// Grubość łuku.
    pub thickness: f64,
    /// Długość łuku jako ułamek pełnego okręgu (0..1).
    pub sweep: f64,
}

/// Geometria dymka podpowiedzi ([`Tooltip`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooltipMetrics {
    /// Padding poziomy.
    pub pad_x: f64,
    /// Padding pionowy.
    pub pad_y: f64,
    /// Promień zaokrąglenia.
    pub radius: f64,
    /// Odstęp od kotwicy (nad kotwicą).
    pub gap: f64,
}

/// Geometria banera ([`Banner`](crate::style)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BannerMetrics {
    /// Padding wewnętrzny.
    pub pad: f64,
    /// Odstęp między elementami.
    pub gap: f64,
    /// Bok ikony.
    pub icon: f64,
    /// Minimalna wysokość.
    pub min_height: f64,
    /// Wysokość przycisku akcji.
    pub button_h: f64,
}

/// Geometria kontrolek („component metrics”) — rozmiary i paddingi w pikselach logicznych.
///
/// Widgety nie mają już własnych stałych rozmiarów: czytają je z motywu, dzięki czemu
/// zmiana tokenów (np. w przyszłym, instalowanym stylu) steruje także geometrią kontrolek —
/// nie tylko kolorami.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub headerbar: HeaderBarMetrics,
    pub button: ButtonMetrics,
    pub switch: SwitchMetrics,
    pub slider: SliderMetrics,
    pub dropdown: DropDownMetrics,
    pub menu: MenuMetrics,
    pub nav: NavMetrics,
    pub segmented: SegmentedMetrics,
    pub tabs: TabsMetrics,
    pub sheet: BottomSheetMetrics,
    pub pager: PagerMetrics,
    pub pathbar: PathBarMetrics,
    pub entry: EntryMetrics,
    pub itemview: ItemViewMetrics,
    pub scrollview: ScrollViewMetrics,
    pub stepper: StepperMetrics,
    pub check: CheckMetrics,
    pub radio: RadioMetrics,
    pub card: CardMetrics,
    pub row: ActionRowMetrics,
    pub toast: ToastMetrics,
    pub status: StatusPageMetrics,
    pub spinner: SpinnerMetrics,
    pub tooltip: TooltipMetrics,
    pub banner: BannerMetrics,
}

impl Metrics {
    pub const fn standard() -> Self {
        Self {
            headerbar: HeaderBarMetrics {
                height: 46.0,
                control: 30.0,
                control_gap: 2.0,
            },
            button: ButtonMetrics {
                height: 34.0,
                pad_x: 14.0,
                chevron: 9.0,
                chevron_gap: 8.0,
            },
            switch: SwitchMetrics {
                width: 44.0,
                height: 24.0,
            },
            slider: SliderMetrics {
                knob_r: 9.0,
                hit_pad: 12.0,
                track_height: 6.0,
            },
            dropdown: DropDownMetrics {
                height: 34.0,
                pad_x: 12.0,
                chevron: 9.0,
                chevron_gap: 10.0,
                min_width: 140.0,
            },
            menu: MenuMetrics {
                item_h: 34.0,
                section_h: 26.0,
                separator_h: 9.0,
                pad_v: 6.0,
                pad_h: 10.0,
                label_gap: 20.0,
                min_width: 200.0,
                margin: 6.0,
                anchor_gap: 4.0,
            },
            nav: NavMetrics {
                item_h: 34.0,
                section_h: 30.0,
                pad_v: 6.0,
                pad_x: 12.0,
            },
            segmented: SegmentedMetrics { track_pad: 2.0 },
            tabs: TabsMetrics {
                height: 34.0,
                min_width: 120.0,
                max_width: 200.0,
                pad_x: 12.0,
                gap: 2.0,
                close: 16.0,
            },
            sheet: BottomSheetMetrics {
                height: 320.0,
                handle_width: 36.0,
                handle_height: 4.0,
                header: 44.0,
                radius: 12.0,
                scrim: 0.4,
                dismiss: 96.0,
            },
            pager: PagerMetrics {
                dot: 8.0,
                gap: 8.0,
                pad: 12.0,
            },
            pathbar: PathBarMetrics {
                height: 34.0,
                pad_x: 10.0,
                gap: 6.0,
            },
            entry: EntryMetrics {
                height: 34.0,
                pad_x: 12.0,
                icon: 16.0,
            },
            itemview: ItemViewMetrics {
                tile_w: 100.0,
                tile_h: 76.0,
                gap: 12.0,
                row_h: 44.0,
                icon: 28.0,
            },
            scrollview: ScrollViewMetrics {
                thickness: 6.0,
                min_thumb: 24.0,
                margin: 2.0,
            },
            stepper: StepperMetrics {
                height: 34.0,
                width: 120.0,
                button: 34.0,
            },
            check: CheckMetrics {
                size: 20.0,
                gap: 10.0,
            },
            radio: RadioMetrics {
                dot: 20.0,
                gap: 10.0,
                row_h: 30.0,
            },
            card: CardMetrics {
                header: 40.0,
                pad: 12.0,
            },
            row: ActionRowMetrics {
                pad_x: 12.0,
                gap: 8.0,
            },
            toast: ToastMetrics {
                margin_bottom: 40.0,
                pad_x: 16.0,
                gap: 16.0,
                height: 40.0,
            },
            status: StatusPageMetrics {
                icon: 64.0,
                gap: 12.0,
                button_w: 160.0,
                button_h: 34.0,
            },
            spinner: SpinnerMetrics {
                size: 24.0,
                thickness: 2.5,
                sweep: 0.75,
            },
            tooltip: TooltipMetrics {
                pad_x: 10.0,
                pad_y: 6.0,
                radius: 8.0,
                gap: 8.0,
            },
            banner: BannerMetrics {
                pad: 12.0,
                gap: 12.0,
                icon: 20.0,
                min_height: 56.0,
                button_h: 30.0,
            },
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::standard()
    }
}

/// Kompletny motyw: odstępy, promienie, typografia, geometria kontrolek i paleta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub spacing: Spacing,
    pub radius: Radius,
    pub metrics: Metrics,
    pub typography: Typography,
    pub colors: ColorScheme,
    pub appearance: Appearance,
}

impl Theme {
    pub const fn light() -> Self {
        Self {
            spacing: Spacing::standard(),
            radius: Radius::standard(),
            metrics: Metrics::standard(),
            typography: Typography::standard(),
            colors: ColorScheme::light(),
            appearance: Appearance::Light,
        }
    }

    pub const fn dark() -> Self {
        Self {
            spacing: Spacing::standard(),
            radius: Radius::standard(),
            metrics: Metrics::standard(),
            typography: Typography::standard(),
            colors: ColorScheme::dark(),
            appearance: Appearance::Dark,
        }
    }

    /// Ten sam motyw z podmienionym akcentem oraz kolorem treści na akcencie.
    pub fn accented(mut self, accent: Color, foreground: Color) -> Self {
        self.colors.accent = accent;
        self.colors.accent_fg = foreground;
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::light()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_and_dark_differ() {
        assert_ne!(
            Theme::light().colors.window_bg,
            Theme::dark().colors.window_bg
        );
        assert_eq!(Theme::light().appearance, Appearance::Light);
        assert_eq!(Theme::dark().appearance, Appearance::Dark);
    }

    #[test]
    fn standard_tokens_are_sane() {
        let s = Spacing::standard();
        assert!(s.xs < s.sm && s.sm < s.md && s.md < s.lg && s.lg < s.xl);
        let t = Typography::standard();
        assert!(t.title.size > t.heading.size && t.heading.size > t.body.size);
    }
}
