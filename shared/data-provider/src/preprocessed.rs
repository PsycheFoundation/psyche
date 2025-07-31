use crate::LengthKnownDataProvider;
use crate::{
    Dataset, Field, Row, Split, TokenizedData, TokenizedDataProvider,
    file_extensions::PARQUET_EXTENSION,
};
use parquet::file::reader::FileReader;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use anyhow::{Result, anyhow, bail};
use parquet::record::RowAccessor;
use psyche_core::{BatchId, Shuffle};
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

fn field_to_int(field: &Field) -> i32 {
    match field {
        Field::Bool(x) => match x {
            true => 1,
            false => 0,
        },
        Field::Byte(x) => *x as i32,
        Field::Short(x) => *x as i32,
        Field::Int(x) => *x as i32,
        Field::Long(x) => *x as i32,
        Field::UByte(x) => *x as i32,
        Field::UShort(x) => *x as i32,
        Field::UInt(x) => *x as i32,
        Field::ULong(x) => *x as i32,
        _ => panic!("Non-integer data type"),
    }
}

fn list_to_vec(row: &Row, column: usize, required_len: Option<usize>) -> Result<Vec<i32>> {
    let ret: Vec<i32> = row
        .get_list(column)?
        .elements()
        .iter()
        .map(field_to_int)
        .collect();
    if let Some(required_len) = required_len {
        let len = ret.len();
        if len != required_len {
            let column_name = row.get_column_iter().nth(column).unwrap().0;
            bail!("`{column_name}` has length {len} instead of {required_len}");
        }
    }
    Ok(ret)
}

pub struct PreprocessedDataProvider {
    data: Vec<TokenizedData>,
}

impl PreprocessedDataProvider {
    pub fn new_from_directory(
        dir: impl AsRef<std::path::Path>,
        num_tokens_per_sequence: usize,
        shuffle: Shuffle,
        split: Option<Split>,
        subset: Option<String>,
    ) -> Result<Self> {
        let dir = std::fs::canonicalize(&dir)
            .map_err(|e| anyhow!("Failed to open data directory {:?}: {e}", dir.as_ref()))?;

        let mut files = vec![];
        for file in std::fs::read_dir(&dir)
            .map_err(|e| anyhow!("couldn't load training data from {}: {e}", dir.display()))?
            .flatten()
        {
            let file = file.path();
            if let Some(extension) = file.extension().and_then(|s| s.to_str()) {
                if extension == PARQUET_EXTENSION {
                    files.push(file);
                }
            }
        }
        if files.is_empty() {
            bail!("No training data files in directory {:?}", dir);
        }

        let dataset = Dataset::load_dataset(&files, split, subset)?;
        let inputs_column = match dataset.get_column_id("inputs") {
            Some(x) => x,
            None => bail!("Dataset does not have `inputs` column"),
        };
        let labels_column = dataset.get_column_id("labels");
        let position_ids_column = dataset.get_column_id("position_ids");
        let sequence_lengths_column = dataset.get_column_id("sequence_lengths");

        let data: Result<Vec<TokenizedData>, _> = dataset
            .files()
            .par_iter()
            .flat_map(|file| -> Vec<anyhow::Result<TokenizedData>> {
                match file.get_row_iter(None) {
                    Ok(rows) => rows
                        .map(|row| {
                            if let Ok(row) = row {
                                let input_ids = list_to_vec(
                                    &row,
                                    inputs_column,
                                    Some(num_tokens_per_sequence),
                                )?;
                                let labels = match labels_column {
                                    Some(column) => Some(list_to_vec(
                                        &row,
                                        column,
                                        Some(num_tokens_per_sequence),
                                    )?),
                                    None => None,
                                };
                                let position_ids = match position_ids_column {
                                    Some(column) => Some(list_to_vec(
                                        &row,
                                        column,
                                        Some(num_tokens_per_sequence),
                                    )?),
                                    None => None,
                                };
                                let sequence_lengths = match sequence_lengths_column {
                                    Some(column) => Some(list_to_vec(&row, column, None)?),
                                    None => None,
                                };
                                Ok(TokenizedData {
                                    input_ids,
                                    labels,
                                    position_ids,
                                    sequence_lengths,
                                })
                            } else {
                                Err(anyhow::anyhow!("Invalid row"))
                            }
                        })
                        .collect(),
                    Err(e) => vec![Err(anyhow::anyhow!("Error reading parquet file {e}"))],
                }
            })
            .collect();

        let mut data = data?;

        if let Shuffle::Seeded(random_seed) = shuffle {
            data.shuffle(&mut ChaCha8Rng::from_seed(random_seed));
        }

        Ok(Self { data })
    }
}

impl TokenizedDataProvider for PreprocessedDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        let start = data_ids.0.start as usize;
        let end = data_ids.0.end as usize;
        if start >= self.data.len() || end >= self.data.len() {
            bail!("{data_ids} out of range");
        }
        Ok(self.data[start..=end].to_vec())
    }
}

impl LengthKnownDataProvider for PreprocessedDataProvider {
    fn num_sequences(&self) -> usize {
        self.data.len()
    }
}
