use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::{broadcast, watch};

use crate::lock;

pub const SCROLLBACK_LINES: usize = 2_000;
/// Raw output kept for replay on attach, so clients rebuild scrollback and the alternate screen.
pub const TAIL_BYTES: usize = 1 << 20;
const READ_BUFFER: usize = 64 * 1024;
const DRAIN_GRACE: Duration = Duration::from_millis(500);

pub struct SpawnSpec {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    /// The complete environment; nothing is inherited from the daemon.
    pub env: Vec<(String, String)>,
    pub rows: u16,
    pub cols: u16,
}

pub struct Snapshot {
    pub screen: Vec<u8>,
    pub rows: u16,
    pub cols: u16,
}

struct Term {
    parser: vt100::Parser,
    tail: VecDeque<u8>,
    truncated: bool,
}

impl Term {
    fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.tail.extend(bytes);
        let excess = self.tail.len().saturating_sub(TAIL_BYTES);
        if excess > 0 {
            self.tail.drain(..excess);
            self.truncated = true;
        }
    }

    fn replay(&self) -> Vec<u8> {
        let (front, back) = self.tail.as_slices();
        let mut bytes = [front, back].concat();
        // A cut tail may start inside an escape sequence or a UTF-8 character; resume at a line start.
        if self.truncated {
            let start = bytes.iter().position(|&b| b == b'\n').map_or(bytes.len(), |i| i + 1);
            bytes.drain(..start);
        }
        // Repaints the exact current state even if the cut tail lost the alternate-screen switch.
        let screen = self.parser.screen();
        if screen.alternate_screen() {
            bytes.extend_from_slice(b"\x1b[?1049h");
        }
        bytes.extend(screen.state_formatted());
        bytes
    }
}

pub struct Pty {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    term: Arc<Mutex<Term>>,
    output: broadcast::Sender<Vec<u8>>,
    exited: watch::Receiver<bool>,
    pid: Option<u32>,
}

fn io_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

fn check_size(rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "rows and cols must be at least 1"));
    }
    Ok(())
}

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

impl Pty {
    pub fn spawn(spec: SpawnSpec) -> io::Result<Arc<Self>> {
        check_size(spec.rows, spec.cols)?;
        let (program, args) = spec
            .argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty argv"))?;
        let pair = native_pty_system().openpty(pty_size(spec.rows, spec.cols)).map_err(io_err)?;

        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        cmd.env("TERM", "xterm-256color");
        for (key, value) in &spec.env {
            cmd.env(key, value);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(io_err)?;
        drop(pair.slave);

        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(io_err)?;
        let writer = pair.master.take_writer().map_err(io_err)?;
        let term = Arc::new(Mutex::new(Term {
            parser: vt100::Parser::new(spec.rows, spec.cols, SCROLLBACK_LINES),
            tail: VecDeque::new(),
            truncated: false,
        }));
        let (output, _) = broadcast::channel(256);
        let (exited_tx, exited) = watch::channel(false);

        let reader_term = term.clone();
        let reader_output = output.clone();
        let (drained_tx, drained) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let _drained = drained_tx;
            let mut buf = vec![0u8; READ_BUFFER];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // Parse and publish under one lock so `attach` never misses or duplicates bytes.
                        lock(&reader_term).process(&buf[..n]);
                        let _ = reader_output.send(buf[..n].to_vec());
                    }
                }
            }
        });
        // Waiting separately from reading reports the exit even if a grandchild keeps the PTY open.
        std::thread::spawn(move || {
            let _ = child.wait();
            // Let the reader parse the final output first (bounded: a grandchild may keep the PTY open).
            let _ = drained.recv_timeout(DRAIN_GRACE);
            let _ = exited_tx.send(true);
        });

        Ok(Arc::new(Self {
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            term,
            output,
            exited,
            pid,
        }))
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        let mut writer = lock(&self.writer);
        writer.write_all(data)?;
        writer.flush()
    }

    pub fn resize(&self, rows: u16, cols: u16) -> io::Result<()> {
        check_size(rows, cols)?;
        lock(&self.master).resize(pty_size(rows, cols)).map_err(io_err)?;
        lock(&self.term).parser.set_size(rows, cols);
        Ok(())
    }

    pub fn text(&self) -> String {
        lock(&self.term).parser.screen().contents()
    }

    pub fn history(&self) -> String {
        let parser = &mut lock(&self.term).parser;
        let (rows, cols) = parser.screen().size();
        parser.set_scrollback(usize::MAX);
        let depth = parser.screen().scrollback();
        // ponytail: vt100 scrolls back one screen at most, so grow the screen per read (depth x cols cells); keep a line log if reads get hot.
        parser.set_size(rows.saturating_add(u16::try_from(depth).unwrap_or(u16::MAX)), cols);
        parser.set_scrollback(depth);
        let text = parser.screen().rows(0, cols).take(depth + usize::from(rows)).collect::<Vec<_>>().join("\n");
        parser.set_scrollback(0);
        parser.set_size(rows, cols);
        text
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output.subscribe()
    }

    pub fn attach(&self) -> (Snapshot, broadcast::Receiver<Vec<u8>>) {
        let term = lock(&self.term);
        let (rows, cols) = term.parser.screen().size();
        (Snapshot { screen: term.replay(), rows, cols }, self.output.subscribe())
    }

    pub fn exited(&self) -> watch::Receiver<bool> {
        self.exited.clone()
    }

    pub fn kill(&self) -> io::Result<()> {
        lock(&self.killer).kill()
    }

    /// SIGKILL for processes that ignore the hangup `kill` sends.
    pub fn force_kill(&self) -> io::Result<()> {
        let Some(pid) = self.pid else { return Ok(()) };
        let kill = |target: String| {
            std::process::Command::new("kill").args(["-KILL", &target]).status().is_ok_and(|s| s.success())
        };
        // The PTY child is a session leader, so its pgid is its pid; fall back to the pid alone.
        if !kill(format!("-{pid}")) {
            kill(pid.to_string());
        }
        Ok(())
    }
}
