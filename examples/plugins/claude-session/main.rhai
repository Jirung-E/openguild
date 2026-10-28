// 댓글 명령 — 본문이 정해 둔 명령이면 터미널을 열고, 댓글은 막거나 기록 한 줄로 바꾼다.
//
// 바뀌기 전(pre) 단계의 답: 글자를 돌려주면 그 글이 막는 이유(안내)가 되고, 표를 돌려주면 그 칸이 바뀐다.
// 막혀도 이 함수가 맡긴 일(`run`)은 돈다 — 그래서 막으면서 터미널을 열 수 있다.

fn on_comment(e) {
    let cmd = config("COMMAND");
    if cmd == () || cmd == "" { cmd = "/session"; }
    let body = e.comment.body;
    body.trim();
    if body != cmd { return; }

    // 세션 이름 = 댓글이 달린 문서의 번호(퀘스트면 DEV-001, 캠페인이면 C-001).
    let name = e.target.id;
    run("open", #{ name: name });

    if config("REPLY") == "record" {
        // 백틱 문자열 안에는 백틱을 못 넣는다 — 코드 표시는 이어 붙인다.
        return #{ body: "터미널에서 `" + name + "` 세션을 열었습니다 (`" + cmd + "`)" };
    }
    `터미널에서 ${name} 세션을 엽니다 — 명령이라 댓글로는 남기지 않았습니다`
}
