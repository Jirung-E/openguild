// 기한이 지난 목록 → 알림 한 줄. `e.items` 의 한 건은 이벤트의 `e.quest` 와 같은 모양이다(id · title · status · …).

fn digest(e) {
    let max = config("MAX");
    if max == () || max < 1 { max = 5; }
    let list = "";
    let shown = 0;
    for q in e.items {
        if shown >= max { break; }
        if shown > 0 { list += " · "; }
        list += q.id + " " + truncate(q.title, 20);
        shown += 1;
    }
    let more = e.count - shown;
    let tail = if more > 0 { ` 외 ${more}건` } else { "" };
    notify(`기한 지남 ${e.count}건 — ${list}${tail}`);
}
