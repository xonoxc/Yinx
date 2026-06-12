use directories::ProjectDirs;
use std::path::PathBuf;

fn project() -> ProjectDirs {
    ProjectDirs::from("com", "", "yinx")
        .expect("failed to determine project directories")
}

pub fn data_dir() -> PathBuf {
    project().data_dir().to_path_buf()
}

pub fn config_dir() -> PathBuf {
    project().config_dir().to_path_buf()
}

pub fn state_dir() -> PathBuf {
    let p = project();
    p.state_dir()
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| p.data_dir().to_path_buf())
}

pub fn cache_dir() -> PathBuf {
    project().cache_dir().to_path_buf()
}
