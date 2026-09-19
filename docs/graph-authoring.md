# Graph authoring contract

Cetz Studio edits direct, named Fletcher graph arguments while Typst remains the
source of truth. Every command creates a bounded concrete-syntax patch, compiles
the candidate, and is adopted only when the instrumented preview maps back to the
same graph.

## Text fields

Nodes expose `text_fields` named `title` and, where the constructor has one,
`body`. Edges expose a `label` field. A field is editable only when the Typst
syntax tree proves the existing argument is plain literal content or a quoted
string. Equations, comments, headings, lists, code expressions, nested content,
and other rich markup remain protected from the plain-text control.

Each recognized field also exposes its exact argument as `source`. When
`source_editable` is true, the source control can replace that selected argument
with one complete Typst expression, including equations and composed content.
The validator rejects additional arguments, named/spread arguments, unmatched
delimiters, and trailing source so this control cannot escape into the enclosing
graph. The candidate still has to compile and pass graph instrumentation before
the session adopts it. This is a focused content-expression editor rather than a
whole-file source editor.

Changed text is written as an escaped Typst string. This preserves ordinary
punctuation such as `$`, `#`, brackets, quotes, backslashes, and line breaks as
text instead of interpreting it as Typst code. Text is limited to 4 KiB and
unsupported control characters are rejected.

## Structural commands

- `duplicate_node` copies the complete direct constructor call, assigns a fresh
  `<name>`, and offsets literal coordinates by 10 mm. Computed positions are
  read-only.
- `add_edge` connects two existing named nodes. Arrow choices are `none`,
  `forward`, `backward`, and `both`; optional labels are safely string encoded.
- `insert_node` accepts only the fixed primitives reported by the parsed
  diagram's `insert_primitives` field: Fletcher rectangle, ellipse, and diamond,
  plus Studio node and card when the source has a proven `studio` import alias.
  It accepts optional plain `name` and `text` values but no source code.
- `delete_edge` removes one recognized direct edge argument. `delete_node`
  removes one recognized direct node and requires `cascade: true` when named
  edges are attached; the node and those edges form one compiled edit and one
  undo step. Snapshot nodes report `attached_edges`, `deletable`, and a specific
  `delete_reason`.

Node deletion is refused when computed or unsupported graph arguments might
contain references that the adapter cannot prove safe. The final recognized node
also remains protected because the current semantic adapter and instrumented
preview require a non-empty named graph. Edge deletion remains available for
recognized direct edges. Deletion removes only constructor and comma spans;
comments and graph-level named options remain in the source.

Primitive availability comes from explicit import aliases. A direct Studio
import can supply both Studio helpers and `studio.fletcher` shapes inside an
ordinary `f.diagram`. A direct Fletcher import supplies Fletcher shapes only.
CeTZ canvas insertion is outside this contract; the snapshot explains that
limitation through `insertion_reason`.
