// **무엇을 보낼지 설정에서 고른다** — 선택상자(`select`)의 예다.
//
// 평범한 댓글까지 다 보내면 소음이 되고, AI 를 부르는 훅이면 돈까지 든다.
// 그래서 기본값은 제일 좁은 "미해결 토론만" 이다.
//
// BUG-311: 건네주는 곳은 **이 컴퓨터**다. 어디로 어떻게 건넬지는 `deliver.sh` 가 설정을
// 보고 정한다 — 여기서는 무엇을 건넬지만 고른다.

fn forward(e) {
    if !e.ok || !wanted(e) { return; }
    // 어느 문서에 달린 댓글인지는 `target` 이 알린다 — `quest` 는 실리지
    // 않는다(캠페인 댓글과 이름을 나누지 않으려는 설계, DEV-388/DEV-391).
    // `run` 동작이라 `run(...)` 으로 부른다 — `send` 는 밖으로 보내는(post) 동작 전용이다.
    // BUG-331: **답글을 달 수 있게** 대상과 댓글 번호를 따로 실어 준다. 예전에는 사람이
    // 읽을 한 줄(`quest:DEV-1`)뿐이라, 받은 쪽이 어디에 답해야 할지 알 수 없었다.
    run("ai", #{
        target: `${e.target.kind}:${e.target.id}`,
        target_kind: e.target.kind,
        target_id: e.target.id,
        comment_id: e.comment.id,
        author: e.comment.author,
        question: e.comment.body
    });
}

fn wanted(e) {
    let what = config("WHAT");
    if what == "all" { return true; }
    if what == "discussion" { return e.comment.discussion; }
    // 기본 — 미해결 토론만.
    e.comment.discussion && !e.comment.resolved
}
