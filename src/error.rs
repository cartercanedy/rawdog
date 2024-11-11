use std::{
    error::Error,
    fmt::{self, Display},
    borrow::Cow
};

#[derive(Clone, Debug)]
pub struct ParseError<'a> {
    pub err_ty: ParseErrorType,
    pub original: Cow<'a, str>,
    pub start: u16,
    pub width: u16,
}

impl<'a> ParseError<'a> {
    pub fn new<S: TryInto<u16>, W: TryInto<u16>>(
        start: S,
        width: W,
        original: &'a str,
        kind: ParseErrorType,
    ) -> Self {
        Self {
            original: Cow::Borrowed(original),
            start: start.try_into().ok().unwrap(),
            width: width.try_into().ok().unwrap(),
            err_ty: kind,
        }
    }

    pub fn unterminated_expansion<S: TryInto<u16>, W: TryInto<u16>>(
        start: S,
        width: W,
        original: &'a str,
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
        original: &'a str,
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

impl<'a> ParseError<'a> {
    fn print_error_details<'b: 'a>(
        &self,
        f: &mut fmt::Formatter<'_>,
        msg: &'a str,
    ) -> fmt::Result {
        let (start, width) = (self.start as usize, self.width as usize);

        let padding = format!("{:>1$}", "", start);
        let underline = format!("{:~<1$}", "^", width);

        write!(f, "{msg}\n")?;
        write!(f, "> {}\n", self.original)?;
        write!(f, "> {}{}\n", padding, underline)
    }

    pub fn deep_clone(&self) -> ParseError<'static> {
        ParseError {
            original: Cow::Owned(self.original.to_string()),
            ..*self
        }
    }
}

impl<'a> Display for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ParseErrorType::*;

        let (start, width, orig) = (
            self.start as usize,
            self.width as usize,
            &self.original,
        );

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

impl<'a> Error for ParseError<'a> {}

