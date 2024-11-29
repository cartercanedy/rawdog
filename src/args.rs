use rawler::decoders::supported_extensions;
use rayon::iter::{IntoParallelIterator as _, ParallelIterator as _};
use std::path::PathBuf;
use tokio::fs;

use clap::{
    arg,
    builder::{
        styling::{AnsiColor, Color, Style},
        IntoResettable, Styles,
    },
    command, value_parser, ArgAction, Args, Parser,
};

use smlog::warn;
use tokio_stream::{wrappers::ReadDirStream, StreamExt as _};

use crate::{error::AppError, map_err, RawbitResult};

fn n_threads() -> usize {
    std::thread::available_parallelism().unwrap().get()
}

macro_rules! style {
    ($style:expr) => {
        Style::new().fg_color(Some(Color::Ansi($style)))
    };
}

const fn cli_style() -> Styles {
    Styles::styled()
        .header(style!(AnsiColor::Yellow))
        .error(style!(AnsiColor::Red))
        .literal(style!(AnsiColor::Cyan))
        .invalid(style!(AnsiColor::Red))
        .usage(style!(AnsiColor::White))
        .placeholder(style!(AnsiColor::Cyan))
}

#[derive(Parser)]
#[command(
    version,
    about = "A camera RAW image preprocessor and importer",
    long_about = None,
    trailing_var_arg = true,
    styles = cli_style(),
    next_line_help = true,
    color = clap::ColorChoice::Always
)]
pub struct ImportArgs {
    #[command(flatten)]
    pub source: ImageSource,

    #[arg(
        short = 'o',
        long = "out-dir",
        value_name = "DIR",
        help = "directory to write converted DNGs"
    )]
    pub dst_dir: PathBuf,

    #[arg(
        short = 'F',
        long = "format",
        value_name = "FORMAT",
        value_parser = value_parser!(String).into_resettable(),
        help = "filename format of converted DNGs; see https://docs.rs/rawbit for info on syntax"
    )]
    pub fmt_str: Option<String>,

    #[arg(
        short,
        long,
        value_name = "ARTIST",
        value_parser = value_parser!(String).into_resettable(),
        help = "value of the \"artist\" field in converted DNGs"
    )]
    pub artist: Option<String>,

    #[arg(
        long = "embed-original",
        default_value_t = false,
        help = "embed the original raw image in the converted DNG\nNOTE: conversion may take considerably longer"
    )]
    pub embed: bool,

    #[arg(
        short,
        long,
        default_value_t = false,
        help = "overwrite existing files, if they exist"
    )]
    pub force: bool,

    #[arg(
        short = 'j',
        long,
        value_name = "N",
        default_value_t = n_threads(),
        help = "number of threads to use while processing input images, defaults to number of CPUs"
    )]
    pub n_threads: usize,

    #[command(flatten)]
    pub log_config: LogConfig,
}

#[derive(Args)]
#[group(multiple = false)]
pub struct LogConfig {
    #[arg(
        short,
        long,
        help = "quiet output, only emit critical errors",
        trailing_var_arg = false
    )]
    pub quiet: bool,

    #[arg(
        short,
        action = ArgAction::Count,
        help = "increase log verbosity; specify multiple times to increase verbosity"
    )]
    pub verbose_logs: u8,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct ImageSource {
    #[arg(
        short = 'i',
        long = "in-dir",
        value_name = "DIR",
        value_parser = value_parser!(PathBuf).into_resettable(),
        help = "directory containing raw files to convert"
    )]
    pub src_dir: Option<PathBuf>,

    #[arg(
        help = "individual files to convert",
        trailing_var_arg = true,
        value_parser = value_parser!(PathBuf).into_resettable()
    )]
    pub files: Option<Vec<PathBuf>>,
}

impl ImageSource {
    pub async fn get_ingest_items(self) -> RawbitResult<Vec<PathBuf>> {
        if let Some(ref dir) = self.src_dir {
            if !dir.exists() || !dir.is_dir() {
                Err(AppError::DirNotFound(
                    "source directory doesn't exist".into(),
                    dir.to_path_buf(),
                ))
            } else {
                let dir_stat = map_err!(
                    fs::read_dir(dir).await,
                    AppError::Io,
                    format!("couldn't stat directory: {}", dir.display()),
                )?;

                let paths = ReadDirStream::new(dir_stat)
                    .filter_map(|entry| entry.ok().map(|e| e.path()))
                    .collect::<Vec<_>>()
                    .await;

                Ok(paths)
            }
        } else {
            let files = self
                .files
                .expect("expected directory or filepath(s), got neither")
                .into_par_iter()
                .filter_map(|input_path| {
                    if !input_path.is_file() {
                        warn!("Ignoring {}: not a file", input_path.display());
                        None
                    } else {
                        let ext = input_path
                            .extension()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();

                        if supported_extensions().contains(&ext.as_ref()) {
                            Some(input_path)
                        } else {
                            None
                        }
                    }
                })
                .collect::<Vec<_>>();

            Ok(files)
        }
    }
}
