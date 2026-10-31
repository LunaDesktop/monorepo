# Meta-repozytorium Luna — wspólne polecenia dla niezależnych workspace'ów.
#
# Każdy katalog (libluna, luna, selene, luna-greeter, apps/*) jest osobnym
# workspace'em Cargo z własnym Cargo.lock. Ten Makefile tylko je orchestruje.
#
#   make build      # zbuduj wszystko
#   make check      # szybka weryfikacja typów
#   make test       # testy wszystkich pakietów
#   make smoke      # test dymny sesji (zagnieżdżony Selene)
#   make bench-render  # baseline rasteryzacji CPU (release)
#   make install    # pełna instalacja środowiska
#   make install-terminal   # aktualizacja tylko terminala
#   make install-store      # aktualizacja tylko sklepu (systemowo)
#   make install-store-user # sklep bez roota w ~/.local
#
# Wspólny target/ sprawia, że `scripts/install.sh` widzi binarki z każdego
# workspace'u; można go nadpisać: `make CARGO_TARGET_DIR=/tmp/t build`.
SHELL := /bin/sh
ROOT := $(CURDIR)

CARGO_TARGET_DIR ?= $(ROOT)/target
export CARGO_TARGET_DIR

.PHONY: all build check test test-doc smoke smoke-x11 bench-render bench-gpu lock install install-terminal install-store install-store-user icons icons-png clean fmt \
	web-assets web-api web-docs

all: build

build:
	./scripts/workspaces.sh build

check:
	./scripts/workspaces.sh check

test:
	./scripts/workspaces.sh test

test-doc:
	./scripts/workspaces.sh test-doc

smoke:
	./scripts/workspaces.sh smoke

smoke-x11:
	./scripts/x11-smoke.sh

bench-gpu:
	cargo run --release --manifest-path libluna/Cargo.toml -p libluna-render --example gpu_probe

bench-render:
	cargo run --release --manifest-path libluna/Cargo.toml -p libluna-render --example render_bench

lock:
	./scripts/workspaces.sh lock

install:
	./scripts/install.sh

install-terminal:
	./scripts/install.sh --only terminal

install-store:
	./scripts/install.sh --only store

install-store-user:
	./scripts/install.sh --user --only store

# Pastelowy pakiet freedesktop: PNG 16–512 px + skalowalne SVG z tego samego kodu.
icons:
	cargo run --manifest-path apps/luna-demo/Cargo.toml --bin export-icons

# Stara nazwa celu — pakiet zawiera teraz również wersje SVG.
icons-png: icons

clean:
	rm -rf "$(CARGO_TARGET_DIR)" libluna/target luna/target selene/target luna-greeter/target apps/*/target

fmt:
	./scripts/workspaces.sh fmt

# Podglądy widgetów (PNG jasny/ciemny + widgets.js) i indeks API dla strony.
web-assets:
	cargo run --manifest-path apps/luna-demo/Cargo.toml --bin export-web-widgets
	python3 scripts/generate-widgets-en.py
	python3 scripts/generate-api-index.py

# Pełne rustdoc publikowane pod /api/ na stronie projektu.
web-api:
	@command -v rustdoc >/dev/null || { echo "Brak rustdoc w PATH — doinstaluj dokumentację Rusta albo komponent toolchainu."; exit 1; }
	CARGO_TARGET_DIR=libluna/target cargo doc --manifest-path libluna/Cargo.toml --no-deps
	rm -rf web/luna.paganlinux.eu/api
	cp -R libluna/target/doc web/luna.paganlinux.eu/api
	printf '%s\n' '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=libluna/"><title>libLuna API</title>' > web/luna.paganlinux.eu/api/index.html

# Wszystko, co trzeba odświeżyć przed rsync na VPS.
web-docs: web-assets web-api
