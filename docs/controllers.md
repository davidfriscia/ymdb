# Controllers

MAME configures inputs in its own menus and saves them in its own files. YMDB never
edits MAME's `cfg` files and has no button editor. What YMDB adds is a way to keep
separate input configurations (controller profiles), to choose them per machine or
software, and to map physical joysticks to players.

Three names are easy to confuse:

- a **launch profile** is a set of launch options (see
  [Launching games](launcher.md#launch-options));
- a **controller profile** is an input configuration: with it, MAME reads and saves
  its configuration (`default.cfg`, `<machine>.cfg`) in a directory of YMDB for that
  profile and MAME installation, optionally starting from a MAME controller
  configuration file used as baseline (`-ctrlr`);
- a **controller** is the logical identity of one of your devices (name, kind, MAME
  device class and, for a joystick or gamepad, the device ID MAME reports, its
  joystick provider and a binding). Controllers can be assigned to players.

The built-in **MAME configuration** controller profile, the initial default, passes
nothing to MAME and keeps MAME's own configuration.

## Controller profiles

**Settings → Controllers** has three blocks:

- **Controller Profiles**: create a profile (empty or from a controller file), rename
  it, import, replace or remove its baseline file, or delete it when no saved
  assignment uses it.
- **Controllers**: detect devices, register, edit and remove controllers.
- **Default Assignments**: the default controller profile, the default players and
  the saved assignments of machines and software.

Each user profile has its own MAME configuration directory per installation
(`installations/<id>/controller-profiles/<profileId>/cfg` in the configuration
directory), passed to MAME as `-cfg_directory`. MAME's configuration files also hold
DIP switches, slot, mixer and video settings, so these are per controller profile
and installation too: a new profile starts from MAME's defaults.

## Choosing a profile for a title

On a detail page, the **Controls** selector of the launch bar shows the controller
profile that applies and where it comes from (for example "specific to this
machine"). Choosing another profile applies to the launches from that page only;
**Save as preference** saves it for the machine, the software or the software on the
selected machine. **Advanced → Controls** shows each saved level with **Reset to
inherited**.

Precedence, from the weakest: default, machine, software, software on a given
machine, then the temporary selection.

## Assigning buttons in MAME

Use **Advanced → Controls → Configure Controls in MAME**: YMDB starts the machine or
software with the controller profile, and you open MAME's menu (normally **Tab**) →
**Input Settings** → **Input Assignments (this system)** for the current machine, or
**Input Assignments (general)** for every machine. MAME saves the changes when you
exit it normally; stopping MAME from YMDB discards them. The next launch with the
same profile finds them; other profiles are not affected.

## Physical controllers and players

**Detect devices** (in **Settings → Controllers**) asks MAME, only when you click it
and without a window, which joysticks are connected, with the joystick provider you
choose (`winhybrid`, MAME's default on Windows, `dinput`, `xinput` or `sdljoy`,
depending on the platform). Each device shows its MAME device ID; **Register…**
creates a controller from it. Nothing is registered automatically, and devices MAME
cannot tell apart (for example identical gamepads with `sdljoy`) cannot be
registered.

Each controller has a **binding**:

- **Device binding**: the device ID identifies this device wherever it is connected
  (for example a controller with its own USB serial number). Choose it only if the
  ID stays the same when the controller is plugged into another USB port: detect
  again to check.
- **Port/order binding**: the ID follows the USB port or the order in which the
  provider lists devices (identical gamepads without a serial number, XInput slots).
  Swapping ports may change player assignments.

Assign controllers to players in **Settings → Controllers** (default players), in
**Advanced → Controls** (per machine, software or software on a machine) or
temporarily with **Players** in the launch bar (**Save players as preference** saves
them). Each launch with assigned players generates a controller file with MAME
`mapdevice` entries (keeping the sections of the profile's baseline file, or of the
`ctrlr` set in `mame.ini` for the MAME configuration profile), passes the
controllers' joystick provider and `-verbose`, and checks MAME's output: the launch
status shows for each player whether the mapping was applied to the device, applied
by port order only, or whether the device was not connected. The command line
preview shows the mappings and the generated file before the launch. Without
assigned players, launches are exactly as before.

## Limitations

- Identical controllers are told apart only by their USB port or enumeration order.
- Only joysticks are mapped; light guns, mice and keyboards keep MAME's order.
- All mapped controllers of a launch must use the same joystick provider.
- With the MAME configuration profile, a `ctrlr` set by an INI file other than
  `mame.ini` prevents mapped launches.
- Mapping verification relies on the verbose messages of MAME 0.289; other versions
  may report an unknown status. Device detection and mapping were verified on
  Windows only.
- Two sessions using the same profile only get a warning: the last one to exit keeps
  its general settings.

See [ARCHITECTURE.md](../ARCHITECTURE.md#controller-management--implemented) for the
entities, files and generated controller files in detail.
