use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sample {
    pub memory_bytes: u64,
    pub cpu_ns: u64,
}

/// `pid` followed by all of its descendants.
pub fn tree(pid: u32) -> Vec<u32> {
    let mut pids = vec![pid];
    let mut i = 0;
    while i < pids.len() {
        pids.extend(children(pids[i]));
        i += 1;
    }
    pids
}

/// Turns cumulative CPU times into a percentage of one core since the previous call.
#[derive(Default)]
pub struct CpuTracker {
    last: Option<(Instant, HashMap<u32, u64>)>,
    current: HashMap<u32, u64>,
}

impl CpuTracker {
    /// Samples `pids` and returns their summed memory and CPU percent.
    pub fn measure(&mut self, pids: &[u32]) -> (u64, f64) {
        let mut memory = 0;
        let mut cpu_delta = 0;
        for &pid in pids {
            let Some(sample) = sample(pid) else { continue };
            memory += sample.memory_bytes;
            if let Some(prev) = self.last.as_ref().and_then(|(_, cpu)| cpu.get(&pid)) {
                cpu_delta += sample.cpu_ns.saturating_sub(*prev);
            }
            self.current.insert(pid, sample.cpu_ns);
        }
        let elapsed = self
            .last
            .as_ref()
            .map_or(0.0, |(at, _)| at.elapsed().as_nanos() as f64);
        let percent = if elapsed > 0.0 {
            cpu_delta as f64 * 100.0 / elapsed
        } else {
            0.0
        };
        (memory, percent)
    }

    /// Ends a round of `measure` calls; the next round compares against this one.
    pub fn finish(&mut self) {
        self.last = Some((Instant::now(), std::mem::take(&mut self.current)));
    }
}

#[cfg(target_os = "macos")]
fn children(pid: u32) -> Vec<u32> {
    let mut buf = vec![0 as libc::pid_t; 1024];
    let size = (buf.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    // SAFETY: the buffer is valid for `size` bytes and the call writes at most that many.
    let count =
        unsafe { libc::proc_listchildpids(pid as libc::pid_t, buf.as_mut_ptr().cast(), size) };
    buf.truncate(count.max(0) as usize);
    buf.into_iter()
        .filter(|&p| p > 0)
        .map(|p| p as u32)
        .collect()
}

// Declared here because libc deprecates its copy in favour of the mach2 crate.
#[cfg(target_os = "macos")]
#[repr(C)]
struct MachTimebase {
    numer: u32,
    denom: u32,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn mach_timebase_info(info: *mut MachTimebase) -> libc::c_int;
}

#[cfg(target_os = "macos")]
pub fn sample(pid: u32) -> Option<Sample> {
    // SAFETY: both structs are plain data filled in by the kernel.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V2,
            (&mut info as *mut libc::rusage_info_v2).cast(),
        )
    };
    if ok != 0 {
        return None;
    }
    let mut timebase = MachTimebase { numer: 0, denom: 0 };
    // SAFETY: the kernel fills in the two fields.
    unsafe { mach_timebase_info(&mut timebase) };
    let ticks = info.ri_user_time + info.ri_system_time;
    let cpu_ns = if timebase.denom == 0 {
        ticks
    } else {
        ticks * timebase.numer as u64 / timebase.denom as u64
    };
    Some(Sample {
        memory_bytes: info.ri_phys_footprint,
        cpu_ns,
    })
}

#[cfg(target_os = "linux")]
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name may contain spaces, so split after its closing parenthesis.
    let rest = &stat[stat.rfind(')')? + 2..];
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

#[cfg(target_os = "linux")]
fn children(pid: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|&p| stat_fields(p).and_then(|f| f.get(1)?.parse::<u32>().ok()) == Some(pid))
        .collect()
}

#[cfg(target_os = "linux")]
pub fn sample(pid: u32) -> Option<Sample> {
    let fields = stat_fields(pid)?;
    let ticks: u64 = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let resident: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // SAFETY: sysconf has no preconditions.
    let (hz, page) = unsafe {
        (
            libc::sysconf(libc::_SC_CLK_TCK),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    Some(Sample {
        memory_bytes: resident * page.max(1) as u64,
        cpu_ns: ticks * 1_000_000_000 / hz.max(1) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn tree_includes_grandchildren_and_sample_reads_memory() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while tree(pid).len() < 2 && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let pids = tree(pid);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(pids[0], pid);
        assert!(pids.len() >= 2, "sleep should be a child of sh: {pids:?}");
        assert!(sample(std::process::id()).unwrap().memory_bytes > 0);
    }

    #[test]
    fn cpu_tracker_reports_busy_time_after_the_first_round() {
        let me = [std::process::id()];
        let mut tracker = CpuTracker::default();
        assert_eq!(tracker.measure(&me).1, 0.0);
        tracker.finish();
        let start = Instant::now();
        let mut x = 0u64;
        while start.elapsed().as_millis() < 200 {
            x = x.wrapping_add(std::hint::black_box(1));
        }
        assert!(x > 0);
        let (_, percent) = tracker.measure(&me);
        assert!(
            percent > 20.0,
            "busy loop should show CPU use, got {percent}"
        );
    }
}
