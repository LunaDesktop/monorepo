#!/usr/bin/env python3
"""Wykrywa binaria i biblioteki z niezaspokojonymi zależnościami ELF (DT_NEEDED).

To strażnik przed klasą błędów, która zepsuła darktable (x265 4.2 → 4.3,
`libx265.so.217` → `.218`) i portale (`libgnome-desktop-3.so.20`): pakiet
został zbudowany przeciw starszej wersji biblioteki, a receptura nie
deklarowała jej w `depends`, więc bump soname nie wymusił przebudowy.

Użycie:
    ./scripts/check-runtime-deps.py            # przegląd całego systemu
    ./scripts/check-runtime-deps.py -v         # dodatkowo liczba sprawdzonych plików

Kod wyjścia 1 = znaleziono zerwane zależności.

Skrypt czyta tylko pliki ELF (nagłówek + sekcja dynamiczna) — nie wymaga
`readelf`/`ldd` (te same mogą być zepsute przez brakujący soname).
"""

from __future__ import annotations

import argparse
import os
import struct
import subprocess
import sys

# Katalogi z programami i bibliotekami „aktywnymi” (bez starych kopii .so).
BIN_DIRS = ("/usr/bin", "/usr/sbin", "/usr/libexec", "/usr/local/bin", "/usr/local/sbin")
LIB_DIRS = ("/usr/lib", "/usr/lib64", "/lib", "/lib64", "/usr/local/lib")

PT_LOAD = 1
PT_DYNAMIC = 2
DT_NEEDED = 1
DT_STRTAB = 5
DT_RPATH = 15
DT_RUNPATH = 29


def active_libraries() -> set[str]:
    """Ścieżki realne bibliotek, które są faktycznie używane (symlinki + cache)."""
    active: set[str] = set()
    for root in LIB_DIRS:
        try:
            entries = os.scandir(root)
        except OSError:
            continue
        with entries:
            for entry in entries:
                try:
                    if entry.is_symlink():
                        active.add(os.path.realpath(entry.path))
                    elif entry.is_file() and ".so" in entry.name:
                        active.add(entry.path)
                except OSError:
                    pass
    # Węzły z cache dynamicznego linkera (np. /lib/x86_64-linux-gnu/...).
    try:
        out = subprocess.run(
            ["ldconfig", "-p"], capture_output=True, text=True, check=False
        ).stdout
        for line in out.splitlines():
            if "=>" in line:
                path = line.rsplit("=>", 1)[1].strip()
                if path:
                    active.add(os.path.realpath(path))
    except OSError:
        pass
    return active


def provided_sonames() -> set[str]:
    names: set[str] = set()
    for root in LIB_DIRS:
        try:
            entries = os.scandir(root)
        except OSError:
            continue
        with entries:
            for entry in entries:
                if ".so" in entry.name:
                    names.add(entry.name)
    try:
        out = subprocess.run(
            ["ldconfig", "-p"], capture_output=True, text=True, check=False
        ).stdout
        for line in out.splitlines():
            if "=>" in line:
                # „libfoo.so.1 (libc6,x86-64) => /lib/...”
                names.add(line.split()[0])
    except OSError:
        pass
    return names


def elf_needed(path: str) -> tuple[list[str], list[str]] | None:
    """`(DT_NEEDED, katalogi RPATH/RUNPATH)` albo None, gdy plik nie jest ELF-em."""
    try:
        with open(path, "rb") as fh:
            data = fh.read()
    except OSError:
        return None
    if len(data) < 64 or data[:4] != b"\x7fELF":
        return None
    is64 = data[4] == 2
    endian = "<" if data[5] == 1 else ">"
    try:
        if is64:
            phoff = struct.unpack_from(endian + "Q", data, 0x20)[0]
            phentsize = struct.unpack_from(endian + "H", data, 0x36)[0]
            phnum = struct.unpack_from(endian + "H", data, 0x38)[0]
        else:
            phoff = struct.unpack_from(endian + "I", data, 0x1C)[0]
            phentsize = struct.unpack_from(endian + "H", data, 0x2A)[0]
            phnum = struct.unpack_from(endian + "H", data, 0x2C)[0]
        segments: list[tuple[int, int, int, int]] = []
        for i in range(phnum):
            off = phoff + i * phentsize
            if off + phentsize > len(data):
                break
            p_type = struct.unpack_from(endian + "I", data, off)[0]
            if is64:
                p_offset = struct.unpack_from(endian + "Q", data, off + 8)[0]
                p_vaddr = struct.unpack_from(endian + "Q", data, off + 16)[0]
                p_filesz = struct.unpack_from(endian + "Q", data, off + 32)[0]
            else:
                p_offset = struct.unpack_from(endian + "I", data, off + 4)[0]
                p_vaddr = struct.unpack_from(endian + "I", data, off + 8)[0]
                p_filesz = struct.unpack_from(endian + "I", data, off + 16)[0]
            segments.append((p_type, p_offset, p_vaddr, p_filesz))
        dynamic = [s for s in segments if s[0] == PT_DYNAMIC]
        if not dynamic:
            return [], []
        _, dyn_off, _, dyn_size = dynamic[0]
        entsize = 16 if is64 else 8
        fmt = endian + ("qQ" if is64 else "iI")
        entries: list[tuple[int, int]] = []
        strtab_addr = None
        for off in range(dyn_off, dyn_off + dyn_size, entsize):
            if off + entsize > len(data):
                break
            tag, val = struct.unpack_from(fmt, data, off)
            if tag == 0:
                break
            if tag == DT_STRTAB:
                strtab_addr = val
            entries.append((tag, val))
        if strtab_addr is None:
            return [], []

        def vaddr_to_offset(addr: int) -> int | None:
            for p_type, p_offset, p_vaddr, p_filesz in segments:
                if p_type == PT_LOAD and p_vaddr <= addr < p_vaddr + p_filesz:
                    return p_offset + (addr - p_vaddr)
            return None

        stroff = vaddr_to_offset(strtab_addr)
        if stroff is None:
            return [], []
        needed = []
        runpaths = []
        origin = os.path.dirname(os.path.realpath(path))
        for tag, val in entries:
            if tag == DT_NEEDED:
                start = stroff + val
                if start >= len(data):
                    continue
                end = data.find(b"\0", start)
                if end < 0:
                    continue
                needed.append(data[start:end].decode("utf-8", "replace"))
            elif tag in (DT_RPATH, DT_RUNPATH):
                start = stroff + val
                if start >= len(data):
                    continue
                end = data.find(b"\0", start)
                if end < 0:
                    continue
                raw = data[start:end].decode("utf-8", "replace")
                for part in raw.split(":"):
                    part = part.replace("$ORIGIN", origin).replace("${ORIGIN}", origin)
                    if part:
                        runpaths.append(part)
        return needed, runpaths
    except (struct.error, IndexError, ValueError):
        return [], []


def scan(paths: list[str]) -> list[tuple[str, str]]:
    provided = provided_sonames()
    missing: list[tuple[str, str]] = []
    seen: set[str] = set()
    for path in paths:
        real = os.path.realpath(path)
        if real in seen:
            continue
        seen.add(real)
        result = elf_needed(path)
        if result is None:
            continue
        needed, runpaths = result
        for soname in needed:
            if soname == "linux-vdso.so.1":
                continue
            if soname.startswith("/"):
                ok = os.path.exists(soname)
            else:
                # Biblioteki prywatne (PulseAudio, man-db, portale) leżą poza
                # cache linkera i są znajdowane przez RUNPATH/RPATH.
                ok = soname in provided or any(
                    os.path.exists(os.path.join(runpath, soname)) for runpath in runpaths
                )
            if not ok:
                missing.append((path, soname))
    return missing


def collect() -> list[str]:
    paths: list[str] = []
    for root in BIN_DIRS:
        try:
            entries = os.scandir(root)
        except OSError:
            continue
        with entries:
            for entry in entries:
                try:
                    # W katalogach bibliotek leżą też pomocnicze binaria
                    # (np. `/usr/lib/xdg-desktop-portal-gtk`) — też je sprawdzamy.
                    if entry.is_file() and os.access(entry.path, os.X_OK):
                        paths.append(entry.path)
                except OSError:
                    pass
    for root in LIB_DIRS:
        try:
            entries = os.scandir(root)
        except OSError:
            continue
        with entries:
            for entry in entries:
                try:
                    if entry.is_file() and os.access(entry.path, os.X_OK):
                        paths.append(entry.path)
                except OSError:
                    pass
    # Tylko „aktywne” biblioteki — stare kopie (`libfoo.so.1.2.3`) są pomijane,
    # żeby nie zgłaszać plików, których nic już nie ładuje.
    paths.extend(active_libraries())
    return paths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-v", "--verbose", action="store_true")
    args = parser.parse_args()

    paths = collect()
    missing = scan(paths)
    if not missing:
        print(f"OK: {len(paths)} plików, wszystkie zależności ELF rozwiązane")
        return 0

    by_soname: dict[str, list[str]] = {}
    by_file: dict[str, list[str]] = {}
    for path, soname in missing:
        by_soname.setdefault(soname, []).append(path)
        by_file.setdefault(path, []).append(soname)

    print(f"ZNALEZIONO {len(by_file)} plików z zerwanymi zależnościami:\n")
    for path, sonames in sorted(by_file.items()):
        print(f"  {path}")
        for soname in sonames:
            print(f"      brakuje: {soname}")
    print("\nPodsumowanie brakujących soname:")
    for soname, files in sorted(by_soname.items()):
        print(f"  {soname}: {len(files)} plik(ów)")
    print(
        "\nWskazówka: dopisz brakującą bibliotekę do `depends` receptury i podbij "
        "`pkgrel`, żeby pakiet został przebudowany (albo zainstaluj pakiet, "
        "który ją dostarcza)."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
