# Per-stream controls: design variants

`sheet.png` compares the ways an app with several streams (Teams, Firefox) could control each stream on its own. `gen.py` draws it (`python3 gen.py && rsvg-convert -z 1.2 sheet.svg -o sheet.png`).

- 1: everything on the face
- 2: a drawer of knobs, then the same with levels on the face and one SPLIT beside MUTE
- 3: a drawer laid out as a mixing console
- 4: a hybrid of the face and the drawer
- 5: two units high
- 6: **chosen**. Each stream is a frame of its own, with its own SPLIT beside its MUTE. A device shows at most three frames in a row, and more go on to further units.

The interactive mockup (`../mockup/index.html`) and the app use 6; the other variants stay here on the sheet only.
