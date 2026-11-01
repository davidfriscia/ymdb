# Launching games

YMDB starts the executable of the active MAME installation for a machine or a
software, with that installation's launch options. MAME does the emulation; YMDB
builds the command line, starts MAME and follows the session.

Requirements:

- the installation's catalog was imported from that same executable (reimport it
  after updating MAME in place);
- MAME finds the files the title needs (ROM sets, CHDs, samples) in its search paths.
  YMDB does not check this before launching: when files are missing, MAME refuses to
  start and YMDB shows its message and output. To fetch missing files from your own
  server, see [Remote repository](remote-repository.md).

## Playing

- **Machines:** open a machine and choose **Play**. YMDB runs
  `mame <machine> [options]` from the MAME directory, so MAME reads its own
  `mame.ini` and relative paths exactly as when you run it there.
- **Software:** open a software, choose the machine and choose **Play**. The machine
  list contains the machines declaring the software list, with the relation MAME
  declares (original or compatible list), whether the list filter (for example NTSC
  or PAL) matches the software, and whether the machine has a media device for it.
  A machine is preselected only when it is the only one that is original and
  matching; otherwise you choose. Each part of the software is mounted on a media
  device, for example
  `mame apple2e -flop1 apple2_flop_orig:ultima4:flop1 -flop2 apple2_flop_orig:ultima4:flop2`;
  the assignment can be changed before launching. This information comes from MAME's
  catalog and does not guarantee that a software works on a machine.

The launch bar below the artwork holds **Play**, the launch **Profile**, the
**Controls** selector (see [Controllers](controllers.md)), **Prepare Download** and
**Download & Play** (see
[Downloading missing files](remote-repository.md#downloading-missing-files)).

The **Advanced** section (collapsed when a detail opens; its counter shows the number
of options set for the title) holds **Options for this machine** or **Options for
this software**, **Controls**, and **Command line**, which shows the exact command
that **Play** runs: executable, working directory and the origin of every option.

## Sessions

While MAME runs, the detail page shows the session and the installation it was
started from. **Stop** terminates MAME after a confirmation; unsaved progress, NVRAM
and configuration changes of that session are lost, because MAME is killed. Game
sessions have no timeout. Closing YMDB or switching the MAME version never closes
MAME, and the same machine can run from two installations at once. Sessions started
before YMDB restarts are not tracked after the restart.

After a session, the launch status shows whether it completed or failed, with MAME's
message and exit code; **Last launch** shows the start and end times and the end of
MAME's output. The output of the 20 most recent sessions is kept in `launch-logs/` in
the local data directory.

## Launch options

YMDB passes only the options you set; everything else follows MAME's own
configuration, which YMDB never modifies. Options are set in the **Launch Options**
section of each installation on the **MAME** page, per installation, because option,
device and machine names can change between MAME versions. They are never shared or
copied between installations.

Levels, the later ones taking precedence over MAME's own configuration and the
earlier levels:

1. **Global options** (Launch Options of the installation).
2. **Launch profiles** (Launch Options of the installation): named sets of options,
   chosen with the **Profile** selector when launching; the default profile applies
   otherwise. The `Default` profile cannot be deleted.
3. **Options for this machine** (machine detail, **Advanced**).
4. **Options for this software** (software detail, **Advanced**).

The available options cover display, performance, sound, search paths and output
directories, BIOS and RAM size and session options (for example `seconds_to_run`).
Only the options YMDB defines are offered, grouped by category, and values are
checked before saving; anything else is configured in MAME's own INI files. Options
are saved in `installations/<id>/launch.json` in the configuration directory.

When the ROM Manager uses custom local directories, YMDB extends
`-rompath`/`-samplepath` after these levels (see
[Remote repository](remote-repository.md#configure-the-rom-manager-of-an-installation)).

A **launch profile** (a set of launch options) is unrelated to a **controller
profile** (an input configuration, see [Controllers](controllers.md)).

## Limitations

- ROM, CHD and software media availability is not checked before launching.
- Slot options (cards, expansions) are not configurable; parts are mounted on the
  media devices of the machine's default configuration.
- The `requirement` feature of software parts and machine RAM options are not
  evaluated for compatibility.
- Profiles and overrides cannot be copied between installations.

The command syntax, the modules and the verification with real MAME builds are
described in [ARCHITECTURE.md](../ARCHITECTURE.md#launcher--implemented).
