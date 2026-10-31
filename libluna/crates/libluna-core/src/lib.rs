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

pub mod accessibility;
pub mod accounts;
pub mod appearance;
pub mod civil;
pub mod clock_state;
pub mod color;
pub mod config;
pub mod display;
pub mod equalizer;
pub mod event;
pub mod geometry;
pub mod greeter;
pub mod holidays;
pub mod icons;
pub mod idle;
pub mod input;
pub mod lock;
pub mod motion;
pub mod os;
pub mod region;
pub mod runtime;
pub mod scale;
pub mod shortcuts;
pub mod style;
pub mod theme_file;
pub mod workspaces;

pub use accessibility::{AccessibilitySettings, TEXT_SCALE_CHOICES};
pub use appearance::{Accent, ThemePreference, ThemeSettings};
pub use color::Color;
pub use display::{DisplaySettings, OutputMode, OutputPreference};
pub use event::{
    ButtonState, Event, KeyEvent, Modifiers, PointerButton, PointerEvent, ScrollEvent, WindowEvent,
};
pub use geometry::{Insets, Point, Rect, Size};
pub use greeter::GreeterSettings;
pub use icons::{IconSet, IconSettings, IconTheme, DEFAULT_ICON_SET};
pub use input::{
    InputSettings, MAX_REPEAT_DELAY, MAX_REPEAT_RATE, MIN_REPEAT_DELAY, MIN_REPEAT_RATE,
};
pub use lock::{LockSettings, LockWallpaper};
pub use motion::{lerp, Animation, Easing};
pub use os::name as os_name;
pub use region::RegionSettings;
pub use scale::Scale;
pub use shortcuts::{Binding, ShortcutAction, ShortcutProfile, ShortcutSettings};
pub use style::{
    ActionRowMetrics, AvatarMetrics, BannerMetrics, BarMetrics, BottomSheetMetrics, ButtonMetrics,
    CardMetrics, CheckMetrics, ContainerMetrics, DropDownMetrics, EntryMetrics, GroupMetrics,
    HeaderBarMetrics, ItemViewMetrics, MenuMetrics, Metrics, NavMetrics, PagerMetrics,
    PathBarMetrics, RadioMetrics, Radius, ScrollViewMetrics, SegmentedMetrics, ShortcutMetrics,
    SliderMetrics, Spacing, SpinnerMetrics, SplitMetrics, StatusPageMetrics, StepperMetrics,
    SwitchMetrics, SyntaxColors, TabsMetrics, Theme, ToastMetrics, TooltipMetrics, Typography,
};
pub use theme_file::{available as available_themes, StyledTheme, ThemePatch};
pub use workspaces::{WorkspaceLayout, MAX_WORKSPACES, MIN_WORKSPACES};
