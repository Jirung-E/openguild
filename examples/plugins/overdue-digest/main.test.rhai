// 시험 — `openguild plugin test examples/plugins/overdue-digest`

fn late(id) { #{ id: id, title: "제목 " + id, status: "open" } }

fn test_one_notice_for_the_list() {
    let r = tick("기한 지남", [late("DEV-1"), late("DEV-2")]);
    assert_eq(r.lines.len(), 1);
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].kind, "notify");
    assert_eq(r.commands[0].body, "기한 지남 2건 — DEV-1 제목 DEV-1 · DEV-2 제목 DEV-2");
}

fn test_long_lists_are_cut() {
    set_config("MAX", 1);
    let r = tick("기한 지남", [late("DEV-1"), late("DEV-2"), late("DEV-3")]);
    assert_eq(r.commands[0].body, "기한 지남 3건 — DEV-1 제목 DEV-1 외 2건");
}

fn test_nothing_late_calls_nothing() {
    assert_eq(tick("기한 지남", []).lines.len(), 0);
}
