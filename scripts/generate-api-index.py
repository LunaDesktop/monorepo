#!/usr/bin/env python3
"""Generuje indeks publicznego API libLuny dla strony dokumentacji.

Skrypt czyta źródła z `libluna/crates/*/src`, wyciąga publiczne elementy
(struktury, enumy, traity, funkcje, metody, stałe, moduły i makra) wraz
z komentarzami `///` i zapisuje `web/luna.paganlinux.eu/assets/api-data.js`.

To indeks „best effort” — pełny, autorytatywny opis generuje rustdoc
(`make web-api`). Indeks jest po to, by dało się przeszukiwać API bez
budowania dokumentacji i bez sieci.

Uruchom z katalogu głównego repozytorium:

    python3 scripts/generate-api-index.py
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES_DIR = ROOT / "libluna" / "crates"
OUT = ROOT / "web" / "luna.paganlinux.eu" / "assets" / "api-data.js"
OUT_EN = OUT.with_name("api-data-en.js")
GITHUB = "https://github.com/mijsys/libLuna/blob/main"

CRATE_ORDER = [
    "libluna",
    "libluna-core",
    "libluna-render",
    "libluna-platform",
    "libluna-widgets",
    "libluna-i18n",
]

ITEM_RE = re.compile(
    r"^\s*pub\s+(?P<kind>struct|enum|trait|type|const|fn|mod)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
MACRO_RE = re.compile(r"^\s*macro_rules!\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)")
IMPL_RE = re.compile(
    r"^\s*impl(?:<[^{]*?>)?\s+(?:[^\{;]*?\s+for\s+)?(?P<owner>[A-Za-z_][A-Za-z0-9_]*)"
)
DOC_RE = re.compile(r"^\s*///\s?(?P<text>.*)$")
ATTR_RE = re.compile(r"^\s*#\[")


@dataclass
class Item:
    crate: str
    module: str
    kind: str
    name: str
    signature: str
    doc: str
    source: str
    line: int
    owner: str = ""


def module_path(crate_dir: Path, file: Path) -> str:
    rel = file.relative_to(crate_dir / "src")
    parts = list(rel.with_suffix("").parts)
    if parts and parts[-1] in {"lib", "mod"}:
        parts.pop()
    return "::".join(parts) if parts else "(root)"


def trim_doc(lines: list[str]) -> str:
    text = " ".join(line.strip() for line in lines if line.strip())
    return re.sub(r"\s+", " ", text).strip()


def signature_of(lines: list[str]) -> str:
    sig = " ".join(line.strip() for line in lines)
    sig = re.sub(r"\s+", " ", sig).strip()
    sig = sig.rstrip("{").strip()
    if not sig.endswith(";"):
        sig += ";"
    return sig


def parse_file(crate: str, crate_dir: Path, file: Path) -> list[Item]:
    try:
        text = file.read_text(encoding="utf-8")
    except OSError:
        return []

    items: list[Item] = []
    module = module_path(crate_dir, file)
    lines = text.splitlines()
    doc: list[str] = []
    owner = ""
    impl_depth: int | None = None
    depth = 0
    i = 0

    while i < len(lines):
        line = lines[i]

        doc_match = DOC_RE.match(line)
        if doc_match:
            doc.append(doc_match.group("text"))
            i += 1
            continue
        if line.strip().startswith("//") or ATTR_RE.match(line) or not line.strip():
            i += 1
            continue

        impl_match = IMPL_RE.match(line)
        if impl_match and line.rstrip().endswith("{"):
            owner = impl_match.group("owner")
            impl_depth = depth + line.count("{") - line.count("}")
            depth += line.count("{") - line.count("}")
            doc = []
            i += 1
            continue

        item_match = ITEM_RE.match(line)
        if item_match:
            kind = item_match.group("kind")
            name = item_match.group("name")
            start_line = i + 1

            if kind == "mod":
                items.append(
                    Item(
                        crate=crate,
                        module=module,
                        kind="mod",
                        name=name,
                        signature=f"pub mod {name};",
                        doc=trim_doc(doc),
                        source=str(file.relative_to(ROOT)),
                        line=start_line,
                    )
                )
                doc = []
                i += 1
                continue

            # Zbierz sygnaturę aż do `{` / `;` na poziomie deklaracji.
            sig_lines = [line]
            while (
                i + 1 < len(lines)
                and "{" not in sig_lines[-1]
                and ";" not in sig_lines[-1]
            ):
                i += 1
                sig_lines.append(lines[i])
            signature = signature_of(sig_lines)

            if kind == "fn" and owner:
                kind = "method"
            items.append(
                Item(
                    crate=crate,
                    module=module,
                    kind=kind,
                    name=name,
                    owner=owner if kind in {"method", "const"} else "",
                    signature=signature,
                    doc=trim_doc(doc),
                    source=str(file.relative_to(ROOT)),
                    line=start_line,
                )
            )
            doc = []
            # Nie przeskakujemy ciała funkcji — potrzebujemy tylko deklaracji.
            depth += line.count("{") - line.count("}")
            i += 1
            continue

        macro_match = MACRO_RE.match(line)
        if macro_match:
            name = macro_match.group("name")
            sig = "macro_rules! " + name
            items.append(
                Item(
                    crate=crate,
                    module=module,
                    kind="macro",
                    name=name,
                    signature=sig,
                    doc=trim_doc(doc),
                    source=str(file.relative_to(ROOT)),
                    line=i + 1,
                )
            )
            doc = []
            depth += line.count("{") - line.count("}")
            i += 1
            continue

        if not line.strip().startswith("//"):
            depth += line.count("{") - line.count("}")
            if impl_depth is not None and depth < impl_depth:
                owner = ""
                impl_depth = None
        doc = []
        i += 1

    return items


def main() -> None:
    items: list[Item] = []
    for crate_dir in sorted(CRATES_DIR.iterdir()):
        if not crate_dir.is_dir():
            continue
        crate = crate_dir.name
        if crate not in CRATE_ORDER:
            continue
        src = crate_dir / "src"
        if not src.exists():
            continue
        for file in sorted(src.rglob("*.rs")):
            items.extend(parse_file(crate, crate_dir, file))

    # Usuń duplikaty i posortuj stabilnie.
    seen: set[tuple[str, str, str, str]] = set()
    unique: list[Item] = []
    for item in items:
        key = (item.crate, item.module, item.kind, item.owner + "::" + item.name)
        if key in seen:
            continue
        seen.add(key)
        unique.append(item)

    version = "0.0.4"
    cargo = CRATES_DIR / "libluna" / "Cargo.toml"
    if cargo.exists():
        match = re.search(r'^version\s*=\s*"([^"]+)"', cargo.read_text(encoding="utf-8"), re.M)
        if match:
            version = match.group(1)

    unique.sort(
        key=lambda item: (
            CRATE_ORDER.index(item.crate) if item.crate in CRATE_ORDER else 99,
            item.module,
            0 if item.kind in {"mod", "struct", "enum", "trait", "type", "macro"} else 1,
            item.owner,
            item.name,
        )
    )

    data = {
        "version": version,
        "items": [
            {
                "crate": item.crate,
                "module": item.module,
                "kind": item.kind,
                "name": item.name,
                "owner": item.owner,
                "signature": item.signature,
                "doc": item.doc,
                "source": item.source,
                "line": item.line,
                "url": f"{GITHUB}/{item.source}#L{item.line}",
            }
            for item in unique
        ],
    }

    OUT.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(data, ensure_ascii=False, indent=2)
    OUT.write_text(
        "/* Wygenerowane przez scripts/generate-api-index.py — nie edytuj ręcznie. */\n"
        f"window.LUNA_API = {payload};\n",
        encoding="utf-8",
    )

    # English index: exact signatures and source links; source doc comments
    # are still written in Polish, so they are omitted instead of shown
    # out of context. Curated English descriptions live in the guide pages.
    data_en = {
        "version": version,
        "items": [{**item, "doc": ""} for item in data["items"]],
    }
    OUT_EN.write_text(
        "/* Generated by scripts/generate-api-index.py — do not edit manually. */\n"
        f"window.LUNA_API = {json.dumps(data_en, ensure_ascii=False, indent=2)};\n",
        encoding="utf-8",
    )
    print(f"zapisano {len(unique)} elementów API do {OUT.relative_to(ROOT)}")
    print(f"zapisano {len(unique)} elementów API do {OUT_EN.relative_to(ROOT)}")
    by_kind: dict[str, int] = {}
    for item in unique:
        by_kind[item.kind] = by_kind.get(item.kind, 0) + 1
    print(", ".join(f"{kind}: {count}" for kind, count in sorted(by_kind.items())))


if __name__ == "__main__":
    main()
