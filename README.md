# money-tally

Adds up currency amounts wherever they show up in text and prints a total
per symbol. Point it at a log file, a pasted email, a CSV export, an
invoice dump -- anywhere numbers are mixed in with other text and you just
want to know what they add up to.

```
$ cat receipts.txt
Coffee            $4.50
Lunch             $12.99
Refund            -$3.00
Parking          $15.00

$ money-tally receipts.txt
$29.49 total across 4 amounts
```

It also reads from stdin, so it works in a pipeline:

```
$ cat access.log | grep checkout | money-tally
```

## Why

Every time I needed "what do these numbers add up to" I ended up writing a
one-off awk or python script. This is that script, done once, that also
doesn't choke on a file too big to fit in memory.

## How it works

Input is read one line at a time with `BufRead::read_line` into a buffer
that gets cleared and reused for the next line. Memory use is bounded by
the longest single line in the input, not by the file's total size, so a
50GB log file and a 50 line file cost the same to scan.

Amounts are matched per line: a currency symbol (`$`, `£`, `€`, `¥`)
followed directly by digits, with optional comma thousands separators and
an optional two-digit decimal fraction. A `-` immediately before the
symbol makes the amount negative, and so does wrapping the whole thing in
parentheses, e.g. `($12.34)`, the accounting convention for a negative
number -- the closing paren has to sit right after the amount for this to
count, otherwise the `(` is just a stray character. Amounts are tracked
separately per symbol -- `$` and `€` totals are never mixed together, and
no attempt is made to guess that a `$` means USD versus CAD versus AUD.

## Usage

```
money-tally [file ...]
```

With no arguments it reads from stdin. With one or more file arguments it
scans each in turn and prints combined totals.

## Current limitations

- No support for suffix notation, e.g. `12.50 USD`.
- Symbols are not mapped to ISO currency codes, so all `$` amounts are
  summed together regardless of which dollar they actually are.

## License

MIT, see LICENSE.
