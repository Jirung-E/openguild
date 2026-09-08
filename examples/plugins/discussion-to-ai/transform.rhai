// **미해결 토론 댓글만** 보낸다. 평범한 댓글까지 보내면 소음이 되고, AI 를
// 부르는 훅이면 돈까지 든다. 거르는 것이 이 스크립트의 존재 이유다.
//
// `comment.unresolved` 도 구독하는 이유: 한 번 해결한 토론이 다시 열리면
// 그것도 "답이 필요한 상태" 다.

fn should_send(e) {
    e.ok && e.comment.discussion && !e.comment.resolved
}

fn payload(e) {
    #{
        quest: e.quest.id,
        author: e.comment.author,
        question: e.comment.body
    }
}
