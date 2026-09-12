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

Amounts are matched per line in two forms. The first is a currency symbol
(`$`, `£`, `€`, `¥`) followed directly by digits, with optional comma
thousands separators and an optional two-digit decimal fraction. The
second is a bare number followed by a three-letter uppercase currency
code, e.g. `12.50 USD` -- the code just has to sit at a word boundary, so
`USDT` or `USD1` don't get misread as `USD`. A `-` immediately before the
amount (the symbol, or the leading digit for a suffix code) makes it
negative, and so does wrapping the whole thing in parentheses, e.g.
`($12.34)` or `(12.34 USD)`, the accounting convention for a negative
number -- the closing paren has to sit right after the amount for this to
count, otherwise the `(` is just a stray character. Amounts are tracked
separately per symbol or code -- `$` and `€` totals are never mixed
together, and no attempt is made to guess that a `$` means USD versus CAD
versus AUD, or that `$` and `USD` are the same currency.

## Usage

```
money-tally [--map SYMBOL=CODE[,SYMBOL=CODE...]] [file ...]
```

With no arguments it reads from stdin. With one or more file arguments it
scans each in turn and prints combined totals.

By default `$`, `£`, `€`, and `¥` are totaled separately from any ISO
code that happens to mean the same currency -- a `$` total is never
combined with a `USD` total, because `$` alone doesn't say which dollar
it is. `--map` lets you say so explicitly:

```
$ money-tally --map '$=USD' receipts.txt
$29.49 USD total across 4 amounts
```

Once mapped, a symbol's amounts are folded into that code's total, so
`$5` and `5 USD` in the same input add up together. The flag can be
repeated, or given a comma-separated list, to map more than one symbol:

```
money-tally --map '$=USD,£=GBP' receipts.txt
```

## Current limitations

- Every dollar sign still has to be mapped to the same code -- there's
  no way to tell a `$` meaning USD from a `$` meaning CAD within the
  same run.

## License

MIT, see LICENSE.
