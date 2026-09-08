// 알림을 켜는 방법: 그 퀘스트에 `notify` 태그를 붙인다.
//
//     openguild quest tag add DEV-001 notify
//
// 새 개념을 만들지 않으려고 태그를 썼다. 태그를 떼면 알림도 멈춘다.
//
// (`const` + `global::` 은 쓰지 않는다 — `no_module` 로 모듈 경로를 없앴기
//  때문에 함수 안에서 그 문법이 안 통한다.)

fn should_send(e) {
    // 실패한 전환까지 알릴 이유는 없다. `ok` 는 post 에만 실린다.
    e.ok && e.quest.tags.contains("notify")
}

fn payload(e) {
    // `chat_id` 는 코어가 body_env 로 채운다 — 스크립트에는 환경변수가 없다.
    #{
        text: `[${e.quest.id}] ${e.quest.title}\n${e.change.from} → ${e.change.to}`,
        disable_notification: false
    }
}
