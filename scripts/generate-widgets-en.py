#!/usr/bin/env python3
"""Generuje angielską wersję danych biblioteki widgetów (`widgets-en.js`).

Czyta `widgets.js` wygenerowany przez `export-web-widgets` i podmienia
kategorie oraz opisy na angielskie. Nazwy, źródła i przykłady kodu zostają
bez zmian (są już po angielsku).

Uruchamiane przez `make web-assets` po eksporcie widgetów.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "web/luna.paganlinux.eu/assets/widgets.js"
OUT = ROOT / "web/luna.paganlinux.eu/assets/widgets-en.js"

CATEGORIES = {
    "Podstawy": "Basics",
    "Formularze": "Forms",
    "Nawigacja": "Navigation",
    "Układ": "Layout",
    "Kolekcje": "Collections",
    "Media": "Media",
    "Informacje": "Feedback",
    "Okna": "Windows",
}

DESCRIPTIONS = {
    "button": "Action button: default, suggested, destructive and disabled.",
    "togglebutton-linkbutton-colorbutton": "Special buttons: toggle, link and color swatch.",
    "splitbutton": "Primary action plus a separate arrow that opens a menu.",
    "menubutton": "A button that opens a menu; also available as a hamburger variant.",
    "label": "Text label with alignment, bold and dimmed styles.",
    "separator": "Thin separator line, horizontal or vertical.",
    "image": "Vector icons available through the ImageIcon set.",
    "avatar": "Round avatar with initials, scaled to the theme size.",
    "spinner": "Busy indicator (rotating arc); the phase can be set manually.",
    "progressbar-levelbar": "Linear progress with a label and a segmented level bar.",
    "shortcutlabel": "Keyboard shortcut rendered as separate key caps.",
    "tooltip": "Tooltip bubble positioned relative to an anchor.",
    "iconname-paint-icon": "A consistent set of 47 pastel vector icons (files, terminal, media, devices).",
    "entry": "Text field: caret, selection, leading icon and clear button.",
    "editablelabel": "A label that becomes an editable field when clicked.",
    "passwordentry": "Password field with masking and an eye reveal toggle.",
    "searchentry": "Search field with a magnifier and a clear button.",
    "searchbar": "Search-mode bar with a reveal button.",
    "textview": "Multi-line text field with editing and selection.",
    "spinbutton": "Numeric field with up/down arrows and a range.",
    "stepper": "A `−  value  +` stepper for small ranges.",
    "slider": "0..1 slider; hit-testing and value derived from the track rectangle.",
    "switch": "On/off switch with an animated thumb.",
    "checkbutton": "Checkbox with a label.",
    "radiogroup": "List of options with a single selection.",
    "dropdown": "Drop-down list; expanding returns a ready-made Menu.",
    "togglegroup": "Group of independent toggles (multiple selection).",
    "calendar": "Month view with a day grid, selection and prev/next navigation.",
    "colorchooser": "Color palette in a grid with selection and hit-testing.",
    "emojichooser": "Grid of symbols/emoji with selection and a preview.",
    "viewswitcher": "Segmented view switcher; alias StackSwitcher.",
    "navlist": "Navigation menu with sections; alias ListBox.",
    "navigationrail": "Vertical navigation rail (icons + labels).",
    "tabbar": "Tab bar with scrolling and close buttons.",
    "tabview": "Tab page model (add, close, select); alias Notebook.",
    "pathbar": "Breadcrumb path that collapses into an ellipsis.",
    "pager": "Page carousel with dragging and dots; alias Carousel.",
    "navigationview": "Stack of pages with a back bar and navigation depth.",
    "navigationsplitview": "Sidebar plus content; the sidebar can be collapsed.",
    "overlaysplitview": "Overlay sidebar for narrow windows with a dimming scrim.",
    "sidebar": "Sidebar background and edge; content goes on a NavList.",
    "popovermenubar": "Row of buttons that open menu popovers.",
    "menu": "Popover with items, separators and shortcuts.",
    "card": "Card with a header and a content area.",
    "preferencesgroup": "Settings group: title plus a frame for rows.",
    "preferencespage": "Vertical stack of settings groups (a ready preferences page).",
    "actionrow": "Settings row: title/subtitle plus space for a control.",
    "row": "The boxed-list family: switch, combo, nav, expander, entry, spin…",
    "expander": "Collapsible section with a header and content.",
    "flowbox": "Fixed-size item grid with wrapping.",
    "paned": "Two panes with a draggable separator.",
    "stack": "Stack of pages with animated transitions (slide, fade, etc.).",
    "grid-fixed-centerbox-bin": "Layout helpers: grid, absolute positioning, three zones and a single child.",
    "clamp-aspectframe-frame-overlay-revealer": "Width clamping, aspect ratio, frame, overlay and revealer.",
    "wrapbox-breakpointbin-buttoncontent": "Item wrapping, responsive breakpoints and icon+label layout.",
    "toolbarview-viewport-windowhandle-actionbar": "Window helpers: toolbar, visible viewport, drag area and action bar.",
    "itemview": "File list/grid with icons, details and multi-selection.",
    "columnview": "Column table with headers, weights and row selection.",
    "scrollview": "Vertical scrolling with a thumb; alias ScrolledWindow.",
    "scrollbar-adjustment": "Scrollbar for both axes and the range model (Adjustment).",
    "mediacontrols": "Media player: title, artist, progress, time and controls.",
    "picture": "Image with content fitting: Contain, Cover, Fill, FitWidth, FitHeight.",
    "infobar": "Message bar: info, question, warning, error plus an action.",
    "banner": "Prominent message with a description and an action.",
    "toast": "Short overlay notification with an action (e.g. Undo).",
    "statuspage": "Status screen: icon, title, description and an action (empty state, no results).",
    "statusbar": "Bottom status bar: message on the left, counter on the right.",
    "popover": "Panel anchored to a widget (above/below/beside) with dismissal.",
    "headerbar": "CSD title bar: title, window controls and a content area.",
    "windowtitle": "Window title and subtitle laid out around the window controls.",
    "dialog": "Base modal window: scrim, panel, header and close button.",
    "alertdialog": "Alert with content and action buttons (result via action_at).",
    "aboutdialog": "About dialog: name, version, author and website.",
    "shortcutsdialog": "List of keyboard shortcuts with action labels.",
    "preferencesdialog": "Settings dialog with a fixed height, ready for PreferencesPage.",
    "bottomsheet": "Bottom modal sheet: grabber, dismissal, animation and dragging.",
    "assistant": "Step-by-step assistant with a header, progress and navigation buttons.",
}


def field(block: str, name: str) -> str:
    match = re.search(rf'^\s*{name}: "((?:\\.|[^"\\])*)",$', block, re.M)
    if not match:
        raise SystemExit(f"brak pola {name} w bloku:\n{block[:200]}")
    return match.group(1)


def main() -> None:
    source = SRC.read_text(encoding="utf-8")
    blocks = re.findall(r"  \{\n(.*?)\n  \},", source, re.S)
    if not blocks:
        raise SystemExit(f"nie znaleziono widgetów w {SRC}")

    lines = [
        "/* English widget library data — generated by scripts/generate-widgets-en.py. */",
        "window.LUNA_WIDGETS = [",
    ]
    missing = []
    for block in blocks:
        slug = field(block, "slug")
        category = field(block, "category")
        if slug not in DESCRIPTIONS:
            missing.append(slug)
            continue
        if category not in CATEGORIES:
            raise SystemExit(f"brak tłumaczenia kategorii: {category}")
        lines.append("  {")
        lines.append(f'    slug: {json.dumps(slug, ensure_ascii=False)},')
        lines.append(f'    name: "{field(block, "name")}",')
        lines.append(f'    category: {json.dumps(CATEGORIES[category], ensure_ascii=False)},')
        lines.append(
            f'    description: {json.dumps(DESCRIPTIONS[slug], ensure_ascii=False)},'
        )
        lines.append(f'    source: "{field(block, "source")}",')
        lines.append(f'    code: "{field(block, "code")}",')
        lines.append("  },")
    lines.append("];")
    lines.append("")

    if missing:
        raise SystemExit("brak angielskich opisów dla: " + ", ".join(missing))
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"zapisano {len(blocks)} angielskich widgetów do {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
