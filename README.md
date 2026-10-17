# Skyhop

A small, friendly flight sim - from your first takeoff in a trainer to hand-flying a 737
down a night ILS. One ~3 MB native binary, three dependencies (macroquad, gilrs for
gamepads, tinyaudio for sound output), one embedded typeface (Barlow, SIL OFL - see
`assets/fonts/OFL.txt`) and a handful of spoken callouts. Terrain, textures, models and the
rest of the sound are generated at startup or synthesized live.

```sh
cargo run --release
```

## What's in it

**Four aircraft**, each with its own flight model, sound and cockpit:

- **Trainer** (Cessna 172 type): lift curve and stall, induced drag, ground effect,
  adverse yaw, P-factor, prop wash over the tail, flaps, sprung gear with brakes and
  nose-wheel steering. A quiet speed damper settles the plane on its trimmed speed (the
  panel shows it: `TRIM 92 KT`) instead of porpoising for minutes.
- **Boeing 737 MAX 8**: 62 t, slow-spooling turbofans that pitch the nose up with thrust,
  flap detents 1-40, auto-deploying speedbrakes, thrust reversers, retractable gear,
  radio-altimeter and ground-proximity callouts ("fifty... forty...", "sink rate",
  "too low, gear", "minimums").
- **F-16**: afterburner, transonic drag rise, 9 g, a 270 deg/s roll, landing at 11 degrees
  nose-up.
- **ASK 21 sailplane**: no engine, glide ratio ~32, airbrakes, a monowheel that settles onto
  a wingtip - and thermals to soar in, with a total-energy variometer that beeps as you
  climb and a thermal map on the minimap.

**Learn, then play:**

- Six guided **lessons** in the trainer (takeoff, trim, turns, climbs & descents, stalls,
  landing), plus guided lessons for the 737 landing and riding a thermal in the sailplane.
- **Challenges** with a landing scorecard (sink rate, g, touchdown point, centreline, crab,
  bank, pitch, rollout, bounces, and a difficulty bonus for wind and conditions): a 20 kt
  crosswind in the 737, a gusty trainer arrival, a night ILS, an ILS to minimums under a
  350 ft overcast, an engine failure glide, the F-16 overhead break, a sailplane final glide,
  and ring races for the trainer and the F-16. Personal bests are saved.
- **Free flight**: pick the aircraft, the start (either runway end, 4 or 10 nm final, the
  grass strip, over the harbour, airborne), the time of day, wind, gusts, turbulence,
  clouds, cloud base, visibility and thermals. Your setup is remembered.
- **Instant replay** from any dialog (crashed, landed or paused), through any camera at any
  speed.

**The world**: a procedural island with fields, hedgerows, forests, roads, villages and a
harbour town with a church, pier, ferry and sailboats; a 3 km airport with a terminal,
parked airliners, parallel taxiway, full approach lighting (sequenced flashers, crossbar,
red side rows), edge, centreline and touchdown-zone lights, PAPI and windsocks; a grass
strip on the south coast; a wind farm on the ridge; a lighthouse on the west cape.

**Weather and time**: wind that strengthens and veers with height, gusts, turbulence
drifting with the wind, cumulus or an overcast deck you can break out of, visibility, and a
sun that moves through golden hour and twilight to a starry night with landing lights and
the airport lit up.

**Tower and traffic**: the tower clears you for takeoff when you line up and to land when
you're inbound on final, reading out the actual wind over a crackly radio, sends you around
if you're low and not lined up, and welcomes you in. In free flight you share the sky with a
trainer touring the coast, a pair of F-16s and a sailplane working a thermal (blue dots on
the minimap).

**Cockpit and instruments**: six-pack and engine cluster, a head-up display (flight path
vector, pitch ladder with the 3 degree glide reference, speed and altitude tapes, ILS),
ILS diamonds, a radio altimeter, wind readout, minimap.

**Autopilot**: a 737-style mode control panel for every aircraft with an engine - heading
select, altitude hold with capture, vertical speed, autothrottle, and approach mode that
captures the localizer and glideslope. In the 737 it flares, retards, lands, tracks the
centreline and brakes to a stop.

**Sound**, all synthesized: a four-cylinder engine whose pitch follows the RPM, prop buzz,
turbofan whine and roar, afterburner, reverse thrust, wind, stall horn and stick shaker,
runway seams, tire chirps, gear and flap motors, trim-wheel clacks, the autopilot
disconnect wailer, Doppler in the tower view, and chimes with a little room reverb.

## Controls

| Keys | Action |
| --- | --- |
| W / S or Up / Down | push (nose down) / pull (nose up): tap to nudge, hold for more |
| A / D or Left / Right | roll |
| Q / E | rudder (steers on the ground too) |
| Shift / Ctrl or = / - | throttle (past 90% in the F-16: afterburner) |
| F / V | flaps down / up |
| T / G | trim nose up / down (tap for fine steps, hold to wind) |
| B or Space | brakes |
| X | speedbrake (737: down / armed / up), sailplane airbrakes |
| L | landing gear up / down |
| Ctrl / - held at idle | 737 on the ground: thrust reversers (Shift / = stows them) |
| C | camera: chase, cockpit, orbit, tower (mouse drag looks around, wheel zooms) |
| U | head-up display in the cockpit view |
| Tab / O | instruments full / compact / off, lesson coach less / more |
| Space | in a dialog or paused: instant replay |
| Y / I / M | auto-rudder, invert pitch, mute |
| R, N, Esc, H | restart, skip lesson step, pause, help |

### Autopilot

| Keys | Action |
| --- | --- |
| Z | autopilot engage / disconnect (moving the controls also disconnects) |
| K | autothrottle (jets; moving the throttle drops it) |
| 1 / 2 / 3 / 4 | heading select, altitude hold, vertical speed, approach (LOC + G/S) |
| , / . | heading bug |
| ; / ' | altitude bug (while holding an altitude, a new one starts a climb or descent) |
| 9 / 0 | vertical speed |
| 7 / 8 | speed |
| Mouse | click a window on the panel to press it, scroll over it to set it |

### Gamepad

Any controller with a standard (Xbox-style) layout. It's picked up as soon as you press
something, and menus and dialogs switch to gamepad hints.

| Control | Action |
| --- | --- |
| Left stick | pitch / roll |
| Right stick left / right | rudder (steers on the ground too) |
| Right stick up / down | throttle (hold down at idle on the ground for reverse thrust) |
| LT / RT | left / right brakes, analog (steer by braking one side) |
| LB / RB | flaps up / down |
| D-pad up / down | trim nose down / up |
| A | both brakes (confirm in menus and dialogs) |
| Y | camera (next lesson on the summary) |
| X | auto-rudder (retry in dialogs) |
| B | speedbrake (main menu from a dialog) |
| D-pad left / right | instruments, landing gear |
| Left stick click | autopilot on / off |
| Right stick click | approach mode (in a dialog: replay) |
| Start / Select | pause / help |

## Development

```sh
cargo test --release          # flight models, autopilot/autoland, scoring, scripted lessons
cargo test --release audio_demo -- --ignored      # target/audio_demo.wav (scripted flight)
cargo test --release jet_audio_demo -- --ignored  # target/jet_demo.wav: 737 final, touchdown, reverse
cargo run --release -- --shot out.png 7 cockpit   # render a lesson and save a screenshot
```

`--shot <file> <lesson index | menu | setup> [chase|cockpit|orbit|tower]` takes extra
`key=value` arguments: `hour=21.5 clouds=0.9 base=400 wind=20 from=30 vis=3` for the
weather, `final=2000` to sit on the glide path, `craft=2 place=4` for a free flight, `ap`
to engage the autopilot, `ab=1` for afterburner, `eye=x,y,z:tx,ty,tz` for a fixed camera,
`replay` and `help`.

Source layout: `flight.rs` (physics and the aircraft specs), `ap.rs` (autopilot), `weather.rs`
(wind, turbulence, thermals, sky colours), `world.rs` (island and placement), `ground.rs`
(baked ground textures), `mesh.rs` (geometry and the trainer), `airliner.rs`, `fighter.rs`,
`glider.rs` (the other models), `scenery.rs` (airport extras, turbines, lighthouse, boats),
`traffic.rs` (other aircraft), `atc.rs` (tower clearances),
`lights.rs` (airport and aircraft lights), `render.rs` (shaders, GPU buffers), `hud.rs`
(instruments, HUD, menus), `lessons.rs`, `score.rs` (landing scorecard), `replay.rs`,
`setup.rs` (free-flight setup and the saved profile), `gpws.rs` (callouts), `audio.rs`
(real-time synth), `camera.rs`, `input.rs` (gamepad), `main.rs`.

Progress and settings are saved to `~/Library/Application Support/Skyhop/profile.txt`
(macOS), `%APPDATA%\Skyhop` (Windows) or `~/.config/skyhop` (Linux).

### Callouts

The 737's spoken callouts in `assets/callouts` and the tower's words in `assets/atc` were
made with macOS `say` (the "Fred" and "Samantha" voices) by `tools/callouts.sh`. Apple licenses its system voices for personal, non-commercial
projects; before distributing Skyhop, regenerate them with another text-to-speech engine
(same file names, 16-bit mono WAV).
