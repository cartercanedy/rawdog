// Copyright (c) Carter J. Canedy <cartercanedy42@gmail.com>
// rawbit is free software, distributable under the terms of the MIT license
// See https://raw.githubusercontent.com/cartercanedy/rawbit/refs/heads/master/LICENSE.txt

use std::{fmt::Display, process::ExitCode};

use clap::Parser as _;
use parse::FilenameFormat;
use rawler::dng::{convert::ConvertParams, CropMode, DngCompression};
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use smlog::{debug, error, ignore, log::LevelFilter, warn, Log};
use tokio::{fs, runtime::Builder};

mod args;
mod error;
mod job;
mod parse;

use args::{ImportArgs, LogConfig};
use error::{AppError, ConvertError};
use job::Job;

type RawbitResult<T> = std::result::Result<T, AppError>;

macro_rules! exit {
    ($c:expr) => {
        std::process::ExitCode::from($c)
    };
}

fn main() -> ExitCode {
    let args = ImportArgs::parse();
    let LogConfig {
        quiet,
        verbose_logs,
    } = args.log_config;

    let filter: LevelFilter = if quiet {
        ignore("rawler");
        LevelFilter::Error
    } else {
        if verbose_logs < 2 {
            ignore("rawler");
        }

        match verbose_logs {
            0 => LevelFilter::Info,
            1 => LevelFilter::Debug,
            2.. => LevelFilter::Trace,
        }
    };

    Log::init(filter);

    let rt = Builder::new_multi_thread()
        .enable_all()
        .thread_name("rawbit-worker")
        .worker_threads(args.n_threads)
        .thread_stack_size(3 * 1024 * 1024)
        .build()
        .unwrap();

    let _rt_guard = rt.enter();

    let result = run(args);

    match rt.block_on(result) {
        Err(err) => {
            use AppError::*;

            let (err_str, cause, exit_code): (String, Option<&dyn Display>, u8) = match err {
                FmtStrParse(e) => (e.to_string(), None, 1),
                Io(s, ref e) => (s, Some(e), 2),
                DirNotFound(s, ref e) => (format!("{s}: {}", e.display()), None, 3),
                AlreadyExists(s, ref e) => (format!("{s}: {}", e.display()), None, 4),
                Other(s, ref e) => (s, Some(e), 5),
            };

            error!("{err_str}");
            if let Some(cause) = cause {
                debug!("{cause}");
            }

            exit!(exit_code)
        }

        Ok(_) => exit!(0),
    }
}

macro_rules! map_err {
    ($r:expr, $err_t:path, $($s:expr),+ $(,)?) => {
        $r.map_err(|e| ($err_t)($($s.into()),+, e))
    };
}

pub(crate) use map_err;

async fn run(args: ImportArgs) -> RawbitResult<()> {
    let ImportArgs {
        source,
        dst_dir: output_dir,
        fmt_str,
        artist,
        force,
        embed,
        ..
    } = args;

    let ingest = source.get_ingest_items().await?.leak();

    if output_dir.exists() {
        if !output_dir.is_dir() {
            Err(AppError::AlreadyExists(
                "destination path exists and isn't a directory".into(),
                (&output_dir).into(),
            ))
        } else {
            Ok(())
        }
    } else {
        map_err!(
            fs::create_dir_all(&output_dir).await,
            AppError::Io,
            "couldn't create destination directory"
        )
    }?;

    let fmt_str = fmt_str.map(|s| s.leak() as &'static str).unwrap_or("");
    let filename_format = Box::<_>::leak(Box::new(FilenameFormat::parse(fmt_str)?));

    let opts = ConvertParams {
        artist,
        apply_scaling: false,
        crop: CropMode::Best,
        compression: DngCompression::Lossless,
        embedded: embed,
        index: 0,
        preview: true,
        thumbnail: true,
        software: "rawbit".into(),
        ..Default::default()
    };

    for chunk in ingest.chunks(args.n_threads) {
        let jobs = chunk
            .par_iter()
            .cloned()
            .map(|input_path| {
                let job = Job::new(
                    input_path,
                    output_dir.clone(),
                    filename_format,
                    force,
                    opts.clone(),
                );

                job.run()
            })
            .collect::<Vec<_>>();

        futures::future::join_all(jobs)
            .await
            .into_iter()
            .zip(chunk.iter())
            .for_each(|(result, input_path)| {
                if let Err(cvt_err) = result {
                    let (err_str, cause): (&str, Option<&dyn Display>) = match cvt_err {
                        ConvertError::AlreadyExists(ref err_str) => (err_str, None),
                        ConvertError::Io(ref err_str, ref cause) => (err_str, Some(cause)),
                        ConvertError::ImgOp(ref err_str, ref cause) => (err_str, Some(cause)),
                        ConvertError::Other(ref err_str, ref cause) => (err_str, Some(cause)),
                    };

                    warn!("while processing \"{}\": {err_str}", input_path.display());
                    if let Some(dbg) = cause {
                        debug!("Cause of last error:\n{dbg}");
                    }
                }
            });
    }

    Ok(())
}
