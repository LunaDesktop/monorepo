//! # libluna-core
//!
//! Fundament libLuny: prymitywy niezależne od platformy ani backendu rysowania.
//!
//! - [`geometry`] — `Point`, `Size`, `Rect`, `Insets` (w pikselach logicznych).
//! - [`color`] — [`Color`] w przestrzeni sRGB z kanałem alfa.
//! - [`scale`] — [`Scale`] do przeliczania pikseli logicznych na fizyczne.
//! - [`event`] — [`Event`] i towarzyszące typy wejścia.
//! - [`motion`] — krzywe [`Easing`] i interpolacja [`lerp`] (matematyka ruchu).
//! - [`runtime`] — wybór katalogu `XDG_RUNTIME_DIR` (wspólny dla kompozytora i klientów).
//! - [`style`] — tokeny wyglądu ([`style::Theme`], spacing, radius, typografia).
//!
//! Moduł nie zależy od Waylanda ani od konkretnego rasteryzatora — jest wspólnym
//! słownikiem typów dla całej reszty toolkitu.

pub mod appearance;
pub mod color;
pub mod event;
pub mod geometry;
pub mod motion;
pub mod runtime;
pub mod scale;
pub mod style;
pub mod theme_file;

pub use appearance::{Accent, ThemePreference, ThemeSettings};
pub use color::Color;
pub use event::{
    ButtonState, Event, KeyEvent, Modifiers, PointerButton, PointerEvent, ScrollEvent, WindowEvent,
};
pub use geometry::{Insets, Point, Rect, Size};
pub use motion::{lerp, Animation, Easing};
pub use scale::Scale;
pub use style::{
    ActionRowMetrics, AvatarMetrics, BannerMetrics, BarMetrics, BottomSheetMetrics, ButtonMetrics,
    CardMetrics, CheckMetrics, ContainerMetrics, DropDownMetrics, EntryMetrics, GroupMetrics,
    HeaderBarMetrics, ItemViewMetrics, MenuMetrics, Metrics, NavMetrics, PagerMetrics,
    PathBarMetrics, RadioMetrics, Radius, ScrollViewMetrics, SegmentedMetrics, ShortcutMetrics,
    SliderMetrics, Spacing, SpinnerMetrics, SplitMetrics, StatusPageMetrics, StepperMetrics,
    SwitchMetrics, TabsMetrics, Theme, ToastMetrics, TooltipMetrics, Typography,
};
pub use theme_file::{available as available_themes, StyledTheme, ThemePatch};
