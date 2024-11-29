use std::{
    fs::{remove_file, OpenOptions},
    io::{BufReader, BufWriter, SeekFrom},
    path::PathBuf,
};

use rawler::{
    dng::{self, convert::ConvertParams},
    get_decoder, RawFile,
};

use smlog::info;

use crate::{error::ConvertError, map_err, parse::FilenameFormat};

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

    pub async fn run(self) -> Result<(), ConvertError> {
        tokio::task::spawn_blocking(move || {
            let input = OpenOptions::new()
                .read(true)
                .write(false)
                .open(&self.input_path);

            let input = map_err!(input, ConvertError::Io, "Couldn't open input file")?;

            let reader = BufReader::new(input);

            let mut raw_file = RawFile::new(self.input_path.as_path(), reader);

            let decoder = map_err!(
                get_decoder(&mut raw_file),
                ConvertError::ImgOp,
                "no compatible RAW image decoder available",
            )?;

            let md = map_err!(
                decoder.raw_metadata(&mut raw_file, Default::default()),
                ConvertError::ImgOp,
                "couldn't extract image metadata",
            )?;

            map_err!(
                raw_file.file.seek(SeekFrom::Start(0)),
                ConvertError::Io,
                "input file io error",
            )?;

            let src_path_str = self.input_path.to_string_lossy().to_string();

            let src_filename_root = self
                .input_path
                .file_stem()
                .unwrap_or_else(|| panic!("couldn't deduce filename from {}", &src_path_str))
                .to_string_lossy();

            let output_filename = self
                .filename_format
                .render_filename(src_filename_root.as_ref(), &md)
                + ".dng";

            let output_path = self.output_dir.join(output_filename);

            if output_path.exists() {
                if !self.force {
                    return Err(ConvertError::AlreadyExists(format!(
                        "won't overwrite existing file: {}",
                        output_path.display()
                    )));
                } else if output_path.is_dir() {
                    return Err(ConvertError::AlreadyExists(format!(
                        "computed filepath already exists as a directory: {}",
                        output_path.display()
                    )));
                } else {
                    map_err!(
                        remove_file(&output_path),
                        ConvertError::Io,
                        format!("couldn't remove existing file: {}", output_path.display()),
                    )?
                };
            }

            let output_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path);

            let mut output_file = BufWriter::new(map_err!(
                output_file,
                ConvertError::Io,
                format!("couldn't create output file: {}", output_path.display()),
            )?);

            info!("Writing DNG: \"{}\"", output_path.display());

            let cvt_result = dng::convert::convert_raw_stream(
                raw_file.file,
                &mut output_file,
                &src_path_str,
                &self.convert_opts,
            );

            map_err!(
                cvt_result,
                ConvertError::ImgOp,
                "couldn't convert image to DNG",
            )
        })
        .await
        .unwrap()
    }
}
