// 시험 — `openguild plugin test examples/plugins/telegram-quest-status`
//
// `fire` 는 실제와 같은 길로 돈다(줄 고르기·조건·`with`). 다른 것은 나가는 것이 없다는 것뿐 —
// 명령은 적히기만 한다. 설정값은 기본값이 쓰이므로, 켜고 볼 것은 `set_config` 로 켠다
// (시험 하나가 끝나면 지워진다).

fn test_new_quest_goes_out() {
    set_config("ON_CREATED", true);
    let r = fire("quest.created", #{
        ok: true,
        quest: #{ id: "DEV-1", title: "훅", tags: ["notify"] }
    });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "telegram");
    assert(r.commands[0].body.text.contains("DEV-1"), "본문에 퀘스트가 없다");
}

// 기본값이 "태그 붙은 것만" 이므로, 태그가 없으면 아무것도 안 나간다.
fn test_untagged_quest_is_quiet() {
    set_config("ON_CREATED", true);
    let r = fire("quest.created", #{ ok: true, quest: #{ id: "DEV-2", title: "조용", tags: [] } });
    assert_eq(r.commands.len(), 0);
}

// 실패한 변경은 안 알린다.
fn test_a_failed_change_is_quiet() {
    set_config("ON_STATUS", true);
    let r = fire("quest.status_changed", #{
        ok: false,
        quest: #{ id: "DEV-3", title: "실패", tags: ["notify"] },
        change: #{ from: "open", to: "done" }
    });
    assert_eq(r.commands.len(), 0);
}

// 댓글 줄은 `with = ["subject"]` 를 받는다 — 시험이 그 값을 직접 넘긴다.
fn test_comment_uses_the_document_tags() {
    set_config("ON_COMMENT", true);
    let e = #{
        ok: true,
        subject: #{ kind: "quest", id: "DEV-4" },
        comment: #{ body: "봐 주세요" }
    };
    let quiet = fire("comment.added", e, #{ subject: #{ id: "DEV-4", tags: [] } });
    assert_eq(quiet.commands.len(), 0);

    let loud = fire("comment.added", e, #{ subject: #{ id: "DEV-4", tags: ["notify"] } });
    assert_eq(loud.commands.len(), 1);
    assert(loud.commands[0].body.text.contains("봐 주세요"), "댓글 본문이 안 실렸다");
}
