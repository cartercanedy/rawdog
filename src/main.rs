mod error;
mod parse;

use std::{
    borrow::Cow, fmt::Display, fs::{self, OpenOptions}, io::{self, Cursor, Seek as _, SeekFrom}, path::PathBuf, process::ExitCode
};

use chrono::NaiveDateTime;
use clap::{arg, command, Parser};
use error::{AppError, ConvertError};
use parse::{parse_name_format, FmtItem, MetadataKind};
use rawler::{
    decoders::*,
    dng::convert::{convert_raw_stream, ConvertParams},
    get_decoder, RawFile,
};
use rayon::iter::{IntoParallelIterator as _, ParallelIterator};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct ImportArgs {
    #[arg(short, long, value_name = "DIR")]
    source_path: PathBuf,
    #[arg(short, long, value_name = "DIR")]
    dest_path: PathBuf,
    #[arg(short = 'F', long, value_name = "FORMAT_STRING")]
    filename_format: Option<String>,
    #[arg(short, long, default_value_t = false)]
    force: bool
}

macro_rules! lazy_wrap {
    ($closure:expr) => {
        std::cell::LazyCell::<_, Box<dyn FnOnce() -> _>>::new(Box::new($closure))
    };
}

type Result<T> = std::result::Result<T, AppError>;

impl MetadataKind {
    pub fn expand_with_metadata<'a>(&self, md: &'a RawMetadata, orig_fname: &str) -> Cow<'a, str> {
        use MetadataKind::*;
        type CowStr<'a> = Cow<'a, str>;

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

            LensFocalLength => CowStr::Owned(if let Some(focal_len) = &md.exif.focal_length {
                focal_len.to_string().replace("/", "_")
            } else {
                String::new()
            }),

            ImageOriginalFilename => CowStr::Owned(orig_fname.to_string()),

            _ => CowStr::Borrowed(""),
        }
    }
}

fn render_filename(orig_fname: &str, md: &rawler::decoders::RawMetadata, items: &[FmtItem]) -> String {
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

            FmtItem::Metadata(md_kind) => md_kind.expand_with_metadata(md, orig_fname),
        };

        fname_str.push_str((rendered).as_ref());
    }

    fname_str
}

const EXIF_DT_FMT: &str = "%Y:%m:%d %H:%M:%S";

macro_rules! exit {
    ($c:expr) => {
        std::process::ExitCode::from($c)
    };
}

fn main() -> ExitCode {
    match run() {
        Err(err) => {
            use AppError::*;

            #[allow(unused)]
            let (s, e, code): (Option<String>, &dyn Display, u8) = match err {
                FmtStrParse(ref e) => (None, e, 1),
                Io(s, ref e) => (Some(s), e, 2),
                DirNotFound(s, ref e) => (Some(s), &e.to_string_lossy(), 3),
                AlreadyExists(s, ref e) => (Some(s), &e.to_string_lossy(), 4),
                Other(s, ref e) => (Some(s), e, 5)
            };

            println!("{e}");
            exit!(code)
        }

        Ok(_) => exit!(0),
    }
}

macro_rules! map_err {
    ($r:expr, $s:literal, $($err_t:tt)+) => {
        $r.map_err(
            |e| ($($err_t)+)($s.into(), e)
        )
    };
}

fn run() -> Result<()> {
    let ImportArgs {
        source_path: src_path,
        dest_path: dst_path,
        filename_format: fmt,
        force
    } = ImportArgs::parse();

    if !src_path.exists() {
        Err(AppError::DirNotFound(
            "source path doesn't exist".into(),
            (&src_path).into(),
        ))
    } else if dst_path.exists() {
        if !dst_path.is_dir() {
            Err(AppError::AlreadyExists(
                "destination path exists and isn't a directory".into(),
                (&dst_path).into(),
            ))
        } else {
            Ok(())
        }
    } else {
        map_err!(
            fs::create_dir_all(&dst_path),
            "couldn't create destination directory",
            AppError::Io
        )
    }?;

    let fmt_items = if let Some(ref fmt) = fmt {
        Some(parse_name_format(fmt)?)
    } else {
        None
    };

    let dir_entries = map_err!(
        fs::read_dir(&src_path),
        "source directory unavailable",
        AppError::Io
    )?;

    let to_convert = dir_entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let ext = path.extension()?.to_string_lossy();

            if supported_extensions().contains(&ext.as_ref()) {
                Some(path)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    type ConvertResult = std::result::Result<(), ConvertError>;
    to_convert.into_par_iter().map(|path| -> ConvertResult {
        let path_str = path.to_string_lossy();

        let f = map_err!(
            OpenOptions::new().read(true).write(false).open(&path),
            "can't open file",
            ConvertError::Io
        )?;

        let mut raw_file = RawFile::new(&path, f);

        let decoder = map_err!(
            get_decoder(&mut raw_file),
            "no compatible RAW image decoder available",
            ConvertError::ImgOp
        )?;

        let md = map_err!(
            decoder.raw_metadata(&mut raw_file, Default::default()),
            "couldn't extract image metadata",
            ConvertError::ImgOp
        )?;

        let mut raw_output_stream = Cursor::new(vec![]);

        let cvt_params = ConvertParams {
            preview: true,
            embedded: false,
            software: "rawdog".to_string(),
            artist: md.exif.artist.clone(),
            ..Default::default()
        };

        raw_file
            .file
            .seek(SeekFrom::Start(0))
            .expect("IO seeking error");

        map_err!(
            convert_raw_stream(
                raw_file.file,
                &mut raw_output_stream,
                &path_str,
                &cvt_params,
            ),
            "couldn't convert image to DNG",
            ConvertError::ImgOp
        )?;

        raw_output_stream
            .seek(SeekFrom::Start(0))
            .expect("IO seeking error");

        let orig_fname = path
            .file_stem()
            .expect(&format!("couldn't deduce the filename from {}", &path_str))
            .to_string_lossy();

        let out_path = dst_path.join(
            match fmt_items {
                Some(ref items) => render_filename(orig_fname.as_ref(), &md, items),
                None => orig_fname.into()
            } + ".dng",
        );

        if !force && out_path.exists() {
            return Err(ConvertError::AlreadyExists(
                "won't overwrite existing file".into(),
                out_path
            ))
        }

        let mut out_file = map_err!(
            OpenOptions::new()
                .write(true)
                .create(true)
                .open(&out_path),
            "couldn't create output file",
            ConvertError::Io
        )?;

        map_err!(
            io::copy(&mut raw_output_stream, &mut out_file),
            "couldn't write converted DNG to disk",
            ConvertError::Io
        )?;

        Ok(())
    }).collect::<Vec<ConvertResult>>();

    Ok(())
}
