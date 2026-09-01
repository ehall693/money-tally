use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

// Symbols we recognize as the start of an amount. Kept as a small fixed set
// rather than anything configurable for now -- see README for what's missing.
const SYMBOLS: [char; 4] = ['$', '\u{a3}', '\u{20ac}', '\u{a5}'];

fn main() {
    let paths: Vec<String> = env::args().skip(1).collect();

    let mut totals: BTreeMap<char, i64> = BTreeMap::new();
    let mut counts: BTreeMap<char, u64> = BTreeMap::new();

    let result = if paths.is_empty() {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        scan(&mut handle, &mut totals, &mut counts)
    } else {
        let mut result = Ok(());
        for path in &paths {
            match File::open(path) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    if let Err(e) = scan(&mut reader, &mut totals, &mut counts) {
                        result = Err(format!("reading {path}: {e}"));
                        break;
                    }
                }
                Err(e) => {
                    result = Err(format!("opening {path}: {e}"));
                    break;
                }
            }
        }
        result
    };

    if let Err(msg) = result {
        eprintln!("money-tally: {msg}");
        std::process::exit(1);
    }

    if totals.is_empty() {
        println!("no currency amounts found");
        return;
    }

    for (symbol, cents) in &totals {
        let n = counts[symbol];
        let plural = if n == 1 { "" } else { "s" };
        println!("{}{} total across {} amount{}", symbol, format_cents(*cents), n, plural);
    }
}

// Reads one line at a time into a reused buffer so total memory use stays
// bounded by the longest single line, not by the size of the input.
fn scan<R: BufRead>(
    reader: &mut R,
    totals: &mut BTreeMap<char, i64>,
    counts: &mut BTreeMap<char, u64>,
) -> Result<(), String> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }
        for (symbol, cents) in find_amounts(&line) {
            *totals.entry(symbol).or_insert(0) += cents;
            *counts.entry(symbol).or_insert(0) += 1;
        }
    }
    Ok(())
}

// Returns (symbol, amount-in-minor-units) for every recognizable amount in a
// line, e.g. "$1,234.56" -> ('$', 123456), "-$5" -> ('$', -500).
fn find_amounts(line: &str) -> Vec<(char, i64)> {
    let chars: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if SYMBOLS.contains(&c) {
            let negative = i > 0 && chars[i - 1] == '-';
            if let Some((cents, next_i)) = parse_amount(&chars, i + 1) {
                found.push((c, if negative { -cents } else { cents }));
                i = next_i;
                continue;
            }
        }
        i += 1;
    }
    found
}

// Parses digits, optional comma thousands separators, and an optional
// decimal fraction starting at `start`. Returns the amount in minor units
// (cents) and the index just past what it consumed.
fn parse_amount(chars: &[char], start: usize) -> Option<(i64, usize)> {
    let mut i = start;
    let mut whole: i64 = 0;
    let mut saw_digit = false;

    while i < chars.len() {
        match chars[i] {
            d if d.is_ascii_digit() => {
                whole = whole * 10 + (d as i64 - '0' as i64);
                saw_digit = true;
                i += 1;
            }
            ',' if saw_digit => i += 1,
            _ => break,
        }
    }

    if !saw_digit {
        return None;
    }

    let mut cents: i64 = 0;
    if i < chars.len() && chars[i] == '.' {
        let mut frac_digits = 0;
        let mut frac: i64 = 0;
        let mut j = i + 1;
        while j < chars.len() && chars[j].is_ascii_digit() && frac_digits < 2 {
            frac = frac * 10 + (chars[j] as i64 - '0' as i64);
            frac_digits += 1;
            j += 1;
        }
        if frac_digits > 0 {
            if frac_digits == 1 {
                frac *= 10;
            }
            cents = frac;
            i = j;
            // A third+ fractional digit still belongs to this number, we
            // just don't count it towards the cents total.
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
        }
    }

    Some((whole * 100 + cents, i))
}

fn format_cents(cents: i64) -> String {
    let negative = cents < 0;
    let cents = cents.unsigned_abs();
    let whole = cents / 100;
    let frac = cents % 100;

    let digits = whole.to_string();
    let mut grouped = String::new();
    for (count, ch) in digits.chars().rev().enumerate() {
        if count != 0 && count % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let grouped: String = grouped.chars().rev().collect();

    format!("{}{}.{:02}", if negative { "-" } else { "" }, grouped, frac)
}
