// 시험 — `openguild plugin test examples/plugins/deleted-audit`
//
// `pre:` 를 붙이면 바뀌기 전 단계로 흘린다. 막혔는지는 `blocked` 에 온다(막을 이유 그대로).

fn test_a_plain_quest_is_recorded_and_passes() {
    let r = fire("pre:quest.deleted", #{ quest: #{ id: "DEV-1", title: "그냥", status: "open", tags: [] } });
    assert_eq(r.blocked, ());
    assert_eq(r.commands.len(), 1);          // 기록 줄만 돈다
    assert_eq(r.commands[0].action, "log");
}

fn test_a_kept_quest_is_blocked() {
    let r = fire("pre:quest.deleted", #{ quest: #{ id: "DEV-2", title: "지키자", status: "open", tags: ["keep"] } });
    assert(r.blocked != (), "안 막혔다");
    assert(r.blocked.contains("keep"), "이유에 태그 이야기가 없다");
    // 막히기 전에 기록은 남는다 — 무엇을 지우려 했는지 알아야 한다.
    assert_eq(r.commands.len(), 1);
}
