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

`-t UNDERLINE` gives latexdiff's default look; the [latexdiff types](#latexdiff-types) below lists the others. The page of the [gallery](gallery.md)'s paper example with each of them, and with `--style ins=purple --style ins-fill=none`:

<table>
<tr>
<td><img src="paper-underline.png" width="260" alt="The paper page with -t UNDERLINE: new words blue and underlined, deleted words red and struck through, no bars"><br><code>-t UNDERLINE</code></td>
<td><img src="paper-color.png" width="260" alt="The paper page with -t COLOR: new words blue, deleted words red and struck through, no bars"><br><code>-t COLOR</code></td>
<td><img src="paper-culinechbar.png" width="260" alt="The paper page with -t CULINECHBAR: as UNDERLINE, with black change bars"><br><code>-t CULINECHBAR</code></td>
</tr>
<tr>
<td><img src="paper-cchangebar.png" width="260" alt="The paper page with -t CCHANGEBAR: new words blue, deleted words red and not struck through, black change bars"><br><code>-t CCHANGEBAR</code></td>
<td><img src="paper-changebar.png" width="260" alt="The paper page with -t CHANGEBAR: black text, deleted content left out, black change bars and numbers"><br><code>-t CHANGEBAR</code></td>
<td><img src="paper-style.png" width="260" alt="The paper page with new content in purple without a highlight"><br><code>--style ins=purple --style ins-fill=none</code></td>
</tr>
</table>

## Keys

| Key | Default | What it sets |
|---|---|---|
| `ins`, `del` | blue, red | Colour of inserted and deleted content, its frames and its bars |
| `ins-fill` | light blue | Highlight behind inserted words; `none` for no highlight |
| `del-fill` | light red | Behind deleted text where the text is light, as on a dark background |
| `underline` | `0.5pt` | Line under inserted words; `none` for none |
| `strike` | `0.065em` | Line through deleted text; `none` for none |
| `ins-frame`, `del-frame` | `0.6pt`, `(thickness: 0.6pt, dash: "dashed")` | Frames of inserted and deleted formulas, figures and other blocks; `none` for none |
| `block-colour` | `true` | Whether changed blocks take the colour of their mark: new ones coloured, old ones tinted or crossed out |
| `outset` | `(x: 0.4em, top: 0.3em, bottom: 0.5em)` | How far frames stand outside their block |
| `deleted`, `del-blocks` | `true`, `true` | Whether deleted content is shown, and of it formulas, figures, tables, code blocks and drawings; `false` leaves it out, and its bar and number stay |
| `bars` | `true` | Change bars in the colour of their mark, a colour for all bars, or `false` for none; the change numbers stay |
| `bar-width`, `del-bar-width` | `2pt`, `1.5pt` | Thickness of insertion and deletion bars |
| `bar-gap`, `lane` | `0.5cm`, `3.5pt` | Distance of the bars from the text, and between bars of different kinds |
| `move`, `move-stroke` | grey, `0.8pt` | Brackets and lines of moved text |
| `number` | grey | Change numbers in the margin and on the summary |
| `steps` | six colours | Colours of `--each-commit` and `--steps`, which replace `ins` and `del` |

`--graphics-markup new-only` sets `del-blocks: false`, and `--graphics-markup none` also `block-colour: false` and both frames to `none`; `both`, the default, leaves them as they are. latexdiff's default is `new-only`, but there it is an option of its own, so the types below leave graphics markup at `both`.

A stroke without a colour, such as `strike: 1pt` or `del-frame: (thickness: 1pt, dash: "dotted")`, takes the colour of its mark, so `del: green` also makes the strike-through green. A dictionary value is merged into the default: `outset: (x: 0pt)` keeps `top` and `bottom`. An unknown key is an error that lists the keys, and so is a value of a type the key does not take, such as `bars: 1`.

## latexdiff types

`-t NAME` (or `--type`, any case) starts from one of latexdiff's markup types, with its pure blue and red; `--style` applies after it.

- `UNDERLINE`: new text blue and underlined, deleted text red and struck through, no bars.
- `COLOR`: as `UNDERLINE`, without the underline.
- `CULINECHBAR`: `UNDERLINE` with black change bars.
- `CCHANGEBAR`: new text blue, deleted text red and not struck through, black change bars.
- `CHANGEBAR`: black change bars, new text not otherwise marked, deleted content left out.

Where Typst or this tool differ from latexdiff: the underline is straight, not wavy; a deletion bar is dashed along the deleted text, where the changebar package puts a short bar; change numbers stand next to the bars; and changed formulas and figures keep their frames, also in black with `CHANGEBAR`.

The other types are not offered: `CFONT`, `CFONTCHBAR`, `FONTSTRIKE` and `BOLD` set new text in another font, which changes the layout of the new version; `TRADITIONAL`, `CTRADITIONAL` and `PDFCOMMENT` move deleted text into footnotes or PDF comments; and `INVISIBLE` marks nothing.

## A preamble of your own

`-p my-preamble.typ` (or `--preamble`) replaces the built-in styling whole, for changes that no key covers. `typst-redline --show-preamble > my-preamble.typ` writes the built-in one to start from; the comment at its top describes the functions the marked document calls. The data the tool passes to these functions can change between versions of `typst-redline`, so a preamble of your own can stop working after an update; `--style` and `-t` apply to the preamble of the version in use.
