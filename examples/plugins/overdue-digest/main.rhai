// 기한이 지난 목록 → 알림 한 줄. `e.items` 는 맞는 퀘스트의 짧은 요약(id · title · status · …)이다.

fn digest(e) {
    let max = config("MAX");
    if max == () || max < 1 { max = 5; }
    let names = [];
    for q in e.items {
        if names.len() >= max { break; }
        names.push(q.id + " " + truncate(q.title, 20));
    }
    let more = e.count - names.len();
    let tail = if more > 0 { ` 외 ${more}건` } else { "" };
    notify(`기한 지남 ${e.count}건 — ${names.reduce(|a, b| a + " · " + b)}${tail}`);
}
