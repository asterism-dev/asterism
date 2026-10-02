use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tokio::sync::{broadcast, watch};

use crate::lock;

pub const SCROLLBACK_LINES: usize = 2_000;
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

pub struct Pty {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    parser: Arc<Mutex<vt100::Parser>>,
    output: broadcast::Sender<Vec<u8>>,
    exited: watch::Receiver<bool>,
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

        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(io_err)?;
        let writer = pair.master.take_writer().map_err(io_err)?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(spec.rows, spec.cols, SCROLLBACK_LINES)));
        let (output, _) = broadcast::channel(256);
        let (exited_tx, exited) = watch::channel(false);

        let reader_parser = parser.clone();
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
                        let mut parser = lock(&reader_parser);
                        parser.process(&buf[..n]);
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
            parser,
            output,
            exited,
        }))
    }

    pub fn write(&self, data: &[u8]) -> io::Result<()> {
        let mut writer = lock(&self.writer);
        writer.write_all(data)?;
        writer.flush()
    }

    pub fn resize(&self, rows: u16, cols: u16) -> io::Result<()> {
        check_size(rows, cols)?;
        lock(&self.master).resize(pty_size(rows, cols)).map_err(io_err)?;
        lock(&self.parser).set_size(rows, cols);
        Ok(())
    }

    pub fn text(&self) -> String {
        lock(&self.parser).screen().contents()
    }

    pub fn history(&self) -> String {
        let mut parser = lock(&self.parser);
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

    // ponytail: snapshot is the visible screen only; serialize scrollback rows when the app needs them.
    pub fn attach(&self) -> (Snapshot, broadcast::Receiver<Vec<u8>>) {
        let parser = lock(&self.parser);
        let (rows, cols) = parser.screen().size();
        let mut screen = b"\x1b[H\x1b[2J".to_vec();
        screen.extend(parser.screen().state_formatted());
        (Snapshot { screen, rows, cols }, self.output.subscribe())
    }

    pub fn exited(&self) -> watch::Receiver<bool> {
        self.exited.clone()
    }

    pub fn kill(&self) -> io::Result<()> {
        lock(&self.killer).kill()
    }
}
