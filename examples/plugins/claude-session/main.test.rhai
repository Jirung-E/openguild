// 시험 — `openguild plugin test examples/plugins/claude-session`

fn comment(body) {
    #{ target: #{ kind: "quest", id: "DEV-7" }, comment: #{ author: "admin", body: body, parent_id: () } }
}

fn test_the_command_opens_the_session_instead_of_the_comment() {
    let r = fire("pre:comment.added", comment("  /session \n"));
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "open");
    assert_eq(r.commands[0].body.name, "DEV-7");
    assert_eq(r.blocked, ());
    assert(r.instead != (), "대신 처리하지 않았다");
    assert(r.instead.contains("DEV-7"), "안내에 번호가 없다");
}

fn test_an_ordinary_comment_passes_untouched() {
    let r = fire("pre:comment.added", comment("/session 은 이렇게 씁니다"));
    assert_eq(r.commands.len(), 0);
    assert_eq(r.blocked, ());
    assert_eq(r.instead, ());
    assert_eq(r.changes.len(), 0);
}

fn test_record_mode_keeps_one_line() {
    set_config("REPLY", "record");
    let r = fire("pre:comment.added", comment("/session"));
    assert_eq(r.commands.len(), 1);
    assert_eq(r.blocked, ());
    assert_eq(r.instead, ());
    assert(r.changes.body.contains("DEV-7"), "기록 줄에 번호가 없다");
}

fn test_the_command_word_is_configurable() {
    set_config("COMMAND", "/세션");
    assert_eq(fire("pre:comment.added", comment("/session")).commands.len(), 0);
    assert_eq(fire("pre:comment.added", comment("/세션")).commands.len(), 1);
}
