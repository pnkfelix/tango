use super::{footnoted_md2rs, md2rs, rs2md};

mod test_snippets;
mod footnote_snippets;

struct DifferingLines<'a> {
    left_line_num: usize,
    left: &'a str,
    right_line_num: usize,
    right: &'a str,
}

enum ComparisonResult<'a> {
    Ok,
    LineDifferences(Vec<DifferingLines<'a>>),
    LineCountMismatch(usize, usize, Vec<String>),
}

// #[cfg(test)]
fn compare_lines<'a>(a: &'a str, b: &'a str) -> ComparisonResult<'a> {
    let a: Vec<_> = a.lines().collect();
    let b: Vec<_> = b.lines().collect();
    let mut i = 0;
    let mut j = 0;

    let mut differing_lines: Vec<DifferingLines> = Vec::new();

    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }

        differing_lines.push(DifferingLines {
            left_line_num: i,
            right_line_num: j,
            left: a[i],
            right: b[j],
        });

        for j_ in (j+1)..b.len() {
            if a[i] == b[j_] {
                j = j_;
                continue;
            }
        }

        for i_ in (i+1)..a.len() {
            if a[i_] == b[j] {
                i = i_;
                continue;
            }
        }

        i += 1;
        j += 1;
    }

    if differing_lines.len() != 0 {
        ComparisonResult::LineDifferences(differing_lines)
    } else if i == a.len() && j == b.len() && i == j {
        ComparisonResult::Ok
    } else {
        let mut v = Vec::new();
        if a.len() > b.len() {
            for i in b.len()..a.len() {
                v.push(a[i].to_string());
            }
        } else {
            for j in a.len()..b.len() {
                v.push(b[j].to_string());
            }
        }
        ComparisonResult::LineCountMismatch(a.len(), b.len(), v)
    }
}

fn panic_if_different<'a>(name_a: &str, a: &'a str, name_b: &str, b: &'a str) {
    match compare_lines(a, b) {
        ComparisonResult::LineDifferences(differences) => {
            for difference in differences {
                println!("lines {lnum} and {rnum} differ:\n{nl:>8}: {l}\n{nr:>8}: {r}",
                         lnum=difference.left_line_num+1,
                         rnum=difference.right_line_num+1,
                         nl=name_a,
                         l=difference.left,
                         nr=name_b,
                         r=difference.right);
            }
            panic!("saw differences");
        }
        ComparisonResult::LineCountMismatch(a, b, v) => {
            for line in v {
                println!("excess line: {}", line);
            }
            panic!("Content differs:\n{nl:>8}: {l} lines\n{nr:>8}: {r} lines",
                     nl=name_a,
                     l=a,
                     nr=name_b,
                     r=b);
        }
        ComparisonResult::Ok => {}
    }
}

#[cfg(test)]
fn core_test_md2rs(md: &str, rs: &str) {
    let mut output = Vec::new();
    md2rs(md.as_bytes(), &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    panic_if_different("actual", &output, "expect", rs);
}

#[cfg(test)]
fn core_test_rs2md(rs: &str, md: &str) {
    let mut output = Vec::new();
    rs2md(rs.as_bytes(), &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    panic_if_different("actual", &output, "expect", md);
}

#[cfg(test)]
fn footnote_test_md2rs(md: &str, rs: &str) {
    let mut output = Vec::new();
    footnoted_md2rs(md.as_bytes(), &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    panic_if_different("actual", &output, "expect", rs);
}

#[cfg(test)]
fn footnote_test_rs2md(rs: &str, md: &str) {
    core_test_rs2md(rs, md)
}
