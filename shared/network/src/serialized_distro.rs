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
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard error: {err}"),
            DistroResultsReaderError::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl fmt::Debug for DistroResultsReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroResultsReaderError::Postcard(err) => write!(f, "Postcard({err:?})"),
            DistroResultsReaderError::Io(err) => write!(f, "Io({err:?})"),
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
                            )));
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
