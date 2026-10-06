use std::fs::OpenOptions;
use std::io::IsTerminal;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use asterism_core::config;
use asterism_core::paths::Paths;
use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::{ErrorKind, RpcError};
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use tokio::net::UnixStream;

const SPAWN_ATTEMPTS: u32 = 60;
const SPAWN_POLL: Duration = Duration::from_millis(50);
const HOOK_TIMEOUT: Duration = Duration::from_secs(2);
const INHERITED_ENV_BLOCKLIST: &[&str] = &["ASTERISM_TASK", "ASTERISM_SESSION", "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"];

#[derive(Parser)]
#[command(name = "asterism", version, about = "Orchestrate coding agents in parallel git worktrees")]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(subcommand)]
    Project(ProjectCmd),
    #[command(subcommand)]
    Task(TaskCmd),
    #[command(subcommand)]
    Session(SessionCmd),
    /// Type text into a session (submits with Enter unless --no-submit).
    Send {
        session: i64,
        text: String,
        #[arg(long)]
        no_submit: bool,
    },
    /// Print the last lines of a session's screen.
    Read {
        session: i64,
        #[arg(long, default_value_t = 50)]
        lines: usize,
    },
    /// Block until a session reaches a status (or exits).
    Wait {
        session: i64,
        #[arg(long, value_enum)]
        until: StatusArg,
        #[arg(long, value_parser = humantime::parse_duration)]
        timeout: Option<Duration>,
    },
    /// Bridge stdin/stdout to the local daemon socket.
    Attach,
    /// Report a normalized agent hook event for the current session; never fails.
    Hook {
        event: HookArg,
        /// The agent's own session id, used to resume after a daemon restart.
        #[arg(long)]
        agent_ref: Option<String>,
    },
    #[command(subcommand)]
    Pr(PrCmd),
    #[command(subcommand)]
    Issue(IssueCmd),
    #[command(subcommand)]
    Daemon(DaemonCmd),
    #[command(subcommand)]
    Plugin(PluginCmd),
    #[command(subcommand)]
    Store(StoreCmd),
    /// A command contributed by a plugin (see `asterism plugin list`).
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum StoreCmd {
    List,
    /// Add a store from a git URL or a local directory.
    Add {
        source: String,
        #[arg(long)]
        yes: bool,
    },
    Remove {
        name: String,
        #[arg(long)]
        uninstall_plugins: bool,
    },
    Refresh { name: Option<String> },
    /// Install plugin updates automatically when stores refresh.
    AutoUpdate { state: OnOff },
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum CapabilityArg {
    Forge,
    Agent,
    Command,
    TaskSource,
}

impl From<CapabilityArg> for CapabilityKind {
    fn from(arg: CapabilityArg) -> Self {
        match arg {
            CapabilityArg::Forge => Self::Forge,
            CapabilityArg::Agent => Self::Agent,
            CapabilityArg::Command => Self::Command,
            CapabilityArg::TaskSource => Self::TaskSource,
        }
    }
}

#[derive(Subcommand)]
enum PluginCmd {
    List,
    /// Use the plugin in PATH directly, for local development.
    Link { path: PathBuf },
    Unlink { name: String },
    /// Re-read manifests and restart plugin backends.
    Reload { name: Option<String> },
    /// Show settings, or set one; secrets are read from the terminal.
    Config {
        name: String,
        key: Option<String>,
        value: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    /// Search the plugins of all stores.
    Search {
        query: Option<String>,
        #[arg(long, value_enum)]
        capability: Option<CapabilityArg>,
        #[arg(long)]
        store: Option<String>,
    },
    /// Show a store plugin's details and README.
    Info {
        name: String,
        #[arg(long)]
        store: Option<String>,
    },
    Install {
        name: String,
        #[arg(long)]
        store: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Update one plugin, or all with updates.
    Update {
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        all: bool,
        #[arg(long)]
        yes: bool,
    },
    Rollback { name: String },
    Uninstall { name: String },
    Enable { name: String },
    Disable { name: String },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Register the git repository at PATH (default: current directory).
    Add { path: Option<PathBuf> },
    List,
    Remove { id: i64 },
    /// List the git worktrees of a project.
    Worktrees {
        #[arg(long)]
        project: Option<String>,
    },
    /// Set the default base branch for new tasks; "auto" uses origin's default branch.
    SetBase {
        base: String,
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
enum PrCmd {
    /// Fetch the pull request status of a project's tasks now.
    Refresh {
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
enum IssueCmd {
    /// List open issues from a task source; by default only yours.
    Search {
        source: String,
        query: Option<String>,
        /// Include issues not assigned to you.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Create a task with its own branch and worktree, optionally starting an agent.
    #[command(alias = "create")]
    New {
        /// Required unless --issue is given.
        #[arg(conflicts_with = "title_flag")]
        title: Option<String>,
        #[arg(long = "title")]
        title_flag: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        prompt: Option<String>,
        /// Branch or ref to start from (default: the project's default base).
        #[arg(long)]
        base: Option<String>,
        /// Create the task from an issue, e.g. linear:TRA-1343 or github-issues:#42.
        #[arg(long)]
        issue: Option<String>,
    },
    List {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Stop the task's sessions; worktree and branch stay.
    Archive { id: i64 },
    Restore { id: i64 },
    /// Delete the task and its worktree, optionally its branch too.
    Delete {
        id: i64,
        #[arg(long)]
        delete_branch: bool,
    },
    /// Show the task's changes against its base branch.
    Diff { id: Option<i64> },
}

#[derive(Subcommand)]
enum SessionCmd {
    /// Start an agent (default: claude), a shell, or a command after `--`.
    Start {
        task: Option<i64>,
        #[arg(long, conflicts_with = "shell")]
        agent: Option<String>,
        #[arg(long)]
        shell: bool,
        #[arg(last = true)]
        command: Vec<String>,
    },
    List {
        #[arg(long)]
        task: Option<i64>,
    },
    Kill { id: i64 },
}

#[derive(Subcommand)]
enum DaemonCmd {
    Status,
    Stop,
}

#[derive(Clone, Copy, ValueEnum)]
enum StatusArg {
    Idle,
    #[value(name = "waiting_input")]
    WaitingInput,
    Exited,
}

impl From<StatusArg> for SessionStatus {
    fn from(arg: StatusArg) -> Self {
        match arg {
            StatusArg::Idle => Self::Idle,
            StatusArg::WaitingInput => Self::WaitingInput,
            StatusArg::Exited => Self::Exited,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum HookArg {
    PromptSubmit,
    Tool,
    Stop,
    Notification,
}

impl From<HookArg> for HookEvent {
    fn from(arg: HookArg) -> Self {
        match arg {
            HookArg::PromptSubmit => Self::PromptSubmit,
            HookArg::Tool => Self::Tool,
            HookArg::Stop => Self::Stop,
            HookArg::Notification => Self::Notification,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(_) if std::env::args().nth(1).as_deref() == Some("hook") => std::process::exit(0),
        Err(err) => err.exit(),
    };
    if let Cmd::Hook { event, agent_ref } = cli.command {
        let _ = tokio::time::timeout(HOOK_TIMEOUT, hook(event, agent_ref)).await;
        std::process::exit(0);
    }
    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), ClientError> {
    let json = cli.json;
    if let Cmd::Attach = cli.command {
        return Ok(attach().await?);
    }
    if let Cmd::Daemon(DaemonCmd::Stop) = cli.command {
        if let Ok(stream) = UnixStream::connect(socket_path()).await {
            let (reader, writer) = stream.into_split();
            Client::new(reader, writer).call::<_, ()>(method::SHUTDOWN, ()).await?;
        }
        print_ok(json);
        return Ok(());
    }
    let client = connect().await?;
    match cli.command {
        Cmd::Project(ProjectCmd::Add { path }) => {
            let path = path.map_or_else(std::env::current_dir, Ok)?;
            let project: Project =
                client.call(method::PROJECT_ADD, ProjectAddParams { path: path.display().to_string() }).await?;
            print(json, &project, || project_line(&project));
        }
        Cmd::Project(ProjectCmd::List) => {
            let projects: Vec<Project> = client.call(method::PROJECT_LIST, ()).await?;
            print(json, &projects, || lines(&projects, project_line));
        }
        Cmd::Project(ProjectCmd::Worktrees { project }) => {
            let project_id = resolve_project(&client, project).await?;
            let list: Vec<Worktree> = client.call(method::PROJECT_WORKTREES, ProjectIdParams { project_id }).await?;
            print(json, &list, || {
                lines(&list, |w| {
                    let branch = w.branch.clone().unwrap_or_else(|| format!("detached@{}", &w.head[..w.head.len().min(7)]));
                    let task = w.task_id.map(|id| format!("\ttask {id}")).unwrap_or_default();
                    format!("{}\t{branch}{task}", w.path)
                })
            });
        }
        Cmd::Project(ProjectCmd::SetBase { base, project }) => {
            let project_id = resolve_project(&client, project).await?;
            let default_base = (base != "auto").then_some(base);
            let updated: Project = client.call(method::PROJECT_UPDATE, ProjectUpdateParams { project_id, default_base }).await?;
            print(json, &updated, || format!("default base: {}", updated.default_base.as_deref().unwrap_or("auto")));
        }
        Cmd::Project(ProjectCmd::Remove { id }) => {
            client.call::<_, ()>(method::PROJECT_REMOVE, ProjectIdParams { project_id: id }).await?;
            print_ok(json);
        }
        Cmd::Task(TaskCmd::New { title, title_flag, project, agent, prompt, base, issue }) => {
            let title = title.or(title_flag);
            let project_id = resolve_project(&client, project).await?;
            // Also fetches origin, so the default and `--base origin/…` see the remote's current state.
            let branches: ProjectBranches = client.call(method::PROJECT_BRANCHES, ProjectIdParams { project_id }).await?;
            if let Some(e) = &branches.fetch_error {
                eprintln!("warning: could not fetch origin: {e}");
            }
            let base = base.or(branches.default);
            let params = match issue {
                Some(spec) => {
                    let (source, key) = spec.split_once(':').ok_or_else(|| invalid("--issue takes <source>:<key>".into()))?;
                    let details: IssueDetails = client
                        .call(method::TASK_SOURCE_GET, TaskSourceGetParams { project_id, source: source.into(), key: key.into() })
                        .await?;
                    TaskCreateParams {
                        project_id,
                        title: title.unwrap_or_default(),
                        prompt: prompt.or(Some(details.prompt)),
                        agent,
                        base,
                        issue: Some(TaskIssue {
                            source: details.source,
                            key: details.key,
                            title: details.title,
                            url: details.url,
                            branch: Some(details.branch).filter(|b| !b.is_empty()),
                        }),
                    }
                }
                None => {
                    let title = title.ok_or_else(|| invalid("give the task a title or --issue <source>:<key>".into()))?;
                    TaskCreateParams { project_id, title, prompt, agent, base, issue: None }
                }
            };
            let created: TaskCreateResult = client.call(method::TASK_CREATE, params).await?;
            print(json, &created, || {
                let mut out = task_line(&created.task);
                if let Some(session) = &created.session {
                    out.push('\n');
                    out.push_str(&session_line(session));
                }
                out
            });
        }
        Cmd::Issue(IssueCmd::Search { source, query, all, project }) => {
            let project_id = resolve_project(&client, project).await?;
            let params = TaskSourceSearchParams { project_id, source, query: query.unwrap_or_default(), assigned_to_me: !all };
            let hits: Vec<IssueHit> = client.call(method::TASK_SOURCE_SEARCH, params).await?;
            print(json, &hits, || {
                lines(&hits, |h| format!("{}\t{}\t{}\t{}", h.key, h.state, h.title, h.assignee.clone().unwrap_or_default()))
            });
        }
        Cmd::Task(TaskCmd::List { project, all }) => {
            let project_id = match project {
                Some(p) => Some(resolve_project(&client, Some(p)).await?),
                None => None,
            };
            let tasks: Vec<Task> =
                client.call(method::TASK_LIST, TaskListParams { project_id, include_archived: all }).await?;
            let prs: PrList = client.call(method::PR_LIST, PrListParams { project_id }).await.unwrap_or_default();
            let pr_of = |id: i64| prs.prs.iter().find(|p| p.task_id == id).map(|p| &p.pr);
            if json {
                let rows: Vec<serde_json::Value> = tasks
                    .iter()
                    .map(|t| {
                        let mut row = serde_json::to_value(t).unwrap_or_default();
                        row["pr"] = serde_json::to_value(pr_of(t.id)).unwrap_or_default();
                        row
                    })
                    .collect();
                print(json, &rows, String::new);
            } else {
                print(json, &tasks, || lines(&tasks, |t| format!("{}{}", task_line(t), pr_suffix(pr_of(t.id)))));
            }
        }
        Cmd::Pr(PrCmd::Refresh { project }) => {
            let project_id = resolve_project(&client, project).await?;
            let list: PrList = client.call(method::PR_REFRESH, ProjectIdParams { project_id }).await?;
            print(json, &list, || {
                let mut out: Vec<String> = list.prs.iter().map(|p| format!("{}\t{}{}", p.task_id, p.branch, pr_suffix(Some(&p.pr)))).collect();
                out.extend(list.errors.iter().map(|e| format!("error\t{}", e.message)));
                out.join("\n")
            });
        }
        Cmd::Task(TaskCmd::Archive { id }) => {
            let task: Task = client.call(method::TASK_ARCHIVE, TaskArchiveParams { task_id: id, force: false }).await?;
            print(json, &task, || task_line(&task));
        }
        Cmd::Task(TaskCmd::Restore { id }) => {
            let task: Task = client.call(method::TASK_RESTORE, TaskIdParams { task_id: id }).await?;
            print(json, &task, || task_line(&task));
        }
        Cmd::Task(TaskCmd::Delete { id, delete_branch }) => {
            let result: TaskDeleteResult = client.call(method::TASK_DELETE, TaskDeleteParams { task_id: id, delete_branch }).await?;
            print(json, &result, || result.warning.clone().unwrap_or_else(|| format!("deleted task {id}")));
        }
        Cmd::Task(TaskCmd::Diff { id }) => {
            let diff: TaskDiffResult = client.call(method::TASK_DIFF, TaskIdParams { task_id: resolve_task(id)? }).await?;
            print(json, &diff, || diff.patch.clone());
        }
        Cmd::Session(SessionCmd::Start { task, agent, shell, command }) => {
            let kind = if shell {
                SessionKind::Shell
            } else if !command.is_empty() {
                SessionKind::Command { argv: command }
            } else {
                SessionKind::Agent { name: agent.unwrap_or_else(|| "claude".into()) }
            };
            let session: Session = client
                .call(method::SESSION_START, SessionStartParams { task_id: resolve_task(task)?, kind, prompt: None })
                .await?;
            print(json, &session, || session_line(&session));
        }
        Cmd::Session(SessionCmd::List { task }) => {
            let sessions: Vec<Session> = client.call(method::SESSION_LIST, SessionListParams { task_id: task }).await?;
            print(json, &sessions, || lines(&sessions, session_line));
        }
        Cmd::Session(SessionCmd::Kill { id }) => {
            client.call::<_, ()>(method::SESSION_KILL, SessionIdParams { session_id: id }).await?;
            print_ok(json);
        }
        Cmd::Send { session, text, no_submit } => {
            client
                .call::<_, ()>(method::SESSION_SEND, SessionSendParams { session_id: session, text, submit: !no_submit })
                .await?;
            print_ok(json);
        }
        Cmd::Read { session, lines: count } => {
            let read: SessionReadResult =
                client.call(method::SESSION_READ, SessionReadParams { session_id: session, lines: count }).await?;
            print(json, &read, || read.text.clone());
        }
        Cmd::Wait { session, until, timeout } => {
            let params = SessionWaitParams {
                session_id: session,
                until: until.into(),
                timeout_ms: timeout.map(|d| d.as_millis().try_into().unwrap_or(u64::MAX)),
            };
            let waited: SessionWaitResult = client.call(method::SESSION_WAIT, params).await?;
            print(json, &waited, || label(&waited.status));
        }
        Cmd::Daemon(DaemonCmd::Status) => {
            let hello: HelloResult = client.call(method::HELLO, hello_params()).await?;
            print(json, &hello, || {
                format!("asterismd {} (pid {}) on {}", hello.daemon_version, hello.pid, hello.hostname)
            });
        }
        Cmd::Plugin(PluginCmd::List) => {
            let plugins: Vec<PluginInfo> = client.call(method::PLUGIN_LIST, ()).await?;
            print(json, &plugins, || lines(&plugins, plugin_line));
        }
        Cmd::Plugin(PluginCmd::Link { path }) => {
            let path = std::path::absolute(&path)?;
            let info: PluginInfo = client.call(method::PLUGIN_LINK, PluginPathParams { path: path.display().to_string() }).await?;
            print(json, &info, || plugin_line(&info));
        }
        Cmd::Plugin(PluginCmd::Unlink { name }) => {
            client.call::<_, ()>(method::PLUGIN_UNLINK, PluginNameParams { name }).await?;
            print_ok(json);
        }
        Cmd::Plugin(PluginCmd::Reload { name }) => {
            client.call::<_, ()>(method::PLUGIN_RELOAD, PluginReloadParams { name }).await?;
            print_ok(json);
        }
        Cmd::Plugin(PluginCmd::Config { name, key, value, clear }) => plugin_config(&client, json, name, key, value, clear).await?,
        Cmd::Store(StoreCmd::List) => {
            let list: StoreList = client.call(method::STORE_LIST, ()).await?;
            print(json, &list, || {
                let mut out = format!("auto-update: {}", if list.auto_update { "on" } else { "off" });
                if let Some(error) = &list.error {
                    out.push_str(&format!("\nerror: {error}"));
                }
                for store in &list.stores {
                    out.push('\n');
                    out.push_str(&store_line(store));
                }
                out
            });
        }
        Cmd::Store(StoreCmd::Add { source, yes }) => {
            if asterism_core::plugins::catalog::official_source().as_deref() != Some(source.as_str()) {
                confirm(&format!("{TRUST_WARNING} Add {source}?"), yes)?;
            }
            let source = std::fs::canonicalize(&source).map(|p| p.display().to_string()).unwrap_or(source);
            let store: StoreInfo = client.call(method::STORE_ADD, StoreAddParams { source }).await?;
            print(json, &store, || store_line(&store));
        }
        Cmd::Store(StoreCmd::Remove { name, uninstall_plugins }) => {
            client.call::<_, ()>(method::STORE_REMOVE, StoreRemoveParams { name, uninstall_plugins }).await?;
            print_ok(json);
        }
        Cmd::Store(StoreCmd::Refresh { name }) => {
            client.call::<_, ()>(method::STORE_REFRESH, StoreRefreshParams { name }).await?;
            print_ok(json);
        }
        Cmd::Store(StoreCmd::AutoUpdate { state }) => {
            let enabled = matches!(state, OnOff::On);
            client.call::<_, ()>(method::STORE_SET_AUTO_UPDATE, AutoUpdateParams { enabled }).await?;
            print_ok(json);
        }
        Cmd::Plugin(PluginCmd::Search { query, capability, store }) => {
            let params = PluginSearchParams { query, capability: capability.map(Into::into), store };
            let hits: Vec<SearchHit> = client.call(method::PLUGIN_SEARCH, params).await?;
            print(json, &hits, || lines(&hits, hit_line));
        }
        Cmd::Plugin(PluginCmd::Info { name, store }) => {
            let store = resolve_store(&client, &name, store).await?;
            let d = details(&client, &store, &name).await?;
            print(json, &d, || {
                let capabilities: Vec<String> = d.capabilities.iter().map(|c| format!("{}:{}", label(&c.kind), c.id)).collect();
                let mut out = format!("{} {} ({})\n{}\ncapabilities: {}\n{}", d.name, d.version, d.store, d.description, capabilities.join(", "), permission_list(&d.permissions));
                if let Some(readme) = &d.readme {
                    out.push_str("\n\n");
                    out.push_str(readme.trim_end());
                }
                out
            });
        }
        Cmd::Plugin(PluginCmd::Install { name, store, yes }) => {
            let store = resolve_store(&client, &name, store).await?;
            let d = details(&client, &store, &name).await?;
            confirm(&format!("Install {name} {} from {store} with {}?", d.version, permission_list(&d.permissions)), yes)?;
            let params = PluginInstallParams { store, name, accept_permissions: d.permissions };
            let info: PluginInfo = client.call(method::PLUGIN_INSTALL, params).await?;
            print(json, &info, || plugin_line(&info));
        }
        Cmd::Plugin(PluginCmd::Update { name, all, yes }) => {
            let names = match (name, all) {
                (Some(name), _) => vec![name],
                (None, true) => {
                    let plugins: Vec<PluginInfo> = client.call(method::PLUGIN_LIST, ()).await?;
                    plugins.into_iter().filter(|p| p.update_available).map(|p| p.name).collect()
                }
                (None, false) => return Err(invalid("name a plugin or pass --all".into())),
            };
            let mut updated = Vec::new();
            for name in names {
                updated.push(update_plugin(&client, name, yes).await?);
            }
            print(json, &updated, || lines(&updated, plugin_line));
        }
        Cmd::Plugin(PluginCmd::Rollback { name }) => {
            let info: PluginInfo = client.call(method::PLUGIN_ROLLBACK, PluginNameParams { name }).await?;
            print(json, &info, || plugin_line(&info));
        }
        Cmd::Plugin(PluginCmd::Uninstall { name }) => {
            client.call::<_, ()>(method::PLUGIN_UNINSTALL, PluginNameParams { name }).await?;
            print_ok(json);
        }
        Cmd::Plugin(PluginCmd::Enable { name }) => {
            client.call::<_, ()>(method::PLUGIN_SET_ENABLED, PluginEnableParams { name, enabled: true }).await?;
            print_ok(json);
        }
        Cmd::Plugin(PluginCmd::Disable { name }) => {
            client.call::<_, ()>(method::PLUGIN_SET_ENABLED, PluginEnableParams { name, enabled: false }).await?;
            print_ok(json);
        }
        Cmd::External(args) => return run_external(&client, args).await,
        Cmd::Attach | Cmd::Hook { .. } | Cmd::Daemon(DaemonCmd::Stop) => {}
    }
    Ok(())
}

fn hello_params() -> HelloParams {
    HelloParams { proto_version: PROTO_VERSION, client_kind: ClientKind::Cli }
}

fn socket_path() -> PathBuf {
    std::env::var_os("ASTERISM_SOCKET").map(PathBuf::from).unwrap_or_else(|| Paths::from_env().socket())
}

fn spawn_daemon() -> std::io::Result<()> {
    let paths = Paths::from_env();
    paths.ensure_dirs()?;
    let log = OpenOptions::new().create(true).append(true).open(paths.log())?;
    let daemon = std::env::current_exe()?.with_file_name("asterismd");
    let mut cmd = Command::new(daemon);
    // The daemon must not inherit the caller's session, agent, or git context.
    cmd.current_dir("/");
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if INHERITED_ENV_BLOCKLIST.contains(&&*name) || config::removed_by_default(&name) {
            cmd.env_remove(&key);
        }
    }
    // Own process group so the daemon outlives the terminal or app that started it.
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .process_group(0)
        .spawn()?;
    Ok(())
}

async fn connect_stream() -> std::io::Result<UnixStream> {
    let socket = socket_path();
    if let Ok(stream) = UnixStream::connect(&socket).await {
        return Ok(stream);
    }
    spawn_daemon()?;
    for _ in 0..SPAWN_ATTEMPTS {
        tokio::time::sleep(SPAWN_POLL).await;
        if let Ok(stream) = UnixStream::connect(&socket).await {
            return Ok(stream);
        }
    }
    UnixStream::connect(&socket).await
}

async fn connect() -> Result<Client, ClientError> {
    let (reader, writer) = connect_stream().await?.into_split();
    let client = Client::new(reader, writer);
    client.call::<_, HelloResult>(method::HELLO, hello_params()).await?;
    Ok(client)
}

async fn attach() -> std::io::Result<()> {
    let (mut reader, mut writer) = connect_stream().await?.into_split();
    let (mut stdin, mut stdout) = (tokio::io::stdin(), tokio::io::stdout());
    tokio::select! {
        res = tokio::io::copy(&mut stdin, &mut writer) => res.map(|_| ()),
        res = tokio::io::copy(&mut reader, &mut stdout) => res.map(|_| ()),
    }
}

/// Runs inside agent hooks: must never fail, block, or start a daemon.
async fn hook(event: HookArg, agent_ref: Option<String>) {
    let Some(session_id) = env_id("ASTERISM_SESSION") else { return };
    let Ok(stream) = UnixStream::connect(socket_path()).await else { return };
    let (reader, writer) = stream.into_split();
    let client = Client::new(reader, writer);
    let params = SessionHookParams { session_id, event: event.into(), agent_ref };
    let _ = client.call::<_, ()>(method::SESSION_HOOK, params).await;
}

async fn resolve_project(client: &Client, project: Option<String>) -> Result<i64, ClientError> {
    if let Some(wanted) = project {
        let projects: Vec<Project> = client.call(method::PROJECT_LIST, ()).await?;
        return projects
            .iter()
            .find(|p| p.id.to_string() == wanted || p.name == wanted)
            .map(|p| p.id)
            .ok_or_else(|| invalid(format!("no project with id or name {wanted}")));
    }
    if let Some(task_id) = env_id("ASTERISM_TASK") {
        let tasks: Vec<Task> =
            client.call(method::TASK_LIST, TaskListParams { project_id: None, include_archived: true }).await?;
        if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
            return Ok(task.project_id);
        }
    }
    let cwd = std::env::current_dir()?;
    let project: Project = client.call(method::PROJECT_ADD, ProjectAddParams { path: cwd.display().to_string() }).await?;
    Ok(project.id)
}

fn resolve_task(task: Option<i64>) -> Result<i64, ClientError> {
    task.or_else(|| env_id("ASTERISM_TASK"))
        .ok_or_else(|| invalid("pass a task id or run inside an asterism session".into()))
}

fn env_id(name: &str) -> Option<i64> {
    std::env::var(name).ok()?.parse().ok()
}

const TRUST_WARNING: &str = "Plugins from this store run code on your machine.";

/// Asks on the terminal; without one, only `--yes` confirms.
fn confirm(question: &str, yes: bool) -> Result<(), ClientError> {
    if yes {
        eprintln!("{question} (accepted via --yes)");
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(invalid(format!("{question} Pass --yes to confirm.")));
    }
    eprint!("{question} [y/N] ");
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if matches!(answer.trim(), "y" | "Y" | "yes") {
        Ok(())
    } else {
        Err(invalid("cancelled".into()))
    }
}

fn permission_list(permissions: &[String]) -> String {
    if permissions.is_empty() {
        "no special permissions".into()
    } else {
        format!("permissions {}", permissions.join(", "))
    }
}

async fn resolve_store(client: &Client, name: &str, store: Option<String>) -> Result<String, ClientError> {
    if let Some(store) = store {
        return Ok(store);
    }
    let params = PluginSearchParams { query: Some(name.to_string()), ..Default::default() };
    let hits: Vec<SearchHit> = client.call(method::PLUGIN_SEARCH, params).await?;
    let stores: Vec<String> = hits.into_iter().filter(|h| h.name == name).map(|h| h.store).collect();
    match stores.as_slice() {
        [] => Err(ClientError::Rpc(RpcError::new(ErrorKind::NotFound, format!("no store provides {name}")))),
        [store] => Ok(store.clone()),
        many => Err(invalid(format!("{name} is in several stores ({}); pass --store", many.join(", ")))),
    }
}

async fn details(client: &Client, store: &str, name: &str) -> Result<PluginDetails, ClientError> {
    client.call(method::PLUGIN_DETAILS, PluginRefParams { store: store.to_string(), name: name.to_string() }).await
}

async fn update_plugin(client: &Client, name: String, yes: bool) -> Result<PluginInfo, ClientError> {
    let first = client.call(method::PLUGIN_UPDATE, PluginUpdateParams { name: name.clone(), accept_permissions: None }).await;
    match first {
        Err(ClientError::Rpc(e)) if e.kind() == ErrorKind::PermissionsChanged => {
            let plugins: Vec<PluginInfo> = client.call(method::PLUGIN_LIST, ()).await?;
            let store = plugins.iter().find(|p| p.name == name).and_then(|p| p.store.clone()).ok_or_else(|| ClientError::Rpc(e.clone()))?;
            let offered = details(client, &store, &name).await?;
            confirm(&format!("{name} {} asks for {}. Update?", offered.version, permission_list(&offered.permissions)), yes)?;
            client.call(method::PLUGIN_UPDATE, PluginUpdateParams { name, accept_permissions: Some(offered.permissions) }).await
        }
        other => other,
    }
}

fn store_line(s: &StoreInfo) -> String {
    let official = if s.official { "\tofficial" } else { "" };
    let error = s.last_error.as_ref().map(|e| format!("\terror: {e}")).unwrap_or_default();
    format!("{}\t{}{official}\t{} plugins{error}", s.name, s.source, s.plugin_count)
}

fn hit_line(h: &SearchHit) -> String {
    let installed = h.installed_version.as_ref().map(|v| format!("\tinstalled {v}")).unwrap_or_default();
    let update = if h.update_available { "\tupdate available" } else { "" };
    format!("{}\t{}\t{}{installed}{update}", h.store, h.name, h.description)
}

fn invalid(message: String) -> ClientError {
    ClientError::Rpc(RpcError::new(ErrorKind::InvalidParams, message))
}

fn print<T: Serialize>(json: bool, value: &T, human: impl FnOnce() -> String) {
    if json {
        println!("{}", serde_json::to_string(value).unwrap_or_default());
    } else {
        let text = human();
        if !text.is_empty() {
            println!("{text}");
        }
    }
}

fn print_ok(json: bool) {
    if json {
        println!("{{\"ok\":true}}");
    }
}

fn lines<T>(items: &[T], line: impl Fn(&T) -> String) -> String {
    items.iter().map(line).collect::<Vec<_>>().join("\n")
}

fn label<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

fn plugin_line(p: &PluginInfo) -> String {
    let state = match &p.state {
        PluginState::Ok => "ok".to_string(),
        PluginState::NeedsSetup { missing } => format!("needs setup ({})", missing.join(", ")),
        PluginState::Broken { reason } => format!("broken: {reason}"),
        PluginState::Failing { reason } => format!("failing: {reason}"),
        PluginState::Disabled => "disabled".to_string(),
    };
    let origin = match p.origin {
        PluginOrigin::Builtin => "builtin".to_string(),
        PluginOrigin::Linked => "linked".to_string(),
        PluginOrigin::Installed => format!("installed from {}", p.store.as_deref().unwrap_or("?")),
    };
    let update = if p.update_available { "\tupdate available" } else { "" };
    let capabilities: Vec<String> = p.capabilities.iter().map(|c| format!("{}:{}", label(&c.kind), c.id)).collect();
    format!("{}\t{}\t{origin}\t{state}\t{}{update}", p.name, p.version.as_deref().unwrap_or("?"), capabilities.join(","))
}

async fn plugin_config(client: &Client, json: bool, name: String, key: Option<String>, value: Option<String>, clear: bool) -> Result<(), ClientError> {
    let settings: PluginSettings = client.call(method::PLUGIN_SETTINGS, PluginNameParams { name: name.clone() }).await?;
    let Some(key) = key else {
        print(json, &settings, || {
            lines(&settings.schema, |spec| match spec.kind {
                SettingType::Secret => {
                    let set = if settings.secrets_set.contains(&spec.key) { "<set>" } else { "<not set>" };
                    format!("{} = {set}", spec.key)
                }
                _ => format!("{} = {}", spec.key, settings.values.get(&spec.key).map_or("<unset>".into(), |v| v.to_string())),
            })
        });
        return Ok(());
    };
    let spec = settings.schema.iter().find(|s| s.key == key).ok_or_else(|| invalid(format!("{name} has no setting {key:?}")))?;
    let new = match (clear, spec.kind, value) {
        (true, _, _) => serde_json::Value::Null,
        (false, SettingType::Secret, None) => match read_secret(&spec.title)? {
            secret if secret.is_empty() => return Err(invalid("a secret cannot be empty; use --clear to remove it".into())),
            secret => serde_json::Value::String(secret),
        },
        (false, SettingType::Secret, Some(_)) => return Err(invalid("secrets are read from the terminal; omit the value".into())),
        (false, SettingType::Bool, Some(v)) => serde_json::Value::Bool(v.parse().map_err(|_| invalid(format!("{key}: expected true or false")))?),
        (false, SettingType::Number, Some(v)) => serde_json::from_str::<serde_json::Number>(&v)
            .map(serde_json::Value::Number)
            .map_err(|_| invalid(format!("{key}: expected a number")))?,
        (false, _, Some(v)) => serde_json::Value::String(v),
        (false, _, None) => {
            let current = settings.values.get(&key).cloned().unwrap_or_default();
            print(json, &current, || current.to_string());
            return Ok(());
        }
    };
    let values = [(key, new)].into();
    client.call::<_, ()>(method::PLUGIN_SET_SETTINGS, PluginSetSettingsParams { name, values }).await?;
    print_ok(json);
    Ok(())
}

/// ponytail: `stty -echo` instead of a password-prompt crate; Unix terminals only.
fn read_secret(title: &str) -> std::io::Result<String> {
    let tty = std::io::stdin().is_terminal();
    if tty {
        eprint!("{title}: ");
        let _ = Command::new("stty").arg("-echo").stdin(Stdio::inherit()).status();
    }
    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line);
    if tty {
        let _ = Command::new("stty").arg("echo").stdin(Stdio::inherit()).status();
        eprintln!();
    }
    read?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// Replaces this process with the plugin backend in command mode, so its exit code is ours.
async fn run_external(client: &Client, args: Vec<String>) -> Result<(), ClientError> {
    let (name, rest) = args.split_first().ok_or_else(|| invalid("missing command".into()))?;
    let plugins: Vec<PluginInfo> = client.call(method::PLUGIN_LIST, ()).await?;
    let runnable = |p: &&PluginInfo| matches!(p.state, PluginState::Ok | PluginState::NeedsSetup { .. });
    let argv = plugins
        .iter()
        .filter(runnable)
        .find(|p| p.capabilities.iter().any(|c| c.kind == CapabilityKind::Command && &c.id == name))
        .and_then(|p| p.backend.clone())
        .ok_or_else(|| invalid(format!("unknown command {name:?}; see `asterism --help` and `asterism plugin list`")))?;
    let err = Command::new(&argv[0])
        .args(&argv[1..])
        .arg("command")
        .arg(name)
        .args(rest)
        .env("ASTERISM_SOCKET", socket_path())
        .env("ASTERISM_CLI", std::env::current_exe()?)
        .env("ASTERISM_PLUGIN_BIN", &argv[0])
        .exec();
    Err(err.into())
}

fn project_line(p: &Project) -> String {
    format!("{}\t{}\t{}", p.id, p.name, p.path)
}

fn pr_suffix(pr: Option<&PullRequest>) -> String {
    let Some(pr) = pr else { return String::new() };
    let checks = match pr.checks.state {
        ChecksState::Failure => ", checks failing",
        ChecksState::Pending => ", checks running",
        ChecksState::Success | ChecksState::None => "",
    };
    format!("\t#{} {}{checks}", pr.number, label(&pr.state))
}

fn task_line(t: &Task) -> String {
    let archived = if t.archived { "\tarchived" } else { "" };
    format!("{}\t{}\t{}{archived}", t.id, t.branch, t.worktree_path)
}

fn session_line(s: &Session) -> String {
    let kind = match &s.kind {
        SessionKind::Agent { name } => name.clone(),
        SessionKind::Shell => "shell".into(),
        SessionKind::Command { argv } => argv.join(" "),
    };
    format!("{}\ttask {}\t{}\t{kind}", s.id, s.task_id, label(&s.status))
}
