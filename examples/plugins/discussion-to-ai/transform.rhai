// **무엇을 보낼지 설정에서 고른다** — 선택상자(`select`)의 예다.
//
// 평범한 댓글까지 다 보내면 소음이 되고, AI 를 부르는 훅이면 돈까지 든다.
// 그래서 기본값은 제일 좁은 "미해결 토론만" 이다.
//
// `comment.unresolved` 도 구독하는 이유: 한 번 해결한 토론이 다시 열리면
// 그것도 "답이 필요한 상태" 다.

fn should_send(e) {
    if !e.ok { return false; }
    let what = config("WHAT");
    if what == "all" { return true; }
    if what == "discussion" { return e.comment.discussion; }
    // 기본 — 미해결 토론만.
    e.comment.discussion && !e.comment.resolved
}

fn payload(e) {
    // 어느 문서에 달린 댓글인지는 `target` 이 알린다 — `quest` 는 실리지
    // 않는다(캠페인 댓글과 이름을 나누지 않으려는 설계, DEV-388/DEV-391).
    // 예전엔 여기서 `e.quest.id` 를 읽어 훅이 **조용히 아무것도 안 보냈다.**
    #{
        target: `${e.target.kind}:${e.target.id}`,
        author: e.comment.author,
        question: e.comment.body
    }
}
