# Personal remote repository

YMDB can download missing ROM, CHD and sample files from a **remote repository**: a
computer of yours (a NAS, a home server, a Linux machine) that stores your MAME
collections and is reachable over SSH. The download happens on demand, for the title
you are about to play, through **SFTP**. Remote repositories are optional: without
them YMDB works fully offline with the files MAME already finds.

> A "remote repository" here is a file server with your MAME content. It has nothing
> to do with the GitHub repository of YMDB's source code.

You must own or be entitled to use the content you store. YMDB does not provide,
locate or share ROMs, BIOS files or CHDs.

## How it works

```text
┌─────────────────────────────┐        SSH / SFTP         ┌──────────────────────────────┐
│ YMDB desktop                │  (read only, on demand)   │ Personal remote repository    │
│  ROM Manager: plan, lookup, ├──────────────────────────►│  <base>/  Machine ROMs, BIOS  │
│  download                   │                           │  <base>/  Machine CHDs        │
└──────────────┬──────────────┘                           │  <base>/  Software List ROMs  │
               │ writes whole files into                  │  <base>/  Software List CHDs  │
               ▼                                          │  <base>/  Samples             │
┌─────────────────────────────┐                           └──────────────────────────────┘
│ Local destinations          │
│ (MAME rompath / samplepath, │
│  or a custom directory)     │
└──────────────┬──────────────┘
               │ searched by
               ▼
┌─────────────────────────────┐
│ Local MAME (runs the game)  │
└─────────────────────────────┘
```

- **YMDB** works out, from the imported catalog, which whole files (ROM archives,
  CHDs, samples) a launch may need, checks which ones MAME already finds locally,
  looks up the missing ones on the server (one lookup per file, no listing of the
  whole server) and downloads them.
- **The remote server** only serves files over SFTP. YMDB never writes, renames or
  deletes anything on it and never runs remote commands.
- **The local destinations** are ordinary directories on your computer. Downloaded
  files stay there permanently, like files you copied yourself; there is no separate
  cache and nothing is deleted automatically.
- **The local MAME** runs the game from the local files. Games are never streamed
  from the server.

Configuration is split in two:

1. **Remote connections** (global, **Settings → Remote Connections**): how to reach a
   server.
2. **ROM Manager settings** (per MAME installation, **MAME → <installation> → ROM
   Manager**): for each category of files, which connection and remote directory to
   use, how the collection is organized and where downloaded files go.

## What is supported

| Aspect                                               | Supported                                                             |
| ---------------------------------------------------- | --------------------------------------------------------------------- |
| Protocol                                             | SFTP over SSH (the only provider; no HTTPS, SMB, FTP or S3)           |
| Authentication                                       | Private key files: OpenSSH (ed25519, ECDSA, RSA), PEM/PKCS#8, PuTTY   |
| Key passphrases                                      | Supported; asked when needed, never saved                             |
| Passwords                                            | Not supported (no password or keyboard-interactive login)             |
| SSH agent, `~/.ssh/config`, jump hosts, certificates | Not supported                                                         |
| Server identity                                      | Always verified: approved in YMDB, or read from OpenSSH `known_hosts` |
| Files downloaded                                     | `.zip` archives, `.chd` disks, `.wav` samples (whole files)           |
| Remote operations                                    | Read only: stat, directory listing (Browse), download                 |

## Preparing the server

This example uses a Linux server with OpenSSH; a NAS with SFTP enabled works the same
way. Replace `nas.example.lan`, `ymdb` and `/srv/mame` with your own names.

### 1. Create an account for YMDB

Use an unprivileged account that can read the collection. Normal access never needs
root.

```sh
sudo adduser --disabled-password ymdb
```

### 2. Store the collection

Create one directory per MAME version and category, readable by the account:

```sh
sudo mkdir -p /srv/mame/0.280/roms /srv/mame/0.280/software /srv/mame/0.280/samples
sudo chown -R root:ymdb /srv/mame
sudo chmod -R u=rwX,g=rX,o= /srv/mame
```

YMDB only reads: the account needs read permission on files and read/execute
permission on directories, and no write permission.

### 3. Create a key pair on your computer

On the computer where YMDB runs (OpenSSH's `ssh-keygen` is available on Windows 10
and later, macOS and Linux):

```sh
ssh-keygen -t ed25519 -f ~/.ssh/ymdb_ed25519 -C "ymdb"
```

A passphrase is optional; YMDB asks for it each time it connects. Prefer ed25519
keys: RSA keys work, but the RSA implementation used by YMDB has a known timing side
channel (RUSTSEC-2023-0071).

Authorize the public key for the `ymdb` account on the server, for example by
appending the content of `~/.ssh/ymdb_ed25519.pub` to
`/home/ymdb/.ssh/authorized_keys` (directory mode `700`, file mode `600`, owned by
`ymdb`), or with `ssh-copy-id` if the account has another way to log in.

### 4. Optionally restrict the account to SFTP

YMDB only uses the SFTP subsystem, so the account can be limited to it in
`/etc/ssh/sshd_config`:

```text
Match User ymdb
    ForceCommand internal-sftp
    AllowTcpForwarding no
    X11Forwarding no
```

Reload the SSH service afterwards. No other service needs to be exposed.

### 5. Note the server fingerprint

YMDB shows the server's fingerprint on the first connection and asks you to approve
it. Get the expected value on the server, from a trusted session, to compare:

```sh
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

Compare it with the key type YMDB shows (`ssh-ed25519`, `ecdsa-sha2-nistp256`,
`rsa-sha2-512`, …); each host key type has its own fingerprint.

### 6. Test from your computer (optional)

```sh
sftp -i ~/.ssh/ymdb_ed25519 ymdb@nas.example.lan
```

Then `ls /srv/mame/0.280/roms` inside `sftp` should list the collection.

## Organizing the collection

YMDB looks up exact file names built from the catalog of the MAME installation, below
the **Remote base path** of each category:

| Category           | Path below the remote base path | Example                        |
| ------------------ | ------------------------------- | ------------------------------ |
| Machine ROMs       | `<set>.zip`                     | `pacman.zip`, `neogeo.zip`     |
| Machine CHDs       | `<set>/<disk>.chd`              | `<set>/<disk>.chd`             |
| Software List ROMs | `<list>/<software>.zip`         | `nes/smb.zip`                  |
| Software List CHDs | `<list>/<software>/<disk>.chd`  | `<list>/<software>/<disk>.chd` |
| Samples (ZIP)      | `<sample set>.zip`              | `<sample set>.zip`             |
| Samples (folders)  | `<sample set>/<sample>.wav`     | `<sample set>/<sample>.wav`    |

These are the names MAME itself expects in its `rompath` and `samplepath`, so a
collection that MAME can use locally can be served as it is. BIOS and device sets
are ordinary `<set>.zip` files among the machine ROMs. Machine ROMs and Machine CHDs
can share one directory (as in a MAME `roms` folder), and so can software list ROMs
and CHDs.

A recommended layout keeps one tree per MAME version, because ROM sets change between
MAME releases:

```text
/srv/mame/0.280/
  roms/                  # Machine ROMs and Machine CHDs
    <set>.zip
    <bios set>.zip
    <set>/<disk>.chd
  software/              # Software List ROMs and Software List CHDs
    <list>/<software>.zip
    <list>/<software>/<disk>.chd
  samples/               # Samples
    <sample set>.zip
```

Only `.zip` archives are downloaded. `.7z` archives and `.flac` samples are recognized
when they are already present locally, but are not looked up on the server.

### Merged, split and non-merged sets

Set **Collection organization** to how your ROM and CHD collection was built:

| Organization   | What the archives contain                                           |
| -------------- | ------------------------------------------------------------------- |
| **Merged**     | A parent archive contains its clones; clones have no archive        |
| **Split**      | A clone archive contains only the files that differ from its parent |
| **Non-merged** | Every archive is complete (parent files included in each clone)     |

YMDB never opens, rebuilds or converts archives. For a launch it always considers
the archive of the machine and of every parent and BIOS it depends on (following
MAME's `cloneof` and `romof`), plus the archives of its devices that have ROMs. The
organization tells YMDB which of these may legitimately be absent (for example a
clone's archive in a merged collection, or a parent in a non-merged one), so those
absences are reported as information rather than warnings, and where clone CHDs are
stored. This is conservative: a parent or BIOS archive may be downloaded even when
the collection would not strictly need it.

For samples, choose **ZIP archives** or **Directories**.

## Configuring YMDB

### Add the connection

1. Open **Settings → Remote Connections** and choose **Add connection…**.
2. Fill in **Connection name** (for example `Home NAS`), **Host** (a host name or IP
   address only, such as `nas.example.lan`), **Port** (22 by default) and
   **Username** (`ymdb`).
3. Choose the **Private key file** with **Choose…** (for example
   `~/.ssh/ymdb_ed25519`, the private file, not the `.pub`). Only its path is saved;
   the key is never copied.
4. Choose the **Host key verification**:
   - **Approve the server identity in YMDB**: the first connection shows the
     fingerprint and asks for approval; YMDB then remembers that identity.
   - **Use OpenSSH known_hosts, then approval in YMDB**: if your `~/.ssh/known_hosts`
     already lists the server (for example after step 6 above), that identity is
     used; otherwise YMDB asks for approval. YMDB only reads that file.
5. Choose **Save connection**.
6. Choose **Test Connection**. YMDB resolves the host, connects, verifies the server
   identity, authenticates with the key and opens SFTP. If the key has a passphrase,
   enter it when asked. If the identity is not approved yet, compare the fingerprint
   with the one you noted and choose **Approve and connect**.
7. Optionally choose **Browse…** to list remote directories (read only) and find the
   exact paths of your collection.

An identity that changes is always refused. If the change is expected (for example a
reinstalled server), use **Forget approved identity…** and approve the new one; for
identities from `known_hosts`, fix that file. Editing the host or port forgets the
approved identity. A connection used by any installation's ROM Manager settings
cannot be removed.

### Configure the ROM Manager of an installation

1. Open the **MAME** page and the **ROM Manager** section of the installation whose
   version matches the collection.
2. For each category you want (**Machine ROMs**, **Machine CHDs**, **Software List
   ROMs**, **Software List CHDs**, **Samples**), set:
   - **Enabled**;
   - **Remote connection**: the connection added above;
   - **Remote base path**: an absolute path on the server, for example
     `/srv/mame/0.280/roms`;
   - **Collection organization**: **Merged**, **Split** or **Non-merged** (ROMs and
     CHDs), **ZIP archives** or **Directories** (samples);
   - **Local destination**: **A MAME directory** (an entry of `rompath` for ROMs and
     CHDs, of `samplepath` for samples, taken from the installation's launch options
     or from `mame.ini`; choose one you can write to) or **A custom directory**
     (chosen with the folder picker).
3. Choose **Save ROM Manager settings**. Incomplete enabled categories are reported
   before saving; **Discard changes** reverts them.

Categories are independent: configure only those you need. A custom directory is
added to MAME's search path when YMDB launches MAME: YMDB passes
`-rompath`/`-samplepath` with the directories that already apply (from your launch
options or `mame.ini`) followed by the custom directory, so nothing MAME already finds
is lost. `mame.ini` is never modified. The command line preview shows these options
as coming from the ROM Manager.

Two installations may use the same local directory; YMDB warns about it, because
files with the same name can differ between MAME versions.

## Downloading missing files

The launch bar of a machine or software detail has two icon buttons (hover them for a
description):

- **Prepare Download** builds a _download plan_ for the launch as configured
  (installation, machine, software, mounted parts, profile) and shows it. Nothing is
  downloaded until you choose **Download N files**.
- **Download & Play** builds the same plan; after you confirm (**Download N & Play**,
  or **Play** when nothing is missing) YMDB downloads the available missing files and
  then starts MAME with the same installation and options.

The plan lists every candidate file with its category, the reason it is included
(**Selected machine**, **Parent**, **BIOS**, **Device**, **Selected software**,
**Parent software**, **Machine running the software**, **Samples**), its status and
size, and where it would be written:

| Status                     | Meaning                                                                      |
| -------------------------- | ---------------------------------------------------------------------------- |
| **Present locally**        | Found where MAME will search for this launch                                 |
| **To download**            | Missing locally, found on the server                                         |
| **Not found remotely**     | Missing locally and on the server (a warning)                                |
| **Missing, not looked up** | Missing locally and no enabled source for its category                       |
| **Unresolved**             | The lookup failed for a technical reason, or the name cannot be used locally |

Missing files never block the plan: the files that exist are still downloaded. A
technical failure (timeout, refused login, unapproved identity) is reported as such,
never as "not found". **Check again** builds the plan again.

During the download the window shows the current file, the bytes and the overall
percentage. Closing the window (**Hide**, or **Esc**) does not cancel anything; a
**Downloading …%** button in the launch bar shows it again. **Cancel download** stops
the transfer. Downloads run one file at a time and continue if you leave the page.

Each file is written first to `<name>.ymdb-download` next to its destination; its
size is checked and only then does it get its final name. **Existing files are never
replaced**, and a cancelled or failed transfer leaves no partial file. Completed files
are kept when a download is cancelled or fails; **Retry** downloads the rest.
Interrupted transfers are not resumed: the next attempt starts the file again.

The result is **Download completed**, **Download completed with warnings**,
**Download failed** or **Download cancelled**. **Download & Play** starts MAME only
when no transfer failed and nothing was cancelled; files that were not available do
not prevent it, because MAME decides whether the files present are enough. Downloaded
files are not verified against MAME's ROM data. The normal **Play** button never uses
the ROM Manager.

## End-to-end example

Starting point:

- MAME 0.280 is registered in YMDB and its catalog is imported (see
  [Getting started](getting-started.md)).
- The server is prepared as above, with `/srv/mame/0.280/roms` holding a non-merged
  collection of ROM sets you are entitled to use (for example the ROMs the MAME
  project makes available for non-commercial use, listed on the MAME website).
- The chosen title, here called `<machine>`, is not present in any local `rompath`
  directory.

Steps:

1. **Settings → Remote Connections → Add connection…**: `Home NAS`,
   `nas.example.lan`, port `22`, user `ymdb`, key `~/.ssh/ymdb_ed25519`, **Approve
   the server identity in YMDB**. **Save connection**, then **Test Connection**,
   compare the fingerprint and **Approve and connect**.
2. **MAME → MAME 0.280 → ROM Manager → Machine ROMs**: **Enabled**, connection
   `Home NAS`, remote base path `/srv/mame/0.280/roms`, organization **Non-merged**,
   local destination **A MAME directory** `roms` (or a custom directory such as
   `D:\MAME\downloads`). **Save ROM Manager settings**.
3. In the **Catalog**, search for `<machine>` and open its detail.
4. Choose **Prepare Download**. The plan shows `<machine>.zip` as **To download**
   (with any BIOS or device sets it needs) and where it will be written.
5. Close the plan and choose **Download & Play**, then **Download N & Play**. YMDB
   downloads the files and starts MAME. If you chose a custom directory, the launch
   includes `-rompath` with that directory after MAME's own paths.
6. The next launches use the local files directly: **Play** works without the
   server, and a new plan shows the files as **Present locally**.

## Limitations

- SFTP is the only provider; key authentication only, with passphrases asked again
  for every connection (no credential manager or agent).
- No ROM audit: YMDB never checks archive contents, rebuilds sets or converts between
  merged, split and non-merged.
- Requirements stated only in free-form software features are not followed.
- No resume of partial transfers, no parallel downloads, and plans and downloads are
  not kept across restarts.
- Remote directory listings in **Browse…** are limited to 2,000 entries.
- Automated tests cover the SFTP provider against an in-process SSH server; a real
  NAS or OpenSSH server over a network has not been part of the automated
  verification.

## Troubleshooting

Problems fall into three groups: reaching the server, finding the content, and MAME
itself. The messages below are the ones YMDB shows.

### Connection problems

| Message (summary)                                                                | What to check                                                                                                                                              |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "The server cannot be reached"                                                   | Host name or address, port, that the SSH service runs, firewalls, VPN; **Host** must not include `user@` or a path                                         |
| "The server did not answer in time"                                              | Network reachability, server load; try again                                                                                                               |
| "… has not been approved yet"                                                    | Normal on the first connection: compare the fingerprint and approve it                                                                                     |
| "The identity of … changed. The connection was refused"                          | Do not approve blindly. If the server was reinstalled, verify the new fingerprint on the server, then **Forget approved identity…** (or fix `known_hosts`) |
| "The private key file cannot be read" / "is not valid"                           | The path points to the private key (not `.pub`), the file exists and is a supported format                                                                 |
| "The private key is protected by a passphrase" / "The passphrase is not correct" | Enter the key's passphrase                                                                                                                                 |
| "The server refused the private key for this user"                               | The public key is in the account's `authorized_keys`, file permissions, the right **Username**                                                             |
| "The server reported an error"                                                   | For example the SFTP subsystem is not enabled on the server                                                                                                |
| "The connection was interrupted"                                                 | Network drop during a transfer: **Retry**; the partial file is discarded                                                                                   |

### Content problems

| Symptom                                            | What to check                                                                                                                                  |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| "Access to … was denied by the server"             | Read permission for the account on the file and execute permission on every parent directory                                                   |
| "… does not exist on the server" for the base path | **Remote base path** is absolute and correct; use **Browse…**                                                                                  |
| **Not found remotely**                             | The exact file (`<set>.zip`, `<set>/<disk>.chd`, `<list>/<software>.zip`) is not below the base path; names are case-sensitive on most servers |
| **Missing, not looked up**                         | The category is not enabled, or has no connection                                                                                              |
| "A local file could not be written"                | The local destination exists and is writable; free disk space                                                                                  |
| File kept as **Kept existing file**                | A file with that name appeared locally; YMDB never replaces it                                                                                 |
| Download failed with a size mismatch               | The file changed on the server or the transfer was truncated: **Retry**                                                                        |

### MAME problems

When the files are present but MAME refuses to start, the launch status shows MAME's
message and output. Typical causes:

- **Wrong MAME version**: the collection was built for another MAME release, so
  archives miss files or have wrong checksums. Keep one collection per MAME version
  and point each installation to its own.
- **Missing BIOS or device sets**: they are part of the plan when the catalog
  declares them; check that they exist on the server.
- **Incomplete or wrong-organization sets**: a merged collection configured as
  non-merged (or the opposite) can lead to missing parents. YMDB does not audit
  archives; use a ROM manager tool of your choice to verify the collection.
- **Incomplete CHD**: a CHD whose size matches the server's but whose content is
  damaged is not detected by YMDB; verify it on the server (for example with MAME's
  `chdman verify`).

See also [Troubleshooting](troubleshooting.md).
