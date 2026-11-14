//! `zxdg_decoration_manager_v1` — negocjacja dekoracji okien.
//!
//! Domyślnie **my** rysujemy pasek tytułu (spójny wygląd całej Luny): na prośbę
//! o `client_side` odpowiadamy `server_side`, więc GTK3 nie rysuje własnego CSD.
//! Wyjątki: `LUNA_CLIENT_DECORATIONS=1` (klient decyduje) — przydatne, gdy jakaś
//! aplikacja naprawdę wymaga własnych dekoracji.

use wayland_protocols::xdg::decoration::zv1::server::{
    zxdg_decoration_manager_v1 as decoration_manager,
    zxdg_toplevel_decoration_v1 as toplevel_decoration,
};
use wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::shell::XdgToplevelData;
use crate::state::State;
use crate::sync::LockExt;

use libluna_core::{Color, Theme};
use libluna_render::ImageData;

/// Uchwyt obiektu dekoracji okna.
pub type ToplevelDecoration = toplevel_decoration::ZxdgToplevelDecorationV1;

/// Negocjowany tryb dekoracji okna.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecoMode {
    /// Dekoracje rysuje serwer (nasz pasek tytułu).
    Server,
    /// Dekoracje rysuje klient (CSD).
    Client,
}

/// Dane obiektu dekoracji — trzymamy powiązany `xdg_toplevel`, by móc odczytać
/// i zapisać tryb w [`XdgToplevelData`].
pub struct DecoData {
    pub toplevel: xdg_toplevel::XdgToplevel,
}

/// Paleta paska SSD dopasowana do **jasności** treści okna.
///
/// Nie kopiujemy koloru aplikacji (barwienie paska na średni kolor treści dawało
/// efekt „paska z innego środowiska”). Zamiast tego wybieramy neutralny wariant
/// motywu — ciemny dla jasnych aplikacji, jasny dla ciemnych — tak jak robi to
/// „adaptive” w libadwaita/KDE. Pasek zostaje spójny z resztą Luny, a jednocześnie
/// nie razi bielą nad czarnym terminalem ani czernią nad białym edytorem.
///
/// `offset_y` to górny margines bufora (cienie CSD), żeby nie próbkować cienia.
/// Zwraca `None`, gdy motyw już pasuje do treści (albo treść jest neutralna).
pub fn adaptive_header(img: &ImageData, offset_y: f64, theme: &Theme) -> Option<(Color, Color)> {
    let lum = top_luminance(img, offset_y)?;
    // Progi z zapasem: treść wideo nie może migotać kolorem paska przy każdej klatce.
    let want_dark = if luminance(theme.colors.headerbar_bg) < 0.5 {
        if lum <= 0.70 {
            return None;
        }
        false
    } else {
        if lum >= 0.35 {
            return None;
        }
        true
    };
    let (bg, fg) = variant_palette(want_dark);
    (bg != theme.colors.headerbar_bg).then_some((bg, fg))
}

/// Neutralna paleta paska (`tło`, `tekst/ikony`) z wbudowanego wariantu motywu.
///
/// Budowana raz — `Theme` ma pola tekstowe, więc nie chcemy alokować go na każdą
/// klatkę okna z jasną treścią.
fn variant_palette(dark: bool) -> (Color, Color) {
    static DARK: std::sync::OnceLock<(Color, Color)> = std::sync::OnceLock::new();
    static LIGHT: std::sync::OnceLock<(Color, Color)> = std::sync::OnceLock::new();
    if dark {
        *DARK.get_or_init(|| {
            let t = Theme::dark();
            (t.colors.headerbar_bg, t.colors.text)
        })
    } else {
        *LIGHT.get_or_init(|| {
            let t = Theme::light();
            (t.colors.headerbar_bg, t.colors.text)
        })
    }
}

/// Średnia luminancja górnego pasa treści (pasek narzędzi aplikacji).
///
/// Próbkujemy rzadko (do 64 kolumn w 36 wierszach) i pomijamy piksele
/// przezroczyste — cień CSD ani zaokrąglenia nie zaburzają wyniku.
fn top_luminance(img: &ImageData, offset_y: f64) -> Option<f64> {
    if img.width == 0 || img.height == 0 {
        return None;
    }
    let expected = img.width as usize * img.height as usize * 4;
    if img.rgba.len() < expected {
        return None;
    }
    let y0 = offset_y.max(0.0) as u32;
    let y1 = (y0 + 36).min(img.height);
    if y0 >= y1 {
        return None;
    }
    let step_x = (img.width / 64).max(1);
    let (mut sum, mut n) = (0.0f64, 0u64);
    for y in y0..y1 {
        let mut x = 0;
        while x < img.width {
            let i = ((y * img.width + x) * 4) as usize;
            if img.rgba[i + 3] > 200 {
                sum += 0.2126 * f64::from(img.rgba[i])
                    + 0.7152 * f64::from(img.rgba[i + 1])
                    + 0.0722 * f64::from(img.rgba[i + 2]);
                n += 1;
            }
            x += step_x;
        }
    }
    (n >= 8).then(|| sum / (n as f64) / 255.0)
}

fn luminance(c: Color) -> f64 {
    0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b)
}

impl GlobalDispatch<decoration_manager::ZxdgDecorationManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<decoration_manager::ZxdgDecorationManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<decoration_manager::ZxdgDecorationManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &decoration_manager::ZxdgDecorationManagerV1,
        request: decoration_manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let decoration_manager::Request::GetToplevelDecoration { id, toplevel } = request {
            let deco = data_init.init(
                id,
                DecoData {
                    toplevel: toplevel.clone(),
                },
            );
            if let Some(td) = toplevel.data::<XdgToplevelData>() {
                *td.decoration.lock_or_recover() = Some(deco);
            }
        }
    }
}

impl Dispatch<toplevel_decoration::ZxdgToplevelDecorationV1, DecoData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &toplevel_decoration::ZxdgToplevelDecorationV1,
        request: toplevel_decoration::Request,
        data: &DecoData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Some(td) = data.toplevel.data::<XdgToplevelData>() else {
            return;
        };
        match request {
            toplevel_decoration::Request::SetMode { mode } => {
                // **Zawsze dekorujemy sami.** GTK3 (thunar, xfce4-terminal, gedit…) domyślnie
                // prosi o `client_side`; gdybyśmy się zgodzili, każde okno miałoby inny
                // pasek tytułu (albo żaden). Odpowiadamy `server_side` — GTK wtedy wyłącza
                // własne CSD i korzysta z naszego paska, spójnego z resztą Luny.
                // `LUNA_CLIENT_DECORATIONS=1` przywraca dawną politykę (klient decyduje).
                let honor = std::env::var("LUNA_CLIENT_DECORATIONS").is_ok_and(|v| v != "0");
                let requested = if honor {
                    match mode {
                        WEnum::Value(toplevel_decoration::Mode::ServerSide) => DecoMode::Server,
                        _ => DecoMode::Client,
                    }
                } else {
                    DecoMode::Server
                };
                *td.deco.lock_or_recover() = Some(requested);
                let reply = match requested {
                    DecoMode::Server => toplevel_decoration::Mode::ServerSide,
                    DecoMode::Client => toplevel_decoration::Mode::ClientSide,
                };
                let _ = resource.send_event(toplevel_decoration::Event::Configure {
                    mode: WEnum::Value(reply),
                });
            }
            // Klient oddaje decyzję kompozytorowi — domyślnie dekorujemy my.
            toplevel_decoration::Request::UnsetMode => {
                *td.deco.lock_or_recover() = Some(DecoMode::Server);
                let _ = resource.send_event(toplevel_decoration::Event::Configure {
                    mode: WEnum::Value(toplevel_decoration::Mode::ServerSide),
                });
            }
            toplevel_decoration::Request::Destroy => {
                *td.deco.lock_or_recover() = None;
                *td.decoration.lock_or_recover() = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(top: [u8; 3], bottom: [u8; 3], alpha: u8) -> ImageData {
        let (w, h) = (80u32, 60u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let c = if y < 36 { top } else { bottom };
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], alpha]);
            }
        }
        ImageData::new(w, h, rgba)
    }

    #[test]
    fn dark_content_gets_dark_bar_on_light_theme() {
        let theme = Theme::light();
        let (bg, _fg) = adaptive_header(&image([20, 20, 20], [0, 0, 0], 255), 0.0, &theme)
            .expect("ciemna treść powinna dać ciemny pasek");
        assert!(luminance(bg) < 0.2);
        assert_ne!(bg, theme.colors.headerbar_bg);
    }

    #[test]
    fn light_content_gets_light_bar_on_dark_theme() {
        let theme = Theme::dark();
        let (bg, fg) = adaptive_header(&image([245, 245, 245], [255, 255, 255], 255), 0.0, &theme)
            .expect("jasna treść powinna dać jasny pasek");
        assert!(luminance(bg) > 0.8);
        assert!(luminance(fg) < 0.3, "na jasnym pasku tekst musi być ciemny");
    }

    #[test]
    fn matching_content_keeps_theme() {
        assert!(
            adaptive_header(&image([20, 20, 20], [0, 0, 0], 255), 0.0, &Theme::dark()).is_none(),
            "ciemna treść na ciemnym motywie nie zmienia paska"
        );
        assert!(
            adaptive_header(
                &image([245, 245, 245], [255, 255, 255], 255),
                0.0,
                &Theme::light()
            )
            .is_none(),
            "jasna treść na jasnym motywie nie zmienia paska"
        );
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        // Sam cień CSD (alfa 0) nie niesie koloru — pasek zostaje z motywu.
        assert!(adaptive_header(&image([0, 0, 0], [0, 0, 0], 0), 0.0, &Theme::light()).is_none());
    }

    #[test]
    fn shadow_offset_skips_the_margin() {
        // 22 px kryjącego cienia nad jasnym paskiem aplikacji: bez pominięcia
        // marginesu średnia wypada szaro (brak adaptacji), z pominięciem — jasno.
        let (w, h) = (80u32, 80u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let c = if y < 22 { [0, 0, 0] } else { [245, 245, 245] };
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let img = ImageData::new(w, h, rgba);
        let theme = Theme::dark();
        assert!(adaptive_header(&img, 0.0, &theme).is_none());
        assert!(adaptive_header(&img, 22.0, &theme).is_some());
    }
}
