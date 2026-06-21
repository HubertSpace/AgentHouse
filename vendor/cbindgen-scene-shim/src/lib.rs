use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Config {
    pub include_guard: Option<String>,
    pub language: Language,
    pub no_includes: bool,
    pub export: ExportConfig,
    pub enumeration: EnumConfig,
}

pub enum Language {
    C,
}

impl Default for Language {
    fn default() -> Self {
        Self::C
    }
}

#[derive(Default)]
pub struct ExportConfig {
    pub include: Vec<String>,
}

#[derive(Default)]
pub struct EnumConfig {
    pub prefix_with_name: bool,
}

#[derive(Default)]
pub struct Builder {
    sources: Vec<PathBuf>,
    config: Config,
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_src(mut self, source: impl AsRef<Path>) -> Self {
        self.sources.push(source.as_ref().to_path_buf());
        self
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn generate(self) -> Result<Bindings, Error> {
        let _ = self.sources;
        let _ = self.config;
        Ok(Bindings)
    }
}

pub struct Bindings;

impl Bindings {
    pub fn write_to_file(&self, path: impl AsRef<Path>) {
        std::fs::write(path, include_str!("scene.h")).expect("failed to write scene header");
    }
}

#[derive(Debug)]
pub struct Error;
