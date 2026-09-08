# What to test

Thanks for trying this out. It's pre-1.0 and I'd rather hear about
something broken now than after a "real" release. A few specific things
worth poking at, roughly in order of "most likely to have rough edges":

- **Build your own pattern in the Grid tab** - shaped (type a stitch
  sequence) and colorwork (paint a grid) both. Does the 3D preview match
  what you'd actually expect the finished piece to look like?
- **Import a photo** and turn it into a colorwork chart. Try a photo with
  a small/rare accent color on a busy background - that's the case most
  likely to get lost.
- **Type text into the Text tab** and check it renders and centers the
  way you'd expect, especially with multiple lines of different lengths.
- **Print or export a PDF** of a colorwork pattern - check it actually
  prints in color, and that the page-to-page alignment (row/column
  numbers along the edges) makes sense when you tape pages together.
- **Force-quit or crash the app** mid-edit (sorry in advance), then
  relaunch - does the recovery prompt show up and actually recover what
  you were working on?
- **A pattern with a custom stitch (`DEF:`)** - check the round-trip
  warning shows up when you'd expect it to (editing the Grid tab should
  warn you it's about to flatten the custom stitch to raw stitches).
- **Anything that makes the app hang, freeze, or crash outright** -
  these are the highest-priority reports by far.

If something looks wrong, please open an issue (there's a template that'll
prompt you for the details that actually help - mainly your OS and the
pattern text/file that triggered it). Found something that looks like a
security issue rather than a regular bug? See [SECURITY.md](SECURITY.md)
instead of filing a public issue for it.

No pressure to test everything - even "I tried X and it worked fine" is
useful signal.
