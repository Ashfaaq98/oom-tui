//! Best-effort host details for the interactive header.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub ram: String,
    pub cpu: String,
    pub os: String,
}

impl DeviceInfo {
    pub fn detect() -> Self {
        Self {
            ram: total_ram().unwrap_or_else(|| "RAM unavailable".to_string()),
            cpu: cpu_model().unwrap_or_else(|| "CPU unavailable".to_string()),
            os: os_version().unwrap_or_else(|| "OS unavailable".to_string()),
        }
    }
}

fn total_ram() -> Option<String> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(format!("{:.1} GiB", kb as f64 / 1024.0 / 1024.0))
}

fn cpu_model() -> Option<String> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let model = cpuinfo.lines().find_map(|line| {
        line.strip_prefix("model name\t: ")
            .or_else(|| line.strip_prefix("Hardware\t: "))
    })?;
    Some(model.trim().to_string())
}

fn os_version() -> Option<String> {
    let release = std::fs::read_to_string("/etc/os-release").ok()?;
    let pretty = release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))?;
    Some(pretty.trim_matches('"').trim().to_string())
}
