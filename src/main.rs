mod error;
mod parse;

use std::{
    borrow::Cow, error::Error, fs::{self, OpenOptions}, io::{Cursor, Read, Seek, SeekFrom, Write}, path::PathBuf, process::ExitCode
};

use chrono::NaiveDateTime;
use clap::{arg, command, Parser};
use parse::{parse_name_format, FmtItem, MetadataKind};
use rawler::{decoders::*, dng::convert::{convert_raw_stream, ConvertParams}, get_decoder, RawFile};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct ImportArgs {
    #[arg(short, long, value_name = "DIR")]
    source_path: PathBuf,
    #[arg(short, long, value_name = "DIR")]
    dest_path: PathBuf,
    #[arg(short, long, value_name = "FORMAT_STRING")]
    filename_format: Option<String>,
}

macro_rules! lazy_wrap {
    ($closure:expr) => {
        std::cell::LazyCell::<_, Box<dyn FnOnce() -> _>>::new(Box::new($closure))
    }
}

impl MetadataKind {
    pub fn expand_with_metadata<'a>(&self, md: &'a RawMetadata) -> Cow<'a, str> {
        type CowStr<'a> = Cow<'a, str>;
        use MetadataKind::*;
        match self {
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
}

#[allow(unused)]
fn render_filename(
    md: &rawler::decoders::RawMetadata,
    items: &[FmtItem],
) -> String {
    let mut fname_str = String::new();

    let date = lazy_wrap!(|| {
        let date_str = &md.exif.date_time_original.clone().unwrap_or(String::new());
        NaiveDateTime::parse_from_str(date_str, EXIF_DT_FMT).ok()
    });

    for atom in items {
        let rendered = match atom {
            FmtItem::Literal(lit) => lit.clone(),

            FmtItem::DateTime(item) => {
                if let Some(date) = date.as_ref() {
                    Cow::Owned(date.format(item.as_ref()).to_string())
                } else {
                    Cow::Borrowed("")
                }
            }

            FmtItem::Metadata(md_kind) => md_kind.expand_with_metadata(md),
        };

        fname_str.push_str((rendered).as_ref());
    }

    fname_str
}

const EXIF_DT_FMT: &str = "%Y:%m:%d %H:%M:%S";

macro_rules! exit {
    ($code:expr) => {
        return Ok(ExitCode::from($code))
    };
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let ImportArgs {
        source_path: src_path,
        dest_path: dst_path,
        filename_format: fmt
    } = ImportArgs::parse();

    if !src_path.exists() {
        Err(format!("invalid source path: {src_path:?}"))
    } else if dst_path.exists() {
        if !dst_path.is_dir() {
            Err(format!("destination path exists and isn't a directory: {dst_path:?}"))
        } else {
            Ok(())
        }
    } else {
        fs::create_dir_all(&dst_path)
            .or_else(|e| Err(format!("couldn't create path: {dst_path:?}\n{e}")))
    }?;

    #[allow(unused)]
    let fmt_items = if let Some(ref fmt) = fmt {
        Some(parse_name_format(fmt).or_else(|e| Err(e.deep_clone()))?)
    } else {
        None
    };

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
        let path_str = path.to_string_lossy();

        let mut raw_file =
            RawFile::new(path, OpenOptions::new().read(true).write(false).open(path)?);

        let decoder = get_decoder(&mut raw_file)?;
        const DECODE_PARAMS: RawDecodeParams = RawDecodeParams { image_index: 0 };

        let md = decoder.raw_metadata(&mut raw_file, DECODE_PARAMS)?;

        let mut raw_output_stream = Cursor::new(vec![]);

        let cvt_params = ConvertParams {
            preview: true,
            embedded: true,
            software: "rawdog".to_string(),
            artist: md.exif.artist.clone(),
            ..Default::default()
        };

        raw_file.file.seek(SeekFrom::Start(0));

        convert_raw_stream(raw_file.file, &mut raw_output_stream, &path_str, &cvt_params)
            .or_else(|e| Err(format!("there was an error converting file to dng: {}", e)))?;

        raw_output_stream.seek(SeekFrom::Start(0));

        let out_path = dst_path.join(match fmt_items {
            Some(ref items) => render_filename(&md, items),
            None => {
                path.file_stem()
                    .ok_or(format!(
                        "couldn't strip the file extension: {}",
                        &path_str
                    ))?
                    .to_string_lossy()
                    .to_string()
            }
        } + ".dng");

        let mut buf = vec![];
        raw_output_stream.read_to_end(&mut buf);
        let mut out_file = OpenOptions::new().create_new(true).write(true).open(&out_path)
            .or_else(|e| Err(format!("there was an error while trying to access {}: {}", out_path.to_string_lossy(), e)))?;

        out_file.write_all(&buf[..])
            .or_else(|e| Err(format!("there was an error while trying to write to {}: {}", out_path.to_string_lossy(), e)))?;
    }

    exit!(0)
}
