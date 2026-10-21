"""Regenerates the shipcheck logo masters and README banners in this folder.

Run: python3 brand/generate.py
The mark: a terminal prompt followed by a tick. You run shipcheck, the site passes. The wordmark is
Poppins SemiBold, outlined by brand/outline_wordmark.py (SIL Open Font License 1.1, see POPPINS-OFL.txt).
The terminal shows the same mark as text (>✓ in amber): see report::opening in src/report.rs.
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))
INK, PAPER, AMBER, OCHRE, STONE = "#111317", "#F6F3EC", "#FFB020", "#9A6100", "#9A9790"

# The mark in a 256 box: the prompt chevron, then the tick. Stroke 28 with round ends and joins.
PROMPT, TICK = "M40,80 L96,128 L40,176", "M128,148 L160,180 L220,96"
MARK_WEIGHT = 28
MARK_INK = (26, 66, 234, 194)  # x0, y0, x1, y1 including the round ends

# "shipcheck" in Poppins SemiBold, font units, y down, baseline at 0.
WORD = "M39 -175H180Q184 -143 211.5 -122Q239 -101 280 -101Q320 -101 342.5 -117Q365 -133 365 -158Q365 -185 337.5 -198.5Q310 -212 250 -228Q188 -243 148.5 -259Q109 -275 80.5 -308Q52 -341 52 -397Q52 -443 78.5 -481Q105 -519 154.5 -541Q204 -563 271 -563Q370 -563 429 -513.5Q488 -464 494 -380H360Q357 -413 332.5 -432.5Q308 -452 267 -452Q229 -452 208.5 -438Q188 -424 188 -399Q188 -371 216 -356.5Q244 -342 303 -327Q363 -312 402 -296Q441 -280 469.5 -246.5Q498 -213 499 -158Q499 -110 472.5 -72Q446 -34 396.5 -12.5Q347 9 281 9Q213 9 159 -15.5Q105 -40 73.5 -82Q42 -124 39 -175ZM1134 -325V0H994V-306Q994 -372 961 -407.5Q928 -443 871 -443Q813 -443 779.5 -407.5Q746 -372 746 -306V0H606V-740H746V-485Q773 -521 818 -541.5Q863 -562 918 -562Q981 -562 1030 -534.5Q1079 -507 1106.5 -453.5Q1134 -400 1134 -325ZM1244 -702Q1244 -737 1268.5 -760.5Q1293 -784 1330 -784Q1367 -784 1391.5 -760.5Q1416 -737 1416 -702Q1416 -667 1391.5 -643.5Q1367 -620 1330 -620Q1293 -620 1268.5 -643.5Q1244 -667 1244 -702ZM1399 -554V0H1259V-554ZM1852 -563Q1923 -563 1980.5 -528Q2038 -493 2071.5 -428.5Q2105 -364 2105 -279Q2105 -194 2071.5 -128.5Q2038 -63 1980.5 -27Q1923 9 1852 9Q1791 9 1744.5 -16Q1698 -41 1669 -79V264H1529V-554H1669V-474Q1696 -512 1743.5 -537.5Q1791 -563 1852 -563ZM1815 -440Q1777 -440 1743.5 -420.5Q1710 -401 1689.5 -364Q1669 -327 1669 -277Q1669 -227 1689.5 -190Q1710 -153 1743.5 -133.5Q1777 -114 1815 -114Q1854 -114 1887.5 -134Q1921 -154 1941.5 -191Q1962 -228 1962 -279Q1962 -329 1941.5 -365.5Q1921 -402 1887.5 -421Q1854 -440 1815 -440ZM2437 -563Q2540 -563 2607.5 -511.5Q2675 -460 2698 -367H2547Q2535 -403 2506.5 -423.5Q2478 -444 2436 -444Q2376 -444 2341 -400.5Q2306 -357 2306 -277Q2306 -198 2341 -154.5Q2376 -111 2436 -111Q2521 -111 2547 -187H2698Q2675 -97 2607 -44Q2539 9 2437 9Q2357 9 2295 -26.5Q2233 -62 2198 -126.5Q2163 -191 2163 -277Q2163 -363 2198 -427.5Q2233 -492 2295 -527.5Q2357 -563 2437 -563ZM3321 -325V0H3181V-306Q3181 -372 3148 -407.5Q3115 -443 3058 -443Q3000 -443 2966.5 -407.5Q2933 -372 2933 -306V0H2793V-740H2933V-485Q2960 -521 3005 -541.5Q3050 -562 3105 -562Q3168 -562 3217 -534.5Q3266 -507 3293.5 -453.5Q3321 -400 3321 -325ZM3957 -235H3552Q3557 -175 3594 -141Q3631 -107 3685 -107Q3763 -107 3796 -174H3947Q3923 -94 3855 -42.5Q3787 9 3688 9Q3608 9 3544.5 -26.5Q3481 -62 3445.5 -127Q3410 -192 3410 -277Q3410 -363 3445 -428Q3480 -493 3543 -528Q3606 -563 3688 -563Q3767 -563 3829.5 -529Q3892 -495 3926.5 -432.5Q3961 -370 3961 -289Q3961 -259 3957 -235ZM3816 -329Q3815 -383 3777 -415.5Q3739 -448 3684 -448Q3632 -448 3596.5 -416.5Q3561 -385 3553 -329ZM4293 -563Q4396 -563 4463.5 -511.5Q4531 -460 4554 -367H4403Q4391 -403 4362.5 -423.5Q4334 -444 4292 -444Q4232 -444 4197 -400.5Q4162 -357 4162 -277Q4162 -198 4197 -154.5Q4232 -111 4292 -111Q4377 -111 4403 -187H4554Q4531 -97 4463 -44Q4395 9 4293 9Q4213 9 4151 -26.5Q4089 -62 4054 -126.5Q4019 -191 4019 -277Q4019 -363 4054 -427.5Q4089 -492 4151 -527.5Q4213 -563 4293 -563ZM4977 0 4789 -236V0H4649V-740H4789V-319L4975 -554H5157L4913 -276L5159 0Z"
WORD_BOX = (39, -784, 5159, 264)  # x0, y0, x1, y1
X_HEIGHT = 554


def stroke(d, color, extra=""):
    return (f'<path{extra} d="{d}" fill="none" stroke="{color}" stroke-width="{MARK_WEIGHT}" '
            f'stroke-linecap="round" stroke-linejoin="round"/>')


def mark(color, prompt_class="", tick_class=""):
    return stroke(PROMPT, color, prompt_class) + stroke(TICK, color, tick_class)


def lockup(mark_color, word_color, height=200, prompt_class="", tick_class=""):
    """Mark beside the word, centred on the letters' x-height, which is about 75% of the mark's height.
    Returns (mark_svg, word_svg, width, word_left)."""
    mark_h = MARK_INK[3] - MARK_INK[1]
    sw = height * 0.34 / X_HEIGHT
    sm = X_HEIGHT * sw / 0.75 / mark_h
    word_ascent = -WORD_BOX[1] * sw
    baseline = (height - (word_ascent + WORD_BOX[3] * sw)) / 2 + word_ascent
    my = baseline - X_HEIGHT * sw / 2 - (MARK_INK[1] + mark_h / 2) * sm
    mx = -MARK_INK[0] * sm
    gap = X_HEIGHT * sw * 0.35
    wx = (MARK_INK[2] - MARK_INK[0]) * sm + gap - WORD_BOX[0] * sw
    width = wx + WORD_BOX[2] * sw
    mark_svg = f'<g transform="translate({mx:.2f},{my:.2f}) scale({sm:.4f})">{mark(mark_color, prompt_class, tick_class)}</g>'
    word_svg = f'<g transform="translate({wx:.2f},{baseline:.2f}) scale({sw:.5f})"><path d="{WORD}" fill="{word_color}"/></g>'
    return mark_svg, word_svg, width, wx + WORD_BOX[0] * sw


def write(name, body, w, h):
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0f} {h:.0f}" width="{w:.0f}" height="{h:.0f}">'
           f'<title>shipcheck</title>{body}</svg>\n')
    with open(os.path.join(HERE, name), "w", encoding="utf-8") as f:
        f.write(svg)


def banner(animated):
    """1280 x 640 README header and social preview. Animated: the prompt appears, the tick pops in,
    the word wipes in, the line fades in.
    Everything is visible by default; hidden start states live only inside the keyframes."""
    style, prompt_class, tick_class, clip = "", "", "", ""
    if animated:
        style = """<style>
.prompt{animation:enter .5s cubic-bezier(.22,1,.36,1) .1s both}
.tick{transform-box:fill-box;transform-origin:30% 100%;animation:pop .6s cubic-bezier(.34,1.56,.64,1) .55s both}
.wipe{transform-box:fill-box;transform-origin:left center;animation:grow 1.1s cubic-bezier(.65,0,.35,1) 1s both}
.line{animation:fade .8s ease-out 1.9s both}
@keyframes enter{from{opacity:0;transform:translateX(-30px)}to{opacity:1;transform:none}}
@keyframes pop{from{opacity:0;transform:scale(.2)}to{opacity:1;transform:scale(1)}}
@keyframes grow{from{transform:scaleX(0)}to{transform:scaleX(1)}}
@keyframes fade{from{opacity:0}to{opacity:1}}
@media (prefers-reduced-motion:reduce){.prompt,.tick,.wipe,.line{animation:none}}
</style>"""
        prompt_class, tick_class = ' class="prompt"', ' class="tick"'
    # The lockup's width grows with its height; size it to 1000 px wide, then centre the lockup and
    # the two text lines on the 640 height.
    _, _, width_at_100, _ = lockup(AMBER, PAPER, height=100)
    lock_h = 100 * 1000 / width_at_100
    mark_svg, word_svg, width, word_left = lockup(AMBER, PAPER, height=lock_h, prompt_class=prompt_class, tick_class=tick_class)
    left, top = 140, (640 - (lock_h + 110)) / 2
    if animated:
        clip = ' clip-path="url(#reveal)"'
        # The clip lives in the lockup group's coordinates, so it starts where the word starts.
        style += (f'<defs><clipPath id="reveal"><rect class="wipe" x="{word_left - 10:.0f}" y="-50" '
                  f'width="{width - word_left + 40:.0f}" height="{lock_h + 100:.0f}"/></clipPath></defs>')
    font = 'font-family="-apple-system,BlinkMacSystemFont,Helvetica Neue,Arial,sans-serif"'
    cls = ' class="line"' if animated else ""
    text = (f'<g{cls}><text x="{left}" y="{top + lock_h + 40:.0f}" {font} font-size="38" fill="{PAPER}">'
            'Audit a website before launch.</text>'
            f'<text x="{left}" y="{top + lock_h + 90:.0f}" font-family="Menlo,monospace" font-size="24" fill="{STONE}">'
            'broken links · link previews · alt text · image weight · CI gate</text></g>')
    return (f'<rect width="1280" height="640" fill="{INK}"/>{style}'
            f'<g transform="translate({left},{top:.1f})">{mark_svg}<g{clip}>{word_svg}</g></g>{text}')


def main():
    for suffix, color in (("", "#000"), ("-white", "#FFFFFF"), ("-amber", AMBER), ("-ochre", OCHRE), ("-ink", INK)):
        write(f"shipcheck-mark{suffix}.svg", mark(color), 256, 256)
    mark_svg, word_svg, width, _ = lockup(OCHRE, INK)
    write("shipcheck-lockup-light.svg", mark_svg + word_svg, width, 200)
    mark_svg, word_svg, width, _ = lockup(AMBER, PAPER)
    write("shipcheck-lockup-dark.svg", mark_svg + word_svg, width, 200)
    write("shipcheck-avatar.svg", f'<rect width="512" height="512" rx="112" fill="{INK}"/>'
          f'<g transform="translate(64,64) scale(1.5)">{mark(AMBER)}</g>', 512, 512)
    write("banner.svg", banner(animated=True), 1280, 640)
    write("banner-static.svg", banner(animated=False), 1280, 640)


if __name__ == "__main__":
    main()
