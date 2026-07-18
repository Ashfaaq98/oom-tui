//! Decode container and service identity from a cgroup path.
//!
//! A cgroup path already encodes who the workload is, so this needs no access
//! to a container runtime, a kubelet, or the network - which matters, because
//! the log being read is often from a machine that no longer exists.

/// What kind of thing the cgroup belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Kubernetes,
    Docker,
    Containerd,
    CriO,
    Podman,
    /// A plain systemd unit, e.g. `nginx.service`.
    SystemdUnit,
    /// A logged-in user's session scope.
    UserSession,
}

impl Runtime {
    pub fn label(self) -> &'static str {
        match self {
            Runtime::Kubernetes => "kubernetes",
            Runtime::Docker => "docker",
            Runtime::Containerd => "containerd",
            Runtime::CriO => "cri-o",
            Runtime::Podman => "podman",
            Runtime::SystemdUnit => "systemd",
            Runtime::UserSession => "user session",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub runtime: Runtime,
    pub container_id: Option<String>,
    pub pod_uid: Option<String>,
    /// Kubernetes QoS class, which is visible in the slice name and explains
    /// why this pod was an eviction candidate at all.
    pub qos_class: Option<String>,
    pub unit: Option<String>,
}

impl Identity {
    /// A single line fit for a table cell.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.runtime.label().to_string()];
        if let Some(unit) = &self.unit {
            parts.push(unit.clone());
        }
        if let Some(qos) = &self.qos_class {
            parts.push(qos.clone());
        }
        if let Some(pod) = &self.pod_uid {
            parts.push(format!("pod {}", short(pod)));
        }
        if let Some(id) = &self.container_id {
            parts.push(format!("container {}", short(id)));
        }
        parts.join(" · ")
    }
}

/// Container and pod IDs are 64 hex characters; nobody reads past the first 12.
fn short(id: &str) -> String {
    id.chars().take(12).collect()
}

/// Best-effort identification. Returns `None` for a cgroup that carries no
/// recognisable identity (e.g. `/`), rather than inventing one.
pub fn identify(cgroup: &str) -> Option<Identity> {
    let path = cgroup.trim();
    if path.is_empty() || path == "/" {
        return None;
    }

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return None;
    }

    // Kubernetes: both the systemd driver (kubepods-burstable.slice) and the
    // cgroupfs driver (kubepods/burstable) put "kubepods" in the path.
    if segments.iter().any(|s| s.starts_with("kubepods")) {
        return Some(Identity {
            runtime: Runtime::Kubernetes,
            container_id: segments.iter().rev().find_map(|s| container_id(s)),
            pod_uid: segments.iter().find_map(|s| pod_uid(s)),
            qos_class: qos_class(path),
            unit: None,
        });
    }

    if let Some(last) = segments.last() {
        if let Some(id) = container_id(last) {
            let runtime = if last.contains("cri-containerd") || last.starts_with("containerd-") {
                Runtime::Containerd
            } else if last.contains("crio") {
                Runtime::CriO
            } else if last.contains("libpod") {
                Runtime::Podman
            } else {
                Runtime::Docker
            };
            return Some(Identity {
                runtime,
                container_id: Some(id),
                pod_uid: None,
                qos_class: None,
                unit: None,
            });
        }

        if let Some(unit) = last.strip_suffix(".service") {
            return Some(Identity {
                runtime: Runtime::SystemdUnit,
                container_id: None,
                pod_uid: None,
                qos_class: None,
                unit: Some(format!("{unit}.service")),
            });
        }
    }

    if segments.iter().any(|s| s.starts_with("user-")) {
        return Some(Identity {
            runtime: Runtime::UserSession,
            container_id: None,
            pod_uid: None,
            qos_class: None,
            unit: segments
                .iter()
                .find(|s| s.starts_with("user-"))
                .map(|s| s.to_string()),
        });
    }

    None
}

/// Pull a 64-hex container ID out of a path segment, tolerating the various
/// runtime prefixes (`docker-`, `cri-containerd-`, `crio-`, `libpod-`) and the
/// `.scope` suffix the systemd driver appends.
fn container_id(segment: &str) -> Option<String> {
    let trimmed = segment.strip_suffix(".scope").unwrap_or(segment);
    let candidate = trimmed
        .rsplit('-')
        .next()
        .filter(|s| is_hex_id(s))
        .or_else(|| Some(trimmed).filter(|s| is_hex_id(s)))?;
    Some(candidate.to_string())
}

fn is_hex_id(s: &str) -> bool {
    // Runtimes use full 64-char SHA256 IDs; some tools truncate to 12.
    s.len() >= 12 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// `kubepods-burstable-pod9f2c_1234.slice` or `pod9f2c-1234`.
///
/// The systemd driver rewrites the dashes in a pod UID as underscores, so they
/// are put back to recover the UID Kubernetes actually knows it by.
fn pod_uid(segment: &str) -> Option<String> {
    let trimmed = segment.strip_suffix(".slice").unwrap_or(segment);

    // "pod" must begin the segment or follow a '-'. A bare `find` also matches
    // the middle of "kubepods", which yields a UID of "s".
    let start = trimmed
        .match_indices("pod")
        .find(|(i, _)| *i == 0 || trimmed.as_bytes()[i - 1] == b'-')?
        .0;

    let uid = &trimmed[start + 3..];
    let plausible = uid.len() >= 8
        && uid
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !plausible {
        return None;
    }
    // The systemd driver writes a pod UID's dashes as underscores.
    Some(uid.replace('_', "-"))
}

fn qos_class(path: &str) -> Option<String> {
    if path.contains("burstable") {
        Some("Burstable".to_string())
    } else if path.contains("besteffort") {
        Some("BestEffort".to_string())
    } else if path.contains("kubepods") {
        // Guaranteed pods sit directly under kubepods with no QoS sub-slice.
        Some("Guaranteed".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_kubernetes_systemd_driver_path() {
        let id = identify("/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod9f2c_4b21.slice/cri-containerd-3f8a9c2b1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a.scope").unwrap();
        assert_eq!(id.runtime, Runtime::Kubernetes);
        assert_eq!(id.qos_class.as_deref(), Some("Burstable"));
        // Underscores in the systemd slice name are really dashes in the UID.
        assert_eq!(id.pod_uid.as_deref(), Some("9f2c-4b21"));
        assert!(id.container_id.unwrap().starts_with("3f8a9c2b"));
    }

    #[test]
    fn decodes_kubernetes_cgroupfs_driver_path() {
        let id = identify(
            "/kubepods/besteffort/pod1234abcd/9f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a0",
        )
        .unwrap();
        assert_eq!(id.runtime, Runtime::Kubernetes);
        assert_eq!(id.qos_class.as_deref(), Some("BestEffort"));
        assert!(id.container_id.is_some());
    }

    #[test]
    fn decodes_plain_docker_path() {
        let id =
            identify("/docker/aabbccddeeff00112233445566778899aabbccddeeff001122334455667788990")
                .unwrap();
        assert_eq!(id.runtime, Runtime::Docker);
        assert!(id.container_id.is_some());
    }

    #[test]
    fn decodes_podman_scope() {
        let id = identify("/machine.slice/libpod-aabbccddeeff00112233445566778899aabbccddeeff0011223344556677889.scope").unwrap();
        assert_eq!(id.runtime, Runtime::Podman);
    }

    #[test]
    fn decodes_systemd_service() {
        let id = identify("/system.slice/nginx.service").unwrap();
        assert_eq!(id.runtime, Runtime::SystemdUnit);
        assert_eq!(id.unit.as_deref(), Some("nginx.service"));
    }

    #[test]
    fn decodes_user_session() {
        let id = identify("/user.slice/user-1000.slice/session-1.scope").unwrap();
        assert_eq!(id.runtime, Runtime::UserSession);
    }

    #[test]
    fn returns_none_rather_than_guessing_for_the_root_cgroup() {
        assert!(identify("/").is_none());
        assert!(identify("").is_none());
    }

    #[test]
    fn summary_is_human_readable() {
        let id = identify("/kubepods.slice/kubepods-burstable.slice/kubepods-burstable-pod9f2c_4b21.slice/cri-containerd-3f8a9c2b1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a.scope").unwrap();
        let summary = id.summary();
        assert!(summary.contains("kubernetes"));
        assert!(summary.contains("Burstable"));
        assert!(!summary.contains("3f8a9c2b1d4e5f6a7b8c9d0e"), "id should be shortened");
    }
}
