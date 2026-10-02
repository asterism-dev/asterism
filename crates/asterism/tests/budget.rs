mod common;

use std::process::Command;
use std::time::{Duration, Instant};

use asterism_proto::client::Client;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use common::Node;

const SESSIONS: usize = 10;
const WINDOW: Duration = Duration::from_secs(30);

fn idle_node() -> (Node, u32, Vec<String>) {
    let node = Node::new();
    let task = node.json(&["task", "new", "budget"])["task"]["id"].to_string();
    let sessions = (0..SESSIONS)
        .map(|_| node.json(&["session", "start", &task, "--shell"])["id"].to_string())
        .collect();
    let pid = node.json(&["daemon", "status"])["pid"].as_u64().unwrap() as u32;
    (node, pid, sessions)
}

#[cfg(target_os = "macos")]
fn sample(pid: u32) -> (Duration, u64) {
    let out = Command::new("ps").args(["-o", "time=,rss=", "-p", &pid.to_string()]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut fields = text.split_whitespace();
    let cpu = fields.next().unwrap().split(':').fold(0.0, |acc, part| acc * 60.0 + part.parse::<f64>().unwrap());
    (Duration::from_secs_f64(cpu), fields.next().unwrap().parse().unwrap())
}

#[cfg(target_os = "linux")]
fn sample(pid: u32) -> (Duration, u64) {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields: Vec<&str> = stat.rsplit_once(')').unwrap().1.split_whitespace().collect();
    // utime and stime are fields 14 and 15 of /proc/<pid>/stat, in clock ticks (100 Hz on Linux).
    let ticks: u64 = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let rss = status.lines().find(|l| l.starts_with("VmRSS:")).unwrap().split_whitespace().nth(1).unwrap();
    (Duration::from_millis(ticks * 10), rss.parse().unwrap())
}

#[test]
#[ignore = "performance budget; run with: cargo test --release -p asterism --test budget -- --ignored --nocapture"]
fn idle_daemon_with_ten_sessions_stays_within_budget() {
    let (_node, pid, _sessions) = idle_node();
    std::thread::sleep(Duration::from_secs(3));
    let (cpu_start, _) = sample(pid);
    std::thread::sleep(WINDOW);
    let (cpu_end, rss_kb) = sample(pid);

    let cpu_percent = (cpu_end - cpu_start).as_secs_f64() / WINDOW.as_secs_f64() * 100.0;
    println!("idle cpu {cpu_percent:.3}% rss {} KB", rss_kb);
    assert!(cpu_percent < 0.1, "idle CPU {cpu_percent:.3}% exceeds 0.1%");
    assert!(rss_kb < 20 * 1024, "RSS {rss_kb} KB exceeds 20 MB");
}

#[tokio::test]
#[ignore = "performance budget; run with: cargo test --release -p asterism --test budget -- --ignored --nocapture"]
async fn attach_returns_a_snapshot_quickly() {
    let (node, _pid, sessions) = idle_node();
    let client = Client::connect_unix(&node.home().join("asterismd.sock")).await.unwrap();
    let _: HelloResult = client
        .call(method::HELLO, HelloParams { proto_version: PROTO_VERSION, client_kind: ClientKind::App })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut slowest = Duration::ZERO;
    for session in &sessions {
        let session_id = session.parse().unwrap();
        let started = Instant::now();
        let _: SessionAttachResult = client.call(method::SESSION_ATTACH, SessionIdParams { session_id }).await.unwrap();
        slowest = slowest.max(started.elapsed());
        let _: () = client.call(method::SESSION_DETACH, SessionIdParams { session_id }).await.unwrap();
    }
    println!("slowest attach {slowest:?}");
    assert!(slowest < Duration::from_millis(50), "attach took {slowest:?}");
}
