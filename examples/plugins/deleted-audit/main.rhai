// **바뀌기 전(pre)** 단계의 예 — 지워지기 **전에** 무엇이 지워질지 남긴다.
//
// soft delete 라 복구는 `openguild quest restore` 로 하고, 이 로그는 무엇을 복구할지
// 찾는 데 쓴다.
//
// pre 줄은 **막을 수도 있다.** 글자를 돌려주면 그것이 막는 이유가 되고, 표를 돌려주면
// 그 칸이 바뀐다. 아무것도 안 돌려주면 그대로 진행한다.
//
// pre 를 내는 이벤트는 몇 개뿐이다 — `openguild plugin events` 로 확인한다.
// 없는 이벤트에 pre 를 걸면 적재 때 거부된다(조용히 안 도는 것보다 낫다).

fn record(e) {
    // pre 에는 `ok` 가 아예 없다 — 아직 결과가 없으니 싣지 않는다.
    run("log", #{ ts: e.ts, guild: e.guild, id: e.quest.id, title: e.quest.title, status: e.quest.status });
}

// `keep` 태그가 붙었는지는 **줄의 조건**(`when`)이 이미 봤다 — 여기서는 막기만 한다.
fn keep(e) {
    `${e.quest.id} 에 keep 태그가 있습니다 — 태그를 떼고 다시 지우세요`
}
