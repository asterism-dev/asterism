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

In **New task**, pick the machine under **Machine** (default: Local). The dialog remembers the last machine you used. You can start the task with a shell or with an agent the machine has installed.

- The project needs an `origin` remote, and the machine needs access to it with its own git credentials. Asterism never sends your git credentials, `secrets.toml` or secret plugin settings to a machine; see [Agents on a machine](#agents-on-a-machine) for what it does send.
- The machine clones `origin` into `~/.asterism-machine/repos/<owner>/<repo>` on first use, creates the branch and worktree there and always pushes the branch to `origin`, so pull request status and review work on the Mac. **Checkout branch** works for a branch that is already on `origin`.
- If the machine cannot clone or create the branch, the dialog shows the machine's error and no task is left behind. If only the push fails, Asterism shows the error as a warning and keeps the task.

Tasks on a machine show a machine badge in the sidebar and the task header.

### What works

- Shells and commands run on the machine, with the machine's environment and `PATH`. You attach, type and resize as for a local task, and they keep running when the connection drops, your Mac sleeps or Asterism restarts. Asterism reattaches when the machine is reachable again.
- The review pane, diffs and file views read the machine's worktree.
- Archive, restore and delete run on the machine: archive ends the task's sessions there, restore recreates its worktree, delete removes the worktree and, if you choose, the branch.
- Agents run on the machine; see [Agents on a machine](#agents-on-a-machine).
- The `asterism` CLI and the mobile app work with these tasks through your Mac. The CLI also works inside a session on the machine; see [The CLI on a machine](#the-cli-on-a-machine).

### What does not work yet

- Third-party agent plugins (installed from a store or linked) on a machine. Only the agents built into Asterism are offered for machine tasks.
- "Open in" (editor, Finder, terminal) for machine tasks; the actions are hidden.
- Hibernation does not apply to sessions on a machine.
- Moving a task between machines, or between a machine and your Mac.

### When a machine is offline

Tasks on an offline machine stay in the sidebar with their last known state and show the machine as offline. You cannot type into their terminals; keystrokes are never queued for later. Archive, restore and delete fail with an error naming the machine, so your Mac and the machine never disagree. Local tasks are never slowed down by an unreachable machine: every call to a machine gives up after 30 s at the latest.

If the machine's daemon restarts (for example after a reboot), the shells and commands running there end. They show as exited once Asterism reconnects. Agent sessions are resumed by the machine daemon itself, even while your Mac is asleep.

## Agents on a machine

The agent runs on the machine, with the machine's files, login and network. Its hooks, workspace trust and session files are written on the machine.

- **Install the agent on the machine** and log in there (for Claude Code: run `claude` once on the machine). Asterism does not copy logins or API keys from your Mac.
- **The agent CLI is found on your login-shell `PATH`.** The machine daemon takes the `PATH` of its user's login shell at startup, so CLIs in `~/.local/bin`, nvm, Homebrew and similar work. The dialog offers only agents the machine reported as available when it connected. An agent installed later shows up after the next reconnect; if it went into a directory that was not on `PATH` when the machine daemon started, stop the machine daemon so it restarts with the new `PATH`.
- **What is sent to the machine.** Whenever an agent session starts on the machine, Asterism first sends that agent's settings: arguments, environment settings, MCP config, hooks and non-secret plugin settings such as **Trust task worktrees**. Environment settings and the MCP config go to the machine as they are, so keep tokens out of them if you do not want them there. Secret plugin settings and `secrets.toml` stay on your Mac.
- The machine keeps the settings it last received, so it can resume the agent after a restart without your Mac. Changes reach the machine with the next agent session started there.

## The CLI on a machine

Inside a shell or agent session on the machine, `asterism` talks to your Mac through the machine's connection: `asterism task list` shows your tasks and `asterism task new` creates a task, as on the Mac. This needs the Mac to be connected; otherwise the CLI fails with "the hub is not connected".

The CLI there is limited to the machine it runs on:

- It sees only the tasks on that machine. Other tasks, and tasks on other machines, are not found.
- A new task always runs on that machine, and only in a project that already has a task on that machine.
- Settings, plugins, machines, projects and pairing cannot be changed from a machine.

Any process on the machine running as the machine's user can take over this connection to your Mac. That user already controls the machine daemon and its tasks, so only add machines whose user you trust with this access.

A machine can be removed only when all its tasks are archived.

## What gets installed

Everything lives in `~/.asterism-machine` on the machine, including the clones under `repos/` and the worktrees of its tasks. The binaries, including the backends of the built-in agents, are under `~/.asterism-machine/bin/<build-key>/`, one directory per Asterism build. To clean up, remove the machine in Settings, stop the machine daemon with `ASTERISM_HOME=~/.asterism-machine ~/.asterism-machine/bin/<build-key>/asterism daemon stop` on the machine, then delete `~/.asterism-machine`.

## Status

| Status | Meaning |
| --- | --- |
| Connecting | Asterism is connecting or installing. |
| Connected | The daemon is running. The platform is shown, plus "update available" when a newer build exists. |
| Offline, retrying | The connection dropped or the host is unreachable. Asterism retries automatically with a backoff of up to 30 s. |
| Needs attention | Retrying cannot help: authentication or host key failure, an unsupported platform, a machine that does not run the Asterism daemon, or an install error. Fix the cause, then click **Retry**. |

For a host key failure, connect once with `ssh <destination>` in a terminal to accept or fix the key in `known_hosts`.
