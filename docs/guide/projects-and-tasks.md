# Projects and tasks

A **project** is a git repository. A **task** is a unit of work in a project: it gets its own branch and its
own git worktree, so several agents can work on the same repository in parallel without touching each other's
files.

## Add a project

Click **+** (**Add project**) next to the computer name at the top of the sidebar. The dialog has three tabs.

=== "Add folder"

    Registers a git repository that already exists on this computer. Click **Choose folder…** and pick the
    repository.

    CLI: `asterism project add [PATH]` (defaults to the current directory).

=== "Clone repository"

    Enter a repository as `owner/repo`, an `https://…` URL or a `git@…` address and click **Clone**. The line
    below the field shows where it will be cloned: `<repositories folder>/<owner>/<repo>`.

    If a forge plugin such as GitHub is set up and logged in, **Browse** lists the repositories of your
    account and organizations; **Filter repositories** narrows the list and clicking one fills in the field.
    With more than one forge installed, pick it under **Forge**. Without a working forge you can still clone
    by URL.

=== "New repository"

    Creates a new repository. Enter a **Name**, then choose:

    - **Local only** — a local repository in `<repositories folder>/local/<name>`.
    - **On *forge*** (for example **On GitHub**) — also creates the repository on the forge. Pick the
      **Owner** (your account or an organization) and the **Visibility**. This option needs a logged-in forge.

    If creating the remote repository fails, the local one is still created and a message tells you why.

The repositories folder defaults to `~/.asterism/repos` and can be changed in
[Settings → Paths](settings.md#paths).

### Remove a project

Right-click the project in the sidebar and choose **Remove project**. This removes it from Asterism only.

CLI: `asterism project list`, `asterism project remove <ID>`.

## The sidebar

The sidebar lists your projects with their tasks underneath. Click the chevron next to a project to collapse
or expand it.

Each task row shows:

- the task title,
- the linked issue key, if the task was created from an issue (click it to open the issue),
- a pull request badge, if the task's branch has a pull request (see [below](#pull-request-status)),
- the time since the last activity (hover for the creation time),
- a status indicator when one of its sessions is working or waiting for input (see
  [Sessions](sessions.md#session-status)).

**Search projects and tasks** filters the list: a project matches by name (and then shows all its tasks), a
task matches by title or branch. Press ++esc++ in the field to clear it, or ++cmd+f++ to jump to it.

The sort button next to **+** orders projects and tasks by **Alphabetical**, **Activity** or **Last added**
(the default).

When sessions are waiting for input, a **N waiting** badge appears next to the computer name. Click it, or
press ++cmd+j++, to jump to the next waiting session.

### Context menus

Right-click a **project** for:

- **Open project page**
- **New task…**
- **Refresh PR status**
- **Reveal in file manager**
- **Remove project**

Right-click a **task** for:

- **New *agent* session** for each available agent (for example **New Claude Code session**)
- **New shell**
- **Reveal worktree**
- **Copy branch name**
- **Archive task**
- **Delete task**

## Create a task

![The Create Task dialog](../assets/screenshots/new-task-light.png#only-light)
![The Create Task dialog](../assets/screenshots/new-task-dark.png#only-dark)

Open the **Create Task** dialog in any of these ways:

- hover a project in the sidebar and click **+ Task**,
- right-click a project and choose **New task…**,
- press ++cmd+n++ (preselects the project of the selected task).

You can switch the project in the dialog's title.

1. **Task name** — type a name, or leave the generated placeholder. The name is turned into a lowercase,
   dash-separated slug as you type.
2. **Based on** (optional) — start from an **Issue** or a **Pull Request**; see
   [below](#start-from-an-issue-or-pull-request).
3. **Initial Conversation** — choose what runs in the new task:
    - **Agent**: pick the agent and optionally describe what it should do. The text is passed to the agent as
      its first prompt.
    - **Shell**: opens a shell in the worktree; no agent is started.
4. **Workspace Settings** — choose the branch (see [below](#branches-and-worktrees)).
5. Click **Create** or press ++cmd+enter++. ++esc++ closes the dialog.

The new task is selected and its session opens right away.

CLI:

```sh
asterism task new "fix login redirect" --agent claude --prompt "The login page loops…"
asterism task new --issue linear:TRA-1343
asterism task new "try new parser" --base origin/develop
```

### Branches and worktrees

The **Workspace Settings** tab offers two modes:

- **Create new branch** — creates a branch **From branch** (by default the project's
  [default base branch](#project-page)). The **Branch name** is filled in as `asterism/<task-name>-<suffix>` and
  can be edited. **Push branch to remote** (on by default) pushes the new branch to `origin` and sets it as
  upstream.
- **Checkout branch** — creates the worktree for an existing branch. Branches that only exist on `origin` are
  checked out as tracking branches.

The branch pickers have **Local** and **Remote** tabs, a search field and a refresh button that fetches from
`origin` again.

The **Worktree** line shows where the worktree will be created:

```
~/.asterism/worktrees/<owner>/<repo>/<branch>
```

`<owner>/<repo>` comes from the `origin` URL; repositories without one use `local/<project name>`. The base
folder can be changed in [Settings → Paths](settings.md#paths).

### Start from an issue or pull request

In **Based on**, choose **Issue** or **Pull Request** and click the placeholder to search.

- **Issue** — searches the issue providers of the project, such as GitHub Issues or Linear (switch the provider
  with the icon left of the search field). With an empty search you see your own open issues; typing searches
  all open issues. Picking an issue fills in the task name and the agent prompt, uses the issue's suggested
  branch name if it has one, and links the task to the issue.
- **Pull Request** — lists the open (or **Closed**) pull requests of the project's repository on its forge.
  Picking one switches to **Checkout branch** with the pull request's branch. Pull requests from forks are not
  supported yet.

If a provider is not set up, the panel says so and offers **Open settings** to configure its plugin. Issue
providers and forges come from [plugins](plugins.md).

CLI: `asterism issue search <SOURCE> [QUERY]` lists your open issues from a source (`--all` includes issues
not assigned to you).

## Pull request status

When a task's branch has a pull request on a supported forge, its number appears as a badge in the sidebar and
on the project page. The badge color reflects the state: draft, merged and closed pull requests, failing checks
or requested changes, running checks, or passing. Hover it for details such as *changes requested · 2 failing:
lint, test*; click it to open the pull request.

Asterism refreshes the status in the background while the app is open. To refresh now, right-click the project
and choose **Refresh PR status**.

CLI: `asterism pr refresh`.

## Project page

Click a project name in the sidebar to open its page. It has three tabs:

- **Tasks** — all tasks of the project, filterable by **all**, **active** and **archived**, with branch, pull
  request, creation and activity times and state. Each row has **Open**, **Archive** or **Restore**, and
  **Delete**.
- **Worktrees** — every git worktree of the repository with branch, base, size and task. **Reveal** shows it in
  Finder, **Open task** opens its task, and **Remove** deletes worktrees that belong to no task. If worktree
  directories were deleted outside Asterism, **Prune missing worktrees** cleans them up.
- **Settings** — **Default base branch** for new tasks. **Automatic** uses `origin`'s default branch.

CLI: `asterism project worktrees`, `asterism project set-base <BRANCH|auto>`.

## Archive, restore and delete

- **Archive task** stops the task's sessions and hides it from the sidebar. Its worktree and branch stay.
- **Restore** (on the project page's **Tasks** tab, filter **archived**) brings an archived task back. If the
  worktree is gone, it is recreated from the branch.
- **Delete task** stops the sessions and removes the worktree. Asterism warns if the worktree has uncommitted
  changes, then asks whether to also delete the branch, mentioning commits that are not on the base branch.

CLI:

```sh
asterism task list [--all]          # --all includes archived tasks
asterism task archive <ID>
asterism task restore <ID>
asterism task delete <ID> [--delete-branch]
```
