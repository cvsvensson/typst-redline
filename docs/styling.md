# Styling

`--style key=value` changes one setting of how changes look. `--style file.typ` reads several from a file that holds a Typst dictionary. Values are Typst code; repeat `--style` as often as needed, and later values win.

```
typst-redline paper.typ --style ins=purple --style ins-fill=none
typst-redline paper.typ --style my-style.typ
```

with `my-style.typ`:

```typ
(del: rgb("#b00000"), strike: 1pt, bars: false)
```

`-t UNDERLINE` gives latexdiff's default look: new text blue and underlined, deleted text red and struck through, no highlight and no bars.

<img src="paper-underline.png" width="480" alt="The paper page with -t UNDERLINE: new words blue and underlined without a highlight, deleted words red and struck through, changed formulas and figures framed, and change numbers in the margin without bars">

## Keys

| Key | Default | What it sets |
|---|---|---|
| `ins`, `del` | blue, red | Colour of inserted and deleted content, its frames and its bars |
| `ins-fill` | light blue | Highlight behind inserted words; `none` for no highlight |
| `del-fill` | light red | Behind deleted text where the text is light, as on a dark background |
| `underline` | `0.5pt` | Line under inserted words; `none` for none |
| `strike` | `0.065em` | Line through deleted text; `none` for none |
| `ins-frame`, `del-frame` | `0.6pt`, `(thickness: 0.6pt, dash: "dashed")` | Frames of inserted and deleted formulas, figures and other blocks |
| `outset` | `(x: 0.4em, top: 0.3em, bottom: 0.5em)` | How far frames stand outside their block |
| `bars` | `true` | Whether change bars are drawn; the change numbers stay |
| `bar-width`, `del-bar-width` | `2pt`, `1.5pt` | Thickness of insertion and deletion bars |
| `bar-gap`, `lane` | `0.5cm`, `3.5pt` | Distance of the bars from the text, and between bars of different kinds |
| `move`, `move-stroke` | grey, `0.8pt` | Brackets and lines of moved text |
| `number` | grey | Change numbers in the margin and on the summary |
| `steps` | six colours | Colours of `--each-commit` and `--steps`, which replace `ins` and `del` |

A stroke without a colour, such as `strike: 1pt` or `del-frame: (thickness: 1pt, dash: "dotted")`, takes the colour of its mark, so `del: green` also makes the strike-through green. A dictionary value is merged into the default: `outset: (x: 0pt)` keeps `top` and `bottom`. An unknown key is an error that lists the keys.

## latexdiff types

`-t NAME` (or `--type`, any case) starts from one of latexdiff's markup types; `--style` applies after it.

- `UNDERLINE`: new text blue and underlined, deleted text red and struck through, no bars. latexdiff's underline is wavy; Typst's is straight.
- `COLOR`: as `UNDERLINE`, without the underline.
- `CULINECHBAR`: `UNDERLINE` with change bars.
- `CCHANGEBAR`: new text blue, deleted text red and not struck through, change bars.
- `CHANGEBAR`: change bars, everything in black, new text not otherwise marked. latexdiff leaves deleted text out; here it is struck through.

The other types are not offered: `CFONT`, `CFONTCHBAR`, `FONTSTRIKE` and `BOLD` set new text in another font, which changes the layout of the new version, and `INVISIBLE`, `TRADITIONAL`, `CTRADITIONAL` and `PDFCOMMENT` leave deleted text out or move it into footnotes or PDF comments.

## A preamble of your own

`-p my-preamble.typ` (or `--preamble`) replaces the built-in styling whole, for changes that no key covers. `typst-redline --show-preamble > my-preamble.typ` writes the built-in one to start from; the comment at its top describes the functions the marked document calls. The data the tool passes to these functions can change between versions of `typst-redline`, so a preamble of your own can stop working after an update; `--style` and `-t` apply to the preamble of the version in use.
