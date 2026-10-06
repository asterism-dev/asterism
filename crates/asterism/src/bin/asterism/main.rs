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
    Daemon(DaemonCmd),
    #[command(subcommand)]
    Plugin(PluginCmd),
    /// A command contributed by a plugin (see `asterism plugin list`).
    #[command(external_subcommand)]
    External(Vec<String>),
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
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Create a task with its own branch and worktree, optionally starting an agent.
    New {
        title: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        prompt: Option<String>,
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
        Cmd::Project(ProjectCmd::Remove { id }) => {
            client.call::<_, ()>(method::PROJECT_REMOVE, ProjectIdParams { project_id: id }).await?;
            print_ok(json);
        }
        Cmd::Task(TaskCmd::New { title, project, agent, prompt }) => {
            let project_id = resolve_project(&client, project).await?;
            let created: TaskCreateResult =
                client.call(method::TASK_CREATE, TaskCreateParams { project_id, title, prompt, agent }).await?;
            print(json, &created, || {
                let mut out = task_line(&created.task);
                if let Some(session) = &created.session {
                    out.push('\n');
                    out.push_str(&session_line(session));
                }
                out
            });
        }
        Cmd::Task(TaskCmd::List { project, all }) => {
            let project_id = match project {
                Some(p) => Some(resolve_project(&client, Some(p)).await?),
                None => None,
            };
            let tasks: Vec<Task> =
                client.call(method::TASK_LIST, TaskListParams { project_id, include_archived: all }).await?;
            print(json, &tasks, || lines(&tasks, task_line));
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
    };
    let origin = match p.origin {
        PluginOrigin::Builtin => "builtin",
        PluginOrigin::Linked => "linked",
    };
    let capabilities: Vec<String> = p.capabilities.iter().map(|c| format!("{}:{}", label(&c.kind), c.id)).collect();
    format!("{}\t{}\t{origin}\t{state}\t{}", p.name, p.version.as_deref().unwrap_or("?"), capabilities.join(","))
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
