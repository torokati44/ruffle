use jiff::Timestamp;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Default)]
pub enum FilenamePattern {
    #[default]
    SingleFile,
    WithTimestamp,
}

impl FromStr for FilenamePattern {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "single_file" => Ok(FilenamePattern::SingleFile),
            "with_timestamp" => Ok(FilenamePattern::WithTimestamp),
            _ => Err(()),
        }
    }
}

impl FilenamePattern {
    pub fn create_path(self, directory: &Path) -> PathBuf {
        match self {
            FilenamePattern::SingleFile => directory.join("ruffle.log"),
            FilenamePattern::WithTimestamp => directory.join(
                Timestamp::now()
                    .strftime("ruffle_%F_%H-%M-%S.log")
                    .to_string(),
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FilenamePattern::SingleFile => "single_file",
            FilenamePattern::WithTimestamp => "with_timestamp",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FilenamePattern;
    use std::path::Path;

    #[test]
    fn timestamped_log_file_name() {
        let path = FilenamePattern::WithTimestamp.create_path(Path::new("logs"));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("Log path should have a UTF-8 file name");
        // e.g. "ruffle_2024-07-15_16-24-59.log"
        assert_eq!(name.len(), "ruffle_2024-07-15_16-24-59.log".len());
        assert!(name.starts_with("ruffle_") && name.ends_with(".log"));
    }
}
