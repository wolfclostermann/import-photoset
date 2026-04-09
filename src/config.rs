use std::path::PathBuf;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub output_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().expect("cannot find home directory");
        Self {
            output_dir: home.join("Pictures/Photosets"),
        }
    }
}

impl Config {
    pub fn photoset_path(&self, date: &NaiveDate) -> PathBuf {
        self.output_dir
            .join(date.format("%Y").to_string())
            .join(date.format("%Y-%m-%d").to_string())
    }
}
