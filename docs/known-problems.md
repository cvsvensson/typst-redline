# Known problems

Each of these was tried with the current version. Within each group, the ones a user is most likely to meet come first.

## What is marked

- Where a function reads its content as text, a changed text can be missing from the page, not only its mark; the summary page lists it.
- A function that fails on marked content makes the whole document fall back to coarser marks, and the summary says so.
- A changed element that sets the page from inside a function, such as a slide, cannot be framed, so the document is compiled again without frames around content computed at layout (calls of functions built on `context` or `layout`, which includes many drawings), and the summary says so.
- Deleted code is evaluated with the definitions its file has at its end, so where a name it uses meant something else at its place (bound by a `let` in a block, a function parameter or a loop, bound again later at the top level of its file, or defined only by the new version), it is shown by its name, such as *[unit]*, not as it looked; a wildcard import after it counts as binding every name.
- In deleted code, a label passed as a named argument or written in math is taken to be attached by the code and is renamed, and one passed positionally is taken to be a reference; a reference made from a label passed as a named argument shows the renamed label, such as *redline-del-label-a*.
- A figure whose body changed is deleted and inserted whole, caption and all, when it is passed to another call, as in `#align(center, figure(...))`, when it is inline, or when an argument other than its body and content in brackets changed.
- A raw block with one changed line is deleted and inserted whole.
- New parts of a formula are only coloured blue, which does not show in greyscale; the bar in the margin marks their line. A changed part is marked with what it is attached to where it cannot stand alone: a changed base such as the `x` of `x^2` marks all of `x^2`. A formula right next to changed words can be shown whole, with them.
- Deleted parts of a display formula widen it, and a wide formula can then reach its equation number. `--math-markup whole` shows the old formula on a line of its own.
- A heading, list item or code line deleted or inserted directly between two lines of text, with no blank line, shows the unchanged text next to it as deleted and inserted again.
- A citation added to or removed from a group such as `@a @b` marks the whole group.
- Inserted or deleted text that the document gives a colour of its own, such as a link or a white tag on a dark box, keeps that colour and is marked only by its strike, highlight or bar; under a rule such as `show text: set text(fill: ...)` it takes that rule's colour.
- Show rules on `strike`, `highlight` or `text` in your document also restyle the marks; a text show rule with a `regex` selector stops matching where a mark cuts its match, and for a string selector, such as `#show "Typst": smallcaps`, the mark is widened to cover the match.
- Pages where a template or a later `set page` rule gives both its own page foreground and background get no margin bars; the summary lists those pages.
- A box or table deleted or inserted whole whose fill comes from a `set` or `show` rule has its text coloured like other text, which can be hard to read on a dark fill.
- Change numbers and bars can overlap the document's own margin notes and line numbers, and with `par.line` numbering a change number stands just left of the line number.
- Where neither margin has room for a number, it is set smaller at the page edge, where it can stand on a bar; on a page without margins it stands just inside the bars, over the text where the text reaches the edge of the page.
- In Chinese and Japanese text, a change that starts a line with a letter or digit right after a Chinese or Japanese character has its bar and number on the line before, and a change that ends with or next to such a character in a table cell or `columns` has its bar in the left margin.

## Moved text

- Some moved text shows as deleted at one place and inserted at the other: a sentence moved within or between paragraphs (text after a display equation belongs to the paragraph of the equation), paragraphs reordered inside a moved section, a nested list item moved on its own, a passage of fewer than four words unless it moves with a longer one next to it, text inside a call's content argument such as `#slide[...]`, text moved into or out of a file that is not shown in order (imported, included by code, or no longer included), and a passage rewritten so much that it keeps less than about 60% of its words.
- Setup code such as `#set` and `#let` moved unchanged within its file is listed on the summary page as moved, not marked.
- The bracket of a moved passage that ends with a block, such as a framed box or a list, reaches down past the spacing below it to the next paragraph or heading.
- A moved passage on a page without margins has a bracket but no "moved" note.

## Deleted content in the new layout

- Deleted content shifts line and page breaks compared with the new version, and a deleted figure, table or drawing that fits on a page is kept whole, so on a full slide it and the new one each take a page of their own; side by side avoids this, and so does `--graphics-markup new-only`, which leaves the old one out. Deleted text still takes room with `new-only`.
- Deleted content is laid out under the new version's show and set rules, so where such a rule changed, it does not look as it did in the old version.
- A reference or citation in deleted text to a label or bibliography entry that the new version lacks shows the label or key, not the number the old version printed. A deleted citation of an entry that the new version keeps shows that entry's number in the new bibliography, or its key where the new version first cites another entry between the two.
- Deleted content that updates a state, for example in a show rule of a package, leaves the state changed after it, because Typst records no initial value to set it back to; the summary page names such states.
- Counters are set back after deleted content, so it and the content after it can get the same number, and content that keys a state by such a number can show the entries of the other.
- Deleted figures, equations and citations are still figures, equations and citations, so the document's own queries for them, such as a count with `query(figure)`, also find them; `outline` leaves them out.

## Side by side

- Change numbers are shown on the new pages only, a deletion's number where the deleted content was.
- A document whose layout does not settle (Typst warns that it did not converge) can show other pages than those of its changes.
- An inserted or deleted page is shown next to an empty one only when the pages have headings or a moved passage on them; otherwise it can be paired with a neighbouring page.
- Pages of the same slide built step by step are shown once, as its last step, only where they have the same headings, and slides whose only change is the same changed header or footer are each shown.
- A version without content, such as an empty file, is shown as a blank page.

## Files and the command line

- Package files are not checked for changes.
- `--only-changes` leaves out a page whose only change is listed on the summary, such as a page that a changed `#set` rule or a reflow redraws.
- With `--only-changes`, links on the summary and cross-references in the text that lead to a page left out lead nowhere, and with Typst 0.15 the PDF has no tags for accessibility, as with `typst compile --pages`.
- Two plain files whose projects need different roots, such as two copies of a project that use paths starting with `/`, cannot be compared, because `--root` is one folder for both; comparing git revisions of the project works.

## Typst versions

- The tests run with Typst 0.15.1 and 0.14.2 and also pass with 0.11.1; Typst 0.12 and 0.13 are not tested.
- With Typst 0.11, `--only-changes` does not work. With Typst 0.11 to 0.13, side by side does not work, and a changed file that the document reads only through a computed path, such as `#include name + ".typ"`, is neither marked nor listed; the summary says that such files were not checked.
- With Typst 0.11, the summary is laid out on the document's pages, with its header and footer, right-to-left text gets no bars, and a changed drawing that needs a finite width causes the coarser marks described above.
