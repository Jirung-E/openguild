//! 서비스 레이어 — sqlx 풀 + 도메인 입력을 받아 결과를 반환.
//!
//! 호출자 (server / cli local / desktop invoke) 는 자기 인터페이스 형식으로
//! 입력만 추출해 넘기면 된다. 이 레이어가 검증·SQL·트랜잭션을 담당.

pub mod campaigns;
pub mod comments;
pub mod meta;
pub mod quests;
