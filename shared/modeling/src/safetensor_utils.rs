use safetensors::{SafeTensors, slice::TensorIndexer};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    io,
    ops::Bound,
    path::PathBuf,
};
use tch::{
    Device, Kind, Tensor,
    nn::{Shard, VarStore},
};
use thiserror::Error;

const MAX_SAFETENSOR_PART_SIZE: usize = 1024 * 1024 * 1024 * 5;

#[derive(Error, Debug)]
pub enum LoadSafetensorsError {
    #[error("Failed to open safetensors file: {0}")]
    OpenFile(#[from] io::Error),

    #[error("Failed to deserialize safetensors: {0}")]
    Deserialize(#[from] safetensors::SafeTensorError),

    #[error("failed to perform tensor operation: {0}")]
    TchError(#[from] tch::TchError),

    #[error(
        "Cannot shard tensor {name} of shape {size:?} along dimension {dim} into {world_size} parts"
    )]
    CantShard {
        name: String,
        size: Vec<i64>,
        dim: usize,
        world_size: usize,
    },

    #[error("Failed to slice tensor {0}")]
    FailedToSlice(String),

    #[error("Checkpoint missing the following variables: {0:?}")]
    MissingVariables(HashSet<String>),
}

pub fn load_safetensors_into_variables(
    vs: &mut VarStore,
    repo_files: &[PathBuf],
) -> Result<(), LoadSafetensorsError> {
    let _no_grad = tch::no_grad_guard();
    let mut unmatched = vs.variables().keys().cloned().collect::<HashSet<_>>();
    for path in repo_files.iter().filter(|x| {
        x.extension()
            .is_some_and(|y| y.eq_ignore_ascii_case("safetensors"))
    }) {
        let file = std::fs::File::open(path)?;
        let content = unsafe { memmap2::MmapOptions::new().map(&file)? };
        let safetensors = SafeTensors::deserialize(&content)?;
        let mut variables = vs.variables_.lock().unwrap();
        let shards = variables.shards.clone();
        for (name, var) in variables.named_variables.iter_mut() {
            if let Ok(view) = safetensors.tensor(name) {
                let mut size: Vec<i64> = view.shape().iter().map(|&x| x as i64).collect();
                let kind: Kind = view.dtype().try_into()?;

                if let Some(Shard {
                    dim,
                    rank,
                    world_size,
                }) = shards.get(name)
                {
                    let (dim, rank, world_size) = (*dim, *rank, *world_size);
                    let total_size = size[dim];
                    if total_size % (world_size as i64) != 0 {
                        return Err(LoadSafetensorsError::CantShard {
                            name: name.clone(),
                            size,
                            dim,
                            world_size,
                        });
                    }
                    let block_size = total_size / (world_size as i64);
                    let start = (rank as i64) * block_size;
                    let stop = ((rank + 1) as i64) * block_size;

                    let slices: Vec<TensorIndexer> = (0..view.shape().len())
                        .map(|i| {
                            if i == dim {
                                TensorIndexer::Narrow(
                                    Bound::Included(start as usize),
                                    Bound::Excluded(stop as usize),
                                )
                            } else {
                                TensorIndexer::Narrow(Bound::Unbounded, Bound::Unbounded)
                            }
                        })
                        .collect();
                    let data_iterator = view
                        .sliced_data(&slices)
                        .map_err(|_| LoadSafetensorsError::FailedToSlice(name.clone()))?;
                    let data: Vec<u8> = data_iterator.flatten().cloned().collect();
                    size[dim] = block_size;
                    let src_tensor =
                        unsafe { Tensor::from_blob(data.as_ptr(), &size, &[], kind, Device::Cpu) };
                    var.f_copy_(&src_tensor)?;
                } else {
                    let src_tensor = unsafe {
                        Tensor::from_blob(view.data().as_ptr(), &size, &[], kind, Device::Cpu)
                    };
                    var.f_copy_(&src_tensor)?;
                }
                unmatched.remove(name);
            }
        }
    }
    if !unmatched.is_empty() {
        return Err(LoadSafetensorsError::MissingVariables(unmatched));
    }
    Ok(())
}

#[derive(Default)]
struct FilePart {
    tensors: Vec<(String, Tensor)>,
    size: usize,
}

#[derive(Error, Debug)]
pub enum SaveSafetensorsError {
    #[error("No tensors to save")]
    NoTensors,

    #[error("Failed to create directory {0}: {1}")]
    CreateDir(PathBuf, io::Error),

    #[error(
        "Tensor {name} too big to save to file -- it's {size} bytes while we have a max of {MAX_SAFETENSOR_PART_SIZE} bytes"
    )]
    TensorTooBig { name: String, size: usize },

    #[error("Torch error: {0}")]
    TchError(#[from] tch::TchError),

    #[error("Failed to write: {0}")]
    Write(#[from] io::Error),
}

pub fn save_tensors_into_safetensors(
    tensors: HashMap<String, Tensor>,
    dir: PathBuf,
) -> Result<Vec<PathBuf>, SaveSafetensorsError> {
    if tensors.is_empty() {
        return Err(SaveSafetensorsError::NoTensors);
    }
    std::fs::create_dir_all(dir.clone())
        .map_err(|e| SaveSafetensorsError::CreateDir(dir.clone(), e))?;
    let mut file_parts = vec![FilePart::default()];
    let mut tensors = tensors.into_iter().collect::<Vec<_>>();
    tensors.sort_by(|a, b| a.0.cmp(&b.0)); // sort so we have stable ordering for chunking
    for (name, tensor) in tensors {
        let size = tensor.numel() * tensor.kind().elt_size_in_bytes();
        if size > MAX_SAFETENSOR_PART_SIZE {
            return Err(SaveSafetensorsError::TensorTooBig { name, size });
        }
        if size + file_parts.last().unwrap().size > MAX_SAFETENSOR_PART_SIZE {
            file_parts.push(FilePart::default());
        }
        let last_part = file_parts.last_mut().unwrap();
        last_part.tensors.push((name, tensor));
        last_part.size += size;
    }
    if file_parts.len() == 1 {
        let path = dir.join("model.safetensors");
        let metadata = HashMap::from([("format".to_string(), "pt".to_string())]);
        Tensor::write_safetensors(&file_parts[0].tensors, path.clone(), &Some(metadata))?;
        Ok(vec![path])
    } else {
        let len = file_parts.len();
        let mut safetensors_index = json!({
            "metadata": {
                "total_size": file_parts.iter().fold(0, |acc, ele| acc + ele.size)
            },
            "weight_map": serde_json::Map::new(),
        });
        let paths: Result<Vec<PathBuf>, _> = file_parts
            .into_iter()
            .enumerate()
            .map(|(index, part)| {
                let filename = format!("model-{:05}-of-{:05}.safetensors", index + 1, len);
                let path = dir.join(filename.clone());
                safetensors_index
                    .get_mut("weight_map")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .append(&mut serde_json::Map::from_iter(part.tensors.iter().map(
                        |(name, _)| (name.clone(), serde_json::Value::String(filename.clone())),
                    )));
                std::thread::spawn(move || {
                    let metadata = HashMap::from([("format".to_string(), "pt".to_string())]);
                    Tensor::write_safetensors(&part.tensors, path.clone(), &Some(metadata))
                        .and(Ok(path))
                })
            })
            .map(|future| future.join().unwrap())
            .collect();
        let mut paths = paths?;
        let safetensors_index_path = dir.join("model.safetensors.index.json");
        paths.push(safetensors_index_path.clone());
        std::fs::write(safetensors_index_path, safetensors_index.to_string())?;
        Ok(paths)
    }
}
