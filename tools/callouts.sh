#!/bin/sh
# Regenerates the 737 radio-altimeter and GPWS callouts in assets/callouts, and the tower
# controller's words in assets/atc, with macOS `say`.
# The "Fred" voice is close to the flat, synthetic voice of real ground-proximity systems.
# Note: Apple licenses its system voices for personal, non-commercial projects - swap in
# another text-to-speech engine (same file names, 16-bit mono WAV) before distributing.
set -e
cd "$(dirname "$0")/../assets/callouts"
say_to() { say -v Fred -r 215 -o "$1.wav" --file-format=WAVE --data-format=LEI16@22050 "$2"; }
say_to 2500 "twenty five hundred"
say_to 1000 "one thousand"
say_to 500 "five hundred"
say_to 100 "one hundred"
say_to 50 "fifty"
say_to 40 "forty"
say_to 30 "thirty"
say_to 20 "twenty"
say_to 10 "ten"
say_to minimums "minimums"
say_to sink_rate "sink rate"
say_to pull_up "pull up"
say_to glideslope "glide slope"
say_to too_low_gear "too low, gear"
say_to too_low_flaps "too low, flaps"
say_to bank_angle "bank angle"

# The tower controller: a different voice, word by word so any number can be read out.
mkdir -p ../atc && cd ../atc
atc() { say -v Samantha -r 205 -o "$1.wav" --file-format=WAVE --data-format=LEI16@22050 "$2"; }
for n in 0:zero 1:one 2:two 3:three 4:four 5:five 6:six 7:seven 8:eight 9:niner; do atc "${n%%:*}" "${n#*:}"; done
atc skyhop "Skyhop"
atc runway "runway"
atc wind "wind"
atc at "at"
atc gusting "gusting"
atc calm "calm"
atc land "cleared to land"
atc takeoff "cleared for takeoff"
atc goaround "go around"
atc welcome "welcome to Skyhop island, taxi to parking"
