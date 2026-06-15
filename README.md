# 🐭 Mackrey

A simple keyboard + mouse **macro recorder and player** for Windows, written in Rust with an [egui](https://github.com/emilk/egui) GUI.

Record what you type and click (with the real timing between actions), edit the
steps in a list, save them to a library, and replay them — either as real input
or posted in the background to a specific window.

## Features

- **Global recording** via low-level Windows hooks — works no matter which app is focused.
- **Record hotkey** (default **F9**) to start/stop recording, plus a Record button.
- **Timed steps**: every key/click stores the delay since the previous action.
- **Editable list**: change delays, reorder (↑/↓), delete, edit key codes / coordinates / buttons.
- **Library**: save macros as JSON and reload them (`macros/` folder next to the exe).
- **Two playback modes**:
  - **Blocking** — `SendInput` synthesizes real OS input (reliable, works with games). Optionally brings the target window to the front first.
  - **Background** — `PostMessage` delivers input to a chosen target window without stealing focus (best-effort; some apps/games ignore it).
- **Speed** multiplier and **repeat** count.
- Optional **mouse-move** recording (off by default to keep lists short).

## Build & run

Requires Rust **1.85+** (uses dependencies on the 2024 edition).

```powershell
cargo run --release
```

The release build hides the console window.

## Usage

1. Launch Mackrey.
2. Press **F9** (or click **⏺ Record**) and perform your keys/clicks.
3. Press **F9** again to stop. The steps appear in the center list.
4. Tweak delays/actions, reorder, or delete steps as needed.
5. Give it a **Name** and click **💾 Save to library**.
6. On the right, pick a **playback mode**, optionally a **target window**, set
   **speed**/**repeat**, and click **▶ Play**. Use **■ Stop** to abort.

> **Tip:** For Background mode you must choose a target window. For Blocking
> mode, selecting a target window will bring it to the foreground before replay;
> leave it as *(none — foreground)* to replay into whatever is already focused.

## Project layout

| File | Responsibility |
|------|----------------|
| `src/main.rs` | App bootstrap; starts hooks + the egui window |
| `src/model.rs` | `Action` / `Step` / `Macro` data model + key naming |
| `src/capture.rs` | Global LL keyboard/mouse hooks on a message-loop thread |
| `src/replay.rs` | Playback via `SendInput` (blocking) and `PostMessage` (background) |
| `src/winutil.rs` | Enumerate top-level windows for targeting |
| `src/library.rs` | Save/load/delete macros as JSON |
| `src/app.rs` | egui UI: recording, editable step list, library, playback panel |

## Notes & limitations

- **Windows only** (uses Win32 hooks and input synthesis).
- Some games use raw input / anti-cheat and may ignore synthesized or posted input.
- Mouse coordinates are absolute screen pixels; replay assumes the same screen
  resolution. Background (`PostMessage`) coordinates are window-relative as posted.
- Antivirus may flag global keyboard hooks — this is expected for input automation tools.
