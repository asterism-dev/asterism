# Building from source

## Prerequisites

- **macOS.** The desktop app is macOS-only; CI and releases run on `macos-14`.
- **Rust**, stable toolchain, with `clippy` and `rustfmt` for `make lint`.
- **Node.js**, current LTS, with npm.
- **git**, which the daemon uses for worktrees and the build uses to stamp a build id.
- **[uv](https://docs.astral.sh/uv/)**, only for `make docs`.

Install the frontend dependencies once:

```sh
cd apps/desktop
npm ci
```

## Make targets

All targets run from the repository root.

| Target | What it does |
|---|---|
| `make dev` | Builds the sidecars, then runs `tauri dev` with hot reload. |
| `make dev-stop` | Stops the development daemon. |
| `make build` | Builds the release app bundle (`npx tauri build`). |
| `make open` | Opens the built bundle, `target/release/bundle/macos/asterism.app`. |
| `make test` | `cargo test`, then the frontend tests (`npm test`, Vitest). |
| `make lint` | Frontend typecheck, ESLint and Prettier check; `cargo fmt --check`; `cargo clippy --workspace --all-targets -- -D warnings`. |
| `make docs` | Serves this documentation locally with `uvx zensical==0.0.68 serve`. |

### Sidecars

The app bundles the daemon, the CLI and the built-in plugin backends as Tauri sidecars. `npm run sidecars` (`apps/desktop/scripts/prepare-sidecars.mjs`) runs `cargo build -p asterism --bins` and copies the binaries to `apps/desktop/src-tauri/binaries/`, suffixed with the target triple. `make dev` and `make lint` run it first, because `tauri dev` and clippy on the Tauri crate need the binaries to exist. `tauri build` runs it with `--release` on its own.

Because of this, the Tauri crate (`apps/desktop/src-tauri`) is a workspace member but not a default member: plain `cargo build` and `cargo test` cover only the four library and binary crates.

### Development instance

`make dev` sets `ASTERISM_HOME` to `DEV_HOME` (default `~/.asterism-dev`), so the development app starts its own daemon with its own database, worktrees and plugins next to an installed app. The frontend dev server listens on `DEV_PORT` (default `1420`).

To run a second development instance at the same time, give it its own port and home:

```sh
make dev DEV_PORT=1430 DEV_HOME=~/.asterism-dev2
```

Quitting the app can leave the daemon running, as in a release build. `make dev-stop` sends `shutdown` to the daemon at `$(DEV_HOME)/asterismd.sock`; pass the same `DEV_HOME` to stop a second instance. The app only replaces a running daemon by itself when its version or build id (`git describe --always --dirty`) differs and no session is running, so stop the daemon after changing daemon or plugin code to be sure the next `make dev` runs the new build.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request, on `macos-14` with stable Rust and Node LTS: `npm ci` in `apps/desktop`, then `make lint` and `make test`. Run both locally before pushing.

## Commits and releases

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat: ...`, `fix(github): ...`, `docs: ...`). They are not only a convention: they drive versioning and the changelog.

Releases are automated with [Commitizen](https://commitizen-tools.github.io/commitizen/), configured in `.cz.toml`:

- `cz_conventional_commits`; tags are `v<version>`.
- The version is read from and written to the workspace `Cargo.toml` (`version_provider = "cargo"`).
- `major_version_zero = true`: while the version is `0.x`, breaking changes bump the minor version.
- `CHANGELOG.md` is updated on every bump.

`.github/workflows/release.yml` runs on every push to `main`:

1. **bump**: `cz bump --get-next` decides whether the commits since the last tag warrant a release. If not, the workflow stops. Otherwise `cz bump` updates the version and changelog, commits, tags, pushes both atomically, and creates a draft GitHub release with the new changelog section as notes.
2. **build**: builds the app for `aarch64-apple-darwin` and `x86_64-apple-darwin` with `tauri-action`, signs the updater artifacts and uploads them to the draft release, including the updater's `latest.json`.
3. **publish**: checks that `latest.json` lists both platforms, then publishes the release as latest.
4. **docs**: calls `.github/workflows/docs.yml` for the new tag, which builds this site with `zensical build --clean` and deploys it to GitHub Pages.

A manual run (`workflow_dispatch`) defaults to a dry run, which builds without bumping, tagging or publishing; runs from any branch other than `main` are always dry runs. `docs.yml` can also be dispatched on its own to redeploy the docs from any ref.
