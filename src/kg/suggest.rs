//! Suggestions for an unknown ID (G28 §2): IDs whose last name is within a small edit distance.

/// The edit distance with adjacent transpositions counted as one edit (optimal string alignment),
/// by characters.
pub fn osa_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The last name of an ID: after the last `.` or `::`, lowercased.
pub fn last_name(text: &str) -> String {
    let after_dot = text.rsplit('.').next().unwrap_or(text);
    after_dot
        .rsplit("::")
        .next()
        .unwrap_or(after_dot)
        .to_lowercase()
}

/// Up to 5 IDs of `ids` whose last name is within 1 edit of `text`'s (2 for a name of 5 or more
/// characters), nearest first, then in byte order.
pub fn suggestions(ids: &[&str], text: &str) -> Vec<String> {
    let name = last_name(text);
    let limit = if name.chars().count() <= 4 { 1 } else { 2 };
    let mut found: Vec<(usize, &str)> = ids
        .iter()
        .map(|id| (osa_distance(&name, &last_name(id)), *id))
        .filter(|(distance, _)| *distance <= limit)
        .collect();
    found.sort();
    found
        .into_iter()
        .take(5)
        .map(|(_, id)| id.to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_transpositions_as_one_edit() {
        assert_eq!(osa_distance("totl", "total"), 1);
        assert_eq!(osa_distance("ab", "ba"), 1);
        assert_eq!(osa_distance("", "abc"), 3);
        assert_eq!(osa_distance("kitten", "sitting"), 3);
        assert_eq!(osa_distance("écrit", "ecrit"), 1);
    }

    #[test]
    fn suggests_near_last_names() {
        let ids = [
            "Acme::Shop::Cart.total",
            "IO.write_line",
            "Acme::Shop::Cart.Line",
        ];
        assert_eq!(suggestions(&ids, "totl"), ["Acme::Shop::Cart.total"]);
        assert_eq!(suggestions(&ids, "IO.writ_line"), ["IO.write_line"]);
        assert!(suggestions(&ids, "zzz").is_empty());
        assert_eq!(last_name("A::B.C"), "c");
        assert_eq!(last_name("A::B"), "b");
    }
}
