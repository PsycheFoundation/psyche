use tch::Device;
#[cfg(test)]
use tch::{Kind, Tensor};

/// Check if CUDA is available
pub fn has_cuda() -> bool {
    tch::utils::has_cuda()
}

/// Check if MPS is available
pub fn has_mps() -> bool {
    tch::utils::has_mps()
}

/// Get the optimal device for the current platform
///
/// Returns:
/// - MPS on macOS if available
/// - CUDA on other platforms if available
/// - CPU as fallback
pub fn get_optimal_device() -> Device {
    #[cfg(target_os = "macos")]
    {
        if has_mps() {
            return Device::Mps;
        }
    }

    if has_cuda() {
        Device::Cuda(0)
    } else {
        Device::Cpu
    }
}

/// Parse device string into Device enum
///
/// Supported formats:
/// - "cpu" -> Device::Cpu
/// - "mps" -> Device::Mps (macOS only)
/// - "cuda" -> Device::Cuda(0)
/// - "cuda:N" -> Device::Cuda(N)
pub fn parse_device(device_str: &str) -> anyhow::Result<Device> {
    match device_str.to_lowercase().as_str() {
        "cpu" => Ok(Device::Cpu),
        "cuda" => Ok(Device::Cuda(0)),
        s if s.starts_with("cuda:") => {
            let id = s
                .strip_prefix("cuda:")
                .ok_or_else(|| anyhow::anyhow!("Invalid CUDA device format"))?
                .parse::<usize>()
                .map_err(|_| anyhow::anyhow!("Invalid CUDA device ID"))?;
            Ok(Device::Cuda(id))
        }
        "mps" => {
            #[cfg(target_os = "macos")]
            {
                if has_mps() {
                    Ok(Device::Mps)
                } else {
                    anyhow::bail!("MPS device requested but not available on this system")
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                anyhow::bail!("MPS device is only available on macOS")
            }
        }
        _ => anyhow::bail!(
            "Invalid device: {}. Supported: cpu, mps, cuda, cuda:N",
            device_str
        ),
    }
}

/// Get device for data parallel training.
///
/// On macOS with MPS, returns MPS for rank 0, CPU for others (MPS doesn't support multi-GPU).
/// On other platforms, returns appropriate CUDA device
#[allow(unused_variables)] // world_size is only used on macOS
pub fn get_device_for_rank(rank: usize, world_size: usize) -> Device {
    #[cfg(target_os = "macos")]
    {
        if rank == 0 && has_mps() {
            return Device::Mps;
        } else if world_size > 1 {
            return Device::Cpu;
        }
    }

    if has_cuda() {
        Device::Cuda(rank)
    } else {
        Device::Cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device() {
        assert_eq!(parse_device("cpu").unwrap(), Device::Cpu);
        assert_eq!(parse_device("CUDA:42").unwrap(), Device::Cuda(42));
        assert!(parse_device("nvidia").is_err());
        assert!(parse_device("").is_err());
        assert!(parse_device("cuda:").is_err());
        assert!(parse_device("cuda:abc").is_err());
        assert!(parse_device("cuda:-1").is_err());
        assert!(parse_device("cuda:1.5").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_mps_parsing_on_macos() {
        let result = parse_device("mps");
        if has_mps() {
            assert_eq!(result.unwrap(), Device::Mps);
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("not available"));
        }
    }

    #[test]
    fn test_get_device_for_rank_single_worker() {
        // Single worker should get optimal device
        let device = get_device_for_rank(0, 1);

        #[cfg(target_os = "macos")]
        {
            if has_mps() {
                assert_eq!(device, Device::Mps);
            } else {
                assert_eq!(device, Device::Cpu);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            if has_cuda() {
                assert_eq!(device, Device::Cuda(0));
            } else {
                assert_eq!(device, Device::Cpu);
            }
        }
    }

    #[test]
    fn test_get_device_for_rank_multiple_workers() {
        // Test multiple workers on different platforms
        #[cfg(target_os = "macos")]
        {
            // On macOS with MPS, only rank 0 gets MPS
            if has_mps() {
                assert_eq!(get_device_for_rank(0, 4), Device::Mps);
                assert_eq!(get_device_for_rank(1, 4), Device::Cpu);
                assert_eq!(get_device_for_rank(2, 4), Device::Cpu);
                assert_eq!(get_device_for_rank(3, 4), Device::Cpu);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            if has_cuda() {
                // Each rank gets its own CUDA device
                assert_eq!(get_device_for_rank(0, 4), Device::Cuda(0));
                assert_eq!(get_device_for_rank(1, 4), Device::Cuda(1));
                assert_eq!(get_device_for_rank(2, 4), Device::Cuda(2));
                assert_eq!(get_device_for_rank(3, 4), Device::Cuda(3));
            } else {
                // Without CUDA, everyone gets CPU
                assert_eq!(get_device_for_rank(0, 4), Device::Cpu);
                assert_eq!(get_device_for_rank(1, 4), Device::Cpu);
            }
        }
    }

    #[test]
    fn test_optimal_device_consistency() {
        // get_optimal_device should be consistent with get_device_for_rank(0, 1)
        let optimal = get_optimal_device();
        let rank_zero = get_device_for_rank(0, 1);
        assert_eq!(optimal, rank_zero);
    }

    #[test]
    fn test_device_functionality() {
        // Test that we can actually create tensors on the returned devices
        let device = get_optimal_device();

        // This should not panic
        let result = std::panic::catch_unwind(|| {
            let tensor = Tensor::zeros([2, 3], (Kind::Float, device));
            assert_eq!(tensor.size(), vec![2, 3]);
            assert_eq!(tensor.device(), device);

            // Test basic operations work
            let result = &tensor + 1.0;
            assert_eq!(result.size(), vec![2, 3]);
        });

        assert!(
            result.is_ok(),
            "Failed to create tensor on device {:?}",
            device
        );
    }
}
