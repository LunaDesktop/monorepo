//! __APP_NAME__ — aplikacja środowiska Luna na libLunie (szablon startowy).

use libluna::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    set_locale(Locale::from_env());

    let config = AppConfig::new("__APP_NAME__").initial_size(800, 600);

    run(config, |frame: &mut Frame| {
        let theme = frame.theme;
        let (w, h) = (frame.size.width, frame.size.height);
        let scene = &mut *frame.scene;

        scene.fill_rect(Rect::from_xywh(0.0, 0.0, w, h), theme.colors.window_bg);
        scene.text(
            TextRun::new(Point::new(24.0, 48.0), &tr("Application"), 20.0)
                .with_weight(700)
                .with_color(theme.colors.text),
        );
    })
}
