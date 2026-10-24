//! Sonda wejścia: okno Wayland, które wypisuje każde kliknięcie i klawisz.
//!
//! Używana w testach Selene do sprawdzenia, czy kliknięcia naprawdę docierają do
//! klienta (a nie są przechwytywane przez panel/powłokę):
//!
//! ```sh
//! SELENE_INPUT_SCRIPT=/tmp/input.txt target/release/selene &
//! WAYLAND_DISPLAY=wayland-0 target/release/input-probe 2>/tmp/probe.log &
//! echo "click 300 300" >>/tmp/input.txt
//! ```
//!
//! Nie jest instalowana przez `scripts/install.sh` — to narzędzie diagnostyczne.

use std::error::Error;

use libluna_core::{ButtonState, Color, Event, Rect};
use libluna_platform::{run, AppConfig, Frame};

fn main() -> Result<(), Box<dyn Error>> {
    let cfg = AppConfig::new("Input Probe")
        .app_id("eu.paganlinux.Luna.InputProbe")
        .initial_size(600, 400);
    run(cfg, |frame: &mut Frame| {
        for event in frame.events {
            match event {
                Event::PointerButton(b) => eprintln!(
                    "probe: button {:?} {:?} at ({:.0},{:.0})",
                    b.button, b.state, b.position.x, b.position.y
                ),
                Event::Key(k) if k.state == ButtonState::Pressed => {
                    eprintln!("probe: key 0x{:x} text={:?}", k.keysym, k.text);
                }
                Event::Scroll(s) => {
                    eprintln!("probe: scroll ({:.1},{:.1})", s.dx, s.dy);
                }
                _ => {}
            }
        }
        let size = frame.size;
        frame.scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, size.width, size.height),
            Color::from_rgba8(0x20, 0x40, 0x60, 0xff),
        );
        // Wyraźny prostokąt, żeby było widać okno na zrzutach.
        frame.scene.fill_rect(
            Rect::from_xywh(40.0, 40.0, 200.0, 120.0),
            Color::from_rgba8(0x30, 0x90, 0x60, 0xff),
        );
        frame.request_frame();
    })
}
