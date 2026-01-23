
/// When there's a NUMA architecture (as normally in the cloud all raw machines 
/// are NUMA), than it is needed to attach the different CPU intensive tasks to
/// to CPU instances. Example:
/// 
/// ```rust
/// // Allocate data
/// let data = vec![0; 1_000_000];
/// // Process on SAME socket where data is allocated
/// thread::spawn(|| {
///     core_affinity::set_for_current(core_on_socket0);
/// 
///     // GOOD: CPU on socket 0 accessing Memory 0 (fast!)
///     for &x in &data {
///         process(x);
///     }
/// });
/// ```
/// 
/// This is necessary, since the memory access locally can be around 100ns, 
/// while memory attached to different CPU can be 200-300 ns or more.
/// 
/// Since not all systems are multi-socket systems, CPU affinity is not 
/// necessary all the time.
/// 
/// Note: core affinity is still useful on not NUMA systems as well (since it
/// helps to:
/// 
/// # cache locally
/// Without affinity (thread bounces between cores):
///     Time 0: Thread on Core 0 → loads data into L1/L2
///     Time 1: OS migrates thread to Core 2
///             → L1/L2 cache on Core 0 wasted!
///             → Must reload data into Core 2's cache
///             → Performance penalty: 50-100+ cycles
/// With affinity (thread stays on Core 0):
///     Time 0: Thread on Core 0 → loads data into L1/L2
///     Time 1: Thread still on Core 0
///             → Data already in cache (hot!)
///             → Fast access: 3-10 cycles
/// )
/// 
/// # avoiding Cache Line Bouncing (Multi-threaded)
/// 
/// In case of a CPU intensive task, this can mean ~23% faster execution (based 
/// on a perf test).
/// 
/// Don't use, when there's short-lived computations, or IO operations. Be aware,
/// when there're more tasks, than CPUs.
/// 
#[cfg(target_os = "linux")]
pub fn check_numa() {
    use std::{fs, thread};
    
    // Check how many NUMA nodes exist
    let numa_dirs = fs::read_dir("/sys/devices/system/node")
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_str().unwrap().starts_with("node"))
        .count();
    
    if numa_dirs > 1 {
        println!("NUMA system with {} nodes", numa_dirs);
    } else {
        println!("Non-NUMA system");
    }
}