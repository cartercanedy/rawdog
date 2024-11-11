mod error;
mod parse;

use std::{
    borrow::Cow,
    error::Error,
    fs::{self, OpenOptions},
    path::PathBuf,
    process::ExitCode,
};

use chrono::NaiveDateTime;
use clap::{arg, command, Parser};
use parse::{parse_name_format, FmtItem, MetadataKind};
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

macro_rules! lazy_wrap {
    ($closure:expr) => {
        std::cell::LazyCell::<_, Box<dyn FnOnce() -> _>>::new(Box::new($closure))
    };
}

#[allow(unused)]
fn render_filename(
    md: &rawler::decoders::RawMetadata,
    items: Box<[FmtItem]>,
) -> Option<String> {
    let mut fname_str = String::new();

    let date = lazy_wrap!(|| {
        let date_str = &md.exif.date_time_original.clone().unwrap_or(String::new());
        NaiveDateTime::parse_from_str(date_str, EXIF_DT_FMT).ok()
    });

    for atom in items {
        let rendered = match atom {
            FmtItem::Literal(lit) => lit,

            FmtItem::DateTime(item) => {
                if let Some(date) = date.as_ref() {
                    Cow::Owned(date.format(item.as_ref()).to_string())
                } else {
                    Cow::Borrowed("")
                }
            }

            FmtItem::Metadata(md_kind) => {
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

                    LensMake => CowStr::Borrowed(if let Some(ref make) = &md.exif.lens_make {
                        make
                    } else {
                        ""
                    }),

                    LensModel => CowStr::Borrowed(if let Some(ref model) = &md.exif.lens_model {
                        model
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
    let fmt_items = parse_name_format(&fname_fmt).or_else(|err| Err(err.deep_clone()))?;

    let to_convert = fs::read_dir(&src_path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path
                .extension()
                .map_or(Cow::default(), |s| s.to_string_lossy());
            if supported_extensions().contains(&ext.as_ref()) {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    for (path, _) in to_convert.iter().zip(1..) {
        let mut raw_file =
            RawFile::new(path, OpenOptions::new().read(true).write(false).open(path)?);

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
