// 백업이 만들어질 때마다 그 파일을 다른 폴더로 복사한다.
//
// 스크립트가 하는 일은 둘이다 — **복사할지 정하고**, 셸이 쓰기 쉬운 모양으로
// 다듬는다. 셸에서 JSON 을 파싱하면 jq 가 있느냐에 따라 도는 예제가 되므로,
// 여기서 경로만 한 줄로 내보낸다.

fn should_send(e) {
    if e.event != "backup.created" { return false; }
    // 자동 백업(ops 50개 / 24시간마다)까지 쌓을지는 설정에서 고른다.
    if config("ONLY_MANUAL") && e.automatic { return false; }
    // 쌓아 둘 곳을 안 정했으면 아무것도 안 한다 — 어디로 복사할지 모른다.
    config("ARCHIVE_DIR") != ""
}

fn payload(e) {
    // 셸이 `read` 한 줄로 받는다. 경로는 절대 경로다.
    e.path
}
