mod common;

use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::paths::Paths;
use asterism_core::store::StoredProvider;
use asterism_proto::client::Client;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::Message;

fn vmfake_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/vmfake")
}

fn options(vms: &Path, fail: &str) -> DaemonOptions {
    DaemonOptions {
        plugin_env: vec![
            ("FIXTURE_VMS".into(), vms.display().to_string()),
            ("FIXTURE_FAIL".into(), fail.into()),
        ],
        ..common::daemon_options()
    }
}

fn link_vmfake(paths: &Paths) {
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(
        paths.plugin_links(),
        format!(
            "[links]\nvmfake = {:?}\n",
            vmfake_dir().display().to_string()
        ),
    )
    .unwrap();
}

fn daemon(home: &Path, fail: &str) -> Arc<Daemon> {
    let paths = Paths {
        home: home.join("h"),
    };
    paths.ensure_dirs().unwrap();
    link_vmfake(&paths);
    let vms = home.join("vms.json");
    std::fs::write(
        &vms,
        json!({"vms": {
            "a": {"name": "one", "state": "running", "destination": "u@one"},
            "b": {"name": "two", "state": "hibernated", "destination": "u@two"},
        }, "calls": []})
        .to_string(),
    )
    .unwrap();
    Daemon::with_options(paths, options(&vms, fail)).unwrap()
}

#[tokio::test]
async fn machine_provider_list_returns_vms_and_options() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "");
    let all = d.machine_providers().await.unwrap();
    assert_eq!(all.len(), 1);
    let info = &all[0];
    assert_eq!(
        (info.provider.as_str(), info.display_name.as_str()),
        ("vmfake", "Fake VMs")
    );
    assert_eq!(info.error, None);
    assert_eq!(info.vms.len(), 2);
    assert_eq!(info.create_options[0].key, "cpus");
}

#[tokio::test]
async fn machine_provider_list_reports_plugin_errors_per_provider() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "machine_provider.list");
    let all = d.machine_providers().await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].error.as_deref().unwrap().contains("forced failure"));
    assert!(all[0].vms.is_empty());
}

#[tokio::test]
async fn machine_provider_list_denied_to_remote_devices() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    let socket = paths.socket();
    tokio::spawn(asterism_core::run_with_options(
        paths,
        DaemonOptions {
            remote_retry: Duration::from_millis(100),
            ..common::daemon_options()
        },
    ));
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = Client::connect_unix(&socket).await.unwrap();
    let _: HelloResult = client
        .call(
            method::HELLO,
            HelloParams {
                proto_version: PROTO_VERSION,
                client_kind: ClientKind::Cli,
            },
        )
        .await
        .unwrap();
    let _: Value = client
        .call(
            method::REMOTE_CONFIG_SET,
            RemoteConfigSetParams {
                config: RemoteConfig {
                    enabled: true,
                    bind: "127.0.0.1".into(),
                    port: 0,
                    ..RemoteConfig::default()
                },
            },
        )
        .await
        .unwrap();
    let addr: SocketAddr = loop {
        let status: RemoteStatus = client.call(method::REMOTE_STATUS, json!({})).await.unwrap();
        if status.state == RemoteState::Listening {
            break status.address.unwrap().parse().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let pairing: RemotePairing = client
        .call(
            method::REMOTE_DEVICE_ADD,
            RemoteDeviceAddParams {
                name: "phone".into(),
            },
        )
        .await
        .unwrap();
    let token = pairing.pairing_url.split("#token=").nth(1).unwrap();

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    ws.send(Message::Text(json!({ "auth": token }).to_string().into()))
        .await
        .unwrap();
    let request =
        json!({"jsonrpc": "2.0", "id": 1, "method": method::MACHINE_PROVIDER_LIST, "params": {}});
    ws.send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let reply = loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["id"] == 1 {
                break value;
            }
        }
    };
    assert_eq!(reply["error"]["data"]["kind"], "forbidden");
}

fn provided(provider: &str, vm_ref: &str, owned: bool, hibernated: bool) -> StoredProvider {
    StoredProvider {
        provider: provider.into(),
        vm_ref: vm_ref.into(),
        owned,
        delete_with_tasks: false,
        hibernated,
    }
}

#[tokio::test]
async fn set_delete_with_tasks_refused_for_unowned() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "");
    let m = d
        .machines()
        .add_provided("one", "u@one", provided("vmfake", "a", false, true))
        .unwrap();
    let err = d.machines().set_delete_with_tasks(m.id, true).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
}

#[tokio::test]
async fn set_delete_with_tasks_updates_owned_machine() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "");
    let m = d
        .machines()
        .add_provided("one", "u@one", provided("vmfake", "a", true, true))
        .unwrap();
    let m = d.machines().set_delete_with_tasks(m.id, true).unwrap();
    assert!(m.provider.unwrap().delete_with_tasks);
}

#[tokio::test]
async fn listed_vms_exclude_existing_machines() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "");
    d.machines()
        .add_provided("one", "u@one", provided("vmfake", "a", false, true))
        .unwrap();
    let all = d.machine_providers().await.unwrap();
    let refs: Vec<_> = all[0].vms.iter().map(|vm| vm.vm_ref.as_str()).collect();
    assert_eq!(refs, ["b"]);
}

#[tokio::test]
async fn hibernated_machine_survives_restart_without_connecting() {
    let home = tempfile::tempdir().unwrap();
    let fake = common::FakeSsh::new();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    paths.ensure_dirs().unwrap();
    {
        let store = asterism_core::store::Store::open(&paths.db()).unwrap();
        store
            .add_provided_machine("one", "u@one", &provided("vmfake", "a", true, true))
            .unwrap();
    }
    let d = Daemon::with_options(
        paths,
        DaemonOptions {
            ssh_bin: fake.bin(),
            ..common::daemon_options()
        },
    )
    .unwrap();
    d.machines().start_stored().unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let machines = d.machines().list().unwrap();
    assert_eq!(machines[0].status.state, MachineState::Hibernated);
    assert_eq!(fake.invocations(), 0);
}

struct Provisioning {
    d: Arc<Daemon>,
    home: tempfile::TempDir,
    _fake: common::FakeSsh,
    project_id: i64,
}

impl Provisioning {
    fn vms(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.home.path().join("vms.json")).unwrap())
            .unwrap()
    }

    /// Arguments of every `ssh-keygen` run, one per line.
    fn keygen_runs(&self) -> String {
        std::fs::read_to_string(self.home.path().join("keygen.log")).unwrap_or_default()
    }

    fn calls(&self) -> Vec<String> {
        self.vms()["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["method"].as_str().unwrap().to_string())
            .collect()
    }

    fn store(&self) -> asterism_core::store::Store {
        let paths = Paths {
            home: self.home.path().join("h"),
        };
        asterism_core::store::Store::open(&paths.db()).unwrap()
    }

    fn break_origin(&self) {
        let missing = format!("file://{}/missing.git", self.home.path().display());
        common::run_git(
            &self.home.path().join("hub/demo"),
            &["remote", "set-url", "origin", &missing],
        );
    }

    async fn create(&self, vm: ProviderVmChoice) -> asterism_core::error::Result<TaskCreateResult> {
        self.d
            .create_task(TaskCreateParams {
                project_id: self.project_id,
                title: "On a VM".into(),
                provider_machine: Some(ProviderMachineChoice {
                    provider: "vmfake".into(),
                    vm,
                }),
                ..Default::default()
            })
            .await
    }
}

fn fixture_vms() -> Value {
    json!({"vms": {
        "a": {"name": "one", "state": "running", "destination": "u@one"},
        "b": {"name": "two", "state": "hibernated", "destination": "u@two"},
    }, "calls": []})
}

fn provisioning_options(home: &Path, fake: &common::FakeSsh) -> DaemonOptions {
    let keygen = home.join("keygen");
    std::fs::write(
        &keygen,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\n",
            home.join("keygen.log").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&keygen, std::fs::Permissions::from_mode(0o755)).unwrap();
    DaemonOptions {
        keygen_bin: keygen,
        plugin_env: vec![
            (
                "FIXTURE_VMS".into(),
                home.join("vms.json").display().to_string(),
            ),
            ("FIXTURE_DESTINATION".into(), "u@created".into()),
            ("FIXTURE_RESUME_DESTINATION".into(), "u@resumed".into()),
        ],
        ssh_bin: fake.bin(),
        machine_bins_dir: Some(fake.bins_dir()),
        machine_health_interval: Duration::from_millis(200),
        machine_health_timeout: Duration::from_secs(2),
        machine_backoff_max: Duration::from_millis(300),
        ..common::daemon_options()
    }
}

async fn provisioning() -> Provisioning {
    provisioning_with(|_| {}).await
}

async fn provisioning_with(configure: impl FnOnce(&mut DaemonOptions)) -> Provisioning {
    let home = tempfile::tempdir().unwrap();
    let fake = common::FakeSsh::new();
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    link_vmfake(&paths);
    std::fs::write(home.path().join("vms.json"), fixture_vms().to_string()).unwrap();
    let mut options = provisioning_options(home.path(), &fake);
    configure(&mut options);
    let d = Daemon::with_options(paths, options).unwrap();

    let work = home.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    common::init_repo(&work);
    let bare = home.path().join("demo.git");
    common::run_git(
        home.path(),
        &[
            "clone",
            "-q",
            "--bare",
            &work.display().to_string(),
            &bare.display().to_string(),
        ],
    );
    let repo = home.path().join("hub/demo");
    std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
    common::run_git(
        home.path(),
        &[
            "clone",
            "-q",
            &format!("file://{}", bare.display()),
            &repo.display().to_string(),
        ],
    );
    let project_id = d.add_project(&repo.display().to_string()).unwrap().id;
    Provisioning {
        d,
        home,
        _fake: fake,
        project_id,
    }
}

fn new_vm(options: Value) -> ProviderVmChoice {
    ProviderVmChoice::New {
        name: "vm-1".into(),
        options: options.as_object().unwrap().clone(),
        delete_with_tasks: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn task_on_new_vm_creates_machine_and_task() {
    let p = provisioning().await;
    let created = p.create(new_vm(json!({"cpus": 4}))).await.unwrap();

    let vms = p.vms();
    let create = vms["calls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["method"] == "machine_provider.create")
        .unwrap();
    assert_eq!(create["params"]["options"]["cpus"], 4);
    let machines = p.d.machines().list().unwrap();
    assert_eq!(machines.len(), 1);
    let m = &machines[0];
    assert_eq!(
        (m.name.as_str(), m.destination.as_str()),
        ("vm-1", "u@created")
    );
    let provider = m.provider.as_ref().unwrap();
    assert!(provider.owned && provider.delete_with_tasks);
    assert_eq!(created.task.machine_id, Some(m.id));
    assert!(p.store().pending_vms().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn task_on_existing_vm_is_unowned() {
    let p = provisioning().await;
    let created = p
        .create(ProviderVmChoice::Existing { vm_ref: "a".into() })
        .await
        .unwrap();

    let machines = p.d.machines().list().unwrap();
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0].name, "one");
    assert!(!machines[0].provider.as_ref().unwrap().owned);
    assert_eq!(created.task.machine_id, Some(machines[0].id));
    assert!(!p.calls().iter().any(|c| c == "machine_provider.create"));
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_hibernated_vm_is_resumed_first() {
    let p = provisioning().await;
    p.create(ProviderVmChoice::Existing { vm_ref: "b".into() })
        .await
        .unwrap();

    assert!(p.calls().iter().any(|c| c == "machine_provider.resume"));
    // The resumed destination can only be on the machine if resume ran before it was added.
    assert_eq!(p.d.machines().list().unwrap()[0].destination, "u@resumed");
}

#[tokio::test(flavor = "multi_thread")]
async fn new_vm_name_clash_creates_nothing() {
    let p = provisioning().await;
    p.d.machines().add("vm-1", "u@elsewhere").unwrap();

    let err = p.create(new_vm(json!({}))).await.unwrap_err();

    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(!p.calls().iter().any(|c| c == "machine_provider.create"));
    assert!(p.store().pending_vms().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_create_option_is_rejected() {
    let p = provisioning().await;

    let err = p.create(new_vm(json!({"cpus": "four"}))).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    let err = p.create(new_vm(json!({"gpus": 1}))).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    let err = p
        .create(new_vm(json!({"token": "s3cret"})))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);

    assert!(!p.calls().iter().any(|c| c == "machine_provider.create"));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_task_creation_deletes_new_vm() {
    let p = provisioning().await;
    p.break_origin();

    p.create(new_vm(json!({}))).await.unwrap_err();

    assert_eq!(p.calls().last().unwrap(), "machine_provider.delete");
    assert_eq!(p.keygen_runs().trim(), "-R created");
    let vms = p.vms();
    assert!(!vms["vms"]
        .as_object()
        .unwrap()
        .values()
        .any(|vm| vm["name"] == "vm-1"));
    assert!(p.d.machines().list().unwrap().is_empty());
    let store = p.store();
    assert!(store.tasks(None, true).unwrap().is_empty());
    assert!(store.pending_vms().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_task_creation_keeps_existing_vm() {
    let p = provisioning().await;
    p.break_origin();

    p.create(ProviderVmChoice::Existing { vm_ref: "a".into() })
        .await
        .unwrap_err();

    assert!(p.d.machines().list().unwrap().is_empty());
    assert!(!p.calls().iter().any(|c| c == "machine_provider.delete"));
    assert!(p.store().tasks(None, true).unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn pending_vm_is_cleaned_up_at_startup() {
    let home = tempfile::tempdir().unwrap();
    let fake = common::FakeSsh::new();
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    link_vmfake(&paths);
    let mut vms = fixture_vms();
    vms["vms"]["x"] = json!({"name": "vm-x", "state": "running", "destination": "u@x"});
    std::fs::write(home.path().join("vms.json"), vms.to_string()).unwrap();
    asterism_core::store::Store::open(&paths.db())
        .unwrap()
        .add_pending_vm("vmfake", "vm-x")
        .unwrap();

    let running = tokio::spawn(asterism_core::run_with_options(
        paths.clone(),
        provisioning_options(home.path(), &fake),
    ));

    let cleaned = common::eventually_within(Duration::from_secs(20), || {
        asterism_core::store::Store::open(&paths.db())
            .unwrap()
            .pending_vms()
            .unwrap()
            .is_empty()
    })
    .await;
    assert!(cleaned);
    let state: Value =
        serde_json::from_str(&std::fs::read_to_string(home.path().join("vms.json")).unwrap())
            .unwrap();
    let deletes: Vec<&Value> = state["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == "machine_provider.delete")
        .map(|c| &c["params"]["ref"])
        .collect();
    assert_eq!(deletes, [&json!("x")]);

    let client = Client::connect_unix(&paths.socket()).await.unwrap();
    let _: Value = client.call(method::SHUTDOWN, Value::Null).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn new_vm_named_like_a_provider_vm_creates_nothing() {
    let p = provisioning().await;
    let err = p
        .create(ProviderVmChoice::New {
            name: "one".into(),
            options: Default::default(),
            delete_with_tasks: true,
        })
        .await
        .unwrap_err();

    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(!p.calls().iter().any(|c| c == "machine_provider.create"));
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_required_create_option_is_rejected() {
    let p = provisioning_with(|o| o.plugin_env.push(("FIXTURE_REQUIRED".into(), "1".into()))).await;

    let err = p.create(new_vm(json!({}))).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(!p.calls().iter().any(|c| c == "machine_provider.create"));
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_id_excludes_provider_machine() {
    let p = provisioning().await;
    let err =
        p.d.create_task(TaskCreateParams {
            project_id: p.project_id,
            title: "Both".into(),
            machine_id: Some(1),
            provider_machine: Some(ProviderMachineChoice {
                provider: "vmfake".into(),
                vm: ProviderVmChoice::Existing { vm_ref: "a".into() },
            }),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(p.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_connect_deletes_new_vm() {
    let empty_bins = tempfile::tempdir().unwrap();
    let bins = empty_bins.path().to_path_buf();
    let p = provisioning_with(|o| o.machine_bins_dir = Some(bins)).await;

    let err = p.create(new_vm(json!({}))).await.unwrap_err();

    assert_eq!(err.kind, ErrorKind::MachineOffline, "{}", err.message);
    assert_eq!(p.calls().last().unwrap(), "machine_provider.delete");
    assert_eq!(p.keygen_runs().trim(), "-R created");
    assert!(p.d.machines().list().unwrap().is_empty());
    let store = p.store();
    assert!(store.tasks(None, true).unwrap().is_empty());
    assert!(store.pending_vms().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_create_keeps_pending_vm_for_the_sweep() {
    let p = provisioning_with(|o| {
        o.plugin_env
            .push(("FIXTURE_FAIL".into(), "machine_provider.create".into()))
    })
    .await;

    p.create(new_vm(json!({}))).await.unwrap_err();

    let pending = p.store().pending_vms().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        (pending[0].1.as_str(), pending[0].2.as_str()),
        ("vmfake", "vm-1")
    );
    assert!(p.keygen_runs().is_empty());
}
