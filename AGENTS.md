<!-- trail:start -->
## Trail — repo context graph

This repo is indexed in `.trail/cache/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `trail map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `trail ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `trail grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `trail skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `trail callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `.trail/cache/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `trail ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `trail build` (deterministic,
no API key, $0).
<!-- trail:end -->

## How work is done here (standing rules)

**Rust.** Every agent writing, reviewing or porting Rust loads the `rust-best-practices`
skill first; reviews use `rust-review`, ports from C/C++/other use `rust-port`. These are
installed under `~/.config/opencode/skills/` and `~/.agents/skills/`. `lints/clippy.toml`
and the `[lints]` block in `lints/lints.toml` are the project's clippy configuration.
Idiomatic is not a preference here: ownership over manual memory, `Result` over error
codes, enums over flag ints and magic strings, and tests that prove behaviour.

**Parity method.** Kontakt is reverse-engineered, not guessed: disassemble the binary
with Kuna or REA, have the agent read the pseudocode (or the disassembly where the
pseudocode is unclear), and measure against the running original where possible.
Every constant, threshold and law cites either a binary address or a measurement. A
value with no citation is not allowed in the tree.

**Architecture: write once in the IR.** A feature is implemented once — in the IR, the
engine or the DSP — in a form that knows nothing about any one format. Kontakt is an
*extension* that maps its own script commands, parameters, menus and DSP records onto
those shared representations, carrying the measured laws. UVI and Opus extensions map
onto the same representations. The test of a change: could another format's reader
reuse it unchanged? If the answer needs a `if format == Kontakt`, the logic belongs in
the extension, not the engine.

**Before searching, ask the graph.** `trail ask "<question>" --source`, `trail callers
<symbol>`, `trail skeleton <file>`, `trail grep "<literal>"`. Use it before grepping or
opening files.
