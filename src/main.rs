use std::{
    borrow::Cow,
    error::Error,
    fmt::{self, Display},
    fs::{self, OpenOptions},
    io::{Cursor, Read as _},
    path::PathBuf,
    process::ExitCode,
};

use chrono::NaiveDateTime;
use clap::{arg, command, Parser};
use phf::{phf_map, phf_set, Map, Set};
use rawler::{decoders::*, get_decoder, RawFile};
use zips::zip;

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

#[derive(Clone, Debug)]
pub struct ParseError<'a> {
    pub err_ty: ParseErrorType,
    pub original_spec: Cow<'a, str>,
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
            original_spec: Cow::Borrowed(original),
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
    InvalidEscape,
    UnterminatedExpansion,
    InvalidExpansion,
    Unknown,
}

impl<'a> ParseError<'a> {
    fn print_error_details<'b: 'a>(&self, f: &mut fmt::Formatter<'_>, msg: &'a str) -> fmt::Result {
        let (start, width) = (self.start as usize, self.width as usize);

        let padding = format!("{:>1$}", "", start);
        let underline = format!("{:~<1$}", "^", width);

        write!(f, "{msg}\n")?;
        write!(f, "> {}\n", self.original_spec)?;
        write!(f, "> {}{}\n", padding, underline)
    }

    pub fn deep_clone(&self) -> ParseError<'static> {
        ParseError {
            original_spec: Cow::Owned(self.original_spec.to_string()),
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
            &self.original_spec,
        );
        let err_seq = &orig[start..start + width];

        let err_msg = format!(
            "{}: {}",
            match self.err_ty {
                InvalidEscape => "unrecognized escape sequence",
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

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum MetadataKind {
    CameraMake,
    CameraModel,
    CameraShutterSpeed,
    CameraExposureComp,
    CameraISO,
    CameraFlash,
    LensFStop,
    LensMake,
    LensModel,
    LensFocalLength,
    LensFocusDist,
    ImageColorSpace,
    ImageSequenceNumber,
    ImageHeight,
    ImageWidth,
    ImageBitDepth,
}

#[derive(Debug)]
pub enum FmtSpec<'a> {
    Literal(Cow<'a, str>),
    DateTime(Cow<'a, str>),
    Metadata(MetadataKind),
}

#[allow(unused)]
const TIME_FMT_SPECS: Set<u8> = phf_set! {
    b'Y',
    b'y',
    b'm',
    b'M',
    b'd',
    b'D',
    b'S',
    b'H',
};

#[allow(unused)]
#[derive(Debug)]
struct FormatSpecification<'a>(Box<[FmtSpec<'a>]>);

// I have to do this bc nvim is dumb dumb and can't tell that a quoted open squirly brace isn't a
// new code block...
//
// :)))))
const EXPANSION_BRACKETS: (char, char) = ('{', '}');
const OPEN_EXPANSION: char = EXPANSION_BRACKETS.0;
const CLOSE_EXPANSION: char = EXPANSION_BRACKETS.1;

const MD_KIND_MAP: Map<&str, MetadataKind> = const {
    use MetadataKind::*;
    phf_map! {
        "camera.make" => CameraMake,
        "camera.model" => CameraModel,
        "camera.shutter_speed" => CameraShutterSpeed,
        "camera.iso" => CameraISO,
        "camera.exposure_compensation" => CameraExposureComp,
        "lens.make" => LensMake,
        "lens.model" => LensModel,
        "lens.focal_length" => LensFocalLength,
        "lens.focus_distance" => LensFocusDist,
        "lens.fstop" => LensFStop,
        "image.width" => ImageWidth,
        "image.height" => ImageHeight,
        "image.bit_depth" => ImageBitDepth,
        "image.color_space" => ImageColorSpace,
        "image.sequence_number" => ImageSequenceNumber,
    }
};

#[inline]
fn expand(s: &str) -> Option<FmtSpec> {
    Some(FmtSpec::Metadata(MD_KIND_MAP.get(s)?.to_owned()))
}

#[allow(unused_parens)]
fn parse_name_format(fmt: &str) -> Result<FormatSpecification, ParseError> {
    let mut spec = vec![];
    let mut to_parse = fmt;

    #[derive(Debug)]
    enum ScanState {
        Start,
        Literal,
        DateTime,
        ExpansionStart,
        ExpansionBody,
    }

    let mut consumed = 0;
    let mut state = ScanState::Start;

    while to_parse.len() > 0 {
        let mut end = false;
        let split_at = to_parse
            .chars()
            .zip(1..)
            .take_while(|(c, _)| {
                use ScanState::*;
                match (&state, c) {
                    _ if end => false,

                    (Start, sym) => {
                        state = match sym {
                            '%' => DateTime,
                            &OPEN_EXPANSION => ExpansionStart,
                            _ => Literal,
                        };

                        true
                    }

                    (ExpansionStart, sym) => {
                        (state, end) = if sym == &OPEN_EXPANSION {
                            (Literal, true)
                        } else {
                            (ExpansionBody, false)
                        };

                        true
                    }

                    (DateTime, _) | (ExpansionBody, &CLOSE_EXPANSION) => {
                        end = true;
                        true
                    }

                    (Literal, '%' | &OPEN_EXPANSION) => false,

                    _ => true,
                }
            })
            .last()
            .unwrap()
            .1;

        if let Some((s, remainder)) = to_parse.split_at_checked(split_at) {
            to_parse = remainder;

            // catch escaped double left squirly braces, only render one
            if s == &format!("{}{}", &OPEN_EXPANSION, &OPEN_EXPANSION) {
                spec.push(FmtSpec::Literal(Cow::Borrowed(&s[0..1])));
            } else {
                spec.push(match state {
                    ScanState::Literal => FmtSpec::Literal(Cow::Borrowed(s)),
                    ScanState::DateTime => FmtSpec::DateTime(Cow::Borrowed(s)),

                    ScanState::ExpansionBody => {
                        assert!(
                            s.starts_with(OPEN_EXPANSION),
                            "An expansion was interpreted incorrectly: fmt: {}, seq: {}",
                            fmt,
                            s
                        );

                        if s.ends_with(CLOSE_EXPANSION) {
                            expand(&s[1..s.len() - 1]).ok_or(ParseError::invalid_expansion(
                                consumed,
                                s.len(),
                                fmt,
                            ))?
                        } else {
                            return Err(ParseError::unterminated_expansion(consumed, s.len(), fmt));
                        }
                    }

                    _ => unreachable!(),
                });
            }

            consumed += s.len();
        } else {
            dbg!(spec, &state);

            return Err(ParseError::new(
                consumed,
                fmt.len() - consumed,
                fmt,
                ParseErrorType::Unknown,
            ));
        }

        state = ScanState::Start;
    }

    Ok(FormatSpecification(spec.into_boxed_slice()))
}

#[cfg(test)]
mod test_parse {
    use std::borrow::Cow;

    use crate::{FmtSpec, OPEN_EXPANSION};

    use super::parse_name_format;
    #[test]
    fn parses_expansions_and_strftime_ok() {
        assert!(parse_name_format("%Y-%m-%d_{camera.make}").is_ok())
    }

    #[test]
    fn fails_to_parse_incomplete_expansion() {
        // again with the bad bracket parsing
        const BAD_EXPANSION: &str = ["{camera.make", "}"][0];
        assert!(parse_name_format(BAD_EXPANSION).is_err())
    }

    #[test]
    fn escaped_double_squirly_brace_only_prints_one() {
        let escaped = format!("{}{}%Y", &OPEN_EXPANSION, &OPEN_EXPANSION);
        let parsed = parse_name_format(&escaped);
        assert!(parsed.is_ok());
        let parsed = parsed.unwrap().0;
        assert!(parsed.len() == 2);
        assert!(matches!(
            parsed[0], FmtSpec::Literal(ref s) if s.chars().nth(0).unwrap() == OPEN_EXPANSION && s.len() == 1
        ));
        assert!(matches!(parsed[1], FmtSpec::DateTime(..)));
    }
}

macro_rules! lazy_wrap {
    ($closure:expr) => {
        std::cell::LazyCell::<_, Box<dyn FnOnce() -> _>>::new(Box::new($closure))
    };
}

#[allow(unused)]
fn render_filename(
    md: &rawler::decoders::RawMetadata,
    fmt_spec: FormatSpecification,
) -> Option<String> {
    let mut fname_str = String::new();

    let date = lazy_wrap!(|| {
        let date_str = &md.exif.date_time_original.clone().unwrap_or(String::new());
        NaiveDateTime::parse_from_str(date_str, EXIF_DT_FMT).ok()
    });

    for atom in fmt_spec.0 {
        let rendered = match atom {
            FmtSpec::Literal(lit) => lit,

            FmtSpec::DateTime(spec) => {
                if let Some(date) = date.as_ref() {
                    Cow::Owned(date.format(spec.as_ref()).to_string())
                } else {
                    Cow::Borrowed("")
                }
            }

            FmtSpec::Metadata(md_kind) => {
                use MetadataKind::*;
                type CowStr<'a> = Cow<'a, str>;

                match md_kind {
                    CameraMake => CowStr::Borrowed(&md.make),
                    CameraModel => CowStr::Borrowed(&md.model),

                    CameraISO => CowStr::Owned(if let Some(iso) = &md.exif.iso_speed {
                        iso.to_string()
                    } else {
                        String::new()
                    }),

                    CameraShutterSpeed => {
                        CowStr::Owned(if let Some(speed) = &md.exif.shutter_speed_value {
                            speed.to_string().replace("/", "_")
                        } else {
                            String::new()
                        })
                    }

                    LensMake => CowStr::Borrowed(if let Some(make) = &md.exif.lens_make {
                        make.as_str()
                    } else {
                        ""
                    }),

                    LensModel => CowStr::Borrowed(if let Some(model) = &md.exif.lens_model {
                        model.as_str()
                    } else {
                        ""
                    }),

                    LensFocalLength => {
                        CowStr::Owned(if let Some(focal_len) = &md.exif.focal_length {
                            focal_len.to_string().replace("/", "_")
                        } else {
                            String::new()
                        })
                    }

                    _ => CowStr::Borrowed(""),
                }
            }
        };
        fname_str.push_str((rendered).as_ref());
    }

    Some(fname_str)
}

const EXIF_DT_FMT: &str = "%Y:%m:%d %H:%M:%S";

macro_rules! exit {
    ($code:expr) => {
        return Ok(ExitCode::from($code))
    };
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = ImportArgs::parse();
    let (src_path, dst_path, fname_fmt) =
        zip!(args.source_path, args.dest_path, args.filename_format).unwrap();

    // some ghetto logging to start with, chef's kiss if I must say so
    if let Some(err) = match (fs::exists(&src_path), fs::exists(&dst_path)) {
        (Ok(false), _) => Some(format!("invalid source path: {src_path:?}")),
        (_, Ok(false)) => Some(format!("invalid destination path: {dst_path:?}")),
        _ => None,
    } {
        println!("fatal:\n{err}");
        exit!(1);
    }

    #[allow(unused)]
    let fmt_spec = parse_name_format(&fname_fmt).or_else(|err| Err(err.deep_clone()))?;

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

    for (path, _) in to_convert.iter().zip(1..) {
        let image_file = OpenOptions::new().read(true).write(false).open(path)?;
        let mut raw_file = RawFile::new(path, image_file);
        let decoder = get_decoder(&mut raw_file)?;
        const DECODE_PARAMS: RawDecodeParams = RawDecodeParams { image_index: 0 };
        #[allow(unused)]
        let md = decoder.raw_metadata(&mut raw_file, DECODE_PARAMS)?;

        //let file_name = if let Some(fmt_str) = args.filename_format {
        //    render_filename(&md, parse_name_format(&fmt_str).unwrap())?;
        //} else {
        //    path.file_name()
        //};
    }

    exit!(0)
}
