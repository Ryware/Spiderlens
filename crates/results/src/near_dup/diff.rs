//! Side-by-side comparison of two pages' main content (plan D8, research
//! R6): paragraphs aligned by a longest common subsequence over paragraph
//! texts, then words diffed inside each aligned pair.

use serde::Serialize;
use spiderlens_engine::extract::{MainSource, MainText};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DiffOp {
    Equal,
    /// Only on this side.
    Changed,
    /// Not part of the compared main content (full-text mode).
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DiffSpan {
    pub text: String,
    pub op: DiffOp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DiffParagraph {
    pub spans: Vec<DiffSpan>,
    /// Index of the aligned paragraph on the other side, when there is one.
    pub pair: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct CompareSide {
    #[ts(type = "number")]
    pub url_id: i64,
    pub address: String,
    pub source: String,
    pub paragraphs: Vec<DiffParagraph>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Comparison {
    /// Similarity from the analysis, when the pair was recorded.
    pub sim: Option<u8>,
    pub exact: bool,
    /// Set when the text cannot be shown, for example `html_not_stored`.
    pub unavailable: Option<String>,
    pub a: Option<CompareSide>,
    pub b: Option<CompareSide>,
}

/// LCS table over two sequences; returns matched index pairs in order.
fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    // Long inputs: fall back to matching equal prefixes and suffixes only, so
    // a huge page never costs quadratic memory.
    if n.saturating_mul(m) > 4_000_000 {
        let mut out = Vec::new();
        let mut i = 0;
        while i < n.min(m) && a[i] == b[i] {
            out.push((i, i));
            i += 1;
        }
        let mut tail = Vec::new();
        let (mut x, mut y) = (n, m);
        while x > i && y > i && a[x - 1] == b[y - 1] {
            tail.push((x - 1, y - 1));
            x -= 1;
            y -= 1;
        }
        out.extend(tail.into_iter().rev());
        return out;
    }
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if a[i] == b[j] {
                t[at(i + 1, j + 1)] + 1
            } else {
                t[at(i + 1, j)].max(t[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[at(i + 1, j)] >= t[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

fn words(s: &str) -> Vec<&str> {
    s.split(' ').filter(|w| !w.is_empty()).collect()
}

/// Spans for one side's words given which of them matched.
fn spans_for(words: &[&str], matched: &[bool]) -> Vec<DiffSpan> {
    let mut out: Vec<DiffSpan> = Vec::new();
    for (w, &m) in words.iter().zip(matched) {
        let op = if m { DiffOp::Equal } else { DiffOp::Changed };
        match out.last_mut() {
            Some(last) if last.op == op => {
                last.text.push(' ');
                last.text.push_str(w);
            }
            _ => out.push(DiffSpan {
                text: (*w).to_string(),
                op,
            }),
        }
    }
    out
}

/// Word diff of two paragraphs.
fn diff_words(a: &str, b: &str) -> (Vec<DiffSpan>, Vec<DiffSpan>) {
    let (wa, wb) = (words(a), words(b));
    let mut ma = vec![false; wa.len()];
    let mut mb = vec![false; wb.len()];
    for (i, j) in lcs(&wa, &wb) {
        ma[i] = true;
        mb[j] = true;
    }
    (spans_for(&wa, &ma), spans_for(&wb, &mb))
}

/// Paragraphs of main content, aligned: equal paragraphs pair by LCS;
/// unmatched paragraphs between two matches pair up in order and are
/// diffed word by word.
pub fn diff_main(a: &[String], b: &[String]) -> (Vec<DiffParagraph>, Vec<DiffParagraph>) {
    let anchors = lcs(a, b);
    let mut pa: Vec<DiffParagraph> = Vec::with_capacity(a.len());
    let mut pb: Vec<DiffParagraph> = Vec::with_capacity(b.len());
    let mut pairs: Vec<(Option<usize>, Option<usize>)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    for &(x, y) in anchors.iter().chain(std::iter::once(&(a.len(), b.len()))) {
        // Gap before this anchor: pair in order, rest alone.
        let gap_a: Vec<usize> = (i..x).collect();
        let gap_b: Vec<usize> = (j..y).collect();
        for k in 0..gap_a.len().max(gap_b.len()) {
            pairs.push((gap_a.get(k).copied(), gap_b.get(k).copied()));
        }
        if x < a.len() && y < b.len() {
            pairs.push((Some(x), Some(y)));
        }
        i = x + 1;
        j = y + 1;
    }
    let (mut ia, mut ib) = (0u32, 0u32);
    for (x, y) in pairs {
        match (x, y) {
            (Some(x), Some(y)) => {
                let (sa, sb) = if a[x] == b[y] {
                    let eq = |t: &str| {
                        vec![DiffSpan {
                            text: t.to_string(),
                            op: DiffOp::Equal,
                        }]
                    };
                    (eq(&a[x]), eq(&b[y]))
                } else {
                    diff_words(&a[x], &b[y])
                };
                pa.push(DiffParagraph {
                    spans: sa,
                    pair: Some(ib),
                });
                pb.push(DiffParagraph {
                    spans: sb,
                    pair: Some(ia),
                });
                ia += 1;
                ib += 1;
            }
            (Some(x), None) => {
                pa.push(DiffParagraph {
                    spans: vec![DiffSpan {
                        text: a[x].clone(),
                        op: DiffOp::Changed,
                    }],
                    pair: None,
                });
                ia += 1;
            }
            (None, Some(y)) => {
                pb.push(DiffParagraph {
                    spans: vec![DiffSpan {
                        text: b[y].clone(),
                        op: DiffOp::Changed,
                    }],
                    pair: None,
                });
                ib += 1;
            }
            (None, None) => {}
        }
    }
    (pa, pb)
}

/// Full-text mode: the whole body by paragraph, with left-out parts marked
/// excluded and main content shown as equal (differences are in the main
/// view).
pub fn full_text(t: &MainText) -> Vec<DiffParagraph> {
    t.paragraphs
        .iter()
        .map(|p| DiffParagraph {
            spans: p
                .runs
                .iter()
                .map(|r| DiffSpan {
                    text: r.text.clone(),
                    op: if r.included {
                        DiffOp::Equal
                    } else {
                        DiffOp::Excluded
                    },
                })
                .collect(),
            pair: None,
        })
        .collect()
}

pub(crate) fn source_label(s: MainSource) -> String {
    s.as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn changed(p: &DiffParagraph) -> Vec<&str> {
        p.spans
            .iter()
            .filter(|s| s.op == DiffOp::Changed)
            .map(|s| s.text.as_str())
            .collect()
    }

    #[test]
    fn marks_exactly_the_changed_words() {
        // US2.3: only "Red" versus "Blue" differs.
        let a = s(&["Shoes in Red for running", "Free delivery", "Size guide"]);
        let b = s(&["Shoes in Blue for running", "Free delivery", "Size guide"]);
        let (pa, pb) = diff_main(&a, &b);
        assert_eq!(pa.len(), 3);
        assert_eq!(changed(&pa[0]), ["Red"]);
        assert_eq!(changed(&pb[0]), ["Blue"]);
        assert!(changed(&pa[1]).is_empty() && changed(&pb[2]).is_empty());
        assert_eq!(pa[1].pair, Some(1));
    }

    #[test]
    fn added_and_removed_paragraphs_stand_alone() {
        let a = s(&["One", "Two", "Three"]);
        let b = s(&["One", "Three", "Four"]);
        let (pa, pb) = diff_main(&a, &b);
        assert_eq!(changed(&pa[1]), ["Two"]);
        assert_eq!(pa[1].pair, None);
        assert_eq!(changed(&pb[2]), ["Four"]);
        assert_eq!(pa[2].pair, Some(1));
    }

    #[test]
    fn huge_inputs_do_not_go_quadratic() {
        let a: Vec<String> = (0..3000).map(|i| format!("p{i}")).collect();
        let mut b = a.clone();
        b[1500] = "changed".into();
        let (pa, _) = diff_main(&a, &b);
        assert_eq!(pa.len(), 3000);
        assert_eq!(changed(&pa[1500]), ["p1500"]);
    }
}
