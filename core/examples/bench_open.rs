//! DEV-246 측정용 예제 — GUI 가 길드를 열 때 도는 경로(`sync_on_open`)의 시간.
//!
//! `cargo run -p openguild-core --example bench_open -- <guild-path>`
//!
//! 벤치 전용이라 프로덕션 경로에는 영향이 없다. 출력:
//!   open        : Store::open (마이그레이션 + 풀 오픈)
//!   sync_on_open: 증분 동기화 (+ 필요 시 풀 reindex fallback)
//!   full_reindex: fallback 이 실제로 돌았는지

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: bench_open <guild-path>");
        std::process::exit(2);
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let t0 = std::time::Instant::now();
        let store = openguild_core::Store::open(&path).await.expect("open 실패");
        let open_ms = t0.elapsed().as_millis();

        let t1 = std::time::Instant::now();
        let (inc, full) = openguild_core::incremental::sync_on_open(&store)
            .await
            .expect("sync 실패");
        let sync_ms = t1.elapsed().as_millis();

        println!("open         : {open_ms} ms");
        println!("sync_on_open : {sync_ms} ms");
        println!(
            "  updated={} inserted={} deleted={} skipped={} needs_full={}",
            inc.updated,
            inc.inserted,
            inc.deleted,
            inc.skipped.len(),
            inc.needs_full_reindex
        );
        for (f, why) in &inc.skipped {
            println!("  skipped: {f} — {why}");
        }
        println!(
            "  full_reindex : {}",
            if full.is_some() { "돌았음" } else { "생략" }
        );
        println!("total        : {} ms", open_ms + sync_ms);
    });
}
