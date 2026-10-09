//! Streaming parser for the output of `mame -listsoftware`.
//!
//! The document is read event by event, so memory use is bounded by one
//! `<software>` entry. Items are returned in document order: a
//! [`SoftwareListItem::List`] header, followed by the software of that list.
//!
//! Elements that are not part of the model (part DIP switches) and unknown
//! elements are skipped; unknown attributes are ignored. Missing required
//! attributes and malformed values are reported as errors.

use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::software_model::{
    DataArea, DiskArea, NameValue, Software, SoftwareDisk, SoftwareListHeader, SoftwarePart,
    SoftwareRom,
};
use super::xml::{
    ParseError, error, for_each_attribute, missing, number, read_children, read_text, required,
    xml_error, yes_no,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoftwareListItem {
    /// Start of a software list. All following software belong to it until
    /// the next `List` item.
    List(SoftwareListHeader),
    Software(Software),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    BeforeRoot,
    InRoot,
    InList,
    Done,
}

pub struct SoftwareListParser<R: BufRead> {
    reader: Reader<R>,
    buf: Vec<u8>,
    scratch: Vec<u8>,
    state: State,
    /// Header read but not yet returned (it may still receive `<notes>`).
    pending_list: Option<SoftwareListHeader>,
    /// Software read while returning the pending list header.
    pending_software: Option<Software>,
}

impl<R: BufRead> SoftwareListParser<R> {
    pub fn new(source: R) -> Self {
        Self {
            reader: Reader::from_reader(source),
            buf: Vec::new(),
            scratch: Vec::new(),
            state: State::BeforeRoot,
            pending_list: None,
            pending_software: None,
        }
    }

    /// Returns the next item, or `None` at the end of a complete document.
    pub fn next_item(&mut self) -> Result<Option<SoftwareListItem>, ParseError> {
        if let Some(software) = self.pending_software.take() {
            return Ok(Some(SoftwareListItem::Software(software)));
        }
        let Self {
            reader,
            buf,
            scratch,
            state,
            pending_list,
            pending_software,
        } = self;
        loop {
            buf.clear();
            let event = reader
                .read_event_into(buf)
                .map_err(|source| xml_error(reader, &source))?;
            match event {
                Event::Start(element) => match (*state, element.name().as_ref()) {
                    (State::BeforeRoot, "softwarelists") => *state = State::InRoot,
                    (State::InRoot, "softwarelist") => {
                        *pending_list =
                            Some(parse_list_header(&element).map_err(|m| error(reader, m))?);
                        *state = State::InList;
                    }
                    (State::InList, "notes") if pending_list.is_some() => {
                        let notes = read_text(reader, scratch).map_err(|m| error(reader, m))?;
                        if let Some(list) = pending_list.as_mut() {
                            list.notes = Some(notes);
                        }
                    }
                    (State::InList, "software") => {
                        let software =
                            parse_software_start(&element).map_err(|m| error(reader, m))?;
                        let software = parse_software_body(reader, scratch, software)?;
                        return Ok(Some(match pending_list.take() {
                            Some(list) => {
                                *pending_software = Some(software);
                                SoftwareListItem::List(list)
                            }
                            None => SoftwareListItem::Software(software),
                        }));
                    }
                    (State::InRoot | State::InList, _) => {
                        let end = element.to_end().into_owned();
                        reader
                            .read_to_end_into(end.name(), scratch)
                            .map_err(|source| xml_error(reader, &source))?;
                    }
                    (_, name) => {
                        let name = name.to_owned();
                        return Err(error(reader, format!("unexpected element <{name}>")));
                    }
                },
                Event::Empty(element) => match (*state, element.name().as_ref()) {
                    (State::BeforeRoot, "softwarelists") => *state = State::Done,
                    (State::InRoot, "softwarelist") => {
                        let list = parse_list_header(&element).map_err(|m| error(reader, m))?;
                        return Ok(Some(SoftwareListItem::List(list)));
                    }
                    (State::InList, "software") => {
                        return Err(error(reader, "software without description".to_owned()));
                    }
                    (State::InRoot | State::InList, _) => {}
                    (_, name) => {
                        let name = name.to_owned();
                        return Err(error(reader, format!("unexpected element <{name}>")));
                    }
                },
                Event::End(_) => match *state {
                    State::InList => {
                        *state = State::InRoot;
                        if let Some(list) = pending_list.take() {
                            return Ok(Some(SoftwareListItem::List(list)));
                        }
                    }
                    _ => *state = State::Done,
                },
                Event::Eof => {
                    return match state {
                        State::Done => Ok(None),
                        State::InRoot | State::InList => Err(error(
                            reader,
                            "unexpected end of document: missing </softwarelists>".to_owned(),
                        )),
                        State::BeforeRoot => Err(error(
                            reader,
                            "no <softwarelists> root element found".to_owned(),
                        )),
                    };
                }
                // Declarations, the DTD, comments and whitespace.
                _ => {}
            }
        }
    }
}

fn parse_list_header(element: &BytesStart<'_>) -> Result<SoftwareListHeader, String> {
    let mut header = SoftwareListHeader::default();
    for_each_attribute(element, |key, value| {
        match key {
            "name" => header.name = value,
            "description" => header.description = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    if header.name.is_empty() {
        return Err(missing("softwarelist", "name"));
    }
    Ok(header)
}

fn parse_software_start(element: &BytesStart<'_>) -> Result<Software, String> {
    let mut software = Software {
        supported: "yes".to_owned(),
        ..Software::default()
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => software.name = value,
            "cloneof" => software.clone_of = Some(value),
            "supported" => software.supported = value,
            _ => {}
        }
        Ok(())
    })?;
    if software.name.is_empty() {
        return Err("software without name".to_owned());
    }
    Ok(software)
}

fn parse_software_body<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    mut software: Software,
) -> Result<Software, ParseError> {
    let mut description = None;
    let mut text_buf = Vec::new();
    let in_software = |reader: &Reader<R>, name: &str, message: String| ParseError {
        message: format!("software '{name}': {message}"),
        position: reader.buffer_position(),
    };

    loop {
        buf.clear();
        let event = reader
            .read_event_into(buf)
            .map_err(|source| xml_error(reader, &source))?;
        match event {
            Event::Start(element) => {
                let element = element.into_owned();
                let result = match element.name().as_ref() {
                    "description" => {
                        read_text(reader, &mut text_buf).map(|text| description = Some(text))
                    }
                    "year" => {
                        read_text(reader, &mut text_buf).map(|text| software.year = Some(text))
                    }
                    "publisher" => {
                        read_text(reader, &mut text_buf).map(|text| software.publisher = Some(text))
                    }
                    "notes" => {
                        read_text(reader, &mut text_buf).map(|text| software.notes = Some(text))
                    }
                    "part" => parse_part_start(&element).and_then(|part| {
                        let part = parse_part_body(reader, &mut text_buf, part)?;
                        software.parts.push(part);
                        Ok(())
                    }),
                    _ => reader
                        .read_to_end_into(element.name(), &mut text_buf)
                        .map(|_| ())
                        .map_err(|source| format!("malformed XML: {source}")),
                };
                result.map_err(|m| in_software(reader, &software.name, m))?;
            }
            Event::Empty(element) => {
                let result = match element.name().as_ref() {
                    "description" => {
                        description = Some(String::new());
                        Ok(())
                    }
                    "info" => parse_name_value(&element, "info").map(|v| software.info.push(v)),
                    "sharedfeat" => parse_name_value(&element, "sharedfeat")
                        .map(|v| software.shared_features.push(v)),
                    "part" => parse_part_start(&element).map(|part| software.parts.push(part)),
                    _ => Ok(()),
                };
                result.map_err(|m| in_software(reader, &software.name, m))?;
            }
            Event::End(_) => break,
            Event::Eof => {
                return Err(in_software(
                    reader,
                    &software.name,
                    "unexpected end of document".to_owned(),
                ));
            }
            _ => {}
        }
    }

    software.description = description
        .ok_or_else(|| in_software(reader, &software.name, "missing <description>".to_owned()))?;
    Ok(software)
}

fn parse_part_start(element: &BytesStart<'_>) -> Result<SoftwarePart, String> {
    let mut name = None;
    let mut interface = None;
    for_each_attribute(element, |key, value| {
        match key {
            "name" => name = Some(value),
            "interface" => interface = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    Ok(SoftwarePart {
        name: name.ok_or_else(|| missing("part", "name"))?,
        interface: interface.ok_or_else(|| missing("part", "interface"))?,
        features: Vec::new(),
        data_areas: Vec::new(),
        disk_areas: Vec::new(),
    })
}

fn parse_part_body<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    mut part: SoftwarePart,
) -> Result<SoftwarePart, String> {
    let mut children = Vec::new();
    loop {
        buf.clear();
        match reader
            .read_event_into(buf)
            .map_err(|source| format!("malformed XML: {source}"))?
        {
            Event::Start(element) => {
                let element = element.into_owned();
                match element.name().as_ref() {
                    "dataarea" => {
                        let mut area = parse_data_area(&element)?;
                        read_children(reader, &mut children, |child| {
                            if child.name().as_ref() == "rom" {
                                area.roms.push(parse_rom(child)?);
                            }
                            Ok(())
                        })?;
                        part.data_areas.push(area);
                    }
                    "diskarea" => {
                        let mut area = DiskArea {
                            name: required(&element, "diskarea", "name")?,
                            disks: Vec::new(),
                        };
                        read_children(reader, &mut children, |child| {
                            if child.name().as_ref() == "disk" {
                                area.disks.push(parse_disk(child)?);
                            }
                            Ok(())
                        })?;
                        part.disk_areas.push(area);
                    }
                    // DIP switches and unknown elements are not part of the model.
                    _ => {
                        reader
                            .read_to_end_into(element.name(), &mut children)
                            .map_err(|source| format!("malformed XML: {source}"))?;
                    }
                }
            }
            Event::Empty(element) => match element.name().as_ref() {
                "feature" => part.features.push(parse_name_value(&element, "feature")?),
                "dataarea" => part.data_areas.push(parse_data_area(&element)?),
                "diskarea" => part.disk_areas.push(DiskArea {
                    name: required(&element, "diskarea", "name")?,
                    disks: Vec::new(),
                }),
                _ => {}
            },
            Event::End(_) => return Ok(part),
            Event::Eof => return Err("unexpected end of document".to_owned()),
            _ => {}
        }
    }
}

fn parse_name_value(element: &BytesStart<'_>, element_name: &str) -> Result<NameValue, String> {
    let mut name = None;
    let mut value = None;
    for_each_attribute(element, |key, attribute| {
        match key {
            "name" => name = Some(attribute),
            "value" => value = Some(attribute),
            _ => {}
        }
        Ok(())
    })?;
    Ok(NameValue {
        name: name.ok_or_else(|| missing(element_name, "name"))?,
        value,
    })
}

fn parse_data_area(element: &BytesStart<'_>) -> Result<DataArea, String> {
    let mut name = None;
    let mut size = None;
    let mut area = DataArea {
        name: String::new(),
        size: 0,
        databits: 8,
        endian: "little".to_owned(),
        roms: Vec::new(),
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => name = Some(value),
            "size" => size = Some(number(&value, "size")?),
            "databits" => area.databits = number(&value, "databits")?,
            "endian" => area.endian = value,
            _ => {}
        }
        Ok(())
    })?;
    area.name = name.ok_or_else(|| missing("dataarea", "name"))?;
    area.size = size.ok_or_else(|| missing("dataarea", "size"))?;
    Ok(area)
}

fn parse_rom(element: &BytesStart<'_>) -> Result<SoftwareRom, String> {
    let mut rom = SoftwareRom {
        name: None,
        size: None,
        length: None,
        crc: None,
        sha1: None,
        offset: None,
        value: None,
        status: "good".to_owned(),
        load_flag: None,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => rom.name = Some(value),
            "size" => rom.size = Some(number(&value, "size")?),
            "length" => rom.length = Some(number(&value, "length")?),
            "crc" => rom.crc = Some(value),
            "sha1" => rom.sha1 = Some(value),
            "offset" => rom.offset = Some(value),
            "value" => rom.value = Some(value),
            "status" => rom.status = value,
            "loadflag" => rom.load_flag = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    Ok(rom)
}

fn parse_disk(element: &BytesStart<'_>) -> Result<SoftwareDisk, String> {
    let mut disk = SoftwareDisk {
        name: String::new(),
        sha1: None,
        status: "good".to_owned(),
        writeable: false,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => disk.name = value,
            "sha1" => disk.sha1 = Some(value),
            "status" => disk.status = value,
            "writeable" => disk.writeable = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    if disk.name.is_empty() {
        return Err(missing("disk", "name"));
    }
    Ok(disk)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("testdata/listsoftware-sample.xml");

    fn parse_all(xml: &str) -> Result<Vec<SoftwareListItem>, ParseError> {
        let mut parser = SoftwareListParser::new(xml.as_bytes());
        let mut items = Vec::new();
        while let Some(item) = parser.next_item()? {
            items.push(item);
        }
        Ok(items)
    }

    /// Returns `(list, software)` pairs in document order.
    fn software_by_list(xml: &str) -> Vec<(String, Software)> {
        let mut current = String::new();
        let mut result = Vec::new();
        for item in parse_all(xml).expect("parses") {
            match item {
                SoftwareListItem::List(list) => current = list.name,
                SoftwareListItem::Software(software) => result.push((current.clone(), software)),
            }
        }
        result
    }

    fn sample_software(list: &str, name: &str) -> Software {
        software_by_list(SAMPLE)
            .into_iter()
            .find(|(l, s)| l == list && s.name == name)
            .map(|(_, s)| s)
            .unwrap_or_else(|| panic!("{list}/{name} not found"))
    }

    #[test]
    fn returns_lists_and_software_in_document_order() {
        let items = parse_all(SAMPLE).expect("parses");

        let summary: Vec<String> = items
            .iter()
            .map(|item| match item {
                SoftwareListItem::List(list) => format!("list:{}", list.name),
                SoftwareListItem::Software(software) => software.name.clone(),
            })
            .collect();
        assert_eq!(
            summary,
            [
                "list:c64_cart",
                "gameb2",
                "gamebase",
                "tetris",
                "list:nes",
                "smb",
                "tetris",
                "list:apple2_flop_orig",
                "ultima4",
                "list:pcecd",
                "ys12"
            ]
        );
    }

    #[test]
    fn parses_list_header_with_notes() {
        let items = parse_all(SAMPLE).expect("parses");

        assert_eq!(
            items[0],
            SoftwareListItem::List(SoftwareListHeader {
                name: "c64_cart".to_owned(),
                description: Some("Commodore 64 cartridges".to_owned()),
                notes: Some("Cartridges for the C64 & C128.".to_owned()),
            })
        );
    }

    #[test]
    fn keeps_same_software_name_in_different_lists() {
        let tetris: Vec<_> = software_by_list(SAMPLE)
            .into_iter()
            .filter(|(_, software)| software.name == "tetris")
            .map(|(list, software)| (list, software.description))
            .collect();

        assert_eq!(
            tetris,
            [
                ("c64_cart".to_owned(), "Tetris (C64)".to_owned()),
                ("nes".to_owned(), "Tetris (NES)".to_owned())
            ]
        );
    }

    #[test]
    fn parses_clone_support_metadata_and_entities() {
        let clone = sample_software("c64_cart", "gameb2");

        assert_eq!(clone.clone_of.as_deref(), Some("gamebase"));
        assert_eq!(clone.supported, "partial");
        assert_eq!(clone.year.as_deref(), Some("198?"));
        assert_eq!(clone.publisher.as_deref(), Some("<unknown>"));
        // Repeated info names are kept.
        assert_eq!(clone.info.len(), 2);
        assert_eq!(clone.info[1].value.as_deref(), Some("GB2"));
        assert_eq!(
            clone.shared_features,
            [NameValue {
                name: "compatibility".to_owned(),
                value: Some("NTSC".to_owned())
            }]
        );
        assert_eq!(sample_software("pcecd", "ys12").description, "Ys I & II");
    }

    #[test]
    fn applies_defaults_for_missing_optional_attributes() {
        let parent = sample_software("c64_cart", "gamebase");

        assert_eq!(parent.clone_of, None);
        assert_eq!(parent.supported, "yes");
        assert!(parent.info.is_empty() && parent.shared_features.is_empty());
        let area = &parent.parts[0].data_areas[0];
        assert_eq!((area.databits, area.endian.as_str()), (8, "little"));
        assert_eq!(area.roms[0].status, "baddump");
        let nodump = &sample_software("c64_cart", "tetris").parts[0].data_areas[0].roms[0];
        assert_eq!(nodump.status, "nodump");
        assert_eq!(
            (nodump.crc.as_deref(), nodump.sha1.as_deref()),
            (None, None)
        );
    }

    #[test]
    fn parses_loading_instructions_without_rom_name() {
        let roms = &sample_software("c64_cart", "gameb2").parts[0].data_areas[0].roms;

        assert_eq!(roms.len(), 2);
        assert_eq!(roms[1].name, None);
        assert_eq!(roms[1].size, Some(8192));
        assert_eq!(roms[1].offset.as_deref(), Some("0x2000"));
        assert_eq!(roms[1].load_flag.as_deref(), Some("reload"));
    }

    #[test]
    fn parses_multiple_data_areas_with_bus_attributes() {
        let part = &sample_software("nes", "smb").parts[0];

        assert_eq!(part.interface, "nes_cart");
        assert_eq!(part.features.len(), 2);
        let names: Vec<_> = part.data_areas.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["prg", "chr"]);
        assert_eq!(part.data_areas[1].databits, 16);
        assert_eq!(part.data_areas[1].endian, "big");
    }

    #[test]
    fn parses_software_with_several_parts() {
        let ultima = sample_software("apple2_flop_orig", "ultima4");

        let parts: Vec<_> = ultima.parts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(parts, ["flop1", "flop2"]);
        assert_eq!(
            ultima.parts[1].features[0].value.as_deref(),
            Some("Side B - Towne Disk")
        );
    }

    #[test]
    fn parses_disk_areas() {
        let ys = sample_software("pcecd", "ys12");

        assert_eq!(ys.parts.len(), 2);
        let cd = &ys.parts[0].disk_areas[0];
        assert_eq!(cd.name, "cdrom");
        assert_eq!(cd.disks[0].name, "ys i & ii (japan)");
        assert_eq!(cd.disks[0].status, "baddump");
        assert!(!cd.disks[0].writeable);
        assert!(ys.parts[1].disk_areas[0].disks[0].writeable);
        assert!(ys.parts[0].data_areas.is_empty());
    }

    #[test]
    fn reports_invalid_documents() {
        let cases = [
            ("", "no <softwarelists> root element"),
            (
                "<softwarelists><softwarelist name=\"a\">",
                "missing </softwarelists>",
            ),
            (
                "<softwarelists><softwarelist><software name=\"s\"><description>S</description></software></softwarelist></softwarelists>",
                "required attribute 'name'",
            ),
            (
                "<softwarelists><softwarelist name=\"a\"><software name=\"s\"><year>1990</year></software></softwarelist></softwarelists>",
                "missing <description>",
            ),
            (
                "<softwarelists><softwarelist name=\"a\"><software name=\"s\"><description>S</description><part name=\"p\"/></software></softwarelist></softwarelists>",
                "required attribute 'interface'",
            ),
            (
                "<softwarelists><softwarelist name=\"a\"><software name=\"s\"><description>S</description><part name=\"p\" interface=\"i\"><dataarea name=\"rom\" size=\"big\"/></part></software></softwarelist></softwarelists>",
                "invalid number 'big'",
            ),
            (
                "<softwarelists><softwarelist name=\"a\"><software name=\"s\"><description>S</descr></software></softwarelist></softwarelists>",
                "malformed",
            ),
            ("<mame/>", "unexpected element <mame>"),
        ];
        for (xml, expected) in cases {
            let error = parse_all(xml).expect_err(xml);
            assert!(error.message.contains(expected), "{xml}: {error}");
        }
    }

    #[test]
    fn includes_software_name_in_errors() {
        let xml = "<softwarelists><softwarelist name=\"a\"><software name=\"broken\"><description>S</description><part name=\"p\" interface=\"i\"><diskarea name=\"cd\"><disk name=\"d\" writeable=\"maybe\"/></diskarea></part></software></softwarelist></softwarelists>";

        let error = parse_all(xml).expect_err("invalid writeable");

        assert!(error.message.starts_with("software 'broken':"), "{error}");
        assert!(error.message.contains("expected 'yes' or 'no'"), "{error}");
    }

    #[test]
    fn accepts_empty_document_and_empty_list() {
        assert!(parse_all("<softwarelists/>").expect("empty").is_empty());
        let items = parse_all("<softwarelists><softwarelist name=\"empty\"/></softwarelists>")
            .expect("list");
        assert!(matches!(&items[..], [SoftwareListItem::List(list)] if list.name == "empty"));
    }
}
