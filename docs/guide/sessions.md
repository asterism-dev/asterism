# Sessions

A **session** is a terminal running inside a task's worktree: an agent such as Claude Code, a shell, or a
command. Sessions run in the Asterism daemon, not in the app window, so they keep running when you switch
tasks, and keep running after you close the window or quit the app if you choose **Keep running** (see [Install](install.md#the-daemon)).

## Start a session

- When you [create a task](projects-and-tasks.md#create-a-task), its first session starts automatically.
- Click **+** (**New tab**) in a tab bar of the task's workspace and choose an agent, **Terminal**,
  **Terminal below** or **Terminal right**. The same menu reopens **Review** or **Activity Monitor** if they are
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

## Review

![The review pane](../assets/screenshots/diff-light.png#only-light)
![The review pane](../assets/screenshots/diff-dark.png#only-dark)

**Review** (top bar, or ++cmd+alt+b++) shows the task's changes against its base branch: everything since the
branch point, including uncommitted and untracked files. It reloads when the task's sessions stop working; click
**Refresh** to reload it yourself.

CLI: `asterism task diff [ID]`.

### Header

The header shows the pull request's title and number, with **↗** to open it in the browser (for a task without a
pull request it shows the task title). When the task has a pull request on a forge that supports reviews, a
`PR #N` | **Local** toggle appears. **PR** shows the pull request's diff, conversation and checks; **Local**
shows the worktree. A hint appears when the worktree differs from the PR head, for example because of unpushed
commits.

In **PR** mode a bar above the tabs shows how many comments are pending in your review and offers **Review
changes** (**Finish review** once a review is pending). It opens a body field with **Comment**, **Approve** and
**Request changes**.

The tabs below are **Conversation**, **Commits**, **Checks** (PR mode only) and **Files changed**, each with a
count. Asterism remembers the last tab per task. The `+` / `−` totals on the right summarize the diff. In PR mode
the pane polls the forge every 15 seconds while any check is queued or running, otherwise every minute.

### Conversation

Shows the pull request description followed by a timeline of comments, review events (approved, requested changes,
reviewed) and code threads. A code thread shows the file and line, an excerpt of the diff around it, and the
thread itself (outdated threads have no excerpt). Filter threads with **open**, **resolved**, **local** or
**all**; the comments and review events appear under **all** only. In PR mode, a field at the bottom posts a
general comment on the pull request.

### Commits

Lists the commits of the branch grouped by day, each with its author, time, short SHA and, in PR mode, its check
state (✓ passed, ✗ failed, ● pending). In **Local** mode an **Uncommitted changes** entry comes first. Click a
commit to open its diff, read-only, in **Files changed** (no comments, viewed flags or **Send to agent**). Click
**Show all changes** in the banner to return to the full diff.

### Checks

Lists the pull request's checks with their status, duration and **↗** to open the check in the browser. The
heading summarizes them (for example "2 failing" or "All checks passed"). Click a check to read its log:

- Steps are collapsible. Failed steps start open; if nothing failed, only the last step does.
- Logs are cut to the last 1 MB, which the viewer says.
- A check that is still running has no log yet; wait for it to finish or use **Open in browser ↗**.

A failed GitHub Actions job has **Re-run**, which turns into **Re-run requested** until the check changes state.
**→ Agent** on a failed check, or in the log viewer, sends the failing part of the log through the send dialog
(see below), where you can edit the prompt.

### Files changed

A file tree on the left lists the changed files with status icons (added, modified, deleted, renamed), the number of
comments per file and **✓** for viewed files; type in **Filter files…** to narrow it, and click a file to jump to
its diff. **Side by side** switches between unified and split view. Files with more than 1000 changed lines, and
every file after the first 30, start collapsed; click one to expand it.

#### Viewed files

The tree shows how many files you have viewed (**N / M viewed**). Check **Viewed** on a file to mark it and collapse
it. In **PR** mode this syncs with GitHub's viewed flag. In **Local** mode it is stored on your machine and resets
when that file's diff changes.

#### Comments

Click **+** on a diff line to comment.

- In **PR** mode you can choose **Local comment** (kept in Asterism only), **Comment** (posted to the pull
  request immediately) or **Start a review** (a pending comment). Once a review is pending, the last option
  reads **Add review comment**.
- In **Local** mode there is a single **Comment** button.

Threads carry badges (local, pending, outdated, resolved) and offer **Reply**, **Resolve** / **Unresolve**,
**Publish** (turns a local thread into a pull request comment, or adds it to your pending review; PR mode only), **→ Agent** and a checkbox to
select the thread.

### Send to agent

Use **→ Agent** on a thread, file or failed check, or **Send to agent (N)** in the Files changed toolbar for the
selected threads. With nothing selected, N is the number of open threads and all of them are sent. In the dialog,
pick an existing agent session of the task or start a new session (and choose its agent), and edit the prompt
before sending. Asterism warns when any agent session in the task is working, because parallel edits in one
worktree can conflict.

CLI:

```sh
asterism review comments [--task ID] [--all] [--local] [--json]
asterism review checks [--task ID] [--log CHECK_ID] [--json]
```

`review comments` prints the task's open review threads as a prompt for an agent; `--all` includes resolved
threads, `--local` reads comments against the worktree instead of the pull request, and `--json` prints the
threads instead. `review checks` lists the pull request's checks (`--json` prints them as JSON); `--log CHECK_ID`
prints one check's log instead. `--task` defaults to `ASTERISM_TASK`.

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
