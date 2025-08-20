use anyhow::{Result, bail};
use parquet::{
    errors::ParquetError,
    file::reader::{FileReader, SerializedFileReader},
    record::reader::RowIter,
};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    fs::File,
    path::{Path, PathBuf},
};

pub type Row = parquet::record::Row;
pub type Field = parquet::record::Field;

const SPLITS: [Split; 4] = [Split::Train, Split::Test, Split::Validation, Split::Dev];

fn looks_like_parquet_file(x: &Path) -> bool {
    if let Some(ext) = x.extension() {
        // if ext.eq_ignore_ascii_case("parquet") {
        //     if let Some(stem) = x.file_stem() {
        //         if let Some(s) = stem.to_str() {
        //             return s.parse::<usize>().is_ok();
        //         }
        //     }
        // }
        return ext.eq_ignore_ascii_case("parquet");
    };
    false
}

#[derive(Debug, Clone, Copy)]
pub enum Split {
    Train,
    Validation,
    Test,
    Dev,
}

impl Display for Split {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Split::Train => "train",
                Split::Validation => "validation",
                Split::Test => "test",
                Split::Dev => "dev",
            }
        )
    }
}

pub struct Dataset {
    files: Vec<SerializedFileReader<File>>,
    split: Split,
    column_ids: HashMap<String, usize>,
    column_types: HashMap<String, Field>,
}

impl Dataset {
    pub fn load_dataset(
        repo_files: &[PathBuf],
        split: Option<Split>,
        subset: Option<String>,
    ) -> Result<Self> {
        let mut split = split;
        let mut to_load: Vec<PathBuf> = Vec::new();
        // parquet converter structure
        for file in repo_files {
            if looks_like_parquet_file(file) {
                let mut path_iter = file.iter().rev().skip(1);
                let parent = path_iter.next();
                let grandparent = path_iter.next();

                match (parent, grandparent) {
                    (Some(split_name), Some(subset_name)) => {
                        let split_str = split_name.to_string_lossy();
                        if let Some(subset_filter) = &subset {
                            if subset_name.to_str().unwrap_or_default() != subset_filter {
                                continue;
                            }
                        }

                        if let Some(actual_split) = split_str.split('-').next() {
                            if split.as_ref().is_some() {
                                if actual_split
                                    .eq_ignore_ascii_case(&split.as_ref().unwrap().to_string())
                                {
                                    to_load.push(file.clone());
                                }
                            } else {
                                for maybe_split in SPLITS {
                                    if actual_split.eq_ignore_ascii_case(&maybe_split.to_string()) {
                                        to_load.push(file.clone());
                                        split = Some(maybe_split);
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    (Some(split_name), _) => {
                        if subset.is_some() {
                            continue;
                        }

                        if split.as_ref().is_some() {
                            if split_name.eq_ignore_ascii_case(split.as_ref().unwrap().to_string())
                            {
                                to_load.push(file.clone());
                            }
                        } else {
                            for maybe_split in SPLITS {
                                if split_name.eq_ignore_ascii_case(maybe_split.to_string()) {
                                    to_load.push(file.clone());
                                    split = Some(maybe_split);
                                    break;
                                }
                            }
                        }
                    }
                    _ => continue,
                }
            }
        }
        if to_load.is_empty() {
            // ad-hoc structure
            for file in repo_files {
                if looks_like_parquet_file(file) {
                    match split {
                        Some(split) => {
                            if file
                                .file_name()
                                .unwrap()
                                .to_string_lossy()
                                .starts_with(&split.to_string())
                            {
                                to_load.push(file.clone());
                            }
                        }
                        None => to_load.push(file.clone()),
                    }
                }
            }
        }
        if to_load.is_empty() {
            bail!("No files in dataset")
        }
        let split = match split {
            Some(split) => split,
            None => {
                bail!("Could not determine split");
            }
        };
        to_load.sort_by(|a, b| a.file_stem().unwrap().cmp(b.file_stem().unwrap()));
        let files: std::io::Result<Vec<File>> = to_load.into_iter().map(File::open).collect();
        let files: Result<Vec<SerializedFileReader<File>>, ParquetError> =
            files?.into_iter().map(SerializedFileReader::new).collect();
        let files = files?;
        if files[0].metadata().file_metadata().num_rows() == 0 {
            bail!("Empty dataset");
        }
        let first_row = files[0]
            .get_row_group(0)
            .unwrap()
            .get_row_iter(None)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        let columns = first_row.get_column_iter().collect::<Vec<_>>();
        let column_ids = HashMap::from_iter(
            columns
                .iter()
                .enumerate()
                .map(|(idx, x)| (x.0.clone(), idx)),
        );
        let column_types =
            HashMap::from_iter(columns.into_iter().map(|x| (x.0.clone(), x.1.clone())));
        Ok(Dataset {
            files,
            split,
            column_ids,
            column_types,
        })
    }

    pub fn num_rows(&self) -> usize {
        self.files
            .iter()
            .fold(0, |acc, x| acc + x.metadata().file_metadata().num_rows()) as usize
    }

    pub fn files(&self) -> &Vec<SerializedFileReader<File>> {
        &self.files
    }

    pub fn split(&self) -> Split {
        self.split
    }

    pub fn iter(&self) -> DatasetIter<'_> {
        let mut files_iter = self.files.iter();
        let row_iter = files_iter.next().unwrap().get_row_iter(None).unwrap();
        DatasetIter {
            files_iter,
            row_iter,
        }
    }

    pub fn columns(&self) -> impl Iterator<Item = (&String, &Field)> {
        self.column_types.iter()
    }

    pub fn get_column_id<T: Into<String>>(&self, name: T) -> Option<usize> {
        self.column_ids.get(&name.into()).copied()
    }
}

pub struct DatasetIter<'a> {
    files_iter: std::slice::Iter<'a, SerializedFileReader<File>>,
    row_iter: RowIter<'a>,
}

impl Iterator for DatasetIter<'_> {
    type Item = Row;

    fn next(&mut self) -> Option<Self::Item> {
        match self.row_iter.next() {
            Some(Ok(item)) => Some(item),
            Some(Err(_)) => None,
            None => match self.files_iter.next() {
                Some(file) => {
                    self.row_iter = file.get_row_iter(None).unwrap();
                    self.next()
                }
                None => None,
            },
        }
    }
}
