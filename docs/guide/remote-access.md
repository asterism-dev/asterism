# Remote access

Asterism's mobile app runs in the browser of your phone or tablet. The daemon serves it itself, so it keeps working while the desktop app is closed. You can check session status, watch and type into terminals, start and close sessions, create tasks and archive tasks from anywhere your device can reach the Mac.

Remote access is off by default.

## Requirements

[Tailscale](https://tailscale.com) on the Mac and on the phone or tablet, both signed in to the same tailnet. Asterism does not use a relay and is never exposed to the public internet.

## Enable it

1. Open **Settings → Remote access**.
2. Tick **Enable remote access**.
3. Pick the **Interface** to listen on. The Tailscale address is preselected; **Custom address…** lets you enter any other.
4. Set the **Port** (default 7420) and click **Save**.

The status line shows the address Asterism listens on, or the reason it cannot. If the interface is missing or the port is taken, the daemon retries every 30 s, so starting Tailscale after Asterism recovers on its own.

## HTTPS with Tailscale serve

By default the mobile app is served over plain HTTP. With **HTTPS via Tailscale serve**, Tailscale terminates HTTPS with a real certificate for your Mac's name in the tailnet (`<mac>.<tailnet>.ts.net`) and forwards to Asterism. HTTPS makes the page a secure context in the browser.

Requirement: HTTPS certificates are enabled for your tailnet (Tailscale admin console → **DNS** → **HTTPS Certificates**). Otherwise the status line says so.

1. In **Settings → Remote access**, choose a mode:
   - **Run it for me**: Asterism runs `tailscale serve` itself when it starts listening and turns it off when you change or disable remote access.
   - **I'll run it myself**: Settings shows the exact `tailscale serve` command with a **Copy** button, and the command that turns it off again.
2. Set the **HTTPS port** (default 8443, so a serve setup of your own on 443 is left alone) and click **Save**.

In both modes Asterism listens on `127.0.0.1` only; the interface picker is hidden because the tailnet reaches Asterism through Tailscale serve. Settings shows the address phones open, e.g. `https://mac.example.ts.net:8443`, and new pairing QR codes use it. Devices paired over HTTP need a new token to switch.

The first connection after serve starts can take up to a minute while Tailscale issues the certificate.

## Pair a device

1. Under **Devices**, enter a name and click **Add device**.
2. Scan the QR code with the device's camera, or copy the URL.
3. The mobile app opens and stores its token. Use the browser's share menu and **Add to Home Screen** to open it like an app.

A device that is not paired, or whose token was replaced or removed, shows a message asking you to scan the QR code again.

## Manage devices

Each device lists when it was last seen and whether it is connected.

- **New token** replaces the device's token and shows a new QR code. The old token stops working.
- **Remove** deletes the device.

Both disconnect the device immediately.

## What works remotely

- List and search projects and tasks, with status indicators and highlighting of sessions waiting for input.
- Create tasks, including from pull requests and issues.
- Start and close sessions.
- Attach to a session's terminal and type into it. A key bar offers Esc, Tab, Ctrl-C, arrow keys and Enter; with a hardware keyboard those keys work directly.
- Archive tasks.

Not in this version:

- Adding projects through a folder picker (cloning by URL works).
- Managing plugins and stores, and node settings.
- Plugin panes.
- Reviewing changes; use the desktop app for that.
- The desktop dock layout on tablets; tablets get a two-column list and detail view.
- Push notifications.

## Security

- Every device has its own token. Only its SHA-256 hash is stored; the token itself exists only in the QR code and on the device.
- A remote device cannot stop the daemon, change the remote access settings or manage devices. Those stay with the desktop app.
- Choosing `0.0.0.0` listens on every network interface of the Mac, not only Tailscale, and Settings warns about it. Prefer the Tailscale interface.
- Without Tailscale serve the connection is plain HTTP. Inside the tailnet WireGuard encrypts the traffic, but the browser does not treat the page as a secure context. Turn on [HTTPS with Tailscale serve](#https-with-tailscale-serve) for that.
