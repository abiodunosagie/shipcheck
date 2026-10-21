# shipcheck brand

## The idea

A terminal prompt followed by a tick: you run shipcheck, the site passes. Beside it, the word `shipcheck` in Poppins SemiBold, outlined so no font is needed. In the terminal the same mark is text, `>✓` in amber, at the start of every run (`report::opening` in `src/report.rs`; plain text when the output is piped).

- **Mark**: avatar, favicon, anywhere square.
- **Lockup**: README, banner, anywhere wide. The mark is centred on the letters' x-height.

## Colour

| Name | HEX | Use |
|---|---|---|
| Amber | `#FFB020` | the mark on dark (10.2:1 on Ink); a warning lamp, not the usual CI green |
| Ochre | `#9A6100` | the mark on light (4.6:1 on Paper) |
| Ink | `#111317` | banner background, the word on light |
| Paper | `#F6F3EC` | the word on dark, light backgrounds |
| Stone | `#9A9790` | secondary text on Ink |

The terminal uses the closest 256-colour amber (`38;5;214`).

## Files

| File | Use |
|---|---|
| `shipcheck-mark-*.svg` | the mark in black, white, Amber, Ochre, Ink |
| `shipcheck-lockup-light.svg`, `shipcheck-lockup-dark.svg` | mark and word for light and dark backgrounds |
| `shipcheck-avatar.svg` | square avatar: Amber mark on an Ink tile |
| `banner.svg` | animated README header: the prompt slides in, the tick pops, the word wipes in. Still under reduced motion. |
| `banner-static.svg`, `banner.png` | still banner; the PNG (exported at 1280x640) is the GitHub social preview |
| `generate.py` | regenerates every SVG above |
| `outline_wordmark.py`, `POPPINS-OFL.txt` | outlines the word from a Poppins font file; Poppins is SIL Open Font License 1.1 |

## Use

- Clear space: at least the stroke's width twice over around the mark or lockup.
- Minimum size: mark 16 px; lockup 120 px wide.
- Keep the prompt before the tick. Do not retype the word in another font.
