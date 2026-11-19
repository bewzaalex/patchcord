#!/bin/sh
# The ENGINE mockup, from the app's own drawing, with the legend under it:
#   docs/design/engine/build.sh     (from the repository; needs a release build and rsvg-convert)
set -e
cd "$(dirname "$0")"
../../../target/release/patchcord --render specimen.svg engine specimen
python3 legend2.py --into specimen.svg
python3 legend2.py legend-variants.svg
python3 legend2.py --app ../../../src/ui/legend.svg
python3 variants.py variants.svg
for f in specimen legend-variants variants; do rsvg-convert -z 1.5 "$f.svg" -o "$f.png"; done
