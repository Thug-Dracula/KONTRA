# KSP string literals: what `\` means inside a string

Written 2026-11-01, from Fredonia Grand Organ. One character, measured.

## The rule we can state

Inside a string literal, `\"` is a quote and does not end the string. Every other
backslash pair is left exactly as the source writes it — **not** measured against
Kontakt, so not interpreted.

That asymmetry is deliberate. The `\"` half is a measurement; the rest is the
absence of one, and guessing would change scripts that currently run.

## How it was found

Fredonia Grand Organ's organ console is script slot 0: 461 controls, nine
`ui_panel`s, four divisions. One help string writes a quote the way Kontakt
accepts it:

```ksp
set_control_par_str(get_ui_id($SwellMIDIChannel), $CONTROL_PAR_HELP,
    "... different keybeds or \"manuals\" on a real organ ...")
```

Our lexer ended the string at the next `"` whatever preceded it, so it closed at
the `\"`, met a stray backslash, and returned `unexpected character`. The whole
slot failed to compile — one diagnostic, entire script gone.

The consequence was not "one help text is wrong". With the console script dead,
the editor drew the instrument's **other** script, slot 1: Impact Soundworks'
mic-mixer topology, 418 widgets, five channel strips. That is what the page
looked like — a mixer where Kontakt shows stops, divisions and a keyboard.

And the sound followed the page: with no console script, nothing set the stops.
Measured by `tools/parity-corpus` (Kontakt minus KONTRA):

| | level | band RMS |
|---|---|---|
| before | −15.97 dB (KONTRA 16 dB **quiet**) | 24.01 dB |
| after | **+0.16 dB** | **0.38 dB** |

A 24 dB band error was the giveaway that this was structural rather than a gain
law: the spectrum was wrong because most of the organ was not sounding.

## What was implemented

- The lexer treats `\` as escaping the next character inside a string, so an
  escaped quote does not end it.
- `unescape` resolves `\"` to `"` and leaves every other backslash pair exactly
  as written.
- String keys are interned as owned `String`s rather than borrowed from the
  source, because an unescaped literal cannot be borrowed.

## Tests, each of which fails on the old lexer

- `an_escaped_quote_stays_inside_the_string` (lexer)
- `a_backslash_that_escapes_nothing_is_kept_verbatim` (lexer)
- `a_string_with_an_escaped_quote_compiles_and_reads_back_unescaped` (VM)
- `a_help_text_with_an_escaped_quote_still_builds_its_panels` (host UI tree: the
  panel, its child, and the child's visibility)

## Open, and what would close it

- **What Kontakt does with `\n`, `\t`, `\\` and a literal `\`** is unmeasured.
  The experiment is one script that sets a label's text to a string containing
  each pair, compiled in Kontakt and read back through the oracle's script
  dump — the same way `%NOTE_DURATION` and the case-sensitive `$EVENT_ID` were
  settled.
- **Whether Kontakt ever rejects a string** is unmeasured; we accept an
  unterminated string to end of line.
- **Fredonia's first load** still shows the instrument's own Welcome panel
  rather than the console: `on init` branches on `%init_settings`, which the
  `.nki` saves as `1 1 0`, and no `init_settings.nka` exists here for
  `load_array` mode 1 to read. That is a separate question from escaping and is
  recorded in `STATUS.md`.
