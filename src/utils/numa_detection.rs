// ============================================================================
// NUMA Topology Detection and CPU Affinity
// Optimizations for multi-socket servers and cache-aware thread placement
// ============================================================================
//
// # Why NUMA Matters for Matching Engines
//
// On multi-socket servers (common in trading infrastructure):
// - Local memory access: ~100ns
// - Remote memory access: ~200-300ns (2-3x slower!)
//
// By pinning threads to specific cores and allocating memory locally,
// we can achieve significant latency improvements.
//
// # Cache Benefits (even on non-NUMA)
//
// Without CPU affinity (thread bounces between cores):
//     Time 0: Thread on Core 0 → loads data into L1/L2
//     Time 1: OS migrates thread to Core 2
//             → L1/L2 cache on Core 0 wasted!
//             → Must reload data into Core 2's cache
//             → Performance penalty: 50-100+ cycles
//
// With affinity (thread stays on Core 0):
//     Time 0: Thread on Core 0 → loads data into L1/L2
//     Time 1: Thread still on Core 0
//             → Data already in cache (hot!)
//             → Fast access: 3-10 cycles
//
// # Usage
//
// ```ignore
// use matching_engine::utils::{NumaTopology, pin_current_thread_to_core};
//
// let topology = NumaTopology::detect();
// println!("NUMA nodes: {}", topology.node_count());
//
// // Pin the hot path thread to core 0
// pin_current_thread_to_core(0);
// ```
//
// # When NOT to Use CPU Affinity
//
// - Short-lived computations
// - I/O-bound operations
// - When there are more tasks than CPUs
// ============================================================================

use std::fmt;

/// Information about a single NUMA node.
#[derive(Debug, Clone)]
pub struct NumaNode {
    /// Node identifier (0-indexed)
    pub id: usize,
    /// CPU core IDs belonging to this node
    pub cpu_ids: Vec<usize>,
}

impl NumaNode {
    /// Get the number of CPUs on this node.
    #[inline]
    pub fn cpu_count(&self) -> usize {
        self.cpu_ids.len()
    }

    /// Check if a CPU belongs to this node.
    #[inline]
    pub fn contains_cpu(&self, cpu_id: usize) -> bool {
        self.cpu_ids.contains(&cpu_id)
    }

    /// Get the first CPU on this node (useful for simple pinning).
    #[inline]
    pub fn first_cpu(&self) -> Option<usize> {
        self.cpu_ids.first().copied()
    }
}

impl fmt::Display for NumaNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Node {} ({} CPUs: {:?})",
            self.id,
            self.cpu_count(),
            self.cpu_ids
        )
    }
}

/// Complete NUMA topology of the system.
#[derive(Debug, Clone)]
pub struct NumaTopology {
    /// All NUMA nodes in the system
    nodes: Vec<NumaNode>,
    /// Total number of CPUs across all nodes
    total_cpus: usize,
}

impl NumaTopology {
    /// Detect the NUMA topology of the current system.
    ///
    /// On Linux, this reads from `/sys/devices/system/node/`.
    /// On other platforms, returns a single-node topology with all CPUs.
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux().unwrap_or_else(Self::fallback)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Self::fallback()
        }
    }

    /// Create a fallback topology (single node with all CPUs).
    fn fallback() -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);

        let node = NumaNode {
            id: 0,
            cpu_ids: (0..num_cpus).collect(),
        };

        Self {
            nodes: vec![node],
            total_cpus: num_cpus,
        }
    }

    /// Detect NUMA topology on Linux by reading sysfs.
    #[cfg(target_os = "linux")]
    fn detect_linux() -> Option<Self> {
        use std::fs;
        use std::path::Path;

        let node_base = Path::new("/sys/devices/system/node");
        if !node_base.exists() {
            return None;
        }

        let mut nodes = Vec::new();
        let mut total_cpus = 0;

        // Read all node directories
        let entries = fs::read_dir(node_base).ok()?;

        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Match "node0", "node1", etc.
            if !name_str.starts_with("node") {
                continue;
            }

            let node_id: usize = name_str[4..].parse().ok()?;
            let cpulist_path = entry.path().join("cpulist");

            if let Ok(cpulist) = fs::read_to_string(&cpulist_path) {
                let cpu_ids = parse_cpu_list(cpulist.trim());
                total_cpus += cpu_ids.len();

                nodes.push(NumaNode { id: node_id, cpu_ids });
            }
        }

        if nodes.is_empty() {
            return None;
        }

        // Sort nodes by ID
        nodes.sort_by_key(|n| n.id);

        Some(Self { nodes, total_cpus })
    }

    /// Check if this is a NUMA system (multiple nodes).
    #[inline]
    pub fn is_numa(&self) -> bool {
        self.nodes.len() > 1
    }

    /// Get the number of NUMA nodes.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the total number of CPUs across all nodes.
    #[inline]
    pub fn total_cpus(&self) -> usize {
        self.total_cpus
    }

    /// Get all NUMA nodes.
    #[inline]
    pub fn nodes(&self) -> &[NumaNode] {
        &self.nodes
    }

    /// Get a specific node by ID.
    pub fn node(&self, id: usize) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Find which NUMA node a CPU belongs to.
    pub fn node_for_cpu(&self, cpu_id: usize) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.contains_cpu(cpu_id))
    }

    /// Get recommended CPU assignments for a given number of workers.
    ///
    /// Distributes workers across NUMA nodes to balance load and
    /// maximize memory locality.
    pub fn recommend_cpu_assignment(&self, worker_count: usize) -> Vec<usize> {
        if worker_count == 0 {
            return Vec::new();
        }

        let mut assignments = Vec::with_capacity(worker_count);

        // Round-robin across nodes, then across CPUs within each node
        let mut node_indices: Vec<usize> = vec![0; self.nodes.len()];

        for i in 0..worker_count {
            let node_idx = i % self.nodes.len();
            let node = &self.nodes[node_idx];

            if !node.cpu_ids.is_empty() {
                let cpu_idx = node_indices[node_idx] % node.cpu_ids.len();
                assignments.push(node.cpu_ids[cpu_idx]);
                node_indices[node_idx] += 1;
            }
        }

        assignments
    }
}

impl fmt::Display for NumaTopology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_numa() {
            writeln!(
                f,
                "NUMA system: {} nodes, {} total CPUs",
                self.node_count(),
                self.total_cpus
            )?;
            for node in &self.nodes {
                writeln!(f, "  {}", node)?;
            }
        } else {
            write!(f, "Non-NUMA system: {} CPUs", self.total_cpus)?;
        }
        Ok(())
    }
}

/// Parse a CPU list string like "0-3,8-11" into a Vec of CPU IDs.
#[cfg(target_os = "linux")]
fn parse_cpu_list(s: &str) -> Vec<usize> {
    let mut cpus = Vec::new();

    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some((start, end)) = part.split_once('-') {
            // Range like "0-3"
            if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                cpus.extend(start..=end);
            }
        } else {
            // Single CPU like "4"
            if let Ok(cpu) = part.parse::<usize>() {
                cpus.push(cpu);
            }
        }
    }

    cpus
}

// ============================================================================
// CPU Affinity Functions (feature-gated)
// ============================================================================

/// Pin the current thread to a specific CPU core.
///
/// This prevents the OS scheduler from migrating the thread to other cores,
/// which improves cache locality and reduces latency variance.
///
/// # Arguments
/// * `core_id` - The CPU core ID to pin to (0-indexed)
///
/// # Returns
/// * `true` if pinning succeeded
/// * `false` if the core doesn't exist or pinning failed
///
/// # Example
/// ```ignore
/// use matching_engine::utils::pin_current_thread_to_core;
///
/// // Pin the hot path to core 0
/// if pin_current_thread_to_core(0) {
///     println!("Thread pinned to core 0");
/// }
/// ```
#[cfg(feature = "numa")]
pub fn pin_current_thread_to_core(core_id: usize) -> bool {
    let core_ids = core_affinity::get_core_ids().unwrap_or_default();

    core_ids
        .into_iter()
        .find(|id| id.id == core_id)
        .map(core_affinity::set_for_current)
        .unwrap_or(false)
}

/// Pin the current thread to any CPU on a specific NUMA node.
///
/// Uses the first available CPU on the node.
///
/// # Arguments
/// * `topology` - The detected NUMA topology
/// * `node_id` - The NUMA node ID to pin to
///
/// # Returns
/// * `true` if pinning succeeded
/// * `false` if the node doesn't exist or pinning failed
#[cfg(feature = "numa")]
pub fn pin_current_thread_to_node(topology: &NumaTopology, node_id: usize) -> bool {
    topology
        .node(node_id)
        .and_then(|node| node.first_cpu())
        .map(pin_current_thread_to_core)
        .unwrap_or(false)
}

/// Get all available core IDs on this system.
#[cfg(feature = "numa")]
pub fn get_available_cores() -> Vec<usize> {
    core_affinity::get_core_ids()
        .unwrap_or_default()
        .into_iter()
        .map(|id| id.id)
        .collect()
}

// ============================================================================
// Stub implementations when numa feature is disabled
// ============================================================================

/// Pin the current thread to a specific CPU core.
///
/// **Note:** This is a no-op stub. Enable the `numa` feature for actual CPU pinning.
#[cfg(not(feature = "numa"))]
pub fn pin_current_thread_to_core(_core_id: usize) -> bool {
    // No-op when numa feature is disabled
    false
}

/// Pin the current thread to any CPU on a specific NUMA node.
///
/// **Note:** This is a no-op stub. Enable the `numa` feature for actual CPU pinning.
#[cfg(not(feature = "numa"))]
pub fn pin_current_thread_to_node(_topology: &NumaTopology, _node_id: usize) -> bool {
    // No-op when numa feature is disabled
    false
}

/// Get all available core IDs on this system.
///
/// **Note:** This is a stub. Enable the `numa` feature for actual core detection.
#[cfg(not(feature = "numa"))]
pub fn get_available_cores() -> Vec<usize> {
    let num_cpus = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1);
    (0..num_cpus).collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_detect() {
        let topology = NumaTopology::detect();

        // Should have at least one node
        assert!(topology.node_count() >= 1);
        assert!(topology.total_cpus() >= 1);

        println!("{}", topology);
    }

    #[test]
    fn test_topology_is_consistent() {
        let topology = NumaTopology::detect();

        // Total CPUs should match sum of CPUs across all nodes
        let sum: usize = topology.nodes().iter().map(|n| n.cpu_count()).sum();
        assert_eq!(topology.total_cpus(), sum);
    }

    #[test]
    fn test_node_for_cpu() {
        let topology = NumaTopology::detect();

        // CPU 0 should belong to some node
        let node = topology.node_for_cpu(0);
        assert!(node.is_some());
        assert!(node.unwrap().contains_cpu(0));
    }

    #[test]
    fn test_recommend_cpu_assignment() {
        let topology = NumaTopology::detect();

        // Request assignments for 4 workers
        let assignments = topology.recommend_cpu_assignment(4);
        assert_eq!(assignments.len(), 4);

        // All assigned CPUs should be valid
        for cpu in &assignments {
            assert!(topology.node_for_cpu(*cpu).is_some());
        }
    }

    #[test]
    fn test_recommend_zero_workers() {
        let topology = NumaTopology::detect();
        let assignments = topology.recommend_cpu_assignment(0);
        assert!(assignments.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_cpu_list() {
        assert_eq!(parse_cpu_list("0"), vec![0]);
        assert_eq!(parse_cpu_list("0-3"), vec![0, 1, 2, 3]);
        assert_eq!(parse_cpu_list("0,2,4"), vec![0, 2, 4]);
        assert_eq!(parse_cpu_list("0-2,8-10"), vec![0, 1, 2, 8, 9, 10]);
        assert_eq!(parse_cpu_list(""), Vec::<usize>::new());
    }

    #[test]
    fn test_numa_node_display() {
        let node = NumaNode {
            id: 0,
            cpu_ids: vec![0, 1, 2, 3],
        };
        let display = format!("{}", node);
        assert!(display.contains("Node 0"));
        assert!(display.contains("4 CPUs"));
    }

    #[test]
    fn test_pin_functions_exist() {
        // Just verify the functions are callable (they may no-op without numa feature)
        let _ = pin_current_thread_to_core(0);
        let topology = NumaTopology::detect();
        let _ = pin_current_thread_to_node(&topology, 0);
        let _ = get_available_cores();
    }

    #[cfg(all(feature = "numa", target_os = "linux"))]
    #[test]
    fn test_pin_to_core() {
        // This test actually pins (only reliable on Linux)
        let cores = get_available_cores();
        if !cores.is_empty() {
            let result = pin_current_thread_to_core(cores[0]);
            assert!(result, "Should be able to pin to first available core");
        }
    }

    #[cfg(all(feature = "numa", not(target_os = "linux")))]
    #[test]
    fn test_pin_to_core() {
        // On non-Linux platforms, just verify the function is callable
        // CPU pinning may not be fully supported
        let cores = get_available_cores();
        if !cores.is_empty() {
            // Don't assert success - macOS has limited CPU affinity support
            let _ = pin_current_thread_to_core(cores[0]);
        }
    }
}
