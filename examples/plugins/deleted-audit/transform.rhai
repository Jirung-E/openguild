// **관찰 pre** 의 예 — 지워지기 **전에** 무엇이 지워질지 남긴다.
//
// pre 는 거부(veto)를 못 한다. 훅이 죽으면 길드가 멈추기 때문이다([[DEV-373]]).
// 그래서 이건 "막는 것" 이 아니라 "기록해 두는 것" 이다. soft delete 라 복구는
// `openguild quest restore` 로 하고, 이 로그는 무엇을 복구할지 찾는 데 쓴다.
//
// pre 를 내는 이벤트는 지금 둘뿐이다: `quest.deleted`, `comment.added`.
// 없는 이벤트에 `pre:` 를 걸면 적재 때 거부된다(조용히 안 도는 것보다 낫다).

fn payload(e) {
    // pre 에는 `ok` 가 아예 없다 — 아직 결과가 없으니 싣지 않는다.
    #{ ts: e.ts, guild: e.guild, id: e.quest.id, title: e.quest.title, status: e.quest.status }
}
