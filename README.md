<div align="center">
<img width="640" height="320" src="https://raw.githubusercontent.com/cartercanedy/rawbit/refs/heads/master/res/rawbit.png"/>
<br>

# rawbit

A **camera RAW image preprocessor and importer** written in Rust.  

Rawbit processes raw image files by converting them to the DNG format in parallel,
while offering the ability to manipulate metadata and customize file name formatting.

</div>

## Features

- **RAW Image Conversion**: Converts camera RAW files to DNG format.
- **Flexible Input/Output**:
  - Process individual files or entire directories.
  - Define output directories with optional overwrite support.
- **Custom Filename Formatting**: Supports user-defined naming conventions for output files.
- **Metadata Handling**: Supports EXIF metadata manipulation
- **Multi-Threaded Processing**: Leverages multiple CPU cores for parallel image processing.

*__all written in Rust, btw...__*

## Installation

### Pre-built binaries
Pre-built binaries releases are available for download from the latest [GitHub release](https://github.com/cartercanedy/rawbit/releases/latest).

I plan on making binary releases available for all major platforms via package managers.
In the meantime, there are [AUR](https://aur.archlinux.org) & [crates.io](https://crates.io) packages available:

### Arch Linux

You can install rawbit with your preferred [AUR helper](https://wiki.archlinux.org/title/AUR_helpers). Example:

```sh
paru -S rawbit
```

### Crates.io

1. Install [Rust](https://www.rust-lang.org/tools/install) and ensure `cargo` is available.
2. Install via cargo:
```sh
cargo install rawbit
```

## Examples

### Convert a single file

```sh
rawbit --out-dir "./dng" --format "%Y-%m-%d_%H-%M-%S_{image.original_filename}" ./raw/ABC1234.ARW

# or 

rawbit -o"./dng" -F"%Y-%m-%d_%H-%M-%S_{image.original_filename}" ./raw/ABC1234.ARW
```

### Convert an entire directory

```sh
rawbit --in-dir "./raw" --out-dir "./dng" --format "%Y-%m-%d_{camera.model}_{lens.model}_{image.original_filename}"

# or

rawbit -i"./raw" -o"./dng" -F"%Y-%m-%d_{camera.model}_{lens.model}_{image.original_filename}"
```

## Usage

<style type="text/css">
.cli-doc-content { white-space: pre; word-wrap: break-word; }
.ansi33 { color: #aa5500; }
.ansi36 { color: #00aaaa; }
.ansi37 { color: #F5F1DE; }
</style>

<body style="font-size: normal;" >
<pre class="cli-doc-content">
<span class="ansi37">Usage:</span> <span class="ansi36">rawbit</span> <span class="ansi36">[OPTIONS]</span> <span class="ansi36">--out-dir</span><span class="ansi36"> </span><span class="ansi36">&lt;DIR&gt;</span> <span class="ansi36">&lt;--in-dir &lt;DIR&gt;|FILES&gt;</span>
<span class="ansi33">Arguments:</span>
<span class="ansi36">[FILES]...</span>
    individual files to convert
<span class="ansi33">Options:</span>
<span class="ansi36">-i</span>, <span class="ansi36">--in-dir</span><span class="ansi36"> </span><span class="ansi36">&lt;DIR&gt;</span>
    directory containing raw files to convert
<span class="ansi36">-o</span>, <span class="ansi36">--out-dir</span><span class="ansi36"> </span><span class="ansi36">&lt;DIR&gt;</span>
    directory to write converted DNGs
<span class="ansi36">-F</span>, <span class="ansi36">--format</span><span class="ansi36"> </span><span class="ansi36">&lt;FORMAT&gt;</span>
    filename format of converted DNGs; see https://docs.rs/rawbit for info on syntax
<span class="ansi36">-a</span>, <span class="ansi36">--artist</span><span class="ansi36"> </span><span class="ansi36">&lt;ARTIST&gt;</span>
    value of the "artist" field in converted DNGs
<span class="ansi36">-e</span>, <span class="ansi36">--embed-original</span><span class="ansi36">[=</span><span class="ansi36">&lt;BOOL&gt;</span><span class="ansi36">]</span>
    embed the original raw image in the converted DNG
    NOTE: conversion may take considerably longer [default: false] [possible values: true, false]
<span class="ansi36">-f</span>, <span class="ansi36">--force</span><span class="ansi36">[=</span><span class="ansi36">&lt;BOOL&gt;</span><span class="ansi36">]</span>
    overwrite existing files, if they exist [default: false] [possible values: true, false]
<span class="ansi36">-r</span>, <span class="ansi36">--recurse</span><span class="ansi36">[=</span><span class="ansi36">&lt;BOOL&gt;</span><span class="ansi36">]</span>
    ingest images from subdirectories as well, preserving directory structure in the output [default: false] [possible values: true, false]
<span class="ansi36">-p</span>, <span class="ansi36">--preview</span><span class="ansi36"> [</span><span class="ansi36">&lt;BOOL&gt;</span><span class="ansi36">]</span>
    Embed image preview in output DNG [default: true] [possible values: true, false]
<span class="ansi36">-t</span>, <span class="ansi36">--thumbnail</span><span class="ansi36"> [</span><span class="ansi36">&lt;BOOL&gt;</span><span class="ansi36">]</span>
    Embed image thumbnail in output DNG [default: true] [possible values: true, false]
<span class="ansi36">-j</span>, <span class="ansi36">--n-threads</span><span class="ansi36"> [</span><span class="ansi36">&lt;N&gt;</span><span class="ansi36">]</span>
    number of threads to use while processing input images, defaults to number of CPUs
<span class="ansi36">-q</span>, <span class="ansi36">--quiet</span>
    quiet output, only emit critical errors
<span class="ansi36">-v</span>, <span class="ansi36">--verbose</span><span class="ansi36">...</span>
    increase log verbosity; specify multiple times to increase verbosity
<span class="ansi36">-h</span>, <span class="ansi36">--help</span>
    Print help
<span class="ansi36">-V</span>, <span class="ansi36">--version</span>
    Print version

</pre>
</body>

## Filename formatting

This is the distinguishing feature of `rawbit`.

### Date/time interpolation

You can insert the date-time information read from the RAW image's metadata using
syntax similar to libc's `strftime`.
More information can be found [here](https://docs.rs/chrono/latest/chrono/format/strftime/index.html)

### Metadata interpolation

Similar to the date/time interpolation, some well-known names in between squirly braces (i.e.
"{...}") expands into image-specific EXIF metadata in the filename:
| Variable      | Description | Example |
|---------------|-------------|---------|
| `camera.make` | Camera make | |
| `camera.model` | Camera model | |
| `camera.shutter_speed` | Shutter speed used to take the image | |
| `camera.iso` | Sensor sensitivity (ISO) used to take the image | |
| `lens.make` | Lens make | |
| `lens.model` | Lens model | |
| `lens.f_stop` | Lens aperture F stop value use to take the image | |
| `image.original_filename` | Image's original filename.<br>Automatically inserted if not specified in the original format string | |

*__Note:__*  
More metadata fields are a WIP, more to come soon...

## Why not use [`dnglab`](https://github.com/dnglab/dnglab)?

`dnglab convert` is extremely versatile and robust, but my main motivation for developing `rawbit` was to enable a more flexible batch DNG conversion/import workflow with entirely free (as in freedom) software enabling it.

This project utilizes the same library that powers DNGLab, so I owe a huge thanks to the DNGLab/Rawler team for their awesome work that made this project possible.

## Special thanks

[DNGLab/Rawler](https://github.com/dnglab/dnglab/blob/main/rawler): Rust-native RAW image manipulation tools from the ground-up  
[rayon](https://github.com/rayon-rs/rayon)/[tokio](https://github.com/tokio-rs/tokio): For making fearless concurrency a peice of cake  
[Adam Perkowski](https://github.com/adamperkowski): Contributing CI and package manager support  
