# Machines

A machine is a remote computer Asterism reaches over SSH. Asterism installs a small daemon on it and keeps a connection to it open. A task can run on a machine instead of your Mac: its worktree, shells and commands then live on that machine.

## Requirements

- Key-based SSH login that works without a prompt. Asterism runs `ssh` non-interactively (`BatchMode`), so password and passphrase prompts fail. Load your key into `ssh-agent`.
- Your `~/.ssh/config` is honoured, so aliases, `ProxyJump`, ports and identity files work. The destination can be `user@host` or a `Host` alias.
- The machine runs Linux (x86_64 or aarch64) or macOS.

## Add a machine

1. Open **Settings → Machines**.
2. Enter a **Name** and the **SSH destination**.
3. Click **Test connection** to check the login. It shows the platform of the machine or the SSH error.
4. Click **Add**.

Asterism installs its daemon on the first connection and updates it when the machine's copy is out of date.

## Run a task on a machine

In **New task**, pick the machine under **Machine** (default: Local). The dialog remembers the last machine you used.

- The project needs an `origin` remote, and the machine needs access to it with its own git credentials. Asterism never sends your credentials, `secrets.toml` or plugin settings to a machine.
- The machine clones `origin` into `~/.asterism-machine/repos/<owner>/<repo>` on first use, creates the branch and worktree there and always pushes the branch to `origin`, so pull request status and review work on the Mac. **Checkout branch** works for a branch that is already on `origin`.
- If the machine cannot clone or create the branch, the dialog shows the machine's error and no task is left behind. If only the push fails, Asterism shows the error as a warning and keeps the task.

Tasks on a machine show a machine badge in the sidebar and the task header.

### What works

- Shells and commands run on the machine, with the machine's environment and `PATH`. You attach, type and resize as for a local task, and they keep running when the connection drops, your Mac sleeps or Asterism restarts. Asterism reattaches when the machine is reachable again.
- The review pane, diffs and file views read the machine's worktree.
- Archive, restore and delete run on the machine: archive ends the task's sessions there, restore recreates its worktree, delete removes the worktree and, if you choose, the branch.
- The `asterism` CLI and the mobile app work with these tasks through your Mac.

### What does not work yet

- Agents (Claude Code etc.) on a machine. The dialog offers only a shell for machine tasks; agents come in a later release.
- "Open in" (editor, Finder, terminal) for machine tasks; the actions are hidden.
- Hibernation does not apply to sessions on a machine.
- Moving a task between machines, or between a machine and your Mac.

### When a machine is offline

Tasks on an offline machine stay in the sidebar with their last known state and show the machine as offline. You cannot type into their terminals; keystrokes are never queued for later. Archive, restore and delete fail with an error naming the machine, so your Mac and the machine never disagree. Local tasks are never slowed down by an unreachable machine: every call to a machine gives up after 30 s at the latest.

If the machine's daemon restarts (for example after a reboot), the shells and commands running there end. They show as exited once Asterism reconnects.

A machine can be removed only when all its tasks are archived.

## What gets installed

Everything lives in `~/.asterism-machine` on the machine, including the clones under `repos/` and the worktrees of its tasks. The binaries are under `~/.asterism-machine/bin/<build-key>/`, one directory per Asterism build. To clean up, remove the machine in Settings, stop the machine daemon with `ASTERISM_HOME=~/.asterism-machine ~/.asterism-machine/bin/<build-key>/asterism daemon stop` on the machine, then delete `~/.asterism-machine`.

## Status

| Status | Meaning |
| --- | --- |
| Connecting | Asterism is connecting or installing. |
| Connected | The daemon is running. The platform is shown, plus "update available" when a newer build exists. |
| Offline, retrying | The connection dropped or the host is unreachable. Asterism retries automatically with a backoff of up to 30 s. |
| Needs attention | Retrying cannot help: authentication or host key failure, an unsupported platform, a machine that does not run the Asterism daemon, or an install error. Fix the cause, then click **Retry**. |

For a host key failure, connect once with `ssh <destination>` in a terminal to accept or fix the key in `known_hosts`.
