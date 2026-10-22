//! Mondes ouverts récemment, mémorisés entre deux lancements.

use std::path::{Path, PathBuf};

const MAX: usize = 8;

pub struct Recent {
    pub paths: Vec<PathBuf>,
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("odasio").join("recent.txt"))
}

impl Recent {
    pub fn load() -> Self {
        let paths = file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|text| {
                text.lines()
                    .map(PathBuf::from)
                    .filter(|p| p.join("level.dat").is_file())
                    .collect()
            })
            .unwrap_or_default();
        Self { paths }
    }

    pub fn push(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(MAX);
        if let Some(f) = file() {
            let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")));
            let text: Vec<String> = self
                .paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let _ = std::fs::write(f, text.join("\n"));
        }
    }
}
