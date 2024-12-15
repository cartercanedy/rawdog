use crate::parse;
use std::{
    error::Error,
    fmt::{self, Display},
    io,
    path::PathBuf,
};

#[derive(Debug)]
pub enum AppError {
    FmtStrParse(parse::Error),
    Io(String, io::Error),
    DirNotFound(String, PathBuf),
    AlreadyExists(String, PathBuf),
    #[allow(unused)]
    Other(String, Box<dyn Error + Send + Sync>),
}

impl Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Error for AppError {}

macro_rules! map_err {
    ($r:expr, $err_t:path, $($s:expr),+ $(,)?) => {
        $r.map_err(|e| ($err_t)($($s.into()),+, e))
    };
}

pub(crate) use map_err;

pub type RawbitResult<T> = std::result::Result<T, AppError>;
