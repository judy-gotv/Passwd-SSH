//! Server monitoring via read-only SSH commands (docs §6.8).
//!
//! | Metric | Command |
//! |---|---|
//! | Load | `cat /proc/loadavg` |
//! | Memory | `free -b` |
//! | Disk | `df -B1` |
//! | CPU | two `/proc/stat` samples 1s apart |
//! | Network | `cat /proc/net/dev` |
//! | Processes | `ps -eo pid,comm,%cpu,%mem --sort=-%cpu \| head` |
//!
//! Only read-only commands are ever executed by the collector.
//!
//! Author: Passwd Word

use std::sync::Arc;
use std::time::Duration;

use opsdeck_ssh::{SessionId, SessionManager};
use opsdeck_storage::models::{MetricSnapshot, now_unix};
use serde::{Deserialize, Serialize};

use crate::error::OpsError;

pub use opsdeck_storage::models::MetricSnapshot as Snapshot;

/// One row of the process table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
}

/// Read-only commands used by the collector.
pub mod commands {
    pub const LOADAVG: &str = "cat /proc/loadavg";
    pub const MEMORY: &str = "free -b";
    pub const DISK: &str = "df -B1 -x tmpfs -x devtmpfs -x overlay /";
    /// Two samples with a 1s gap so CPU % can be derived from deltas.
    pub const CPU_STAT: &str = "cat /proc/stat; sleep 1; cat /proc/stat";
    pub const NET_DEV: &str = "cat /proc/net/dev";
    pub const TOP_PROCS: &str = "ps -eo pid,args,%cpu,%mem --sort=-%cpu | head -n 11";
    pub const SYSINFO: &str = "grep PRETTY_NAME= /etc/os-release 2>/dev/null | head -1; uname -srm; cat /proc/uptime; nproc 2>/dev/null | sed 's/^/NPROC=/'; date +'TZ=%Z %z'";
    pub const MOUNTS: &str = "df -B1 -T -x tmpfs -x devtmpfs -x overlay -x squashfs";
    pub const DISKSTATS: &str = "cat /proc/diskstats";
}

/// Static host facts for the monitoring dashboard (fetched once per session).
#[derive(Debug, Clone, Default)]
pub struct SysInfo {
    /// e.g. "Debian GNU/Linux 12 (bookworm)".
    pub os_name: String,
    /// e.g. "Linux 6.1.0-13-amd64 x86_64".
    pub kernel: String,
    pub uptime_secs: u64,
    /// Logical CPU cores (`nproc`), when reported.
    pub cpu_cores: Option<u32>,
    /// Preformatted timezone, e.g. "GMT+0800 CST".
    pub timezone: Option<String>,
    /// Filesystem mounts (df -T), root first when present.
    pub mounts: Vec<MountInfo>,
}

/// One filesystem mount from `df -B1 -T`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub mount: String,
    pub fstype: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
}

/// Parse the SYSINFO output: an optional PRETTY_NAME line, the
/// `uname -srm` line and the /proc/uptime line (order may vary).
pub fn parse_sysinfo(out: &str) -> Result<SysInfo, OpsError> {
    let mut os_name = String::new();
    let mut kernel = String::new();
    let mut uptime_secs = 0u64;
    let mut cpu_cores = None;
    let mut timezone = None;
    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("TZ=") {
            // "CST +0800" → "GMT+0800 CST"
            let mut parts = v.split_whitespace();
            if let (Some(abbr), Some(off)) = (parts.next(), parts.next()) {
                timezone = Some(format!("GMT{off} {abbr}"));
            }
        } else if let Some(v) = line.strip_prefix("NPROC=") {
            cpu_cores = v.trim().parse::<u32>().ok().filter(|n| *n > 0);
        } else if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
            os_name = v.trim_matches('"').to_string();
        } else if let Some((first, rest)) = line.split_once(char::is_whitespace) {
            // /proc/uptime: two floats. uname: words only.
            let is_uptime = first.parse::<f64>().is_ok()
                && rest
                    .split_whitespace()
                    .next()
                    .is_some_and(|v| v.parse::<f64>().is_ok());
            if is_uptime {
                uptime_secs = first.parse::<f64>().map(|v| v as u64).unwrap_or(0);
            } else if kernel.is_empty() {
                kernel = line.to_string();
            }
        } else if kernel.is_empty() && !line.is_empty() {
            kernel = line.to_string();
        }
    }
    if os_name.is_empty() {
        os_name = "Linux".to_string();
    }
    Ok(SysInfo {
        os_name,
        kernel,
        uptime_secs,
        cpu_cores,
        timezone,
        mounts: Vec::new(),
    })
}

/// Parse `cat /proc/loadavg` → (1m, 5m, 15m).
pub fn parse_loadavg(out: &str) -> Result<(f64, f64, f64), OpsError> {
    let parts: Vec<&str> = out.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(OpsError::Parse(format!("bad loadavg: {out:?}")));
    }
    let p = |s: &str| {
        s.parse::<f64>()
            .map_err(|_| OpsError::Parse(format!("bad loadavg number: {s}")))
    };
    Ok((p(parts[0])?, p(parts[1])?, p(parts[2])?))
}

/// Parse `free -b` → (used_bytes, total_bytes, cached_bytes).
pub fn parse_free_b(out: &str) -> Result<(u64, u64, u64), OpsError> {
    for line in out.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("Mem:") {
            let f: Vec<&str> = rest.split_whitespace().collect();
            if f.len() < 2 {
                return Err(OpsError::Parse(format!("bad free line: {line:?}")));
            }
            let total = f[0]
                .parse::<u64>()
                .map_err(|_| OpsError::Parse("bad mem total".into()))?;
            let used = f[1]
                .parse::<u64>()
                .map_err(|_| OpsError::Parse("bad mem used".into()))?;
            let cached = f.get(4).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            return Ok((used, total, cached));
        }
    }
    Err(OpsError::Parse("no Mem: line in free output".into()))
}

/// Parse `df -B1 -T` (all mounts) → mount table, pseudo filesystems and
/// duplicate mount points dropped.
pub fn parse_mounts(out: &str) -> Vec<MountInfo> {
    const PSEUDO: [&str; 10] = [
        "proc",
        "sysfs",
        "devpts",
        "cgroup",
        "cgroup2",
        "mqueue",
        "autofs",
        "pstore",
        "securityfs",
        "debugfs",
    ];
    let mut mounts: Vec<MountInfo> = Vec::new();
    for line in out.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 7 || PSEUDO.contains(&f[1]) {
            continue;
        }
        let (Ok(total), Ok(used), Ok(avail)) = (
            f[2].parse::<u64>(),
            f[3].parse::<u64>(),
            f[4].parse::<u64>(),
        ) else {
            continue;
        };
        let mount = f[6..].join(" ");
        if mounts.iter().any(|m| m.mount == mount) {
            continue;
        }
        mounts.push(MountInfo {
            mount,
            fstype: f[1].to_string(),
            total,
            used,
            avail,
        });
    }
    mounts.sort_by_key(|m| m.mount != "/");
    mounts
}

/// Parse `/proc/diskstats` → (read_bytes, write_bytes) summed over whole
/// disks (partitions, loop and ram devices excluded).
pub fn parse_diskstats(out: &str) -> (u64, u64) {
    fn whole_disk(name: &str) -> bool {
        if let Some(rest) = name
            .strip_prefix("sd")
            .or_else(|| name.strip_prefix("hd"))
            .or_else(|| name.strip_prefix("vd"))
            .or_else(|| name.strip_prefix("xvd"))
        {
            return rest.len() == 1 && rest.chars().all(|ch| ch.is_ascii_alphabetic());
        }
        if let Some(rest) = name.strip_prefix("nvme") {
            // nvme0n1 (whole) vs nvme0n1p1 (partition)
            return !rest.contains('p') && rest.contains('n');
        }
        if let Some(rest) = name.strip_prefix("mmcblk") {
            return rest.chars().all(|ch| ch.is_ascii_digit());
        }
        false
    }
    let (mut rd, mut wr) = (0u64, 0u64);
    for line in out.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 || !whole_disk(f[2]) {
            continue;
        }
        rd += f[5].parse::<u64>().unwrap_or(0) * 512;
        wr += f[9].parse::<u64>().unwrap_or(0) * 512;
    }
    (rd, wr)
}

/// Per-core busy percentages from the two `/proc/stat` samples.
pub fn parse_cpu_per_core(out: &str) -> Vec<f64> {
    fn core_line(line: &str) -> Option<(u64, u64)> {
        let f: Vec<&str> = line.split_whitespace().collect();
        let name = f.first()?;
        if !name.starts_with("cpu") || name.len() <= 3 || f.len() < 8 {
            return None;
        }
        let nums: Vec<u64> = f[1..8].iter().filter_map(|s| s.parse().ok()).collect();
        if nums.len() < 7 {
            return None;
        }
        let idle = nums[3] + nums[4];
        Some((nums.iter().sum(), idle))
    }
    let samples: Vec<(u64, u64)> = out.lines().filter_map(core_line).collect();
    let half = samples.len() / 2;
    if half == 0 {
        return Vec::new();
    }
    (0..half)
        .map(|i| {
            let (t1, i1) = samples[i];
            let (t2, i2) = samples[i + half];
            let dt = t2.saturating_sub(t1);
            if dt == 0 {
                0.0
            } else {
                (1.0 - i2.saturating_sub(i1) as f64 / dt as f64) * 100.0
            }
        })
        .collect()
}

/// Parse `df -B1` → (used_bytes, total_bytes) for the first data row.
pub fn parse_df_b1(out: &str) -> Result<(u64, u64), OpsError> {
    for line in out.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 6 {
            continue;
        }
        let total = f[1]
            .parse::<u64>()
            .map_err(|_| OpsError::Parse("bad df total".into()))?;
        let used = f[2]
            .parse::<u64>()
            .map_err(|_| OpsError::Parse("bad df used".into()))?;
        return Ok((used, total));
    }
    Err(OpsError::Parse("no data rows in df output".into()))
}

/// Parse the two `/proc/stat` samples → CPU busy percent.
pub fn parse_cpu_stat(out: &str) -> Result<f64, OpsError> {
    fn sample(line: &str) -> Option<(u64, u64)> {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.first() != Some(&"cpu") || f.len() < 8 {
            return None;
        }
        let nums: Vec<u64> = f[1..8].iter().filter_map(|s| s.parse().ok()).collect();
        if nums.len() < 7 {
            return None;
        }
        // user nice system idle iowait irq softirq
        let idle = nums[3] + nums[4];
        let total: u64 = nums.iter().sum();
        Some((total, idle))
    }
    let samples: Vec<(u64, u64)> = out.lines().filter_map(sample).collect();
    if samples.len() < 2 {
        return Err(OpsError::Parse("need two cpu samples".into()));
    }
    let (t1, i1) = samples[0];
    let (t2, i2) = samples[1];
    let dt = t2.saturating_sub(t1);
    let di = i2.saturating_sub(i1);
    if dt == 0 {
        return Ok(0.0);
    }
    Ok((1.0 - di as f64 / dt as f64) * 100.0)
}

/// Parse `/proc/net/dev` → (rx_bytes, tx_bytes) summed over non-loopback ifaces.
pub fn parse_net_dev(out: &str) -> (u64, u64) {
    let mut rx = 0u64;
    let mut tx = 0u64;
    for line in out.lines() {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() == "lo" {
            continue;
        }
        let f: Vec<&str> = rest.split_whitespace().collect();
        if f.len() < 9 {
            continue;
        }
        rx += f[0].parse::<u64>().unwrap_or(0);
        tx += f[8].parse::<u64>().unwrap_or(0);
    }
    (rx, tx)
}

/// Parse `ps -eo pid,args,%cpu,%mem` → top processes (header skipped).
/// The command line is everything between the pid and the two % columns.
pub fn parse_ps(out: &str) -> Vec<ProcessInfo> {
    let mut procs = Vec::new();
    for line in out.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 {
            continue;
        }
        let n = f.len();
        let (Ok(pid), Ok(cpu), Ok(mem)) = (
            f[0].parse::<u32>(),
            f[n - 2].parse::<f32>(),
            f[n - 1].parse::<f32>(),
        ) else {
            continue;
        };
        procs.push(ProcessInfo {
            pid,
            name: f[1..n - 2].join(" "),
            cpu_percent: cpu,
            mem_percent: mem,
        });
    }
    procs
}

/// Collects metric snapshots over an SSH session.
#[derive(Clone)]
pub struct MetricsCollector {
    ssh: Arc<SessionManager>,
}

impl MetricsCollector {
    pub fn new(ssh: Arc<SessionManager>) -> Self {
        Self { ssh }
    }

    /// Collect one snapshot. Individual command failures degrade gracefully
    /// (the corresponding fields stay `None`) instead of failing the whole
    /// snapshot.
    pub async fn collect(
        &self,
        session: SessionId,
        host_id: &str,
    ) -> Result<MetricSnapshot, OpsError> {
        let timeout = Duration::from_secs(15);
        let (load, mem, disk, cpu, net, diskio) = tokio::join!(
            self.ssh.exec_timeout(session, commands::LOADAVG, timeout),
            self.ssh.exec_timeout(session, commands::MEMORY, timeout),
            self.ssh.exec_timeout(session, commands::DISK, timeout),
            self.ssh.exec_timeout(session, commands::CPU_STAT, timeout),
            self.ssh.exec_timeout(session, commands::NET_DEV, timeout),
            self.ssh.exec_timeout(session, commands::DISKSTATS, timeout),
        );

        let (load_1, load_5, load_15) = load
            .ok()
            .and_then(|o| parse_loadavg(&o.stdout).ok())
            .map(|(a, b, c)| (Some(a), Some(b), Some(c)))
            .unwrap_or((None, None, None));
        let (memory_used_bytes, memory_total_bytes, memory_cached_bytes) = mem
            .ok()
            .and_then(|o| parse_free_b(&o.stdout).ok())
            .map(|(u, t, c)| (Some(u as i64), Some(t as i64), Some(c as i64)))
            .unwrap_or((None, None, None));
        let (disk_used_bytes, disk_total_bytes) = disk
            .ok()
            .and_then(|o| parse_df_b1(&o.stdout).ok())
            .map(|(u, t)| (Some(u as i64), Some(t as i64)))
            .unwrap_or((None, None));
        let cpu_out = cpu.ok().map(|o| o.stdout);
        let cpu_percent = cpu_out.as_deref().and_then(|s| parse_cpu_stat(s).ok());
        let cpu_per_core = cpu_out
            .as_deref()
            .map(parse_cpu_per_core)
            .unwrap_or_default();
        let (disk_read_bytes, disk_write_bytes) = diskio
            .ok()
            .map(|o| parse_diskstats(&o.stdout))
            .map(|(r, w)| (Some(r as i64), Some(w as i64)))
            .unwrap_or((None, None));
        let (network_rx_bytes, network_tx_bytes) = net
            .ok()
            .map(|o| parse_net_dev(&o.stdout))
            .map(|(r, t)| (Some(r as i64), Some(t as i64)))
            .unwrap_or((None, None));

        Ok(MetricSnapshot {
            id: None,
            host_id: host_id.to_string(),
            collected_at: now_unix(),
            cpu_percent,
            memory_used_bytes,
            memory_total_bytes,
            disk_used_bytes,
            disk_total_bytes,
            load_1,
            load_5,
            load_15,
            network_rx_bytes,
            network_tx_bytes,
            memory_cached_bytes,
            disk_read_bytes,
            disk_write_bytes,
            cpu_per_core,
        })
    }

    /// Fetch static host facts (OS / kernel / uptime) once per session.
    pub async fn sysinfo(&self, session: SessionId) -> Result<SysInfo, OpsError> {
        let out = self
            .ssh
            .exec_timeout(session, commands::SYSINFO, Duration::from_secs(15))
            .await?;
        let mut info = parse_sysinfo(&out.stdout)?;
        if let Ok(m) = self
            .ssh
            .exec_timeout(session, commands::MOUNTS, Duration::from_secs(15))
            .await
        {
            info.mounts = parse_mounts(&m.stdout);
        }
        Ok(info)
    }

    /// Fetch the top-process table.
    pub async fn top_processes(&self, session: SessionId) -> Result<Vec<ProcessInfo>, OpsError> {
        let out = self
            .ssh
            .exec_timeout(session, commands::TOP_PROCS, Duration::from_secs(15))
            .await?;
        Ok(parse_ps(&out.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysinfo_parses() {
        let out = "PRETTY_NAME=\"Debian GNU/Linux 12 (bookworm)\"\nLinux 6.1.0-13-amd64 x86_64\n3370000.25 6600000.50\n";
        let info = parse_sysinfo(out).unwrap();
        assert_eq!(info.os_name, "Debian GNU/Linux 12 (bookworm)");
        assert_eq!(info.kernel, "Linux 6.1.0-13-amd64 x86_64");
        assert_eq!(info.uptime_secs, 3_370_000);

        let bare = parse_sysinfo("Linux 5.15.0 armv7l\n123.4 900.1\n").unwrap();
        assert_eq!(bare.os_name, "Linux");
        assert_eq!(bare.uptime_secs, 123);
    }

    #[test]
    fn loadavg_parses() {
        let (a, b, c) = parse_loadavg("0.12 0.34 0.56 1/234 5678\n").unwrap();
        assert!((a - 0.12).abs() < 1e-9);
        assert!((b - 0.34).abs() < 1e-9);
        assert!((c - 0.56).abs() < 1e-9);
    }

    #[test]
    fn free_parses() {
        let out = "               total        used        free      shared  buff/cache   available\nMem:       4024548      512000     1234567       12345     2277981     3512345\nSwap:        2097148           0     2097148\n";
        assert_eq!(parse_free_b(out).unwrap(), (512000, 4024548, 2277981));
    }

    #[test]
    fn df_parses() {
        let out = "Filesystem     1B-blocks       Used  Available Use% Mounted on\n/dev/sda1    53687091200 12345678900 41341412300  23% /\n";
        assert_eq!(parse_df_b1(out).unwrap(), (12345678900, 53687091200));
    }

    #[test]
    fn cpu_stat_parses() {
        let out = "cpu  100 0 50 850 0 0 0 0 0 0\ncpu  200 0 100 1600 0 0 0 0 0 0\n";
        // dt = 150+750=... total1=1000, idle1=850; total2=1900, idle2=1600
        // busy = 1 - 750/900 = 16.67%
        let pct = parse_cpu_stat(out).unwrap();
        assert!((pct - 16.6667).abs() < 0.01, "got {pct}");
    }

    #[test]
    fn net_dev_parses() {
        let out = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n  eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n    lo: 500 5 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n";
        assert_eq!(parse_net_dev(out), (1000, 2000));
    }

    #[test]
    fn ps_parses() {
        let out = "    PID COMMAND         %CPU %MEM\n      1 systemd          0.1  0.5\n    123 nginx            2.5  1.2\n";
        let procs = parse_ps(out);
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[1].pid, 123);
        assert!((procs[1].cpu_percent - 2.5).abs() < 1e-6);
    }
}
