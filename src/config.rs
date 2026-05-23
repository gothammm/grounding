use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub index_dir: PathBuf,
}

impl Config {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir: PathBuf = data_dir.into();
        let index_dir = data_dir.join("index");
        Self { data_dir, index_dir }
    }
}
