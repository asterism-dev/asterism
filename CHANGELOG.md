## v0.4.0 (2026-10-06)

### Feat

- **desktop**: copy branch name from task menu, drop archive hover button (#7)

## v0.3.1 (2026-10-06)

### Fix

- **desktop**: ad-hoc sign the whole app bundle (#6)

## v0.3.0 (2026-10-06)

### Feat

- release pipeline with in-app auto-update (#5)
- **plugins**: create tasks from Linear and GitHub issues (#4)

## v0.2.0 (2026-10-06)

### Feat

- **desktop**: search projects and tasks in the sidebar
- **desktop**: ask whether to stop the daemon when quitting with running sessions
- **plugins**: plugin stores with search, install and updates (#3)
- turn agents and forges into plugins (#1)
- **desktop**: project page with tasks and worktrees, restore and delete tasks
- list, size, remove and prune a project's git worktrees
- keep worktrees on archive, restore and delete tasks
- restore scrollback and alternate screen on attach
- **desktop**: square-plus icon for adding a project
- **desktop**: use Lucide icons throughout
- **desktop**: two-column layout with one dockview per task and a per-task free mode
- **desktop**: per-task workspace with splittable, draggable session groups
- **desktop**: dockable outer layout with Projects, Workspace, Diff and Activity Monitor panes
- **desktop**: add dockview and pure layout models
- **desktop**: three-column layout with top bar and resizable, collapsible side columns
- activity monitor with per-session CPU and memory
- sort projects and tasks and show task activity age
- **desktop**: add-project modal with tabs and animated star status indicator
- **desktop**: show status indicators after names and only when meaningful
- detect a daemon from another build and offer a restart
- **desktop**: add path settings and collapsible projects
- **desktop**: add, clone or create projects from the sidebar
- **desktop**: add project, GitHub and path settings data model
- **daemon**: clone and create projects, serve path settings, group worktrees by owner
- **daemon**: add non-interactive git clone/init and a gh CLI wrapper
- **daemon**: parse clone sources and store per-node path settings
- **proto**: add project clone/create, GitHub and path settings types
- **desktop**: add an Interface theme setting and pin Settings to the sidebar bottom
- **agent-settings**: resume Claude sessions leniently when hooks.json or mcp.json is broken
- **desktop**: close session tabs and remove their sessions
- **desktop**: add the Settings view with per-agent settings
- **desktop**: add agent settings data model and session removal handling
- **daemon**: serve agent settings and remove sessions on request
- **daemon**: load, validate and save per-agent settings with Claude launch flags
- **proto**: add agent settings and session removal to the protocol
- **desktop**: add the read-only diff view
- **desktop**: show session tabs with attachable xterm terminals
- **desktop**: add sidebar, context menus, new-task dialog and notifications
- **desktop**: add typed node API and the event-driven store
- **desktop**: scaffold the Tauri app that starts the local node
- **node**: manage the local node connection with reconnect and update policy
- **node**: resolve the login PATH and spawn the bundled daemon
- **daemon**: configure agent session environments per agent
- **cli**: add asterism CLI with daemon autostart and agent hooks
- **daemon**: resume agent sessions after a daemon restart
- **daemon**: serve JSON-RPC over a private unix socket
- **daemon**: manage projects, task worktrees and live sessions
- **daemon**: detect session status from hooks and PTY activity
- **daemon**: host processes in PTYs with vt100 terminal state
- **daemon**: add Claude agent profile and hook settings
- **daemon**: add git worktree and diff helpers
- **daemon**: add paths, error type and SQLite store
- **proto**: add JSON-RPC wire types and domain types

### Fix

- **dev**: build sidecars first and allow parallel dev instances
- refuse restoring onto an occupied path and keep archived tasks' layouts and sessions
- **desktop**: menus and dialogs above floating windows, centred +, sessions open in floating groups
- **desktop**: move floating windows by their tab bar and theme their frame
- **desktop**: load dockview styles again after removing the outer layout
- **desktop**: disable Tauri's native drag-drop so in-page drag and drop works
- **desktop**: let session tabs be dragged between workspace groups
- **desktop**: put the session + right after the tabs and hide the Workspace's outer tab strip
- **desktop**: keep pane sizes on close, show workspace tab strips, theme bridge and split guards
- **desktop**: make the banner button visible and align it right
- clone github web urls, canonicalize create owner and validate paths before creating dirs
- **desktop**: guard repo list against stale owner responses
- **desktop**: validate and parse source URLs like daemon
- **daemon**: resolve worktree root before inserting tasks and reserve repo directories atomically
- **daemon**: bound clone and gh metadata calls with timeouts and keep core.sshCommand
- **daemon**: reject option-like clone sources and read github web urls
- **desktop**: settings repair, prototype-safe env keys, guarded close and new task
- **daemon**: keep args/env in raw config, check mcp/hooks on launch, silent session removal
- **desktop**: remove duplicate keydown.backspace handler on tab-label
- **desktop**: make session tabs accessible with proper semantic HTML
- **daemon**: kill the process group on forced session removal
- **daemon**: serialise settings writes and reject reserved agent parameters
- **desktop**: order keystrokes, confirm destructive actions and polish app behaviour
- **node**: keep newer daemons, fall back to a usable PATH, harden the daemon test
- **desktop**: cap diff size so large diffs do not freeze the UI
- **desktop**: detach on every unmount and test attach ordering
- **desktop**: serialize attach and detach, guard stale attaches and zero-size panes
- **desktop**: aggregate node status dot and avoid terminal key collisions
- **node**: resolve the login-shell PATH lazily, off the setup path
- **node**: publish connected only when ready and make restarts robust
- **daemon**: harden sessions, reads and startup from final review
- **cli**: isolate the autostarted daemon env and treat the idle-prompt notification as idle
- **proto**: fail calls on undecodable responses and accept unknown error kinds
- **cli**: json ok output, bounded hook, non-autostarting daemon stop
- **daemon**: bound per-connection output queue and close forwarder leak
- **daemon**: avoid task id reuse, exit race and git-config test flakiness

### Refactor

- share node paths through the protocol crate
