//! Streaming parser for the output of `mame -listxml`.
//!
//! The document is read event by event with a reusable buffer, so memory use
//! is bounded by the size of a single `<machine>` entry, whatever the size of
//! the whole document. Both the current `<machine>` element (MAME 0.162 and
//! later) and the older `<game>` element are accepted.
//!
//! Attributes and elements that are not part of the model (for example DIP
//! switches, configurations, ports and slots) are skipped. Unknown attributes
//! and elements are ignored, so newer MAME versions can still be imported.
//! Missing required attributes and malformed values are reported as errors.

use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::model::{
    BiosSet, Chip, Control, DeviceRef, Disk, Display, Driver, Feature, Input, ListXmlHeader,
    Machine, MediaDevice, RamOption, Rom, SoftwareListRef,
};
pub use super::xml::ParseError;
use super::xml::{
    decimal, error, for_each_attribute, missing, number, read_children, read_text, required,
    xml_error, yes_no,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    BeforeRoot,
    InRoot,
    Done,
}

pub struct ListXmlParser<R: BufRead> {
    reader: Reader<R>,
    buf: Vec<u8>,
    scratch: Vec<u8>,
    header: Option<ListXmlHeader>,
    state: State,
}

impl<R: BufRead> ListXmlParser<R> {
    pub fn new(source: R) -> Self {
        Self {
            reader: Reader::from_reader(source),
            buf: Vec::new(),
            scratch: Vec::new(),
            header: None,
            state: State::BeforeRoot,
        }
    }

    /// Attributes of the root element, available once the first machine has
    /// been read (or the document has ended).
    pub fn header(&self) -> Option<&ListXmlHeader> {
        self.header.as_ref()
    }

    /// Returns the next machine, or `None` at the end of a complete document.
    pub fn next_machine(&mut self) -> Result<Option<Machine>, ParseError> {
        let Self {
            reader,
            buf,
            scratch,
            header,
            state,
        } = self;
        loop {
            buf.clear();
            let event = reader
                .read_event_into(buf)
                .map_err(|error| xml_error(reader, &error))?;
            match event {
                Event::Start(element) => match (*state, element.name().as_ref()) {
                    (State::BeforeRoot, "mame") => {
                        *header = Some(parse_header(&element).map_err(|m| error(reader, m))?);
                        *state = State::InRoot;
                    }
                    (State::InRoot, "machine" | "game") => {
                        let machine =
                            parse_machine_start(&element).map_err(|m| error(reader, m))?;
                        return parse_machine_body(reader, scratch, machine).map(Some);
                    }
                    (State::InRoot, _) => {
                        let end = element.to_end().into_owned();
                        reader
                            .read_to_end_into(end.name(), scratch)
                            .map_err(|error| xml_error(reader, &error))?;
                    }
                    (_, name) => {
                        let name = name.to_owned();
                        return Err(error(reader, format!("unexpected element <{name}>")));
                    }
                },
                Event::Empty(element) => match (*state, element.name().as_ref()) {
                    (State::BeforeRoot, "mame") => {
                        *header = Some(parse_header(&element).map_err(|m| error(reader, m))?);
                        *state = State::Done;
                    }
                    (State::InRoot, "machine" | "game") => {
                        return Err(error(reader, "machine without description".to_owned()));
                    }
                    (State::InRoot, _) => {}
                    (_, name) => {
                        let name = name.to_owned();
                        return Err(error(reader, format!("unexpected element <{name}>")));
                    }
                },
                Event::End(_) => *state = State::Done,
                Event::Eof => {
                    return match state {
                        State::Done => Ok(None),
                        State::InRoot => Err(error(
                            reader,
                            "unexpected end of document: missing </mame>".to_owned(),
                        )),
                        State::BeforeRoot => {
                            Err(error(reader, "no <mame> root element found".to_owned()))
                        }
                    };
                }
                // Declarations, the DTD, comments and whitespace.
                _ => {}
            }
        }
    }
}

fn parse_header(element: &BytesStart<'_>) -> Result<ListXmlHeader, String> {
    let mut header = ListXmlHeader::default();
    for_each_attribute(element, |key, value| {
        match key {
            "build" => header.build = Some(value),
            "debug" => header.debug = yes_no(&value)?,
            "mameconfig" => header.mame_config = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    Ok(header)
}

fn parse_machine_start(element: &BytesStart<'_>) -> Result<Machine, String> {
    let mut machine = Machine {
        runnable: true,
        ..Machine::default()
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => machine.name = value,
            "sourcefile" => machine.source_file = Some(value),
            "isbios" => machine.is_bios = yes_no(&value)?,
            "isdevice" => machine.is_device = yes_no(&value)?,
            "ismechanical" => machine.is_mechanical = yes_no(&value)?,
            "runnable" => machine.runnable = yes_no(&value)?,
            "cloneof" => machine.clone_of = Some(value),
            "romof" => machine.rom_of = Some(value),
            "sampleof" => machine.sample_of = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    if machine.name.is_empty() {
        return Err("machine without name".to_owned());
    }
    Ok(machine)
}

fn parse_machine_body<R: BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
    mut machine: Machine,
) -> Result<Machine, ParseError> {
    let mut description = None;
    let mut text_buf = Vec::new();
    let in_machine = |reader: &Reader<R>, name: &str, message: String| ParseError {
        message: format!("machine '{name}': {message}"),
        position: reader.buffer_position(),
    };

    loop {
        buf.clear();
        let event = reader
            .read_event_into(buf)
            .map_err(|error| xml_error(reader, &error))?;
        match event {
            Event::Start(element) => {
                let element = element.into_owned();
                let result = match element.name().as_ref() {
                    "description" => {
                        read_text(reader, &mut text_buf).map(|text| description = Some(text))
                    }
                    "year" => {
                        read_text(reader, &mut text_buf).map(|text| machine.year = Some(text))
                    }
                    "manufacturer" => read_text(reader, &mut text_buf)
                        .map(|text| machine.manufacturer = Some(text)),
                    "input" => parse_input(&element).and_then(|mut input| {
                        read_children(reader, &mut text_buf, |child| {
                            if child.name().as_ref() == "control" {
                                input.controls.push(parse_control(child)?);
                            }
                            Ok(())
                        })?;
                        machine.input = Some(input);
                        Ok(())
                    }),
                    "device" => parse_media_device(&element).and_then(|mut device| {
                        read_children(reader, &mut text_buf, |child| {
                            parse_media_device_child(&mut device, child)
                        })?;
                        machine.media_devices.push(device);
                        Ok(())
                    }),
                    "ramoption" => {
                        parse_ram_option_attributes(&element).and_then(|(name, is_default)| {
                            let text = read_text(reader, &mut text_buf)?;
                            machine.ram_options.push(RamOption {
                                name,
                                size: number(&text, "ramoption")?,
                                is_default,
                            });
                            Ok(())
                        })
                    }
                    _ => {
                        // Elements not part of the model, such as dipswitch or slot.
                        reader
                            .read_to_end_into(element.name(), &mut text_buf)
                            .map(|_| ())
                            .map_err(|error| format!("malformed XML: {error}"))
                    }
                };
                result.map_err(|m| in_machine(reader, &machine.name, m))?;
            }
            Event::Empty(element) => {
                parse_empty_child(&mut machine, &mut description, &element)
                    .map_err(|m| in_machine(reader, &machine.name, m))?;
            }
            Event::End(_) => break,
            Event::Eof => {
                return Err(in_machine(
                    reader,
                    &machine.name,
                    "unexpected end of document".to_owned(),
                ));
            }
            _ => {}
        }
    }

    machine.description = description
        .ok_or_else(|| in_machine(reader, &machine.name, "missing <description>".to_owned()))?;
    Ok(machine)
}

fn parse_empty_child(
    machine: &mut Machine,
    description: &mut Option<String>,
    element: &BytesStart<'_>,
) -> Result<(), String> {
    match element.name().as_ref() {
        "description" => *description = Some(String::new()),
        "biosset" => machine.bios_sets.push(parse_bios_set(element)?),
        "rom" => machine.roms.push(parse_rom(element)?),
        "disk" => machine.disks.push(parse_disk(element)?),
        "device_ref" => {
            let name = required(element, "device_ref", "name")?;
            match machine.device_refs.iter_mut().find(|r| r.name == name) {
                Some(existing) => existing.count += 1,
                None => machine.device_refs.push(DeviceRef { name, count: 1 }),
            }
        }
        "sample" => machine.samples.push(required(element, "sample", "name")?),
        "chip" => machine.chips.push(parse_chip(element)?),
        "display" => machine.displays.push(parse_display(element)?),
        "sound" => {
            machine.sound_channels = Some(number(
                &required(element, "sound", "channels")?,
                "channels",
            )?);
        }
        "input" => machine.input = Some(parse_input(element)?),
        "driver" => machine.driver = Some(parse_driver(element)?),
        "feature" => machine.features.push(parse_feature(element)?),
        "device" => machine.media_devices.push(parse_media_device(element)?),
        "softwarelist" => machine.software_lists.push(parse_software_list(element)?),
        "ramoption" => {
            return Err("ramoption without size".to_owned());
        }
        _ => {}
    }
    Ok(())
}

fn parse_bios_set(element: &BytesStart<'_>) -> Result<BiosSet, String> {
    let mut name = None;
    let mut description = None;
    let mut is_default = false;
    for_each_attribute(element, |key, value| {
        match key {
            "name" => name = Some(value),
            "description" => description = Some(value),
            "default" => is_default = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    Ok(BiosSet {
        name: name.ok_or_else(|| missing("biosset", "name"))?,
        description: description.ok_or_else(|| missing("biosset", "description"))?,
        is_default,
    })
}

fn parse_rom(element: &BytesStart<'_>) -> Result<Rom, String> {
    let mut rom = Rom {
        name: String::new(),
        bios: None,
        size: 0,
        crc: None,
        sha1: None,
        merge: None,
        region: None,
        offset: None,
        status: "good".to_owned(),
        optional: false,
    };
    let mut has_name = false;
    let mut has_size = false;
    for_each_attribute(element, |key, value| {
        match key {
            "name" => {
                rom.name = value;
                has_name = true;
            }
            "bios" => rom.bios = Some(value),
            "size" => {
                rom.size = number(&value, "size")?;
                has_size = true;
            }
            "crc" => rom.crc = Some(value),
            "sha1" => rom.sha1 = Some(value),
            "merge" => rom.merge = Some(value),
            "region" => rom.region = Some(value),
            "offset" => rom.offset = Some(value),
            "status" => rom.status = value,
            "optional" => rom.optional = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    if !has_name {
        return Err(missing("rom", "name"));
    }
    if !has_size {
        return Err(missing("rom", "size"));
    }
    Ok(rom)
}

fn parse_disk(element: &BytesStart<'_>) -> Result<Disk, String> {
    let mut disk = Disk {
        name: String::new(),
        sha1: None,
        merge: None,
        region: None,
        index: None,
        writable: false,
        status: "good".to_owned(),
        optional: false,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "name" => disk.name = value,
            "sha1" => disk.sha1 = Some(value),
            "merge" => disk.merge = Some(value),
            "region" => disk.region = Some(value),
            "index" => disk.index = Some(number(&value, "index")?),
            "writable" => disk.writable = yes_no(&value)?,
            "status" => disk.status = value,
            "optional" => disk.optional = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    if disk.name.is_empty() {
        return Err(missing("disk", "name"));
    }
    Ok(disk)
}

fn parse_chip(element: &BytesStart<'_>) -> Result<Chip, String> {
    let mut name = None;
    let mut tag = None;
    let mut chip_type = None;
    let mut clock = None;
    for_each_attribute(element, |key, value| {
        match key {
            "name" => name = Some(value),
            "tag" => tag = Some(value),
            "type" => chip_type = Some(value),
            "clock" => clock = Some(number(&value, "clock")?),
            _ => {}
        }
        Ok(())
    })?;
    Ok(Chip {
        name: name.ok_or_else(|| missing("chip", "name"))?,
        tag,
        chip_type: chip_type.ok_or_else(|| missing("chip", "type"))?,
        clock,
    })
}

fn parse_display(element: &BytesStart<'_>) -> Result<Display, String> {
    let mut display = Display {
        tag: None,
        display_type: String::new(),
        rotate: None,
        flip_x: false,
        width: None,
        height: None,
        refresh: None,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "tag" => display.tag = Some(value),
            "type" => display.display_type = value,
            "rotate" => display.rotate = Some(number(&value, "rotate")?),
            "flipx" => display.flip_x = yes_no(&value)?,
            "width" => display.width = Some(number(&value, "width")?),
            "height" => display.height = Some(number(&value, "height")?),
            "refresh" => display.refresh = Some(decimal(&value, "refresh")?),
            _ => {}
        }
        Ok(())
    })?;
    if display.display_type.is_empty() {
        return Err(missing("display", "type"));
    }
    Ok(display)
}

fn parse_input(element: &BytesStart<'_>) -> Result<Input, String> {
    let mut players = None;
    let mut input = Input {
        players: 0,
        coins: None,
        service: false,
        tilt: false,
        controls: Vec::new(),
    };
    for_each_attribute(element, |key, value| {
        match key {
            "players" => players = Some(number(&value, "players")?),
            "coins" => input.coins = Some(number(&value, "coins")?),
            "service" => input.service = yes_no(&value)?,
            "tilt" => input.tilt = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    input.players = players.ok_or_else(|| missing("input", "players"))?;
    Ok(input)
}

fn parse_control(element: &BytesStart<'_>) -> Result<Control, String> {
    let mut control_type = None;
    let mut control = Control {
        control_type: String::new(),
        player: None,
        buttons: None,
        required_buttons: None,
        ways: None,
        ways2: None,
        ways3: None,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "type" => control_type = Some(value),
            "player" => control.player = Some(number(&value, "player")?),
            "buttons" => control.buttons = Some(number(&value, "buttons")?),
            "reqbuttons" => control.required_buttons = Some(number(&value, "reqbuttons")?),
            "ways" => control.ways = Some(value),
            "ways2" => control.ways2 = Some(value),
            "ways3" => control.ways3 = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    control.control_type = control_type.ok_or_else(|| missing("control", "type"))?;
    Ok(control)
}

fn parse_driver(element: &BytesStart<'_>) -> Result<Driver, String> {
    let mut status = None;
    let mut driver = Driver {
        status: String::new(),
        emulation: None,
        cocktail: None,
        save_state: None,
        requires_artwork: false,
        unofficial: false,
        no_sound_hardware: false,
        incomplete: false,
    };
    for_each_attribute(element, |key, value| {
        match key {
            "status" => status = Some(value),
            "emulation" => driver.emulation = Some(value),
            "cocktail" => driver.cocktail = Some(value),
            "savestate" => driver.save_state = Some(value),
            "requiresartwork" => driver.requires_artwork = yes_no(&value)?,
            "unofficial" => driver.unofficial = yes_no(&value)?,
            "nosoundhardware" => driver.no_sound_hardware = yes_no(&value)?,
            "incomplete" => driver.incomplete = yes_no(&value)?,
            _ => {}
        }
        Ok(())
    })?;
    driver.status = status.ok_or_else(|| missing("driver", "status"))?;
    Ok(driver)
}

fn parse_feature(element: &BytesStart<'_>) -> Result<Feature, String> {
    let mut feature_type = None;
    let mut status = None;
    let mut overall = None;
    for_each_attribute(element, |key, value| {
        match key {
            "type" => feature_type = Some(value),
            "status" => status = Some(value),
            "overall" => overall = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    Ok(Feature {
        feature_type: feature_type.ok_or_else(|| missing("feature", "type"))?,
        status,
        overall,
    })
}

fn parse_media_device(element: &BytesStart<'_>) -> Result<MediaDevice, String> {
    let mut device_type = None;
    let mut device = MediaDevice {
        device_type: String::new(),
        tag: None,
        interface: None,
        mandatory: None,
        fixed_image: None,
        instance_name: None,
        instance_brief_name: None,
        extensions: Vec::new(),
    };
    for_each_attribute(element, |key, value| {
        match key {
            "type" => device_type = Some(value),
            "tag" => device.tag = Some(value),
            "interface" => device.interface = Some(value),
            "mandatory" => device.mandatory = Some(value),
            "fixed_image" => device.fixed_image = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    device.device_type = device_type.ok_or_else(|| missing("device", "type"))?;
    Ok(device)
}

fn parse_media_device_child(
    device: &mut MediaDevice,
    child: &BytesStart<'_>,
) -> Result<(), String> {
    match child.name().as_ref() {
        "instance" => {
            device.instance_name = Some(required(child, "instance", "name")?);
            device.instance_brief_name = Some(required(child, "instance", "briefname")?);
        }
        "extension" => device
            .extensions
            .push(required(child, "extension", "name")?),
        _ => {}
    }
    Ok(())
}

fn parse_software_list(element: &BytesStart<'_>) -> Result<SoftwareListRef, String> {
    let mut tag = None;
    let mut name = None;
    let mut status = None;
    let mut filter = None;
    for_each_attribute(element, |key, value| {
        match key {
            "tag" => tag = Some(value),
            "name" => name = Some(value),
            "status" => status = Some(value),
            "filter" => filter = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    Ok(SoftwareListRef {
        tag,
        name: name.ok_or_else(|| missing("softwarelist", "name"))?,
        status: status.ok_or_else(|| missing("softwarelist", "status"))?,
        filter,
    })
}

fn parse_ram_option_attributes(element: &BytesStart<'_>) -> Result<(Option<String>, bool), String> {
    let mut name = None;
    let mut is_default = false;
    for_each_attribute(element, |key, value| {
        match key {
            "name" => name = Some(value),
            // CDATA in the DTD; MAME prints `yes`.
            "default" => is_default = value == "yes" || value == "1",
            _ => {}
        }
        Ok(())
    })?;
    Ok((name, is_default))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("testdata/listxml-sample.xml");

    fn parse_all(xml: &str) -> Result<(Vec<Machine>, Option<ListXmlHeader>), ParseError> {
        let mut parser = ListXmlParser::new(xml.as_bytes());
        let mut machines = Vec::new();
        while let Some(machine) = parser.next_machine()? {
            machines.push(machine);
        }
        Ok((machines, parser.header().cloned()))
    }

    fn sample_machine(name: &str) -> Machine {
        let (machines, _) = parse_all(SAMPLE).expect("sample parses");
        machines
            .into_iter()
            .find(|machine| machine.name == name)
            .unwrap_or_else(|| panic!("machine {name} not found"))
    }

    #[test]
    fn parses_header_and_all_machines_in_order() {
        let (machines, header) = parse_all(SAMPLE).expect("sample parses");

        let names: Vec<_> = machines.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "neogeo",
                "kof98",
                "pacman",
                "puckman",
                "005",
                "fruitbank",
                "c64",
                "z80"
            ]
        );
        assert_eq!(
            header,
            Some(ListXmlHeader {
                build: Some("0.280 (mame0280)".to_owned()),
                debug: false,
                mame_config: Some("10".to_owned()),
            })
        );
    }

    #[test]
    fn parses_bios_machine_with_bios_sets_and_roms() {
        let neogeo = sample_machine("neogeo");

        assert!(neogeo.is_bios);
        assert!(neogeo.runnable);
        assert_eq!(neogeo.description, "Neo-Geo MV-6F");
        assert_eq!(neogeo.year.as_deref(), Some("1990"));
        assert_eq!(neogeo.source_file.as_deref(), Some("neogeo/neogeo.cpp"));
        assert_eq!(neogeo.bios_sets.len(), 2);
        assert!(!neogeo.bios_sets[0].is_default);
        assert!(neogeo.bios_sets[1].is_default);
        assert_eq!(neogeo.roms[0].bios.as_deref(), Some("euro"));
        assert_eq!(neogeo.roms[0].size, 131_072);
        assert_eq!(neogeo.roms[0].status, "good");
        assert_eq!(neogeo.chips[0].clock, Some(4_000_000));
        assert_eq!(neogeo.sound_channels, Some(2));
        let input = neogeo.input.expect("input");
        assert_eq!(
            (input.players, input.coins, input.service),
            (2, Some(2), true)
        );
        assert_eq!(input.controls.len(), 2);
        assert_eq!(input.controls[0].ways.as_deref(), Some("8"));
    }

    #[test]
    fn aggregates_repeated_device_references() {
        let neogeo = sample_machine("neogeo");

        assert_eq!(
            neogeo.device_refs,
            vec![DeviceRef {
                name: "z80".to_owned(),
                count: 2
            }]
        );
    }

    #[test]
    fn keeps_parent_clone_and_rom_relations() {
        let pacman = sample_machine("pacman");
        let kof98 = sample_machine("kof98");

        assert_eq!(pacman.clone_of.as_deref(), Some("puckman"));
        assert_eq!(pacman.rom_of.as_deref(), Some("puckman"));
        // A parent machine that uses a BIOS has `romof` without `cloneof`.
        assert_eq!(kof98.clone_of, None);
        assert_eq!(kof98.rom_of.as_deref(), Some("neogeo"));
        assert_eq!(kof98.roms[1].merge.as_deref(), Some("sp-s2.sp1"));
        assert_eq!(kof98.roms[1].bios.as_deref(), Some("euro"));
    }

    #[test]
    fn applies_defaults_for_missing_optional_attributes() {
        let puckman = sample_machine("puckman");
        let pacman = sample_machine("pacman");

        assert!(!puckman.is_bios && !puckman.is_device && !puckman.is_mechanical);
        assert!(puckman.runnable);
        assert_eq!(puckman.clone_of, None);
        assert_eq!(puckman.sample_of, None);
        assert_eq!(puckman.sound_channels, None);
        let nodump = &pacman.roms[1];
        assert_eq!(nodump.status, "nodump");
        assert!(nodump.optional);
        assert_eq!(nodump.crc, None);
        assert_eq!(nodump.sha1, None);
        let driver = pacman.driver.expect("driver");
        assert!(!driver.requires_artwork && !driver.incomplete && !driver.unofficial);
        assert_eq!(driver.cocktail, None);
    }

    #[test]
    fn decodes_entities_and_keeps_text_verbatim() {
        let fruitbank = sample_machine("fruitbank");
        let kof98 = sample_machine("kof98");

        assert_eq!(fruitbank.description, "Fruit Bank & Co <Club> ©");
        assert_eq!(fruitbank.manufacturer.as_deref(), Some("<unknown>"));
        assert_eq!(fruitbank.year.as_deref(), Some("199?"));
        assert_eq!(kof98.description, "The King of Fighters '98 - The Slugfest");
    }

    #[test]
    fn parses_mechanical_machine_media_and_driver_flags() {
        let fruitbank = sample_machine("fruitbank");

        assert!(fruitbank.is_mechanical);
        assert_eq!(fruitbank.sample_of.as_deref(), Some("fruitsamples"));
        assert_eq!(fruitbank.disks[0].index, Some(0));
        assert!(fruitbank.disks[0].writable);
        assert_eq!(fruitbank.input.as_ref().map(|i| i.players), Some(0));
        let driver = fruitbank.driver.expect("driver");
        assert!(driver.requires_artwork && driver.incomplete);
        // Unknown feature types from newer MAME versions are kept verbatim.
        assert_eq!(fruitbank.features[0].feature_type, "mechanical-reels");
        assert_eq!(fruitbank.features[0].overall.as_deref(), Some("unemulated"));
    }

    #[test]
    fn parses_computer_devices_software_lists_and_ram_options() {
        let c64 = sample_machine("c64");

        assert_eq!(c64.media_devices.len(), 2);
        let cartridge = &c64.media_devices[0];
        assert_eq!(cartridge.device_type, "cartridge");
        assert_eq!(cartridge.interface.as_deref(), Some("c64_cart,vic10_cart"));
        assert_eq!(cartridge.mandatory.as_deref(), Some("1"));
        assert_eq!(cartridge.instance_brief_name.as_deref(), Some("cart"));
        assert_eq!(cartridge.extensions, ["80", "crt"]);
        assert_eq!(c64.media_devices[1].instance_name, None);
        assert_eq!(c64.software_lists.len(), 2);
        assert_eq!(c64.software_lists[0].filter.as_deref(), Some("NTSC"));
        assert_eq!(c64.software_lists[1].tag, None);
        assert_eq!(c64.software_lists[1].status, "compatible");
        assert_eq!(
            c64.ram_options,
            vec![
                RamOption {
                    name: Some("64K".to_owned()),
                    size: 65_536,
                    is_default: true
                },
                RamOption {
                    name: None,
                    size: 262_144,
                    is_default: false
                },
            ]
        );
        assert_eq!(c64.chips[1].clock, None);
    }

    #[test]
    fn parses_device_entries() {
        let z80 = sample_machine("z80");

        assert!(z80.is_device);
        assert!(!z80.runnable);
        assert_eq!(z80.year, None);
        assert_eq!(z80.manufacturer, None);
        assert_eq!(z80.driver, None);
    }

    #[test]
    fn accepts_legacy_game_element() {
        let xml = concat!(
            r#"<?xml version="1.0"?><mame build="0.150">"#,
            r#"<game name="pacman" cloneof="puckman"><description>Pac-Man</description>"#,
            r#"<driver status="good" color="good" sound="good" graphic="good"/></game>"#,
            r#"<game name="puckman"><description>Puck Man</description></game></mame>"#,
        );

        let (machines, _) = parse_all(xml).expect("legacy format parses");

        assert_eq!(machines.len(), 2);
        assert_eq!(machines[0].clone_of.as_deref(), Some("puckman"));
        assert_eq!(
            machines[0].driver.as_ref().map(|d| d.status.as_str()),
            Some("good")
        );
    }

    #[test]
    fn reports_truncated_document() {
        let cut = SAMPLE.find(r#"<machine name="c64""#).expect("c64") + 120;

        let error = parse_all(&SAMPLE[..cut]).expect_err("truncated document must fail");

        assert!(error.message.contains("'c64'"), "{error}");
    }

    #[test]
    fn reports_missing_root_end() {
        let xml = r#"<mame build="x"><machine name="a"><description>A</description></machine>"#;

        let error = parse_all(xml).expect_err("missing </mame> must fail");

        assert!(error.message.contains("missing </mame>"), "{error}");
    }

    #[test]
    fn reports_malformed_xml_and_invalid_values() {
        let cases = [
            (
                r#"<mame><machine name="a"><description>A</descr></machine></mame>"#,
                "malformed",
            ),
            (
                r#"<mame><machine name="a"><description>A</description><rom name="r" size="big"/></machine></mame>"#,
                "invalid number 'big'",
            ),
            (
                r#"<mame><machine name="a"><description>A</description><rom size="1"/></machine></mame>"#,
                "required attribute 'name'",
            ),
            (
                r#"<mame><machine name="a" isbios="maybe"><description>A</description></machine></mame>"#,
                "expected 'yes' or 'no'",
            ),
            (
                r#"<mame><machine><description>A</description></machine></mame>"#,
                "machine without name",
            ),
            (
                r#"<mame><machine name="a"><year>1980</year></machine></mame>"#,
                "missing <description>",
            ),
            (
                r#"<mame><machine name="a"><description>&bogus;</description></machine></mame>"#,
                "unknown entity",
            ),
            ("<other/>", "unexpected element <other>"),
            ("", "no <mame> root element"),
        ];
        for (xml, expected) in cases {
            let error = parse_all(xml).expect_err(xml);
            assert!(error.message.contains(expected), "{xml}: {error}");
        }
    }

    #[test]
    fn includes_machine_name_and_position_in_errors() {
        let xml = r#"<mame><machine name="broken"><description>A</description><sound channels="x"/></machine></mame>"#;

        let error = parse_all(xml).expect_err("invalid channels");

        assert!(error.message.starts_with("machine 'broken':"), "{error}");
        assert!(error.position > 0);
    }

    #[test]
    fn accepts_locale_decimal_comma_in_refresh_rate() {
        // Real output of MAME 0.280 for `arkanoiduo` on Windows with an Italian locale.
        let xml = concat!(
            r#"<mame><machine name="arkanoiduo"><description>Arkanoid</description>"#,
            r#"<display tag="screen" type="raster" rotate="90" width="256" height="224" refresh="59,185606"/>"#,
            r#"</machine></mame>"#,
        );

        let (machines, _) = parse_all(xml).expect("decimal comma accepted");

        assert_eq!(machines[0].displays[0].refresh, Some(59.185_606));
    }

    #[test]
    fn rejects_ambiguous_decimal_values() {
        for refresh in ["1,234,5", "1.234,5", "abc"] {
            let xml = format!(
                r#"<mame><machine name="a"><description>A</description><display type="raster" refresh="{refresh}"/></machine></mame>"#
            );

            let error = parse_all(&xml).expect_err(refresh);

            assert!(
                error.message.contains("invalid number"),
                "{refresh}: {error}"
            );
        }
    }

    #[test]
    fn accepts_document_without_machines() {
        let (machines, header) = parse_all(r#"<mame build="0.280"></mame>"#).expect("empty");

        assert!(machines.is_empty());
        assert!(header.is_some());
    }
}
