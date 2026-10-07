# Install

## Requirements

- A Mac. Releases are built for Apple silicon (`aarch64`) and Intel (`x86_64`).
- `git`, which Asterism uses for every worktree and branch operation.
- The tools behind the agents and plugins you want to use, for example:
    - the [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI (`claude`) for Claude Code sessions,
    - the [GitHub CLI](https://cli.github.com) (`gh`), logged in with `gh auth login`, for cloning, creating
      repositories, pull request status and GitHub Issues.

Asterism reads `PATH` from your login shell when it starts its daemon, so tools installed through Homebrew or
similar are found even though the app is launched from Finder.

## Download

1. Open the [releases page](https://github.com/asterism-dev/asterism/releases).
2. Download the `.dmg` of the latest release for your Mac's architecture.
3. Open the disk image and drag **asterism** into your Applications folder.

!!! note "First launch"
    Release builds are ad-hoc signed, not notarized. If macOS refuses to open the app the first time, allow
    it under **System Settings → Privacy & Security**.

## First launch

When the app starts it connects to the Asterism daemon (`asterismd`) and starts it if it is not running yet.
The computer's name appears at the top of the sidebar; while the daemon starts it shows *starting…*.

From there, add a project with the **+** button next to the computer name — see
[Projects and tasks](projects-and-tasks.md).

### The daemon

The daemon owns your projects, tasks and sessions, so agents can keep working in the background after the app
is gone. Closing the window counts as quitting. When you quit (++cmd+q++ or closing the window) while sessions are
still running, Asterism asks:

- **Keep running** — quit the app but leave the daemon and its sessions running. Reopen the app to pick them up
  again.
- **Stop daemon** — end all sessions and stop the daemon.
- **Cancel** — stay in the app.

With no running sessions, quitting stops the daemon without asking.

To restart the daemon manually, right-click the computer name at the top of the sidebar and choose
**Restart daemon**. On restart, agent sessions that support resuming (such as Claude Code) are resumed;
other sessions end.

## Updates

Asterism checks for app updates on launch and every six hours. When an update is available:

- an **Update to *version*** button appears next to the version number at the bottom of the sidebar, and
- **Settings → About** shows the release notes.

Click the button to download and install the update; Asterism restarts when it is done. You can also check
manually with **Check for updates** in **Settings → About**.

After an app update, the daemon from the previous version may still be running. A banner then says
*Daemon update ready* — click **Restart daemon** to switch to the new one.

## The `asterism` command

The app bundles an `asterism` command-line tool that talks to the same daemon. Inside every Asterism session
it is already on `PATH`, so agents and shells in a task can use it directly:

```sh
asterism task list
asterism session list
asterism --help
```

Inside a session, commands that take a task ID default to the session's task, and `--project` defaults to
that task's project. Elsewhere, `--project` (a project name or ID) defaults to the git repository in the current
directory, which is registered as a project if it is not one yet. Add `--json` to any command for
machine-readable output.

## Uninstall and data

Everything Asterism stores lives in `~/.asterism` (or in `$ASTERISM_HOME` if you set it):

| Path | Contents |
|---|---|
| `state.db` | Projects, tasks and sessions |
| `config.toml` | Paths, agent and plugin settings |
| `secrets.toml` | Secret plugin settings such as API keys |
| `worktrees/` | Task worktrees (default location) |
| `repos/` | Cloned and newly created repositories (default location) |
| `plugins/` | Installed plugins, stores and plugin data |
| `asterismd.log` | Daemon log |

To uninstall:

1. Quit Asterism and choose **Stop daemon**, so no sessions keep running.
2. Move the app from Applications to the Trash.
3. Delete `~/.asterism` if you no longer need its data.

!!! warning
    `~/.asterism/worktrees` and `~/.asterism/repos` can contain uncommitted or unpushed work. Check them
    before deleting the folder.
