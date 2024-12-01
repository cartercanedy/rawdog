use std::{
    error,
    fs::{remove_file, OpenOptions},
    io::{self, BufReader, BufWriter},
    path::PathBuf,
};

use rawler::{
    decoders::{RawDecodeParams, RawMetadata},
    dng::{self, convert::ConvertParams},
    get_decoder, RawFile, RawlerError,
};

use smlog::info;

use crate::{map_err, parse::FilenameFormat};

#[derive(Debug)]
pub enum Error {
    ImgOp(String, RawlerError),
    Io(String, io::Error),
    AlreadyExists(String),
    #[allow(unused)]
    Other(String, Box<dyn error::Error + Send + Sync>),
}

pub struct Job {
    pub input_path: PathBuf,
    pub output_dir: PathBuf,
    pub filename_format: &'static FilenameFormat<'static>,
    pub force: bool,
    pub convert_opts: ConvertParams,
}

impl Job {
    pub fn new(
        input_path: PathBuf,
        output_dir: PathBuf,
        filename_format: &'static FilenameFormat,
        force: bool,
        convert_opts: ConvertParams,
    ) -> Self {
        assert!(input_path.is_file());
        assert!(output_dir.is_dir());

        Self {
            input_path,
            output_dir,
            filename_format,
            force,
            convert_opts,
        }
    }

    fn get_output_path(&self, md: &RawMetadata) -> Result<PathBuf, Error> {
        let input_filename_root = self
            .input_path
            .file_stem()
            .unwrap_or_else(|| {
                panic!(
                    "couldn't deduce filename from {}",
                    self.input_path.display()
                )
            })
            .to_string_lossy();

        let output_filename = self
            .filename_format
            .render_filename(input_filename_root.as_ref(), md)
            + ".dng";

        let output_path = self.output_dir.join(output_filename + ".dng");

        if output_path.exists() {
            if !self.force {
                Err(Error::AlreadyExists(format!(
                    "won't overwrite existing file: {}",
                    output_path.display()
                )))
            } else if output_path.is_dir() {
                Err(Error::AlreadyExists(format!(
                    "computed filepath already exists as a directory: {}",
                    output_path.display()
                )))
            } else {
                map_err!(
                    remove_file(&output_path),
                    Error::Io,
                    format!("couldn't remove existing file: {}", output_path.display()),
                )
            }?;
        }

        Ok(output_path)
    }

    fn run_blocking(self) -> Result<(), Error> {
        let input = map_err!(
            OpenOptions::new()
                .read(true)
                .write(false)
                .open(&self.input_path),
            Error::Io,
            "Couldn't open input RAW file",
        )?;

        let mut raw_file = RawFile::new(self.input_path.as_path(), BufReader::new(input));

        let decoder = map_err!(
            get_decoder(&mut raw_file),
            Error::ImgOp,
            "no compatible RAW image decoder available",
        )?;

        let md = map_err!(
            decoder.raw_metadata(&mut raw_file, RawDecodeParams::default()),
            Error::ImgOp,
            "couldn't extract image metadata",
        )?;

        map_err!(raw_file.file.rewind(), Error::Io, "input file io error",)?;

        let output_path = self.get_output_path(&md)?;

        let output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path);

        let mut output_file = BufWriter::new(map_err!(
            output_file,
            Error::Io,
            format!("couldn't create output file: {}", output_path.display()),
        )?);

        info!("Writing DNG: \"{}\"", output_path.display());

        let cvt_result = dng::convert::convert_raw_stream(
            raw_file.file,
            &mut output_file,
            self.input_path.to_string_lossy(),
            &self.convert_opts,
        );

        map_err!(cvt_result, Error::ImgOp, "couldn't convert image to DNG",)
    }

    pub async fn run(self) -> Result<(), Error> {
        tokio::task::spawn_blocking(|| self.run_blocking())
            .await
            .unwrap()
    }
}
