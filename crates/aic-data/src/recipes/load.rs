use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use crate::recipes::RecipeBook;

pub fn load_recipe_book(path: impl AsRef<Path>) -> Result<RecipeBook, LoadRecipeBookError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadRecipeBookError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| LoadRecipeBookError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug)]
pub enum LoadRecipeBookError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadRecipeBookError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(
                    formatter,
                    "failed to open recipe file '{}': {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "failed to parse recipe file '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for LoadRecipeBookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}
