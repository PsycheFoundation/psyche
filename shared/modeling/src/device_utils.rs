use std::{fmt, str::FromStr};

use itertools::Itertools;
use tch::{Device, utils::has_mps};
#[cfg(test)]
use tch::{Kind, Tensor};
use thiserror::Error;

/// Get all available CUDA devices
fn get_cuda_devices() -> Vec<usize> {
    (0..tch::Cuda::device_count() as usize).collect()
}

/// Get the optimal devices for the current platform
///
/// Returns:
/// - MPS on macOS if available
/// - CUDA on other platforms if available
/// - CPU as fallback
pub fn get_optimal_devices() -> Devices {
    #[cfg(target_os = "macos")]
    {
        if has_mps() {
            return Devices::Mps;
        }
    }

    let cuda_device_indices = get_cuda_devices();

    if !cuda_device_indices.is_empty() {
        return Devices::Cuda(cuda_device_indices);
    }

    Devices::Cpu
}

#[derive(Clone, Debug, PartialEq)]
pub enum Devices {
    Cpu,
    Mps,
    Cuda(Vec<usize>),
}

impl Default for Devices {
    fn default() -> Self {
        get_optimal_devices()
    }
}

impl fmt::Display for Devices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Devices::Cpu => write!(f, "CPU"),
            Devices::Mps => write!(f, "MPS"),
            Devices::Cuda(device_ids) => {
                write!(
                    f,
                    "CUDA({})",
                    device_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        }
    }
}

impl Devices {
    /// The number of unique usable devices.
    ///
    /// For CPU & MPS, this returns 1, since parallelism doesn't make sense to use on one single physical device.
    ///
    /// For CUDA, this returns the number of CUDA devices connected to this system.
    pub fn size(&self) -> usize {
        match self {
            Devices::Cpu => 1,
            Devices::Mps => 1,
            Devices::Cuda(device_indices) => device_indices.len(),
        }
    }

    /// Returns the nth device in this devices set.
    pub fn device_for_rank(&self, n: usize) -> Option<Device> {
        match self {
            Devices::Cpu if n == 0 => Some(Device::Cpu),
            Devices::Mps if n == 0 => Some(Device::Mps),
            Devices::Cuda(device_indices) => device_indices.get(n).map(|idx| Device::Cuda(*idx)),
            _ => None,
        }
    }

    /// Returns if the device is available to be accessed
    pub fn is_probably_available(&self) -> bool {
        match self {
            Devices::Cpu => true,
            Devices::Cuda(_) => tch::utils::has_cuda() && tch::Cuda::is_available(),
            Devices::Mps => has_mps(),
        }
    }
}

/// Get all available devices, for debugging purposes
fn get_all_device_strings() -> Vec<String> {
    let mut strings = vec!["auto".to_string(), "cpu".to_string()];
    if has_mps() {
        strings.push("mps".to_owned());
    }
    let cuda = get_cuda_devices();
    if !cuda.is_empty() {
        strings.push("cuda".to_string());
        strings.push(format!("cuda:{}", cuda.into_iter().join(",")).to_string());
    }
    strings
}

#[derive(Error, Debug)]
pub enum DevicesParseError {
    #[error("device {0} is not available on this system. Available devices are: {1}")]
    DeviceNotAvailable(String, String),

    #[error("invalid format for device(s) {0}: '{1}'")]
    InvalidDeviceFormat(String, String),

    #[error("invalid device '{0}'. Available devices are: {1}")]
    InvalidDevicesString(String, String),
}

impl FromStr for Devices {
    type Err = DevicesParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(get_optimal_devices()),
            "cpu" => Ok(Devices::Cpu),
            "cuda" => {
                let available_cuda_devices = get_cuda_devices();
                if available_cuda_devices.is_empty() {
                    return Err(DevicesParseError::DeviceNotAvailable(
                        "CUDA".to_owned(),
                        get_all_device_strings().join(", "),
                    ));
                }
                Ok(Devices::Cuda(available_cuda_devices))
            }
            "mps" => {
                if !has_mps() {
                    return Err(DevicesParseError::DeviceNotAvailable(
                        "MPS".to_owned(),
                        get_all_device_strings().join(", "),
                    ));
                }
                Ok(Devices::Mps)
            }

            s if s.starts_with("cuda:") => {
                let devices_str = s
                    .strip_prefix("cuda:")
                    .expect("if it starts_with cuda:, strip_prefix can't fail");

                let available_cuda_devices = get_cuda_devices();
                if available_cuda_devices.is_empty() {
                    return Err(DevicesParseError::DeviceNotAvailable(
                        "CUDA".to_owned(),
                        get_all_device_strings().join(", "),
                    ));
                }

                let device_ids = devices_str
                    .split(',')
                    .map(|id_str| {
                        let id = id_str.trim().parse::<usize>().map_err(|_| {
                            DevicesParseError::InvalidDeviceFormat(s.to_owned(), id_str.to_owned())
                        })?;
                        if !available_cuda_devices.contains(&id) {
                            return Err(DevicesParseError::DeviceNotAvailable(
                                format!("cuda:{id}"),
                                get_all_device_strings().join(", "),
                            ));
                        }
                        Ok(id)
                    })
                    .collect::<Result<Vec<usize>, _>>()?;

                if device_ids.is_empty() {
                    return Err(DevicesParseError::InvalidDevicesString(
                        s.to_string(),
                        get_all_device_strings().join(", "),
                    ));
                }

                Ok(Devices::Cuda(device_ids))
            }
            s => Err(DevicesParseError::InvalidDevicesString(
                s.to_string(),
                get_all_device_strings().join(", "),
            )),
        }
    }
}

#[cfg(feature = "python")]
pub trait DevicePytorchStr {
    fn to_pytorch_device_string(&self) -> String;
}

#[cfg(feature = "python")]
impl DevicePytorchStr for Device {
    fn to_pytorch_device_string(&self) -> String {
        match self {
            Device::Cpu => "cpu".to_owned(),
            Device::Cuda(i) => format!("cuda:{i}"),
            Device::Mps => "mps".to_owned(),
            Device::Vulkan => "vulkan".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tch::utils::has_cuda;

    use super::*;

    #[test]
    fn test_parse_device() {
        assert!(("auto".parse::<Devices>().is_ok()));
        assert!(("".parse::<Devices>().is_err()));
        assert!(("banana".parse::<Devices>().is_err()));
        assert_eq!(("cpu".parse::<Devices>().unwrap()), Devices::Cpu);
        if tch::Cuda::device_count() > 0 {
            for idx in 0..tch::Cuda::device_count() {
                assert_eq!(
                    format!("cuda:{idx}").parse::<Devices>().unwrap(),
                    Devices::Cuda(vec![idx as usize])
                );
            }
            assert_eq!(
                format!("cuda:{}", (0..tch::Cuda::device_count()).join(","))
                    .parse::<Devices>()
                    .unwrap(),
                Devices::Cuda((0..tch::Cuda::device_count() as usize).collect())
            );
            assert!(
                format!("cuda:{}", tch::Cuda::device_count())
                    .parse::<Devices>()
                    .is_err()
            );
        } else {
            assert!(matches!(
                "cuda".parse::<Devices>(),
                Err(DevicesParseError::DeviceNotAvailable(_, _))
            ));
            assert!(matches!(
                "cuda:0".parse::<Devices>(),
                Err(DevicesParseError::DeviceNotAvailable(_, _))
            ));
        }

        if has_mps() {
            assert_eq!("mps".parse::<Devices>().unwrap(), Devices::Mps);
        } else {
            assert!(matches!(
                "mps".parse::<Devices>(),
                Err(DevicesParseError::DeviceNotAvailable(_, _))
            ));
        }

        assert!(("nvidia".parse::<Devices>()).is_err());
        assert!(("cuda:abc".parse::<Devices>()).is_err());
        assert!(("cuda:-1".parse::<Devices>()).is_err());
        assert!(("cuda:1.5".parse::<Devices>()).is_err());
    }

    #[test]
    fn test_get_device_for_rank_single_worker() {
        // Single worker (rank 0) should get optimal device
        let device = get_optimal_devices().device_for_rank(0);
        if has_mps() {
            assert_eq!(device, Some(Device::Mps));
        } else if has_cuda() {
            assert_eq!(device, Some(Device::Cuda(0)));
        } else {
            assert_eq!(device, Some(Device::Cpu));
        }
    }

    #[test]
    fn test_get_device_for_rank_multiple_workers() {
        // Rank 1 should only work with CUDA with more than one GPU.
        let rank_1_device = get_optimal_devices().device_for_rank(1);

        let all_cuda_devices = get_cuda_devices();
        if all_cuda_devices.len() > 1 {
            assert_eq!(
                rank_1_device,
                Some(Device::Cuda(1)),
                "this system has more than 1 cuda device, but the device for rank 1 is not Cuda(1): {all_cuda_devices:?}"
            );
        } else {
            assert_eq!(
                rank_1_device, None,
                "this system has 0 or 1 cuda devices, but the device for rank 1 is not None"
            );
        }
    }

    #[test]
    fn test_device_functionality() {
        // Test that we can actually create tensors on the returned devices
        let device = get_optimal_devices().device_for_rank(0).unwrap();

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
            "Failed to create tensor on device {device:?}"
        );

        let uint32_result = std::panic::catch_unwind(|| {
            let tensor = Tensor::randint(100000, [4], (Kind::Int64, device));
            let compressed = tensor.to_kind(Kind::UInt32).view_dtype(Kind::Uint8);
            let _decompressed = compressed.view_dtype(Kind::UInt32).to_kind(Kind::Int64);
        });

        assert!(
            uint32_result.is_ok(),
            "Failed to convert to uint32 with view_dtype on device {device:?}"
        );
    }
}
