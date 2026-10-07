# Plugins

Plugins add capabilities to Asterism:

- **Agents** — coding agents you can run in sessions, such as Claude Code.
- **Forges** — code hosts for cloning, creating repositories and pull request status, such as GitHub.
- **Task sources** — issue trackers you can create tasks from, such as Linear or GitHub Issues.
- **Commands** — extra `asterism` subcommands.

Manage them in **Settings → Plugins**, which has three tabs: **Installed**, **Discover** and **Stores**.

To write your own plugin, see [Writing plugins](../developer/writing-plugins.md).

## Bundled plugins

Asterism ships with three plugins, marked **built-in**:

| Plugin | Provides | Needs | Settings |
|---|---|---|---|
| **claude** | Agent **Claude Code**, with status hooks so Asterism knows when it is working or waiting | The `claude` CLI | **Trust task worktrees** |
| **github** | Forge **GitHub** (github.com, with pull requests) and task source **GitHub Issues** | The `gh` CLI, logged in with `gh auth login` | — |
| **linear** | Task source **Linear** | A Linear API key | **API key** (required) |

- **Trust task worktrees** skips Claude's folder trust prompt in new task worktrees. The repository's Claude
  settings, hooks and MCP servers then load without asking. Off by default.
- **API key** is a personal API key from Linear → Settings → Security & access. Until it is set, the Linear
  plugin shows **needs setup**.

The Claude Code agent also has the agent settings described in [Settings](settings.md#agent-and-plugin-settings).

## Installed

Lists every plugin with its version, origin (**built-in**, the store it came from, or **linked (dev)**) and
the capabilities it provides. A status appears next to the name when something needs attention:

- **needs setup** — a required setting is missing; hover or read the line below for which one.
- **failing** or **broken** — the plugin cannot run; the reason is shown below it.
- **disabled** — turned off with the **Enabled** switch.
- **update available** — a newer version is in its store.

Per plugin you can:

- **Configure** — open its settings section.
- **Enabled** — turn it on or off.
- **Update** — install the newer version. If it asks for new permissions, Asterism lists them and asks first.
- **Rollback** — go back to the previously installed version.
- **Uninstall** — remove a store-installed plugin. Its settings are kept.

At the top, **Update all** updates every plugin with an update, **Reload** re-reads plugin manifests and
restarts plugin backends, and **Link local plugin…** uses a plugin folder on disk directly (for development;
see [Writing plugins](../developer/writing-plugins.md)).

When updates are available, the **Settings** button in the sidebar shows their number.

## Discover

Searches the plugins of all your stores. Narrow the list with **Search plugins**, by capability (**All**,
**Forges**, **Agents**, **Task sources**) and, with more than one store, by store.

Select a plugin to see its description, capabilities, permissions and README. **Install** shows the
permissions it asks for, such as *Runs gh* or *Network access*, and asks for confirmation. If the new plugin
needs setup, its settings open right away.

## Stores

A store is a git repository or local folder that offers plugins. The official **asterism-dev** store is
included and marked **official**.

- **Add store** — enter a git URL or click **Choose folder…**, then **Add store**. Plugins from a store run
  code on your machine, so only add stores you trust.
- **Refresh** — fetch the store's latest plugin list. Stores also refresh automatically once a day.
- **Remove** — remove the store. Plugins installed from it are uninstalled too.
- **Install updates automatically** — install plugin updates when stores refresh. Updates that ask for new
  permissions always wait for you.

## CLI

```sh
asterism plugin list
asterism plugin search [QUERY] [--capability forge|agent|command|task-source]
asterism plugin info <NAME>
asterism plugin install <NAME>
asterism plugin update [NAME | --all]
asterism plugin rollback <NAME>
asterism plugin uninstall <NAME>
asterism plugin enable <NAME>
asterism plugin disable <NAME>
asterism plugin config <NAME> [KEY] [VALUE]   # secrets are read from the terminal
asterism plugin reload [NAME]

asterism store list
asterism store add <URL|DIR>
asterism store remove <NAME> [--uninstall-plugins]
asterism store refresh [NAME]
asterism store auto-update on|off
```
