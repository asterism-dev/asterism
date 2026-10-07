# Sessions

A **session** is a terminal running inside a task's worktree: an agent such as Claude Code, a shell, or a
command. Sessions run in the Asterism daemon, not in the app window, so they keep running when you switch
tasks, close the window or quit the app with **Keep running** (see [Install](install.md#the-daemon)).

## Start a session

- When you [create a task](projects-and-tasks.md#create-a-task), its first session starts automatically.
- Click **+** (**New tab**) in a tab bar of the task's workspace and choose an agent, **Terminal**,
  **Terminal below** or **Terminal right**. The same menu reopens **Diff** or **Activity Monitor** if they are
  closed.
- In an empty workspace, click **New session**.
- Right-click a task in the sidebar and choose **New *agent* session** or **New shell**.

Each session gets these environment variables: `ASTERISM_TASK` and `ASTERISM_SESSION` (the IDs), and
`ASTERISM_HOME`. The bundled `asterism` command is on `PATH`.

CLI:

```sh
asterism session start [TASK] [--agent NAME]   # an agent, claude by default
asterism session start [TASK] --shell          # a shell
asterism session start [TASK] -- make test     # any command
asterism session list [--task ID]
asterism session kill <ID>
```

## Session status

Every session tab shows its status:

| Status | Meaning |
|---|---|
| **Working** (animated stars) | The session is producing output, or the agent reported that it is working. |
| **Waiting for input** | The agent needs you, for example to approve a tool call. |
| **Idle** | No output for about two seconds. |
| **Exited** | The process has ended. |

Agents with Asterism hooks, such as Claude Code, report their status directly. For other sessions the status
is derived from terminal activity.

The sidebar only marks tasks with a session that is working or waiting for input. When a session starts
waiting and you are not looking at it, Asterism sends a desktop notification. The **N waiting** badge in the
sidebar and ++cmd+j++ jump to the next waiting session.

CLI: `asterism wait <SESSION> --until idle|waiting_input|exited [--timeout 10m]` blocks until a session reaches
a status.

## The terminal

- Scrollback keeps the last 2,000 lines. When you reopen a session, its earlier output is replayed.
- ++cmd++-click a URL to open it in your browser.
- ++cmd++-click a file path, optionally with a line number such as `src/main.rs:42`, to open the file in a
  [file tab](#file-tabs).
- An exited session's tab keeps showing its final screen, read-only.

CLI:

```sh
asterism send <SESSION> "run the tests"   # types the text and presses Enter (--no-submit to skip Enter)
asterism read <SESSION> [--lines 50]      # prints the last lines of the screen
```

## Close a session

Click the **×** on a tab, or right-click it and choose **Close session**. Closing a running agent session asks
for confirmation first and stops the agent.

## Arrange the workspace

Each task has its own layout of tabs, which Asterism remembers. A new task starts from the layout you arranged
last.

- Drag tabs to move them between groups or to split the view.
- Right-click a tab for **Split right** and **Split down** (when its group has more than one tab).
- The lock button in the top bar turns on free mode, which lets tabs float. Right-click a tab and choose
  **Float** to detach it, or **Dock** to put a floating tab back.
- The reset button (**Reset layout**) returns the task to the default layout.
- **Hide projects** / **Show projects** at the top left, or ++cmd+b++, toggles the sidebar.

The top bar also shows the project and branch of the selected task; click the branch to copy its name.
**Reveal worktree** opens the worktree in Finder.

## Diff

**Diff** (top bar, or ++cmd+alt+b++) shows the task's changes against its base branch: everything since the
branch point, including uncommitted and untracked files. It reloads when the task's sessions stop working; click
**Refresh** to reload it yourself. **Side by side** switches between unified and split view.

CLI: `asterism task diff [ID]`.

## File tabs

++cmd++-clicking a file path in a terminal opens it in a file tab with syntax highlighting, jumping to the line
if one was given. The tab updates when the file changes on disk.

## Activity Monitor

**Activity Monitor** (top bar) lists memory and CPU usage of the app, the daemon (`asterismd`) and every
session, with a total. Agent sessions include their child processes, such as MCP servers. CPU is relative to
one core. Click a session row to jump to it.

## Quitting and restarting

Sessions belong to the daemon, so what happens to them depends on how you quit:

- **Keep running** — sessions continue; reopen Asterism to see them again.
- **Stop daemon** — all sessions end.

When the daemon starts again, agent sessions that can be resumed (Claude Code) continue their previous
conversation. Shells and other sessions are marked as exited.
