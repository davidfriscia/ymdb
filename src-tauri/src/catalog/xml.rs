//! Helpers shared by the streaming XML parsers of MAME output.

use std::fmt;
use std::io::BufRead;
use std::str::FromStr;

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Byte offset in the document where the error was detected.
    pub position: u64,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.position)
    }
}

impl std::error::Error for ParseError {}

pub(crate) fn error<R>(reader: &Reader<R>, message: String) -> ParseError {
    ParseError {
        message,
        position: reader.buffer_position(),
    }
}

pub(crate) fn xml_error<R>(reader: &Reader<R>, source: &quick_xml::Error) -> ParseError {
    ParseError {
        message: format!("malformed XML: {source}"),
        position: reader.error_position(),
    }
}

/// Reads the text content of the current element up to its end tag.
pub(crate) fn read_text<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
) -> Result<String, String> {
    let mut text = String::new();
    loop {
        buf.clear();
        match reader
            .read_event_into(buf)
            .map_err(|error| format!("malformed XML: {error}"))?
        {
            Event::Text(content) => text.push_str(&content.xml10_content()),
            Event::CData(content) => text.push_str(&content),
            Event::GeneralRef(reference) => {
                if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|error| format!("invalid character reference: {error}"))?
                {
                    text.push(character);
                } else {
                    let name = reference.xml10_content();
                    let value = resolve_predefined_entity(&name)
                        .ok_or_else(|| format!("unknown entity &{name};"))?;
                    text.push_str(value);
                }
            }
            Event::End(_) => return Ok(text),
            Event::Eof => return Err("unexpected end of document".to_owned()),
            Event::Start(_) | Event::Empty(_) => {
                return Err("unexpected element inside text".to_owned());
            }
            _ => {}
        }
    }
}

/// Calls `on_child` for each empty child element up to the end tag of the
/// current element. Nested elements with content are skipped.
pub(crate) fn read_children<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    mut on_child: impl FnMut(&BytesStart<'_>) -> Result<(), String>,
) -> Result<(), String> {
    loop {
        buf.clear();
        match reader
            .read_event_into(buf)
            .map_err(|error| format!("malformed XML: {error}"))?
        {
            Event::Empty(child) => on_child(&child)?,
            Event::Start(child) => {
                let end = child.to_end().into_owned();
                let mut skip = Vec::new();
                reader
                    .read_to_end_into(end.name(), &mut skip)
                    .map_err(|error| format!("malformed XML: {error}"))?;
            }
            Event::End(_) => return Ok(()),
            Event::Eof => return Err("unexpected end of document".to_owned()),
            _ => {}
        }
    }
}

pub(crate) fn for_each_attribute(
    element: &BytesStart<'_>,
    mut on_attribute: impl FnMut(&str, String) -> Result<(), String>,
) -> Result<(), String> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| format!("malformed attribute: {error}"))?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| format!("malformed attribute value: {error}"))?;
        on_attribute(attribute.key.as_ref(), value.into_owned())?;
    }
    Ok(())
}

pub(crate) fn required(
    element: &BytesStart<'_>,
    element_name: &str,
    attribute: &str,
) -> Result<String, String> {
    let mut found = None;
    for_each_attribute(element, |key, value| {
        if key == attribute {
            found = Some(value);
        }
        Ok(())
    })?;
    found.ok_or_else(|| missing(element_name, attribute))
}

pub(crate) fn missing(element: &str, attribute: &str) -> String {
    format!("<{element}> without required attribute '{attribute}'")
}

pub(crate) fn yes_no(value: &str) -> Result<bool, String> {
    match value {
        "yes" => Ok(true),
        "no" => Ok(false),
        other => Err(format!("expected 'yes' or 'no', found '{other}'")),
    }
}

pub(crate) fn number<T: FromStr>(value: &str, attribute: &str) -> Result<T, String> {
    value
        .trim()
        .parse()
        .map_err(|_| format!("invalid number '{value}' for '{attribute}'"))
}

/// Parses a decimal number printed by MAME with `%f`.
///
/// MAME 0.280 on Windows with a locale that uses a decimal comma (for
/// example Italian) sporadically prints such values with a comma, such as
/// `refresh="61,419025"`. `%f` never prints digit grouping, so a single comma
/// without a dot can only be the decimal separator.
pub(crate) fn decimal(value: &str, attribute: &str) -> Result<f64, String> {
    number(value, attribute).or_else(|error| {
        let trimmed = value.trim();
        if trimmed.matches(',').count() == 1 && !trimmed.contains('.') {
            number(&trimmed.replace(',', "."), attribute)
        } else {
            Err(error)
        }
    })
}
