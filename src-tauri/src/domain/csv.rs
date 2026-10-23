//! A small CSV reader for imports (bank statements now, products and parties in Phase 12).
//! RFC 4180 quoting ("a ""quoted"" field", line breaks inside quotes), a UTF-8 byte-order mark,
//! CRLF or LF, and comma, semicolon or tab separators (guessed from the first line).

/// Guesses the separator from the first line: whichever of `,` `;` and tab occurs most outside
/// quotes (comma when none does).
pub fn guess_separator(text: &str) -> char {
    let first = text.lines().next().unwrap_or_default();
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for c in first.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            for (sep, n) in &mut counts {
                if c == *sep {
                    *n += 1;
                }
            }
        }
    }
    counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map_or(',', |(c, _)| *c)
}

/// Splits CSV text into rows of fields. Blank lines are skipped; fields are not trimmed.
pub fn parse(text: &str, separator: char) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            c if c == separator => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                if row.iter().any(|f| !f.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if row.iter().any(|f| !f.trim().is_empty()) {
            rows.push(row);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{guess_separator, parse};

    #[test]
    fn reads_quotes_line_breaks_and_separators() {
        let text = "\u{feff}Date,Narration,Amount\r\n01/10/2026,\"UPI, Ravi \"\"R\"\" Stores\",\"1,250.00\"\r\n\r\n02/10/2026,\"two\nlines\",-40\n";
        let rows = parse(text, guess_separator(text));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], ["Date", "Narration", "Amount"]);
        assert_eq!(
            rows[1],
            ["01/10/2026", "UPI, Ravi \"R\" Stores", "1,250.00"]
        );
        assert_eq!(rows[2], ["02/10/2026", "two\nlines", "-40"]);
        assert_eq!(guess_separator("a;b;c\n1;2;3"), ';');
        assert_eq!(guess_separator("a\tb\n"), '\t');
        assert_eq!(guess_separator("single"), ',');
        assert_eq!(parse("a,b", ','), [["a", "b"]]);
        assert_eq!(parse("a,", ','), [["a", ""]]);
    }
}
