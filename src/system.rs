//! Best-effort host details for the interactive header.

use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub ram: String,
    pub cpu: String,
    pub gpu: String,
    pub os: String,
}

impl DeviceInfo {
    pub fn detect() -> Self {
        Self {
            ram: total_ram().unwrap_or_else(|| "RAM unavailable".to_string()),
            cpu: cpu_model().unwrap_or_else(|| "CPU unavailable".to_string()),
            gpu: gpu_model().unwrap_or_else(|| "GPU unavailable".to_string()),
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
    Some(format!("{:.1} GiB RAM", kb as f64 / 1024.0 / 1024.0))
}

fn cpu_model() -> Option<String> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name\t: ").or_else(|| line.strip_prefix("Hardware\t: ")))?;
    Some(format!("CPU {}", compact(model, 42)))
}

fn gpu_model() -> Option<String> {
    let output = Command::new("lspci").arg("-mm").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| {
        line.contains("\"VGA compatible controller\"")
            || line.contains("\"3D controller\"")
            || line.contains("\"Display controller\"")
    })?;
    let fields: Vec<&str> = line.split('"').collect();
    let vendor = fields.get(5).copied().unwrap_or_default();
    let device = fields.get(7).copied().unwrap_or_default();
    let name = match (vendor, device) {
        ("", "") => return None,
        (_, "") => vendor.to_string(),
        ("", _) => device.to_string(),
        _ => format!("{vendor} {device}"),
    };
    Some(format!("GPU {}", compact(&name, 32)))
}

fn os_version() -> Option<String> {
    let release = std::fs::read_to_string("/etc/os-release").ok()?;
    let pretty = release.lines().find_map(|line| line.strip_prefix("PRETTY_NAME="))?;
    Some(format!("OS {}", compact(pretty.trim_matches('"'), 34)))
}

fn compact(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::compact;

    #[test]
    fn compact_marks_truncated_device_names() {
        assert_eq!(compact("abcdef", 4), "abc…");
        assert_eq!(compact("abc", 4), "abc");
    }
}
