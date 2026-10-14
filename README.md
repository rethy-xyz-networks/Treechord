# Treechord

<img src="Treechord.ico" alt="Treechord" width="128">

Treechord is a keybind execution system for Windows. Press a key chord and Treechord checks whether the program is already open. If it is, the program is raised to the current view. If it isn't, the program is launched. If several windows of the program exist, each subsequent press cycles through them.

Keybinds are stored in a JSON file, found by default in your user home directory.

## Key chords

1. Windows + {Letter}
2. Windows + Shift + {Letter}
3. Windows + Alt + {Letter}
4. Windows + Enter and Windows + Shift + Enter

Windows + D, Windows + R and Windows + X (without Shift or Alt) are left to Windows. Windows + Ctrl + R reloads the configuration file.

Treechord also supports the five Favorites keys of the Microsoft Natural Ergonomic Keyboard 4000 (`bindLaunch1` to `bindLaunch5`). Without that keyboard, everything else works as normal.

## Download

Each [release](../../releases) includes a build for each architecture:

- `Treechord-<version>-x86_64.exe`
- `Treechord-<version>-x86.exe`
- `Treechord-<version>-arm64.exe`

Run the executable. It stays in the system tray, where you can reload the configuration, suspend the hotkeys, or exit.

## Configuration

The configuration file is `C:\Users\<user>\Treechord.json`. If it doesn't exist, it is created with the defaults below when Treechord first runs.

### [Window class, Program path] vs. [Program path]

Most of the time, the program path is enough to identify the window:

```json
"bindShiftG": "C:/tools/vim/vim91/gvim.exe",
```

Some programs can't be identified that way. Explorer is the usual example, since every Explorer window belongs to the same executable as the desktop and taskbar. For those, give the window class in addition to the program path:

```json
"bindE": [ "CabinetWClass", "C:/Windows/explorer.exe" ],
```

An empty value (`""`) leaves the chord unassigned, but still reserves it so Windows doesn't act on it.

### Default configuration

```json
{
    "bindA": "",
    "bindB": "",
    "bindC": "",
    "bindE": [ "CabinetWClass", "C:/Windows/explorer.exe" ],
    "bindF": "",
    "bindG": "",
    "bindH": "",
    "bindI": "",
    "bindJ": "",
    "bindK": "",
    "bindL": "",
    "bindM": "",
    "bindN": "",
    "bindO": "",
    "bindP": "",
    "bindQ": "",
    "bindS": "",
    "bindT": "",
    "bindU": "",
    "bindV": "",
    "bindW": "",
    "bindY": "",
    "bindZ": "",

    "bindShiftA": "",
    "bindShiftB": "",
    "bindShiftC": "",
    "bindShiftD": "",
    "bindShiftE": "",
    "bindShiftF": "",
    "bindShiftG": "",
    "bindShiftH": "",
    "bindShiftI": "",
    "bindShiftJ": "",
    "bindShiftK": "",
    "bindShiftL": "",
    "bindShiftM": "",
    "bindShiftN": "",
    "bindShiftO": "",
    "bindShiftP": "",
    "bindShiftQ": "",
    "bindShiftR": "",
    "bindShiftS": "",
    "bindShiftT": "",
    "bindShiftU": "",
    "bindShiftV": "",
    "bindShiftW": "",
    "bindShiftX": "",
    "bindShiftY": "",
    "bindShiftZ": "",

    "bindAltA": "",
    "bindAltB": "",
    "bindAltC": "",
    "bindAltD": "",
    "bindAltE": "",
    "bindAltF": "",
    "bindAltG": "",
    "bindAltH": "",
    "bindAltI": "",
    "bindAltJ": "",
    "bindAltK": "",
    "bindAltL": "",
    "bindAltM": "",
    "bindAltN": "",
    "bindAltO": "",
    "bindAltP": "",
    "bindAltQ": "",
    "bindAltR": "",
    "bindAltS": "",
    "bindAltT": "",
    "bindAltU": "",
    "bindAltV": "",
    "bindAltW": "",
    "bindAltX": "",
    "bindAltY": "",
    "bindAltZ": "",

    "bindEnter": "",
    "bindShiftEnter": "",

    "bindLaunch1": "",
    "bindLaunch2": "",
    "bindLaunch3": "",
    "bindLaunch4": "",
    "bindLaunch5": ""
}
```

If the file contains an unknown key or a malformed value, Treechord reports it and suspends the hotkeys until the file is fixed and reloaded.

## Command line

```
treechord.exe [CONFIG.json]                     run, optionally with another configuration file
treechord.exe --check [CONFIG.json]             validate the configuration and list its bindings
treechord.exe --trigger bindE [CONFIG.json]     run one binding and exit
```

## Building

```
cargo build --release
```

This needs the [Rust toolchain](https://rustup.rs) and, on Windows, the Visual Studio Build Tools with the C++ workload. Releases are built by `.github/workflows/release.yml` when a GitHub release is published.
