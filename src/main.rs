use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

// Symbols we recognize as the start of an amount. Kept as a small fixed set
// rather than anything configurable for now -- see README for what's missing.
const SYMBOLS: [char; 4] = ['$', '\u{a3}', '\u{20ac}', '\u{a5}'];

// A symbol amount ("$5") and a suffix-code amount ("5 USD") are tracked
// under the same key type so they can share one total/count map, but they
// print differently, hence the enum instead of just a String.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Currency {
    Symbol(char),
    Code(String),
}

fn main() {
    let paths: Vec<String> = env::args().skip(1).collect();

    let mut totals: BTreeMap<Currency, i64> = BTreeMap::new();
    let mut counts: BTreeMap<Currency, u64> = BTreeMap::new();

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

    for (currency, cents) in &totals {
        let n = counts[currency];
        let plural = if n == 1 { "" } else { "s" };
        let amount = format_cents(*cents);
        match currency {
            Currency::Symbol(s) => println!("{s}{amount} total across {n} amount{plural}"),
            Currency::Code(code) => println!("{amount} {code} total across {n} amount{plural}"),
        }
    }
}

// Reads one line at a time into a reused buffer so total memory use stays
// bounded by the longest single line, not by the size of the input.
fn scan<R: BufRead>(
    reader: &mut R,
    totals: &mut BTreeMap<Currency, i64>,
    counts: &mut BTreeMap<Currency, u64>,
) -> Result<(), String> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if bytes_read == 0 {
            break;
        }
        for (currency, cents) in find_amounts(&line) {
            *totals.entry(currency.clone()).or_insert(0) += cents;
            *counts.entry(currency).or_insert(0) += 1;
        }
    }
    Ok(())
}

// Returns (currency, amount-in-minor-units) for every recognizable amount in
// a line, e.g. "$1,234.56" -> (Symbol('$'), 123456), "-$5" -> (Symbol('$'),
// -500), "($5.00)" -> (Symbol('$'), -500), "12.50 USD" -> (Code("USD"),
// 1250).
fn find_amounts(line: &str) -> Vec<(Currency, i64)> {
    let chars: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if SYMBOLS.contains(&c) {
            let dash_negative = i > 0 && chars[i - 1] == '-';
            let paren_open = i > 0 && chars[i - 1] == '(';
            if let Some((cents, next_i)) = parse_amount(&chars, i + 1) {
                let mut end = next_i;
                let mut negative = dash_negative;
                // Only treat this as a parenthesized negative if the paren
                // actually closes right after the amount -- "(" alone isn't
                // evidence of anything.
                if paren_open && end < chars.len() && chars[end] == ')' {
                    negative = true;
                    end += 1;
                }
                found.push((Currency::Symbol(c), if negative { -cents } else { cents }));
                i = end;
                continue;
            }
        } else if c.is_ascii_digit() {
            // Only start a bare-number scan at a genuine word boundary --
            // otherwise "AB123 USD" would read as "123 USD".
            let left_ok = i == 0 || !chars[i - 1].is_ascii_alphanumeric();
            if left_ok {
                let dash_negative = i > 0 && chars[i - 1] == '-';
                let paren_open = i > 0 && chars[i - 1] == '(';
                if let Some((cents, next_i)) = parse_amount(&chars, i) {
                    if let Some((code, code_end)) = currency_code_after(&chars, next_i) {
                        let mut end = code_end;
                        let mut negative = dash_negative;
                        if paren_open && end < chars.len() && chars[end] == ')' {
                            negative = true;
                            end += 1;
                        }
                        found.push((Currency::Code(code), if negative { -cents } else { cents }));
                    }
                    // Whether or not a code followed, the whole number has
                    // been accounted for -- don't re-parse a sub-run of it.
                    i = next_i;
                    continue;
                }
            }
        }
        i += 1;
    }
    found
}

// Looks for a three-letter uppercase currency code starting at `i`, skipping
// any spaces first. Requires the code to end at a word boundary so "USDT" or
// "USD1" don't get misread as "USD".
fn currency_code_after(chars: &[char], mut i: usize) -> Option<(String, usize)> {
    while i < chars.len() && chars[i] == ' ' {
        i += 1;
    }
    if i + 3 > chars.len() {
        return None;
    }
    if !chars[i..i + 3].iter().all(|c| c.is_ascii_uppercase()) {
        return None;
    }
    if i + 3 < chars.len() && chars[i + 3].is_ascii_alphanumeric() {
        return None;
    }
    Some((chars[i..i + 3].iter().collect(), i + 3))
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
