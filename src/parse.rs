use std::borrow::Cow;

use phf::{phf_map, phf_set, Map, Set};

use crate::error::{ParseError, ParseErrorType};


#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum MetadataKind {
    CameraMake,
    CameraModel,
    CameraShutterSpeed,
    CameraExposureComp,
    CameraISO,
    #[allow(unused)]
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
pub enum FmtItem<'a> {
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
fn expand(s: &str) -> Option<FmtItem> {
    Some(FmtItem::Metadata(MD_KIND_MAP.get(s)?.to_owned()))
}

#[allow(unused_parens)]
pub fn parse_name_format(fmt: &str) -> Result<Box<[FmtItem]>, ParseError> {
    let mut items = vec![];
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
                items.push(FmtItem::Literal(Cow::Borrowed(&s[0..1])));
            } else {
                items.push(match state {
                    ScanState::Literal => FmtItem::Literal(Cow::Borrowed(s)),
                    ScanState::DateTime => FmtItem::DateTime(Cow::Borrowed(s)),

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
            dbg!(items, &state);

            return Err(ParseError::new(
                consumed,
                fmt.len() - consumed,
                fmt,
                ParseErrorType::Unknown,
            ));
        }

        state = ScanState::Start;
    }

    Ok(items.into_boxed_slice())
}

#[cfg(test)]
mod test_parse {
    use super::{FmtItem, OPEN_EXPANSION};

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

        let parsed = parsed.unwrap();

        assert!(parsed.len() == 2);

        assert!(matches!(
        parsed[0], FmtItem::Literal(ref s) if s.chars().nth(0).unwrap() == OPEN_EXPANSION && s.len() == 1
    ));

        assert!(matches!(parsed[1], FmtItem::DateTime(..)));
    }
}

