use std::{
    borrow::Cow,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    fs::{self, OpenOptions},
    path::PathBuf,
    process::ExitCode,
};

use clap::{arg, command, Parser};
use zips::zip;

use rawler::{decoders::*, get_decoder, RawFile};

#[derive(Debug)]
pub struct ParserErrorContext<'a> {
    pub original_spec: &'a str,
    pub start: u16,
    pub width: u16,
}

macro_rules! code {
    ($code:expr) => {
        ExitCode::from($code)
    };
}

#[derive(Debug)]
pub enum FormatParseError<'a> {
    InvalidEscape(ParserErrorContext<'a>),
}

impl<'a> Error for FormatParseError<'a> {}

impl<'a> FormatParseError<'a> {
    fn print_error_details<'b: 'a>(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        msg: &'a str,
        ctx: &'b ParserErrorContext,
    ) -> std::fmt::Result {
        let underscore_fmt = format!("{:~<1$}", "^", ctx.width as usize);
        write!(f, "{msg}\n")?;
        write!(f, "> {}\n", ctx.original_spec)?;
        write!(
            f,
            "> {}{}\n",
            format!("{:>1$}", "", ctx.start as usize),
            underscore_fmt
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{FormatParseError::InvalidEscape, ParserErrorContext};

    #[test]
    fn print_error() {
        let err = InvalidEscape(ParserErrorContext {
            original_spec: "%M%D_TestString",
            start: 4,
            width: 2,
        });

        println!("{err}");
    }
}

impl<'a> Display for FormatParseError<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (ctx, err_msg) = match self {
            Self::InvalidEscape(ctx) => {
                let (start, width) = (ctx.start as usize, ctx.width as usize);
                let err_seq = &ctx.original_spec[start..start + width];
                (ctx, format!("fatal: unrecognized escape sequence: \"{}\"", err_seq))
            },
        };

        self.print_error_details(f, err_msg.as_str(), ctx)
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct ImportArgs {
    #[arg(short, long, value_name = "DIR")]
    source_path: Option<PathBuf>,
    #[arg(short, long, value_name = "DIR")]
    dest_path: Option<PathBuf>,
    #[arg(short, long, value_name = "FORMAT_STRING")]
    filename_format: Option<String>,
}

pub enum DateSpecifier {
    Year,
    Month,
    Day,
}

pub enum TimeSpecifier {
    Hour,
    Minute,
    Second,
}

pub enum FormatSpecifier<'a> {
    Literal(Cow<'a, str>),
    Date(DateSpecifier),
    Time(TimeSpecifier),
    SequenceNumber,
}

type FmtSpec<'a> = Box<[FormatSpecifier<'a>]>;

fn parse_name_format<'a, 'b: 'a>(fmt: &'a str) -> Result<FmtSpec, FormatParseError<'b>> {
    let _ = fmt;
    todo!("impl filename format parsing")

    // use FormatSpecifier::*;
    // use DateSpecifier::*;

    // Ok(Box::new([Date(Year), Date(Month), Date(Day), Literal(Cow::Borrowed("_")), SequenceNumber]))
}

fn render_filename(metadata: rawler::exif::Exif, fmt_spec: Box<[FormatSpecifier]>) -> String {
    let _ = (metadata, fmt_spec);
    todo!("impl filename rendering")
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = ImportArgs::parse();
    let (src_path, dst_path, fname_fmt) =
        zip!(args.source_path, args.dest_path, args.filename_format).unwrap();

    if let Some(err) = match (fs::exists(&src_path), fs::exists(&dst_path)) {
        (Ok(false), _) => Some(format!("invalid source path: {src_path:?}")),
        (_, Ok(false)) => Some(format!("invalid destination path: {dst_path:?}")),
        _ => None,
    } {
        println!("fatal:\n{err}");
        return Ok(code!(1));
    }

    let fmt_spec = parse_name_format(&fname_fmt)?;

    let to_convert = fs::read_dir(&src_path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_string_lossy();
            if rawler::decoders::supported_extensions().contains(&ext.as_ref()) {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    for (path, i) in to_convert.iter().zip(1..) {
        let mut image_file = OpenOptions::new().read(true).write(false).open(path)?;
        let mut raw_file = RawFile::new(path, image_file);
        let decoder = get_decoder(&mut raw_file)?;
        const DECODE_PARAMS = RawDecodeParams { image_index: 0 };
        let md = decoder.raw_metadata(&mut raw_file, DECODE_PARAMS);
    }

    Ok(code!(0))
}
