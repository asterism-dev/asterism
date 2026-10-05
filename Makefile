DESKTOP := apps/desktop
# Separate home so the dev app runs its own daemon next to the installed app.
DEV_HOME ?= $(HOME)/.asterism-dev
APP := target/release/bundle/macos/asterism.app

.PHONY: dev dev-stop build open test lint

dev:
	cd $(DESKTOP) && ASTERISM_HOME=$(DEV_HOME) npm run tauri dev

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
	cd $(DESKTOP) && npm run sidecars && npm run typecheck
	cargo clippy --workspace --all-targets -- -D warnings
