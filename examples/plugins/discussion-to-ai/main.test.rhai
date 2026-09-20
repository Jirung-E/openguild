// 시험 — `openguild plugin test examples/plugins/discussion-to-ai`
//
// 이 예제의 존재 이유는 **거르는 것**이다. 그러니 "나가는 경우" 만큼 "안 나가는 경우" 를
// 본다 — 한 번도 거르지 않는 필터는 필터가 아니다.
//
// 나가는 곳은 이 컴퓨터다(BUG-311). 어디로 건네줄지는 `deliver.sh` 가 설정을 보고 정하므로
// 여기서 볼 것은 "건넸는가 / 무엇을 건넸는가" 까지다.

fn test_an_unresolved_discussion_goes_out() {
    let r = fire("comment.added", #{
        ok: true,
        target: #{ kind: "quest", id: "DEV-1" },
        comment: #{ author: "admin", body: "이 방식 맞나요?", discussion: true, resolved: false }
    });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "ai");
    assert_eq(r.commands[0].body.target, "quest:DEV-1");
    assert_eq(r.commands[0].body.author, "admin");
    assert(r.commands[0].body.question.contains("맞나요"), "본문이 안 실렸다");
}

// 기본값은 제일 좁다 — 평범한 댓글은 조용하다.
fn test_a_plain_comment_is_quiet() {
    let r = fire("comment.added", #{
        ok: true,
        target: #{ kind: "quest", id: "DEV-2" },
        comment: #{ author: "admin", body: "ㅇㅇ", discussion: false, resolved: false }
    });
    assert_eq(r.commands.len(), 0);
}

// 이미 해결한 토론도 조용하다 — 답이 필요 없다.
fn test_a_resolved_discussion_is_quiet() {
    let r = fire("comment.added", #{
        ok: true,
        target: #{ kind: "quest", id: "DEV-3" },
        comment: #{ author: "admin", body: "끝", discussion: true, resolved: true }
    });
    assert_eq(r.commands.len(), 0);
}

// "모든 댓글" 로 넓히면 평범한 댓글도 나간다.
fn test_widening_the_scope_lets_a_plain_comment_through() {
    set_config("WHAT", "all");
    let r = fire("comment.added", #{
        ok: true,
        target: #{ kind: "campaign", id: "C-001" },
        comment: #{ author: "admin", body: "ㅇㅇ", discussion: false, resolved: false }
    });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].body.target, "campaign:C-001");
}

// 실패한 변경은 안 건넨다.
fn test_a_failed_change_is_quiet() {
    set_config("WHAT", "all");
    let r = fire("comment.added", #{
        ok: false,
        target: #{ kind: "quest", id: "DEV-4" },
        comment: #{ author: "admin", body: "실패", discussion: true, resolved: false }
    });
    assert_eq(r.commands.len(), 0);
}

// 한 번 해결한 토론이 다시 열리는 것도 답이 필요한 상태다.
fn test_reopening_a_discussion_goes_out() {
    let r = fire("comment.unresolved", #{
        ok: true,
        target: #{ kind: "quest", id: "DEV-5" },
        comment: #{ author: "admin", body: "다시 봅시다", discussion: true, resolved: false }
    });
    assert_eq(r.commands.len(), 1);
}
