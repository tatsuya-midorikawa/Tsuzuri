//! Line diffs for `tsuzuri fix` (G24): the shortest edit script between two texts (Myers'
//! forward greedy algorithm), unified diffs in the format of `diff -u`, and the line edits that
//! turn one text into the other. No external `diff` runs.

/// The most inserted and deleted lines that a diff computes; beyond it there is no diff.
pub const MAX_DIFF_CHANGES: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Equal,
    Delete,
    Insert,
}

/// Replaces the bytes `start..end` of the old text (whole lines) with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineEdit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// The lines of `text`, each with its line break.
pub fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The shortest edit script from `a` to `b`, one `Step` per line; `None` when it changes more
/// than `MAX_DIFF_CHANGES` lines.
pub fn shortest_script(a: &[&str], b: &[&str]) -> Option<Vec<Step>> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let limit = ((n + m) as usize).min(MAX_DIFF_CHANGES) as isize;
    let offset = limit + 1;
    let mut v = vec![0isize; (2 * limit + 3) as usize];
    // trace[d][k + d] = the furthest x on diagonal k after round d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=limit {
        let mut done = false;
        for k in (-d..=d).step_by(2) {
            let index = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x >= n && y >= m {
                done = true;
                break;
            }
        }
        trace.push(v[(offset - d) as usize..=(offset + d) as usize].to_vec());
        if done {
            return Some(backtrack(&trace, n, m));
        }
    }
    None
}

fn backtrack(trace: &[Vec<isize>], n: isize, m: isize) -> Vec<Step> {
    let mut steps = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..trace.len() as isize).rev() {
        let k = x - y;
        if d == 0 {
            while x > 0 && y > 0 {
                steps.push(Step::Equal);
                x -= 1;
                y -= 1;
            }
            break;
        }
        let previous = &trace[(d - 1) as usize];
        let at = |j: isize| previous[(j + d - 1) as usize];
        let (previous_k, step) = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            (k + 1, Step::Insert)
        } else {
            (k - 1, Step::Delete)
        };
        let previous_x = at(previous_k);
        let previous_y = previous_x - previous_k;
        // The snake of round d ends at (x, y); the move came from (previous_x, previous_y).
        let (start_x, start_y) = match step {
            Step::Insert => (previous_x, previous_y + 1),
            _ => (previous_x + 1, previous_y),
        };
        while x > start_x && y > start_y {
            steps.push(Step::Equal);
            x -= 1;
            y -= 1;
        }
        steps.push(step);
        x = previous_x;
        y = previous_y;
    }
    steps.reverse();
    steps
}

/// `{start + 1},{len}` of a hunk header, as `diff -u` writes it.
fn range(start: usize, len: usize) -> String {
    match len {
        0 => format!("{start},0"),
        1 => format!("{}", start + 1),
        _ => format!("{},{len}", start + 1),
    }
}

fn push_line(output: &mut String, mark: char, line: &str) {
    output.push(mark);
    output.push_str(line);
    if !line.ends_with('\n') {
        output.push_str("\n\\ No newline at end of file\n");
    }
}

/// The unified diff of `old` and `new` with `context` lines around each change, headed by
/// `--- a/{path}` and `+++ b/{path}`: empty when the texts are equal, `None` when they differ
/// in more than `MAX_DIFF_CHANGES` lines.
pub fn unified(path: &str, old: &str, new: &str, context: usize) -> Option<String> {
    if old == new {
        return Some(String::new());
    }
    let (a, b) = (split_lines(old), split_lines(new));
    let steps = shortest_script(&a, &b)?;
    // The position (i, j) before each step.
    let mut positions = Vec::with_capacity(steps.len());
    let (mut i, mut j) = (0, 0);
    for step in &steps {
        positions.push((i, j));
        match step {
            Step::Equal => (i, j) = (i + 1, j + 1),
            Step::Delete => i += 1,
            Step::Insert => j += 1,
        }
    }
    let changes: Vec<usize> = (0..steps.len())
        .filter(|index| steps[*index] != Step::Equal)
        .collect();
    let mut output = format!("--- a/{path}\n+++ b/{path}\n");
    let mut first = 0;
    while first < changes.len() {
        // Join the changes whose gaps hold at most `2 * context` equal lines.
        let mut last = first;
        while last + 1 < changes.len() && changes[last + 1] - changes[last] - 1 <= 2 * context {
            last += 1;
        }
        let start = changes[first].saturating_sub(context);
        let end = (changes[last] + 1 + context).min(steps.len());
        let (old_start, new_start) = positions[start];
        let (old_end, new_end) = if end < steps.len() {
            positions[end]
        } else {
            (a.len(), b.len())
        };
        output.push_str(&format!(
            "@@ -{} +{} @@\n",
            range(old_start, old_end - old_start),
            range(new_start, new_end - new_start)
        ));
        for index in start..end {
            let (i, j) = positions[index];
            match steps[index] {
                Step::Equal => push_line(&mut output, ' ', a[i]),
                Step::Delete => push_line(&mut output, '-', a[i]),
                Step::Insert => push_line(&mut output, '+', b[j]),
            }
        }
        first = last + 1;
    }
    Some(output)
}

/// The edits that turn `old` into `new`, one per run of changed lines, in order; `None` when the
/// texts differ in more than `MAX_DIFF_CHANGES` lines.
pub fn line_edits(old: &str, new: &str) -> Option<Vec<LineEdit>> {
    let (a, b) = (split_lines(old), split_lines(new));
    let steps = shortest_script(&a, &b)?;
    let mut edits: Vec<LineEdit> = Vec::new();
    let (mut i, mut j, mut offset) = (0, 0, 0);
    let mut open: Option<LineEdit> = None;
    for step in steps {
        match step {
            Step::Equal => {
                edits.extend(open.take());
                offset += a[i].len();
                (i, j) = (i + 1, j + 1);
            }
            Step::Delete => {
                let edit = open.get_or_insert_with(|| LineEdit {
                    start: offset,
                    end: offset,
                    text: String::new(),
                });
                offset += a[i].len();
                edit.end = offset;
                i += 1;
            }
            Step::Insert => {
                open.get_or_insert_with(|| LineEdit {
                    start: offset,
                    end: offset,
                    text: String::new(),
                })
                .text
                .push_str(b[j]);
                j += 1;
            }
        }
    }
    edits.extend(open);
    Some(edits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(old: &str, edits: &[LineEdit]) -> String {
        let mut text = String::new();
        let mut at = 0;
        for edit in edits {
            text.push_str(&old[at..edit.start]);
            text.push_str(&edit.text);
            at = edit.end;
        }
        text + &old[at..]
    }

    fn lcs(a: &[&str], b: &[&str]) -> usize {
        let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                table[i][j] = if a[i] == b[j] {
                    table[i + 1][j + 1] + 1
                } else {
                    table[i + 1][j].max(table[i][j + 1])
                };
            }
        }
        table[0][0]
    }

    #[test]
    fn matches_the_lcs_length_and_round_trips() {
        let mut state: u64 = 7;
        let mut next = |bound: u64| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) % bound
        };
        for _ in 0..3000 {
            let alphabet = next(5) + 1;
            let text = |keep_newline: bool, next: &mut dyn FnMut(u64) -> u64| {
                let lines = next(12);
                let mut text = String::new();
                for _ in 0..lines {
                    text.push_str(&format!("l{}\n", next(alphabet)));
                }
                if !keep_newline && text.ends_with('\n') {
                    text.pop();
                }
                text
            };
            let drop_old = next(3) == 0;
            let old = text(!drop_old, &mut next);
            let drop_new = next(4) == 0;
            let new = text(!drop_new, &mut next);
            let (a, b) = (split_lines(&old), split_lines(&new));
            let steps = shortest_script(&a, &b).unwrap();
            let equal = steps.iter().filter(|step| **step == Step::Equal).count();
            assert_eq!(equal, lcs(&a, &b), "{old:?} -> {new:?}");
            assert_eq!(apply(&old, &line_edits(&old, &new).unwrap()), new);
        }
    }

    #[test]
    fn unified_output_matches_diff_dash_u() {
        let head = "--- a/M.tz\n+++ b/M.tz\n";
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        assert_eq!(
            unified("M.tz", old, &old.replace('e', "E"), 3).unwrap(),
            format!("{head}@@ -2,7 +2,7 @@\n b\n c\n d\n-e\n+E\n f\n g\n h\n")
        );
        assert_eq!(
            unified("M.tz", old, &old.replace('a', "A").replace('j', "J"), 3).unwrap(),
            format!(
                "{head}@@ -1,4 +1,4 @@\n-a\n+A\n b\n c\n d\n@@ -7,4 +7,4 @@\n g\n h\n i\n-j\n+J\n"
            )
        );
        assert_eq!(
            unified("M.tz", "x\ny", "x\nz", 3).unwrap(),
            format!(
                "{head}@@ -1,2 +1,2 @@\n x\n-y\n\\ No newline at end of file\n+z\n\\ No newline at end of file\n"
            )
        );
        assert_eq!(
            unified("M.tz", "", "a\n", 3).unwrap(),
            format!("{head}@@ -0,0 +1 @@\n+a\n")
        );
        assert_eq!(
            unified("M.tz", "a\n", "", 3).unwrap(),
            format!("{head}@@ -1 +0,0 @@\n-a\n")
        );
        assert_eq!(unified("M.tz", old, old, 3).unwrap(), "");
    }

    #[test]
    fn the_change_limit_returns_none() {
        let text = |prefix: &str, count: usize| -> String {
            (0..count)
                .map(|index| format!("{prefix}{index}\n"))
                .collect()
        };
        let (old, new) = (text("a", 1500), text("b", 1500));
        assert_eq!(
            shortest_script(&split_lines(&old), &split_lines(&new)),
            None
        );
        assert_eq!(unified("M.tz", &old, &new, 3), None);
        let (old, new) = (text("a", 900), text("b", 900));
        assert!(shortest_script(&split_lines(&old), &split_lines(&new)).is_some());
    }

    #[test]
    fn line_edits_cover_each_changed_run() {
        let edit = |start, end, text: &str| LineEdit {
            start,
            end,
            text: text.to_string(),
        };
        assert_eq!(
            line_edits("a\nb\nc\n", "a\nX\nY\nc\n").unwrap(),
            [edit(2, 4, "X\nY\n")]
        );
        assert_eq!(line_edits("a\n", "a\nb\n").unwrap(), [edit(2, 2, "b\n")]);
        assert_eq!(line_edits("a\nb\n", "").unwrap(), [edit(0, 4, "")]);
    }
}
