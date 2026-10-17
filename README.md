# Skyhop

A small, friendly flight sim for learning the basics of flying. One ~1 MB native
binary, one dependency (macroquad), no asset files: terrain, models and sounds are
all generated at startup.

```sh
cargo run --release
```

## What's in it

- **A real-ish flight model** of a Cessna-172-like trainer. It covers lift curve and stall,
  induced drag, ground effect, adverse yaw, P-factor, prop wash over the tail, trim,
  flaps, and a sprung landing gear with brakes and nose-wheel steering.
- **Six guided lessons** with live coaching: takeoff, straight & level with trim,
  turns (and the slip ball), climbs & descents, stalls, and landing with PAPI lights.
  Each step tells you what to do and gives hints when you're off.
- **Ring Challenge**: a timed race through rings around the island.
- **Free flight** from the runway or in the air.
- Six-pack instruments, throttle/flaps/trim/AoA cluster, minimap, four camera views
  (chase, cockpit, orbit, tower), synthesized engine/wind/stall-horn audio.

## Controls

| Keys | Action |
| --- | --- |
| W / S or Up / Down | push (nose down) / pull (nose up) |
| A / D or Left / Right | roll |
| Q / E | rudder (steers on the ground too) |
| Shift / Ctrl or = / - | throttle |
| F / V | flaps down / up |
| T / G | trim nose up / down |
| B or Space | brakes |
| C | camera (mouse drag looks around, wheel zooms) |
| Y | auto-rudder |
| Tab | full / compact instruments |
| I / M | invert pitch keys / mute |
| R, N, Esc, H | restart, skip lesson step, pause, help |

## Development

```sh
cargo test --release          # flight model + scripted takeoff/landing lessons
cargo run --release -- --shot out.png 5 cockpit   # render lesson 6 and save a screenshot
```

Source layout: `flight.rs` (physics), `world.rs` (island, scenery, rings),
`mesh.rs` (geometry), `render.rs` (shaders, static GPU buffers), `hud.rs`
(instruments, overlays), `lessons.rs`, `audio.rs`, `camera.rs`, `main.rs`.
