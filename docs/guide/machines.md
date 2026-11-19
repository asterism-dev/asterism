# Machines

A machine is a remote computer Asterism reaches over SSH. Asterism installs a small daemon on it and keeps a connection to it open.

At the moment Asterism only connects machines and shows their status. Running tasks on a machine comes later.

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

## What gets installed

Everything lives in `~/.asterism-machine` on the machine. The binaries are under `~/.asterism-machine/bin/<build-key>/`, one directory per Asterism build. Remove the machine in Settings and delete that directory to clean up.

## Status

| Status | Meaning |
| --- | --- |
| Connecting | Asterism is connecting or installing. |
| Connected | The daemon is running. The platform is shown, plus "update available" when a newer build exists. |
| Offline, retrying | The connection dropped or the host is unreachable. Asterism retries automatically with a backoff of up to 30 s. |
| Needs attention | Retrying cannot help: authentication or host key failure, an unsupported platform, a machine that does not run the Asterism daemon, or an install error. Fix the cause, then click **Retry**. |

For a host key failure, connect once with `ssh <destination>` in a terminal to accept or fix the key in `known_hosts`.
