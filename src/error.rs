use std::{
    error::Error,
    fmt::{self, Display},
    io,
    path::PathBuf,
};

use rawler::RawlerError;

#[derive(Clone, Debug)]
pub struct ParseError {
    pub err_ty: ParseErrorType,
    pub original: String,
    pub start: u16,
    pub width: u16,
}

impl ParseError {
    pub fn new<S: TryInto<u16>, W: TryInto<u16>>(
        start: S,
        width: W,
        original: &str,
        kind: ParseErrorType,
    ) -> Self {
        Self {
            original: original.to_string(),
            start: start.try_into().ok().unwrap(),
            width: width.try_into().ok().unwrap(),
            err_ty: kind,
        }
    }

    pub fn unterminated_expansion<S: TryInto<u16>, W: TryInto<u16>>(
        start: S,
        width: W,
        original: &str,
    ) -> Self {
        Self::new(
            start,
            width,
            original,
            ParseErrorType::UnterminatedExpansion,
        )
    }

    pub fn invalid_expansion<S: TryInto<u16>, W: TryInto<u16>>(
        start: S,
        width: W,
        original: &str,
    ) -> Self {
        Self::new(start, width, original, ParseErrorType::InvalidExpansion)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ParseErrorType {
    UnterminatedExpansion,
    InvalidExpansion,
    Unknown,
}

impl ParseError {
    fn print_error_details(&self, f: &mut fmt::Formatter<'_>, msg: &str) -> fmt::Result {
        let (start, width) = (self.start as usize, self.width as usize);

        let padding = format!("{:>1$}", "", start);
        let underline = format!("{:~<1$}", "^", width);

        write!(f, "{msg}\n")?;
        write!(f, "> {}\n", self.original)?;
        write!(f, "> {}{}\n", padding, underline)
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ParseErrorType::*;

        let (start, width, orig) = (self.start as usize, self.width as usize, &self.original);

        let err_seq = &orig[start..start + width];

        let err_msg = format!(
            "{}: {}",
            match self.err_ty {
                UnterminatedExpansion => "unterminated variable expansion",
                InvalidExpansion => "invalid variable expansion",
                Unknown => "unknown error",
            },
            err_seq
        );

        self.print_error_details(f, err_msg.as_str())
    }
}

impl Error for ParseError {}

pub enum ConvertError {
    ImgOp(String, RawlerError),
    Io(String, io::Error),
    AlreadyExists(String),
    #[allow(unused)]
    Other(String, Box<dyn Error + Send>),
}

#[derive(Debug)]
pub enum AppError {
    FmtStrParse(ParseError),
    Io(String, io::Error),
    DirNotFound(String, PathBuf),
    AlreadyExists(String, PathBuf),
    #[allow(unused)]
    Other(String, Box<dyn Error + Send>),
}
