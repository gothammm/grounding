use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub index_dir: PathBuf,
}

impl Config {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir: PathBuf = data_dir.into();
        let data_dir = shellexpand::tilde(&data_dir.to_string_lossy()).to_string();
        let mut data_dir = PathBuf::from(&data_dir);
        if data_dir.is_relative() {
            data_dir = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(&data_dir);
        }
        let index_dir = data_dir.join("index");
        Self { data_dir, index_dir }
    }
}
