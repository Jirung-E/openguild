//! DEV-435: 같은 본문 동시 편집 — 줄 단위 3방향 병합(위키백과 방식, REQ-024).
//!
//! 세 판을 받는다: **시작할 때 본 것**(base), **지금 저장된 것**(current — 그사이 남이 고친 것), **내 것**(mine).
//! base → current 와 base → mine 의 바뀐 곳을 줄 단위로 구해 나란히 놓는다.
//!
//! - 한쪽만 고친 곳은 그쪽을 쓴다. 두 쪽이 똑같이 고쳤으면 그대로 쓴다.
//! - 두 쪽이 **같은 줄**을 다르게 고쳤을 때만 충돌이다. 이웃한 줄을 따로 고친 것은 충돌이 아니다("같은 줄을 고칠
//!   때만" — git 은 이웃한 줄도 충돌로 보지만 그건 사람을 귀찮게 한다).
//! - 같은 자리에 두 쪽이 서로 다른 줄을 끼워 넣었으면 순서를 정할 수 없으니 충돌이다.
//!
//! **충돌 표시(`<<<<<<<`)를 글자에 박지 않는다** — 충돌은 구간 목록([`Segment`])으로 돌려준다. 박으면 문서가 깨지고
//! 그걸 에이전트 · 플러그인 · 다른 사람이 그대로 읽는다.

use serde::Serialize;
use similar::{Algorithm, DiffOp, capture_diff_slices};

/// 병합 결과의 한 구간.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Segment {
    /// 합의된 글 — 안 바뀌었거나, 한쪽만 고쳤거나, 둘이 똑같이 고친 곳.
    Same { text: String },
    /// 같은 줄을 두 쪽이 다르게 고친 곳. `base` 는 시작할 때의 그 구간(끼워 넣기면 빈 글).
    Conflict { base: String, current: String, mine: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    /// 겹치는 줄이 없어 합쳤다.
    Clean(String),
    /// 같은 줄 충돌이 있다 — 구간을 차례대로 이으면 전체 글이 된다(충돌 구간은 고른 쪽으로).
    Conflict(Vec<Segment>),
}

/// 한쪽의 바뀐 곳 — base 의 `[start, end)` 줄을 `lines` 로 바꾼다(끼워 넣기면 start == end).
#[derive(Debug, Clone)]
struct Change {
    start: usize,
    end: usize,
    lines: Vec<String>,
}

fn split_lines(s: &str) -> Vec<String> {
    s.split_inclusive('\n').map(str::to_string).collect()
}

/// 끝에 줄바꿈이 없는 마지막 줄이 "고친 줄" 로 잡히지 않게 — 비교는 줄바꿈을 붙여서 하고, 결과는 `mine` 의
/// 끝 모양을 따른다.
fn with_newline(s: &str) -> String {
    if s.is_empty() || s.ends_with('\n') { s.to_string() } else { format!("{s}\n") }
}

fn changes(base: &[String], other: &[String]) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, base, other) {
        let (start, end, ns, ne) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete { old_index, old_len, new_index } => (old_index, old_index + old_len, new_index, new_index),
            DiffOp::Insert { old_index, new_index, new_len } => (old_index, old_index, new_index, new_index + new_len),
            DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                (old_index, old_index + old_len, new_index, new_index + new_len)
            }
        };
        let lines = other[ns..ne].to_vec();
        // 지우기 바로 뒤의 끼워 넣기처럼 붙어 있는 것은 한 번의 바꾸기로 본다.
        if let Some(last) = out.last_mut()
            && last.end == start
        {
            last.end = end;
            last.lines.extend(lines);
            continue;
        }
        out.push(Change { start, end, lines });
    }
    out
}

/// 바뀐 곳 `c` 가 구간 `[s, e)` 와 같은 줄을 건드리나. 끼워 넣기는 구간 **안쪽**에 들어갈 때만, 같은 자리 끼워
/// 넣기끼리는 겹친다.
fn overlaps(c: &Change, s: usize, e: usize) -> bool {
    match (c.start == c.end, s == e) {
        (true, true) => c.start == s,
        (true, false) => s < c.start && c.start < e,
        (false, true) => c.start < s && s < c.end,
        (false, false) => c.start < e && s < c.end,
    }
}

/// base 의 `[s, e)` 에 한쪽의 바뀐 곳들을 입힌 글.
fn apply(base: &[String], s: usize, e: usize, cs: &[&Change]) -> String {
    let mut out = String::new();
    let mut pos = s;
    for c in cs {
        out.extend(base[pos..c.start].iter().map(String::as_str));
        out.extend(c.lines.iter().map(String::as_str));
        pos = c.end;
    }
    out.extend(base[pos..e].iter().map(String::as_str));
    out
}

/// 세 판을 합친다. 자세한 규칙은 모듈 머리말.
pub fn merge3(base: &str, current: &str, mine: &str) -> Merge {
    let base_l = split_lines(&with_newline(base));
    let cur_l = split_lines(&with_newline(current));
    let mine_l = split_lines(&with_newline(mine));
    let a = changes(&base_l, &cur_l);
    let b = changes(&base_l, &mine_l);

    let mut segs: Vec<Segment> = Vec::new();
    let push_same = |segs: &mut Vec<Segment>, text: String| {
        if text.is_empty() {
            return;
        }
        if let Some(Segment::Same { text: t }) = segs.last_mut() {
            t.push_str(&text);
        } else {
            segs.push(Segment::Same { text });
        }
    };

    let (mut ia, mut ib, mut pos) = (0usize, 0usize, 0usize);
    loop {
        // 다음에 시작하는 것 — 같은 자리면 끼워 넣기가 먼저(그 줄을 바꾸기보다 앞에 놓인다).
        let key = |c: &Change| (c.start, c.end != c.start);
        let first_a = match (a.get(ia), b.get(ib)) {
            (None, None) => break,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (Some(x), Some(y)) => key(x) <= key(y),
        };
        let (mut s, mut e) = if first_a { (a[ia].start, a[ia].end) } else { (b[ib].start, b[ib].end) };
        let (ja, jb) = (ia, ib);
        if first_a { ia += 1 } else { ib += 1 }
        // 구간에 겹치는 것을 양쪽에서 계속 끌어들인다.
        loop {
            let grew_a = a.get(ia).is_some_and(|c| overlaps(c, s, e));
            if grew_a {
                s = s.min(a[ia].start);
                e = e.max(a[ia].end);
                ia += 1;
            }
            let grew_b = b.get(ib).is_some_and(|c| overlaps(c, s, e));
            if grew_b {
                s = s.min(b[ib].start);
                e = e.max(b[ib].end);
                ib += 1;
            }
            if !grew_a && !grew_b {
                break;
            }
        }
        push_same(&mut segs, base_l[pos..s].concat());
        let ca: Vec<&Change> = a[ja..ia].iter().collect();
        let cb: Vec<&Change> = b[jb..ib].iter().collect();
        let ta = apply(&base_l, s, e, &ca);
        let tb = apply(&base_l, s, e, &cb);
        if ca.is_empty() {
            push_same(&mut segs, tb);
        } else if cb.is_empty() || ta == tb {
            push_same(&mut segs, ta);
        } else {
            segs.push(Segment::Conflict { base: base_l[s..e].concat(), current: ta, mine: tb });
        }
        pos = e;
    }
    push_same(&mut segs, base_l[pos..].concat());

    // 끝 모양은 내 것을 따른다(내 것이 비었으면 지금 것을).
    let shape = if mine.is_empty() { current } else { mine };
    let trim_end = !shape.is_empty() && !shape.ends_with('\n');
    let fix = |t: &mut String| {
        if trim_end && t.ends_with('\n') {
            t.pop();
        }
    };
    if segs.iter().any(|s| matches!(s, Segment::Conflict { .. })) {
        if let Some(last) = segs.last_mut() {
            match last {
                Segment::Same { text } => fix(text),
                Segment::Conflict { current, mine, base } => {
                    fix(current);
                    fix(mine);
                    fix(base);
                }
            }
        }
        return Merge::Conflict(segs);
    }
    let mut out: String = segs
        .into_iter()
        .map(|s| match s {
            Segment::Same { text } => text,
            Segment::Conflict { .. } => unreachable!(),
        })
        .collect();
    fix(&mut out);
    Merge::Clean(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "하나\n둘\n셋\n넷\n다섯\n여섯\n일곱\n여덟\n아홉\n열\n";

    fn clean(m: Merge) -> String {
        match m {
            Merge::Clean(s) => s,
            Merge::Conflict(segs) => panic!("충돌이 아니어야 한다: {segs:?}"),
        }
    }

    #[test]
    fn edits_on_different_lines_both_stay() {
        let cur = BASE.replace("하나", "하나 (남이 고침)");
        let mine = BASE.replace("열", "열 (내가 고침)");
        let got = clean(merge3(BASE, &cur, &mine));
        assert!(got.contains("하나 (남이 고침)") && got.contains("열 (내가 고침)"), "{got}");
    }

    #[test]
    fn neighbouring_lines_are_not_a_conflict() {
        let cur = BASE.replace("셋", "셋!");
        let mine = BASE.replace("넷", "넷!");
        let got = clean(merge3(BASE, &cur, &mine));
        assert!(got.contains("셋!\n넷!\n"), "{got}");
    }

    #[test]
    fn the_same_edit_on_both_sides_is_not_a_conflict() {
        let both = BASE.replace("다섯", "5");
        assert_eq!(clean(merge3(BASE, &both, &both)), both);
    }

    #[test]
    fn different_edits_on_the_same_line_conflict_without_markers() {
        let cur = BASE.replace("다섯", "다섯 — 남");
        let mine = BASE.replace("다섯", "다섯 — 나");
        let Merge::Conflict(segs) = merge3(BASE, &cur, &mine) else { panic!("충돌이어야 한다") };
        let conflicts: Vec<&Segment> = segs.iter().filter(|s| matches!(s, Segment::Conflict { .. })).collect();
        assert_eq!(
            conflicts,
            vec![&Segment::Conflict {
                base: "다섯\n".into(),
                current: "다섯 — 남\n".into(),
                mine: "다섯 — 나\n".into()
            }]
        );
        // 구간을 이으면(충돌은 내 것으로) 내 글이 된다 — 아무것도 잃지 않는다.
        let rebuilt: String = segs
            .iter()
            .map(|s| match s {
                Segment::Same { text } => text.as_str(),
                Segment::Conflict { mine, .. } => mine.as_str(),
            })
            .collect();
        assert_eq!(rebuilt, mine);
        assert!(segs.iter().all(|s| !format!("{s:?}").contains("<<<<<<<")));
    }

    #[test]
    fn inserts_at_the_same_place_with_different_lines_conflict() {
        let cur = BASE.replace("둘\n", "둘\n남이 넣음\n");
        let mine = BASE.replace("둘\n", "둘\n내가 넣음\n");
        assert!(matches!(merge3(BASE, &cur, &mine), Merge::Conflict(_)));
    }

    #[test]
    fn a_delete_and_an_edit_elsewhere_merge() {
        let cur = BASE.replace("셋\n", "");
        let mine = BASE.replace("아홉", "9");
        let got = clean(merge3(BASE, &cur, &mine));
        assert!(!got.contains("셋") && got.contains("9\n"), "{got}");
    }

    #[test]
    fn deleting_a_line_the_other_side_edited_conflicts() {
        let cur = BASE.replace("셋\n", "");
        let mine = BASE.replace("셋", "셋 고침");
        assert!(matches!(merge3(BASE, &cur, &mine), Merge::Conflict(_)));
    }

    #[test]
    fn appends_by_both_sides_conflict_but_appending_to_an_edit_elsewhere_does_not() {
        let cur = format!("{BASE}남의 끝 줄\n");
        let mine = format!("{BASE}내 끝 줄\n");
        assert!(matches!(merge3(BASE, &cur, &mine), Merge::Conflict(_)));
        let mine = BASE.replace("하나", "1");
        let got = clean(merge3(BASE, &cur, &mine));
        assert!(got.starts_with("1\n") && got.ends_with("남의 끝 줄\n"), "{got}");
    }

    #[test]
    fn a_missing_final_newline_is_not_an_edit() {
        let base = "가\n나\n다";
        let cur = "가 고침\n나\n다";
        let mine = "가\n나\n다 고침";
        assert_eq!(clean(merge3(base, cur, mine)), "가 고침\n나\n다 고침");
    }

    #[test]
    fn empty_base_with_one_side_writing_takes_it() {
        assert_eq!(clean(merge3("", "", "새 글\n")), "새 글\n");
        assert_eq!(clean(merge3("", "남의 글\n", "")), "남의 글\n");
        assert!(matches!(merge3("", "남의 글\n", "내 글\n"), Merge::Conflict(_)));
    }
}
