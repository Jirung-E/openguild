// 무엇을 알릴지는 **설정 화면에서** 켜고 끈다(REQ-021). `config(키)` 가 그 값을
// 돌려주고, 체크박스는 bool 로 온다 — 문자열이면 `if config(k)` 가 항상 참이다.
//
// 설정 → 플러그인 에서, 또는:
//     echo -n 켤값 | openguild plugin set telegram-quest-status ON_COMMENT
//
// (`const` + `global::` 은 쓰지 않는다 — `no_module` 로 모듈 경로를 없앴기
//  때문에 함수 안에서 그 문법이 안 통한다.)

fn should_send(e) {
    // 실패한 변경까지 알릴 이유는 없다. `ok` 는 post 에만 실린다.
    if !e.ok { return false; }

    if e.event == "quest.created" {
        return config("ON_CREATED") && tagged(e);
    }
    if e.event == "quest.status_changed" {
        return config("ON_STATUS") && tagged(e);
    }
    if e.event == "comment.added" {
        // **댓글에는 태그 필터를 못 건다.** `comment.added` 는 어느 문서에
        // 달렸는지를 `target: {kind, id}` 로만 알리고(캠페인 댓글과 이름을
        // 나누지 않으려는 설계), 그 퀘스트의 태그는 안 싣는다. 여기서 켜고
        // 끄는 것은 "댓글 알림 전체" 다.
        return config("ON_COMMENT");
    }
    false
}

// "이 퀘스트만 알림" 을 태그로 고른다 — 새 개념을 안 만들려고 그렇게 했다.
// 설정에서 끄면 전부 알린다. 퀘스트 이벤트에만 쓴다(위 참고).
fn tagged(e) {
    if !config("ONLY_TAGGED") { return true; }
    e.quest.tags.contains("notify")
}

fn payload(e) {
    // `chat_id` 는 코어가 body_env 로 채운다.
    if e.event == "comment.added" {
        return #{
            text: `💬 ${e.target.id}\n${e.comment.body}`,
            disable_notification: false
        };
    }
    let head = `[${e.quest.id}] ${e.quest.title}`;
    let tail = if e.event == "quest.status_changed" {
        `\n${e.change.from} → ${e.change.to}`
    } else {
        "\n새 퀘스트"
    };
    #{ text: head + tail, disable_notification: false }
}
