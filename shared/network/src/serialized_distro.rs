use psyche_core::BatchId;
use psyche_modeling::DistroResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt,
    io::{BufReader, Read},
    num::TryFromIntError,
};
use tch::Device;
use thiserror::Error;

use crate::serializable_tensor::SerializableTensor;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SerializedDistroResult {
    pub sparse_idx: SerializableTensor,
    pub sparse_val: SerializableTensor,
    pub xshape: Vec<u16>,
    pub totalk: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransmittableDistroResult {
    pub step: u32,
    pub trainer_nonce: u32,
    pub batch_id: BatchId,
    pub distro_results: Vec<SerializedDistroResult>,
}

impl TransmittableDistroResult {
    pub fn comptue_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.step.to_be_bytes());
        hasher.update(self.batch_id.0.start.to_be_bytes());
        hasher.update(self.batch_id.0.end.to_be_bytes());
        for result in &self.distro_results {
            hasher.update(result.sparse_idx.raw_tensor_data());
            hasher.update(result.sparse_val.raw_tensor_data());
        }
        hasher.finalize().into()
    }

    /// Add fixed padding for P2P testing purposes - creates larger blobs without affecting training
    pub fn with_test_padding(mut self, target_size_mb: usize) -> Self {
        // Always add exactly 3 padding entries of roughly equal size to reach target
        const NUM_PADDING_ENTRIES: usize = 3;

        let target_bytes = target_size_mb * 1024 * 1024;
        let current_size = postcard::to_stdvec(&self).unwrap_or_default().len();

        if current_size >= target_bytes {
            return self; // Already large enough
        }

        let padding_needed = target_bytes - current_size;
        let padding_per_entry = padding_needed / (NUM_PADDING_ENTRIES * 8); // 8 bytes per entry (2 tensors * 4 bytes)

        if padding_per_entry > 0 {
            for _ in 0..NUM_PADDING_ENTRIES {
                // Create dummy tensors using tch and convert to SerializableTensor
                let dummy_tensor = tch::Tensor::zeros(
                    &[padding_per_entry as i64],
                    (tch::Kind::Float, tch::Device::Cpu),
                );
                let padding_result = SerializedDistroResult {
                    sparse_idx: (&dummy_tensor).try_into().unwrap_or_else(|_| {
                        // Fallback: create minimal tensor if conversion fails
                        let small_tensor =
                            tch::Tensor::zeros(&[1], (tch::Kind::Float, tch::Device::Cpu));
                        (&small_tensor).try_into().unwrap()
                    }),
                    sparse_val: (&dummy_tensor).try_into().unwrap_or_else(|_| {
                        let small_tensor =
                            tch::Tensor::zeros(&[1], (tch::Kind::Float, tch::Device::Cpu));
                        (&small_tensor).try_into().unwrap()
                    }),
                    xshape: vec![padding_per_entry.min(65535) as u16], // Ensure it fits in u16
                    totalk: padding_per_entry as u32,
                };
                self.distro_results.push(padding_result);
            }
        }

        self
    }

    /// Remove the last N padding entries added by with_test_padding()
    pub fn without_test_padding(mut self) -> Self {
        const NUM_PADDING_ENTRIES: usize = 3;

        let original_len = self.distro_results.len();

        // Only remove padding if we have more entries than expected from real training
        if original_len > NUM_PADDING_ENTRIES {
            // Remove the last N entries (these should be the padding)
            self.distro_results
                .truncate(original_len - NUM_PADDING_ENTRIES);
            tracing::info!(
                "Removed {} test padding entries ({} -> {})",
                NUM_PADDING_ENTRIES,
                original_len,
                self.distro_results.len()
            );
        }

        self
    }
}

#[derive(Debug, Error)]
pub enum SerializeDistroResultError {
    #[error("Torch error: {0}")]
    Tch(#[from] tch::TchError),
    #[error("Shape had invalid u16: {0}")]
    ShapeInt(#[from] TryFromIntError),
}

impl TryFrom<&DistroResult> for SerializedDistroResult {
    type Error = SerializeDistroResultError;
    fn try_from(value: &DistroResult) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            sparse_idx: (&value.sparse_idx).try_into()?,
            sparse_val: (&value.sparse_val).try_into()?,
            xshape: value
                .xshape
                .iter()
                .map(|&x| u16::try_from(x))
                .collect::<Result<Vec<u16>, _>>()?,
            totalk: value.totalk as u32,
        })
    }
}

impl TryFrom<&SerializedDistroResult> for DistroResult {
    type Error = tch::TchError;

    fn try_from(value: &SerializedDistroResult) -> std::result::Result<Self, Self::Error> {
        let mut distro_result = Self {
            sparse_idx: (&value.sparse_idx).try_into()?,
            sparse_val: (&value.sparse_val).try_into()?,
            xshape: value.xshape.iter().map(|x| *x as i64).collect(),
            totalk: value.totalk as i64,
            stats: None,
        };
        // don't pin - it would crash if no CUDA is available.
        if Device::cuda_if_available().is_cuda() {
            // the index of the CUDA device doesn't matter here.
            distro_result.sparse_idx = distro_result.sparse_idx.pin_memory(None);
            distro_result.sparse_val = distro_result.sparse_val.pin_memory(None);
        }
        Ok(distro_result)
    }
}

pub fn distro_results_to_bytes(
    results: &[SerializedDistroResult],
) -> Result<Vec<u8>, postcard::Error> {
    let mut buf = Vec::new();
    for result in results {
        buf.extend(postcard::to_stdvec(result)?);
    }
    Ok(buf)
}

pub fn distro_results_from_reader<R: Read>(reader: R) -> DistroResultIterator<R> {
    DistroResultIterator::new(reader)
}

pub enum DistroResultsReaderError {
    Postcard(postcard::Error),
    Io(std::io::Error),
}

impl Error for DistroResultsReaderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DistroResultsReaderError::Postcard(err) => Some(err),
            DistroResultsReaderError::Io(err) => Some(err),
        }
    }
}

impl fmt::Display for DistroResultsReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard error: {}", err),
            DistroResultsReaderError::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl fmt::Debug for DistroResultsReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard({:?})", err),
            DistroResultsReaderError::Io(err) => write!(f, "Io({:?})", err),
        }
    }
}

pub struct DistroResultIterator<R: Read> {
    reader: BufReader<R>,
    buffer: Vec<u8>,
}

impl<R: Read> DistroResultIterator<R> {
    pub fn new(reader: R) -> Self {
        DistroResultIterator {
            reader: BufReader::new(reader),
            buffer: Vec::new(),
        }
    }
}

impl<R: Read> Iterator for DistroResultIterator<R> {
    type Item = Result<SerializedDistroResult, DistroResultsReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match postcard::take_from_bytes::<SerializedDistroResult>(&self.buffer) {
                Ok((result, remaining)) => {
                    self.buffer = remaining.to_vec();
                    return Some(Ok(result));
                }
                Err(postcard::Error::DeserializeUnexpectedEnd) => {
                    // Not enough data, need to read more
                    let mut chunk = [0u8; 1024]; // Adjust chunk size as needed
                    match self.reader.read(&mut chunk) {
                        Ok(0) if self.buffer.is_empty() => return None, // EOF and no partial data
                        Ok(0) => {
                            return Some(Err(DistroResultsReaderError::Postcard(
                                postcard::Error::DeserializeUnexpectedEnd,
                            )))
                        }
                        Ok(n) => self.buffer.extend_from_slice(&chunk[..n]),
                        Err(e) => return Some(Err(DistroResultsReaderError::Io(e))),
                    }
                }
                Err(e) => return Some(Err(DistroResultsReaderError::Postcard(e))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use psyche_modeling::CompressDCT;
    use tch::{Device, Kind, Tensor};

    use crate::serializable_tensor::SerializableTensor;

    #[test]
    fn test_roundtrip_distro_result_1bit() {
        let truth = Tensor::from_slice2(&[
            [0.5000, 0.5000, 0.5000, 0.5000],
            [0.6533, 0.2706, -0.2706, -0.6533],
            [0.5000, -0.5000, -0.5000, 0.5000],
            [0.2706, -0.6533, 0.6533, -0.2706],
        ])
        .to_kind(Kind::Float)
        .to(Device::Cpu);

        let (sparse_idx, raw_sparse_val, xshape, totalk) = CompressDCT::compress(&truth, i64::MAX);
        // turn raw sparse vals into bools
        let bool_sparse_val = raw_sparse_val.greater(0);

        // and compress to 1bit
        let ser_sparse_val = SerializableTensor::try_from(&bool_sparse_val).unwrap();

        // decompress back into bool tensor
        let sparse_val = Tensor::try_from(&ser_sparse_val).unwrap();

        assert_eq!(sparse_val.kind(), Kind::Bool);

        // when it's quantized to bools, we need to transform it back into -1/+1.
        let sparse_val = sparse_val.to_kind(Kind::Int8) * 2 - 1;

        // finally decompress back to ground truth
        let decompressed_signed = CompressDCT::decompress(
            &sparse_idx,
            &sparse_val,
            &xshape,
            totalk,
            truth.kind(),
            Device::Cpu,
        );
        let signed_truth = truth.sign();

        assert!(decompressed_signed.equal(&signed_truth));
    }
}
