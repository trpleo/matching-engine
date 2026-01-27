// ============================================================================
// CPU Detection and SIMD Matcher Factory
// Runtime detection of CPU capabilities and optimal matcher selection
// ============================================================================

use super::scalar::ScalarMatcher;
use super::traits::SimdMatcher;
use std::sync::Arc;

/// CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Architecture {
    /// x86_64 (Intel/AMD 64-bit)
    X86_64,
    /// aarch64 (ARM 64-bit, including Apple Silicon)
    Aarch64,
    /// Unknown or unsupported architecture
    Other,
}

impl Architecture {
    /// Detect the current CPU architecture.
    #[inline]
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Architecture::X86_64
        }
        #[cfg(target_arch = "aarch64")]
        {
            Architecture::Aarch64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Architecture::Other
        }
    }
}

impl std::fmt::Display for Architecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Architecture::X86_64 => write!(f, "x86_64"),
            Architecture::Aarch64 => write!(f, "aarch64"),
            Architecture::Other => write!(f, "other"),
        }
    }
}

/// SIMD capability level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SimdLevel {
    /// No SIMD, scalar operations only
    None,
    /// ARM NEON (128-bit, 2x i64)
    Neon,
    /// x86 AVX2 (256-bit, 4x i64)
    Avx2,
    /// x86 AVX-512 (512-bit, 8x i64)
    Avx512,
}

impl SimdLevel {
    /// Detect the highest available SIMD level for the current CPU.
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            // AVX-512 detection only when feature is enabled (requires nightly)
            #[cfg(feature = "avx512")]
            if is_x86_feature_detected!("avx512f") {
                return SimdLevel::Avx512;
            }
            if is_x86_feature_detected!("avx2") {
                return SimdLevel::Avx2;
            }
            return SimdLevel::None;
        }

        #[cfg(target_arch = "aarch64")]
        {
            // NEON is always available on aarch64
            SimdLevel::Neon
        }

        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            return SimdLevel::None;
        }
    }
}

impl std::fmt::Display for SimdLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SimdLevel::None => write!(f, "None (Scalar)"),
            SimdLevel::Neon => write!(f, "ARM NEON"),
            SimdLevel::Avx2 => write!(f, "AVX2"),
            SimdLevel::Avx512 => write!(f, "AVX-512"),
        }
    }
}

/// Detected CPU capabilities.
#[derive(Debug, Clone, Copy)]
pub struct CpuCapabilities {
    /// The CPU architecture
    pub architecture: Architecture,
    /// The highest available SIMD level
    pub simd_level: SimdLevel,
}

impl CpuCapabilities {
    /// Detect CPU capabilities at runtime.
    pub fn detect() -> Self {
        Self {
            architecture: Architecture::detect(),
            simd_level: SimdLevel::detect(),
        }
    }
}

impl std::fmt::Display for CpuCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CPU: {} with {}", self.architecture, self.simd_level)
    }
}

/// Create the optimal SIMD matcher for the current CPU.
///
/// This function detects the CPU capabilities and returns the best
/// available SIMD implementation:
///
/// - AVX-512 on x86_64 with AVX-512F support
/// - AVX2 on x86_64 with AVX2 support
/// - NEON on aarch64 (always available)
/// - Scalar fallback on other platforms
///
/// # Example
/// ```ignore
/// let matcher = create_simd_matcher();
/// println!("Using SIMD: {}", matcher.name());
/// ```
pub fn create_simd_matcher() -> Arc<dyn SimdMatcher> {
    let caps = CpuCapabilities::detect();

    match caps.simd_level {
        #[cfg(all(target_arch = "x86_64", feature = "avx512"))]
        SimdLevel::Avx512 => {
            use super::avx512::Avx512Matcher;
            Arc::new(Avx512Matcher::new())
        },

        #[cfg(target_arch = "x86_64")]
        SimdLevel::Avx2 => {
            use super::avx2::Avx2Matcher;
            Arc::new(Avx2Matcher::new())
        },

        #[cfg(target_arch = "aarch64")]
        SimdLevel::Neon => {
            use super::neon::NeonMatcher;
            Arc::new(NeonMatcher::new())
        },

        _ => Arc::new(ScalarMatcher::new()),
    }
}

/// Create a scalar matcher (for testing or comparison).
pub fn create_scalar_matcher() -> Arc<dyn SimdMatcher> {
    Arc::new(ScalarMatcher::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_detect() {
        let arch = Architecture::detect();
        // Just verify it returns a valid value
        assert!(matches!(
            arch,
            Architecture::X86_64 | Architecture::Aarch64 | Architecture::Other
        ));
    }

    #[test]
    fn test_simd_level_detect() {
        let level = SimdLevel::detect();
        // Just verify it returns a valid value
        assert!(matches!(
            level,
            SimdLevel::None | SimdLevel::Neon | SimdLevel::Avx2 | SimdLevel::Avx512
        ));
    }

    #[test]
    fn test_cpu_capabilities_detect() {
        let caps = CpuCapabilities::detect();
        println!("{}", caps);

        // Verify consistency
        #[cfg(target_arch = "x86_64")]
        assert_eq!(caps.architecture, Architecture::X86_64);

        #[cfg(target_arch = "aarch64")]
        {
            assert_eq!(caps.architecture, Architecture::Aarch64);
            assert_eq!(caps.simd_level, SimdLevel::Neon);
        }
    }

    #[test]
    fn test_create_simd_matcher() {
        let matcher = create_simd_matcher();
        let name = matcher.name();

        // Verify the name matches what we expect for this platform
        #[cfg(target_arch = "aarch64")]
        assert_eq!(name, "NEON");

        #[cfg(target_arch = "x86_64")]
        assert!(
            name == "AVX-512" || name == "AVX2" || name == "Scalar",
            "Unexpected matcher name: {}",
            name
        );

        println!("Created matcher: {}", name);
    }

    #[test]
    fn test_create_scalar_matcher() {
        let matcher = create_scalar_matcher();
        assert_eq!(matcher.name(), "Scalar");
    }

    #[test]
    fn test_matcher_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<dyn SimdMatcher>>();
    }

    #[test]
    fn test_simd_level_ordering() {
        // Verify SIMD levels are ordered by capability
        assert!(SimdLevel::None < SimdLevel::Neon);
        assert!(SimdLevel::Neon < SimdLevel::Avx2);
        assert!(SimdLevel::Avx2 < SimdLevel::Avx512);
    }
}
