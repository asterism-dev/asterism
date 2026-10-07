DESKTOP := apps/desktop
# Separate home so the dev app runs its own daemon next to the installed app.
DEV_HOME ?= $(HOME)/.asterism-dev
# Give a second dev instance its own port and home, e.g. make dev DEV_PORT=1430 DEV_HOME=~/.asterism-dev2
DEV_PORT ?= 1420
APP := target/release/bundle/macos/asterism.app

.PHONY: dev dev-stop build open test lint docs

dev:
	@# tauri dev compiles the app while beforeDevCommand still builds the sidecars, so build them first.
	cd $(DESKTOP) && npm run sidecars && ASTERISM_HOME=$(DEV_HOME) ASTERISM_DEV_PORT=$(DEV_PORT) npm run tauri dev -- \
		--config '{"build":{"devUrl":"http://localhost:$(DEV_PORT)"}}'

dev-stop:
	@# The daemon drops a request whose client hangs up before the reply, so keep nc connected briefly.
	@(printf '{"jsonrpc":"2.0","id":1,"method":"shutdown","params":null}\n'; sleep 1) | nc -U $(DEV_HOME)/asterismd.sock >/dev/null 2>&1 || true

build:
	cd $(DESKTOP) && npx tauri build

open:
	open $(APP)

test:
	cargo test
	cd $(DESKTOP) && npm test

lint:
	cd $(DESKTOP) && npm run sidecars && npm run typecheck && npm run lint && npm run format:check
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings

docs:
	uvx zensical==0.0.68 serve
