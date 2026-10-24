//! MAME joystick providers and what their device ids identify.
//!
//! MAME numbers joysticks in the order its provider enumerates them; a
//! `mapdevice` element moves the device whose id matches to a given index.
//! Device ids depend on the provider (verified with MAME 0.289 on Windows):
//!
//! - `winhybrid` (what `auto` selects on Windows) enumerates DirectInput
//!   devices first, as `<name> product_<guid> instance_<guid>`, then XInput
//!   devices as `XInput Player <n>`.
//! - `dinput` reports every device with its DirectInput instance GUID.
//!   Windows keeps one instance per device instance path: a device without
//!   its own USB serial number gets another instance on another USB port.
//! - `xinput` reports the XInput user index (`XInput Player 1` to `4`),
//!   assigned in the order the controllers were connected.
//! - `sdljoy` reports the SDL joystick GUID, which identifies the model
//!   (bus, vendor, product, version): identical devices share it.
//!
//! Nothing here asserts that an id identifies a physical device: the user
//! chooses the `device` binding when the id is known to follow the device.

use serde::{Deserialize, Serialize};

use super::model::ControllerBinding;
use crate::mame::platform;

/// The MAME joystick providers whose device ids YMDB knows how to map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoystickProvider {
    Winhybrid,
    Dinput,
    Xinput,
    Sdljoy,
}

impl JoystickProvider {
    pub const ALL: [Self; 4] = [Self::Winhybrid, Self::Dinput, Self::Xinput, Self::Sdljoy];

    /// The value of MAME's `-joystickprovider` option.
    pub fn name(self) -> &'static str {
        match self {
            Self::Winhybrid => "winhybrid",
            Self::Dinput => "dinput",
            Self::Xinput => "xinput",
            Self::Sdljoy => "sdljoy",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.name() == name)
    }

    /// Whether MAME builds for this operating system have the provider.
    pub fn is_available(self) -> bool {
        platform::JOYSTICK_PROVIDERS.contains(&self.name())
    }

    /// The providers of this operating system, the one MAME selects with
    /// `auto` first.
    pub fn available() -> Vec<Self> {
        platform::JOYSTICK_PROVIDERS
            .iter()
            .filter_map(|name| Self::from_name(name))
            .collect()
    }
}

/// What a device id says about the device behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceIdentity {
    /// `XInput Player <n>`: the XInput user index, in connection order.
    XinputSlot,
    /// A DirectInput instance GUID: one per Windows device instance, which
    /// depends on the USB port for devices without their own serial number.
    DirectInputInstance,
    /// An SDL joystick GUID: the device model, shared by identical devices.
    SdlModel,
    /// Another id format: nothing is assumed about it.
    Other,
}

impl DeviceIdentity {
    /// Whether a controller with this id may use `binding`. An XInput slot
    /// never identifies a device.
    pub fn allows(self, binding: ControllerBinding) -> bool {
        self != Self::XinputSlot || binding == ControllerBinding::PortOrder
    }

    pub fn allowed_bindings(self) -> Vec<ControllerBinding> {
        [ControllerBinding::Device, ControllerBinding::PortOrder]
            .into_iter()
            .filter(|binding| self.allows(*binding))
            .collect()
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::XinputSlot => {
                "an XInput player slot, assigned in the order the controllers were connected"
            }
            Self::DirectInputInstance => {
                "a DirectInput instance, which follows the USB port unless the device has its own serial number"
            }
            Self::SdlModel => "an SDL model GUID, shared by identical devices",
            Self::Other => "an identifier of the provider",
        }
    }
}

/// What the device id `id` reported by `provider` identifies.
pub fn identity(provider: JoystickProvider, id: &str) -> DeviceIdentity {
    let xinput = id
        .strip_prefix("XInput Player ")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()));
    match provider {
        JoystickProvider::Winhybrid | JoystickProvider::Xinput if xinput => {
            DeviceIdentity::XinputSlot
        }
        JoystickProvider::Winhybrid | JoystickProvider::Dinput
            if id.contains(" product_") && id.contains(" instance_") =>
        {
            DeviceIdentity::DirectInputInstance
        }
        JoystickProvider::Sdljoy if id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()) => {
            DeviceIdentity::SdlModel
        }
        _ => DeviceIdentity::Other,
    }
}

/// USB vendor and product ids encoded in a DirectInput product GUID
/// (`product_<PID><VID>-0000-0000-0000-504944564944`, "PIDVID" in ASCII).
pub fn direct_input_usb_ids(id: &str) -> Option<(String, String)> {
    let guid = id.split(" product_").nth(1)?.split(' ').next()?;
    if !guid
        .to_ascii_uppercase()
        .ends_with("-0000-0000-0000-504944564944")
    {
        return None;
    }
    let ids = guid.get(..8)?;
    if !ids.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((ids[4..].to_ascii_uppercase(), ids[..4].to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GP100_A: &str = "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000";
    const F310_D: &str = "Controller (Gamepad F310) product_c21d046d-0000-0000-0000-504944564944 instance_9c0d1e2f-3333-11f0-8001-444553540000";

    #[test]
    fn classifies_the_device_ids_of_each_provider() {
        use DeviceIdentity::*;
        use JoystickProvider::*;
        assert_eq!(identity(Winhybrid, "XInput Player 1"), XinputSlot);
        assert_eq!(identity(Xinput, "XInput Player 4"), XinputSlot);
        assert_eq!(identity(Winhybrid, GP100_A), DirectInputInstance);
        assert_eq!(identity(Dinput, F310_D), DirectInputInstance);
        assert_eq!(
            identity(Sdljoy, "0300007c1f08000001e4000000000000"),
            SdlModel
        );
        // The same text means nothing for another provider.
        assert_eq!(identity(Dinput, "XInput Player 1"), Other);
        assert_eq!(identity(Sdljoy, GP100_A), Other);
        assert_eq!(identity(Winhybrid, "XInput Player x"), Other);
    }

    #[test]
    fn xinput_slots_only_allow_the_port_order_binding() {
        assert_eq!(
            DeviceIdentity::XinputSlot.allowed_bindings(),
            [ControllerBinding::PortOrder]
        );
        for identity in [
            DeviceIdentity::DirectInputInstance,
            DeviceIdentity::SdlModel,
            DeviceIdentity::Other,
        ] {
            assert_eq!(
                identity.allowed_bindings(),
                [ControllerBinding::Device, ControllerBinding::PortOrder]
            );
        }
    }

    #[test]
    fn reads_usb_ids_from_direct_input_product_guids() {
        assert_eq!(
            direct_input_usb_ids(F310_D),
            Some(("046D".to_owned(), "C21D".to_owned()))
        );
        assert_eq!(
            direct_input_usb_ids(GP100_A),
            Some(("081F".to_owned(), "E401".to_owned()))
        );
        assert_eq!(direct_input_usb_ids("XInput Player 1"), None);
        assert_eq!(
            direct_input_usb_ids("pad product_12345678-1111-2222-3333-444455556666 instance_x"),
            None
        );
    }

    #[test]
    fn providers_round_trip_their_names() {
        for provider in JoystickProvider::ALL {
            assert_eq!(JoystickProvider::from_name(provider.name()), Some(provider));
            assert_eq!(
                serde_json::to_value(provider).expect("json"),
                provider.name()
            );
        }
        assert_eq!(JoystickProvider::from_name("auto"), None);
        assert!(!JoystickProvider::available().is_empty());
        assert!(
            JoystickProvider::available()
                .iter()
                .all(|p| p.is_available())
        );
    }
}
