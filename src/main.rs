use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

// e.g. "$=USD" or "$=USD,£=GBP" via one or more --map flags.
type SymbolMap = BTreeMap<char, String>;

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
    let args = match parse_args(env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("money-tally: {msg}");
            std::process::exit(1);
        }
    };

    if args.per_file && args.paths.is_empty() {
        eprintln!("money-tally: --per-file requires at least one file argument");
        std::process::exit(1);
    }

    let mut overall_totals: BTreeMap<Currency, i64> = BTreeMap::new();
    let mut overall_counts: BTreeMap<Currency, u64> = BTreeMap::new();

    let result = if args.paths.is_empty() {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        scan(&mut handle, &args, &mut overall_totals, &mut overall_counts)
    } else if args.per_file {
        let mut result = Ok(());
        for path in &args.paths {
            let mut file_totals: BTreeMap<Currency, i64> = BTreeMap::new();
            let mut file_counts: BTreeMap<Currency, u64> = BTreeMap::new();
            match File::open(path) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    if let Err(e) = scan(&mut reader, &args, &mut file_totals, &mut file_counts) {
                        result = Err(format!("reading {path}: {e}"));
                        break;
                    }
                }
                Err(e) => {
                    result = Err(format!("opening {path}: {e}"));
                    break;
                }
            }
            println!("{path}:");
            print_totals(&file_totals, &file_counts);
            println!();
            merge_into(&mut overall_totals, &mut overall_counts, &file_totals, &file_counts);
        }
        result
    } else {
        let mut result = Ok(());
        for path in &args.paths {
            match File::open(path) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    if let Err(e) = scan(&mut reader, &args, &mut overall_totals, &mut overall_counts) {
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

    if args.per_file && args.paths.len() > 1 {
        println!("overall:");
    }
    if !args.per_file || args.paths.len() > 1 {
        print_totals(&overall_totals, &overall_counts);
    }
}

fn print_totals(totals: &BTreeMap<Currency, i64>, counts: &BTreeMap<Currency, u64>) {
    if totals.is_empty() {
        println!("no currency amounts found");
        return;
    }

    for (currency, cents) in totals {
        let n = counts[currency];
        let plural = if n == 1 { "" } else { "s" };
        let amount = format_cents(*cents);
        match currency {
            Currency::Symbol(s) => println!("{s}{amount} total across {n} amount{plural}"),
            Currency::Code(code) => println!("{amount} {code} total across {n} amount{plural}"),
        }
    }
}

// Folds one file's totals/counts into the running combined totals, used to
// build the "overall" summary after a --per-file breakdown without
// re-reading every file a second time.
fn merge_into(
    totals: &mut BTreeMap<Currency, i64>,
    counts: &mut BTreeMap<Currency, u64>,
    file_totals: &BTreeMap<Currency, i64>,
    file_counts: &BTreeMap<Currency, u64>,
) {
    for (currency, cents) in file_totals {
        *totals.entry(currency.clone()).or_insert(0) += cents;
    }
    for (currency, n) in file_counts {
        *counts.entry(currency.clone()).or_insert(0) += n;
    }
}

// Parsed command-line arguments, kept as one struct once there were enough
// flags that a tuple got hard to read at the call site.
struct Args {
    map: SymbolMap,
    per_file: bool,
    min: Option<i64>,
    max: Option<i64>,
    paths: Vec<String>,
}

// Splits the recognized flags (and the file-path arguments) out of the raw
// args. `--map SPEC` takes one or more "symbol=CODE" pairs separated by
// commas, e.g. "$=USD,£=GBP"; the flag can also be repeated to build up one
// map. `--min`/`--max` each take a plain decimal amount, e.g. "10" or
// "10.50", and bound which amounts count toward the total.
fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Args, String> {
    let mut map = SymbolMap::new();
    let mut per_file = false;
    let mut min = None;
    let mut max = None;
    let mut paths = Vec::new();
    let mut args = args;

    while let Some(arg) = args.next() {
        if let Some(spec) = arg.strip_prefix("--map=") {
            parse_map_spec(spec, &mut map)?;
        } else if arg == "--map" {
            let spec = args
                .next()
                .ok_or_else(|| "--map requires an argument, e.g. --map $=USD".to_string())?;
            parse_map_spec(&spec, &mut map)?;
        } else if arg == "--per-file" {
            per_file = true;
        } else if let Some(value) = arg.strip_prefix("--min=") {
            min = Some(parse_decimal_arg("min", value)?);
        } else if arg == "--min" {
            let value = args
                .next()
                .ok_or_else(|| "--min requires an argument, e.g. --min 10".to_string())?;
            min = Some(parse_decimal_arg("min", &value)?);
        } else if let Some(value) = arg.strip_prefix("--max=") {
            max = Some(parse_decimal_arg("max", value)?);
        } else if arg == "--max" {
            let value = args
                .next()
                .ok_or_else(|| "--max requires an argument, e.g. --max 100".to_string())?;
            max = Some(parse_decimal_arg("max", &value)?);
        } else {
            paths.push(arg);
        }
    }

    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            return Err(format!(
                "--min ({}) is greater than --max ({})",
                format_cents(min),
                format_cents(max)
            ));
        }
    }

    Ok(Args { map, per_file, min, max, paths })
}

// Parses a whole `--min`/`--max` argument (optionally negative) into minor
// units, rejecting anything left over -- "10x" isn't a number just because
// it starts with one.
fn parse_decimal_arg(flag: &str, s: &str) -> Result<i64, String> {
    let chars: Vec<char> = s.chars().collect();
    let negative = chars.first() == Some(&'-');
    let start = if negative { 1 } else { 0 };
    match parse_amount(&chars, start) {
        Some((cents, end)) if end == chars.len() => Ok(if negative { -cents } else { cents }),
        _ => Err(format!(
            "invalid --{flag} value \"{s}\", expected a number like 10 or 10.50"
        )),
    }
}

fn parse_map_spec(spec: &str, map: &mut SymbolMap) -> Result<(), String> {
    for pair in spec.split(',') {
        let (symbol, code) = pair
            .split_once('=')
            .ok_or_else(|| format!("invalid --map entry \"{pair}\", expected SYMBOL=CODE"))?;

        let mut symbol_chars = symbol.chars();
        let symbol_char = symbol_chars
            .next()
            .filter(|_| symbol_chars.next().is_none())
            .ok_or_else(|| format!("invalid --map symbol \"{symbol}\", expected a single character"))?;

        if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(format!(
                "invalid --map code \"{code}\", expected three uppercase letters"
            ));
        }

        map.insert(symbol_char, code.to_string());
    }
    Ok(())
}

// Reads one line at a time into a reused buffer so total memory use stays
// bounded by the longest single line, not by the size of the input.
fn scan<R: BufRead>(
    reader: &mut R,
    args: &Args,
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
        for (currency, cents) in find_amounts(&line, &args.map) {
            if args.min.is_some_and(|min| cents < min) || args.max.is_some_and(|max| cents > max) {
                continue;
            }
            *totals.entry(currency.clone()).or_insert(0) += cents;
            *counts.entry(currency).or_insert(0) += 1;
        }
    }
    Ok(())
}

// Returns (currency, amount-in-minor-units) for every recognizable amount in
// a line, e.g. "$1,234.56" -> (Symbol('$'), 123456), "-$5" -> (Symbol('$'),
// -500), "($5.00)" -> (Symbol('$'), -500), "12.50 USD" -> (Code("USD"),
// 1250). Symbols present in `symbol_map` are reported as their mapped
// `Code` instead, so e.g. "$5" and "5 USD" land in the same total.
fn find_amounts(line: &str, symbol_map: &SymbolMap) -> Vec<(Currency, i64)> {
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
                let currency = match symbol_map.get(&c) {
                    Some(code) => Currency::Code(code.clone()),
                    None => Currency::Symbol(c),
                };
                found.push((currency, if negative { -cents } else { cents }));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(chars_str: &str, start: usize) -> Option<(i64, usize)> {
        let chars: Vec<char> = chars_str.chars().collect();
        parse_amount(&chars, start)
    }

    #[test]
    fn parse_amount_plain_whole_number() {
        assert_eq!(parse("500", 0), Some((50000, 3)));
    }

    #[test]
    fn parse_amount_with_decimal() {
        assert_eq!(parse("12.34", 0), Some((1234, 5)));
    }

    #[test]
    fn parse_amount_single_fraction_digit_is_scaled_to_tenths() {
        assert_eq!(parse("1.5", 0), Some((150, 3)));
    }

    #[test]
    fn parse_amount_extra_fraction_digits_are_consumed_but_ignored() {
        // Only the first two fractional digits count toward cents, but a
        // third one still has to be swallowed so it isn't mistaken for the
        // start of a new number.
        assert_eq!(parse("1.239", 0), Some((123, 5)));
    }

    #[test]
    fn parse_amount_trailing_dot_with_no_digits_is_not_consumed() {
        assert_eq!(parse("5.", 0), Some((500, 1)));
    }

    #[test]
    fn parse_amount_thousands_separators() {
        assert_eq!(parse("1,234,567.89", 0), Some((123456789, 12)));
    }

    #[test]
    fn parse_amount_comma_before_any_digit_is_rejected() {
        assert_eq!(parse(",500", 0), None);
    }

    #[test]
    fn parse_amount_no_digits_is_none() {
        assert_eq!(parse("abc", 0), None);
    }

    fn amounts(line: &str) -> Vec<(Currency, i64)> {
        find_amounts(line, &SymbolMap::new())
    }

    #[test]
    fn find_amounts_empty_line() {
        assert_eq!(amounts(""), vec![]);
    }

    #[test]
    fn find_amounts_plain_text_with_no_numbers() {
        assert_eq!(amounts("no money here"), vec![]);
    }

    #[test]
    fn find_amounts_simple_symbol() {
        assert_eq!(amounts("$5"), vec![(Currency::Symbol('$'), 500)]);
    }

    #[test]
    fn find_amounts_symbol_with_no_following_digits_is_ignored() {
        assert_eq!(amounts("$ five dollars"), vec![]);
    }

    #[test]
    fn find_amounts_bare_symbol_at_end_of_line() {
        assert_eq!(amounts("cost: $"), vec![]);
    }

    #[test]
    fn find_amounts_dash_negative_symbol() {
        assert_eq!(amounts("-$3.00"), vec![(Currency::Symbol('$'), -300)]);
    }

    #[test]
    fn find_amounts_parenthesized_negative_symbol() {
        assert_eq!(amounts("($5.00)"), vec![(Currency::Symbol('$'), -500)]);
    }

    #[test]
    fn find_amounts_unclosed_paren_is_not_negative() {
        assert_eq!(amounts("($5.00 refund pending"), vec![(Currency::Symbol('$'), 500)]);
    }

    #[test]
    fn find_amounts_suffix_code() {
        assert_eq!(amounts("12.50 USD"), vec![(Currency::Code("USD".to_string()), 1250)]);
    }

    #[test]
    fn find_amounts_suffix_code_with_no_space() {
        assert_eq!(amounts("12.50USD"), vec![(Currency::Code("USD".to_string()), 1250)]);
    }

    #[test]
    fn find_amounts_negative_suffix_code() {
        assert_eq!(amounts("-12.50 USD"), vec![(Currency::Code("USD".to_string()), -1250)]);
    }

    #[test]
    fn find_amounts_parenthesized_negative_suffix_code() {
        assert_eq!(amounts("(12.50 USD)"), vec![(Currency::Code("USD".to_string()), -1250)]);
    }

    #[test]
    fn find_amounts_number_glued_to_extra_letters_is_not_a_code() {
        assert_eq!(amounts("12.50 USDT"), vec![]);
    }

    #[test]
    fn find_amounts_code_glued_to_extra_digit_is_not_a_code() {
        assert_eq!(amounts("12.50 USD1"), vec![]);
    }

    #[test]
    fn find_amounts_number_preceded_by_letters_is_not_a_word_boundary() {
        assert_eq!(amounts("AB123 USD"), vec![]);
    }

    #[test]
    fn find_amounts_lowercase_suffix_is_not_a_code() {
        assert_eq!(amounts("12.50 usd"), vec![]);
    }

    #[test]
    fn find_amounts_multiple_amounts_one_line() {
        assert_eq!(
            amounts("$5 and $10"),
            vec![(Currency::Symbol('$'), 500), (Currency::Symbol('$'), 1000)]
        );
    }

    #[test]
    fn find_amounts_distinct_symbols_stay_distinct() {
        assert_eq!(
            amounts("$5 and \u{a3}5"),
            vec![(Currency::Symbol('$'), 500), (Currency::Symbol('\u{a3}'), 500)]
        );
    }

    #[test]
    fn find_amounts_maps_symbol_to_code() {
        let mut map = SymbolMap::new();
        map.insert('$', "USD".to_string());
        assert_eq!(
            find_amounts("$5", &map),
            vec![(Currency::Code("USD".to_string()), 500)]
        );
    }

    #[test]
    fn parse_map_spec_single_entry() {
        let mut map = SymbolMap::new();
        parse_map_spec("$=USD", &mut map).unwrap();
        assert_eq!(map.get(&'$'), Some(&"USD".to_string()));
    }

    #[test]
    fn parse_map_spec_multiple_entries() {
        let mut map = SymbolMap::new();
        parse_map_spec("$=USD,\u{a3}=GBP", &mut map).unwrap();
        assert_eq!(map.get(&'$'), Some(&"USD".to_string()));
        assert_eq!(map.get(&'\u{a3}'), Some(&"GBP".to_string()));
    }

    #[test]
    fn parse_map_spec_missing_equals_is_an_error() {
        let mut map = SymbolMap::new();
        assert!(parse_map_spec("$USD", &mut map).is_err());
    }

    #[test]
    fn parse_map_spec_multi_char_symbol_is_an_error() {
        let mut map = SymbolMap::new();
        assert!(parse_map_spec("$$=USD", &mut map).is_err());
    }

    #[test]
    fn parse_map_spec_lowercase_code_is_an_error() {
        let mut map = SymbolMap::new();
        assert!(parse_map_spec("$=usd", &mut map).is_err());
    }

    #[test]
    fn parse_map_spec_wrong_length_code_is_an_error() {
        let mut map = SymbolMap::new();
        assert!(parse_map_spec("$=US", &mut map).is_err());
    }

    #[test]
    fn parse_args_defaults_with_no_flags() {
        let args = parse_args(vec!["a.txt".to_string()].into_iter()).unwrap();
        assert!(args.map.is_empty());
        assert!(!args.per_file);
        assert_eq!(args.min, None);
        assert_eq!(args.max, None);
        assert_eq!(args.paths, vec!["a.txt".to_string()]);
    }

    #[test]
    fn parse_args_map_flag_with_separate_argument() {
        let raw = vec!["--map".to_string(), "$=USD".to_string(), "a.txt".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert_eq!(args.map.get(&'$'), Some(&"USD".to_string()));
        assert_eq!(args.paths, vec!["a.txt".to_string()]);
    }

    #[test]
    fn parse_args_map_flag_with_equals_form() {
        let raw = vec!["--map=$=USD".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert_eq!(args.map.get(&'$'), Some(&"USD".to_string()));
    }

    #[test]
    fn parse_args_map_with_no_argument_is_an_error() {
        let raw = vec!["--map".to_string()];
        assert!(parse_args(raw.into_iter()).is_err());
    }

    #[test]
    fn parse_args_per_file_flag() {
        let raw = vec!["--per-file".to_string(), "a.txt".to_string(), "b.txt".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert!(args.per_file);
        assert_eq!(args.paths, vec!["a.txt".to_string(), "b.txt".to_string()]);
    }

    #[test]
    fn parse_args_min_flag_with_separate_argument() {
        let raw = vec!["--min".to_string(), "10.50".to_string(), "a.txt".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert_eq!(args.min, Some(1050));
        assert_eq!(args.paths, vec!["a.txt".to_string()]);
    }

    #[test]
    fn parse_args_max_flag_with_equals_form() {
        let raw = vec!["--max=100".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert_eq!(args.max, Some(10000));
    }

    #[test]
    fn parse_args_min_flag_accepts_negative_value() {
        let raw = vec!["--min=-5.00".to_string()];
        let args = parse_args(raw.into_iter()).unwrap();
        assert_eq!(args.min, Some(-500));
    }

    #[test]
    fn parse_args_min_with_no_argument_is_an_error() {
        let raw = vec!["--min".to_string()];
        assert!(parse_args(raw.into_iter()).is_err());
    }

    #[test]
    fn parse_args_min_with_garbage_value_is_an_error() {
        let raw = vec!["--min=abc".to_string()];
        assert!(parse_args(raw.into_iter()).is_err());
    }

    #[test]
    fn parse_args_min_with_trailing_garbage_is_an_error() {
        let raw = vec!["--min=10x".to_string()];
        assert!(parse_args(raw.into_iter()).is_err());
    }

    #[test]
    fn parse_args_min_greater_than_max_is_an_error() {
        let raw = vec!["--min=100".to_string(), "--max=10".to_string()];
        assert!(parse_args(raw.into_iter()).is_err());
    }

    #[test]
    fn scan_min_filters_out_smaller_amounts() {
        let args = parse_args(vec!["--min=10".to_string()].into_iter()).unwrap();
        let mut totals = BTreeMap::new();
        let mut counts = BTreeMap::new();
        let mut input = "$5 and $15".as_bytes();
        scan(&mut input, &args, &mut totals, &mut counts).unwrap();
        assert_eq!(totals.get(&Currency::Symbol('$')), Some(&1500));
        assert_eq!(counts.get(&Currency::Symbol('$')), Some(&1));
    }

    #[test]
    fn scan_max_filters_out_larger_amounts() {
        let args = parse_args(vec!["--max=10".to_string()].into_iter()).unwrap();
        let mut totals = BTreeMap::new();
        let mut counts = BTreeMap::new();
        let mut input = "$5 and $15".as_bytes();
        scan(&mut input, &args, &mut totals, &mut counts).unwrap();
        assert_eq!(totals.get(&Currency::Symbol('$')), Some(&500));
        assert_eq!(counts.get(&Currency::Symbol('$')), Some(&1));
    }

    #[test]
    fn scan_min_and_max_together_keep_only_the_range() {
        let args =
            parse_args(vec!["--min=10".to_string(), "--max=20".to_string()].into_iter()).unwrap();
        let mut totals = BTreeMap::new();
        let mut counts = BTreeMap::new();
        let mut input = "$5 $15 $25".as_bytes();
        scan(&mut input, &args, &mut totals, &mut counts).unwrap();
        assert_eq!(totals.get(&Currency::Symbol('$')), Some(&1500));
        assert_eq!(counts.get(&Currency::Symbol('$')), Some(&1));
    }

    #[test]
    fn format_cents_basic() {
        assert_eq!(format_cents(2949), "29.49");
    }

    #[test]
    fn format_cents_negative() {
        assert_eq!(format_cents(-300), "-3.00");
    }

    #[test]
    fn format_cents_thousands_grouping() {
        assert_eq!(format_cents(123456789), "1,234,567.89");
    }

    #[test]
    fn format_cents_zero() {
        assert_eq!(format_cents(0), "0.00");
    }
}
