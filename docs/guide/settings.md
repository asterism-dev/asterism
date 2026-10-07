# Settings

Open Settings with the **Settings** button at the bottom of the sidebar, the gear in the top bar, or by
right-clicking the computer name and choosing **Settings…**. **← Back** returns to your tasks. If you leave a
section with unsaved changes, Asterism asks whether to discard them.

## Interface

**Theme** — **System** (follows macOS appearance), **Light** or **Dark**. Terminals always stay dark.

## Paths

- **Repositories** — where cloned and newly created repositories go, as `<owner>/<repo>`. Default:
  `~/.asterism/repos`.
- **Worktrees** — where task worktrees go, as `<owner>/<repo>/<task>`. Default: `~/.asterism/worktrees`.

Use **Choose…** to pick a folder or **Default** to reset a field, then **Save**. Changes apply to new
repositories and tasks; existing worktrees stay where they are.

## Sessions

Configures sessions that are not agents: **Shell** (terminals) and **Command** (sessions started with
`asterism session start -- <command>`).

- **Environment** — variables to set (**+ Variable**) and inherited variables to remove (**+ Pattern**). A
  pattern is an exact name or a prefix ending in `*`, such as `AWS_*`.

`CLAUDE*` and `ANTHROPIC_*` variables are never inherited from the environment Asterism runs in; set them here
if a session needs them. Click **Save**; changes apply to newly started sessions.

## Agent and plugin settings

![Claude Code agent settings](../assets/screenshots/settings-agents-light.png#only-light)
![Claude Code agent settings](../assets/screenshots/settings-agents-dark.png#only-dark)

Every enabled plugin that has settings or provides an agent gets its own section, named after its agent (for
example **Claude Code**) or the plugin (for example **Linear**). It shows the plugin's own settings with a
**Save** button, followed by the agent's settings:

- **Parameters** — extra command-line arguments for every session of this agent (**+ Parameter**). The line
  below shows the resulting command.
- **Environment** — as for [Sessions](#sessions) above.
- **MCP servers** — JSON in the agent's own MCP format, added to your existing agent configuration.
- **Hooks** — JSON in the agent's own hooks format. Asterism's status hooks stay active as well.

Which of these an agent supports depends on its plugin. Agent settings apply to newly started sessions.

See [Plugins](plugins.md#bundled-plugins) for the settings of the bundled plugins.

## Plugins

Installs, updates and configures plugins. See [Plugins](plugins.md).

## About

Shows the installed version. **Check for updates** looks for a new release; if one is available, its release
notes appear and the update button installs it. **All releases** opens the releases page.
See [Install → Updates](install.md#updates).

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| ++cmd+n++ | New task |
| ++cmd+j++ | Jump to the next session waiting for input |
| ++cmd+f++ | Search projects and tasks |
| ++cmd+b++ | Show or hide the sidebar |
| ++cmd+alt+b++ | Show or hide the Diff |
| ++cmd+q++ | Quit (asks what to do with running sessions) |
| ++cmd+enter++ | Create the task (in the **Create Task** dialog) |
| ++esc++ | Close a dialog or picker; clear the sidebar search |
| ++cmd++ + click | Open a URL or file path in a terminal |
