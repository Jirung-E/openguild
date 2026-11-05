// 시험 — `openguild plugin test examples/plugins/desktop-notify`
//
// 스크립트가 없는 예제다. 어느 이벤트에 알림 프로그램을 부르는지를 본다. 실제 알림은 `notify.sh` · `notify.ps1` 이 띄운다.

fn test_a_new_quest_notifies() {
    let r = fire("quest.created", #{ quest: #{ id: "DEV-1", title: "새 일", status: "open" } });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "notify");
}

fn test_a_comment_notifies() {
    let r = fire("comment.added", #{ comment: #{ id: 1, body: "글" }, subject: #{ id: "DEV-1" } });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "notify");
}

fn test_other_changes_stay_quiet() {
    let r = fire("quest.updated", #{ quest: #{ id: "DEV-1", title: "고침", status: "open" } });
    assert_eq(r.commands.len(), 0);
}
