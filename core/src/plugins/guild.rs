//! DEV-406: 스크립트가 시킨 **길드 일**을 실제로 한다 — 알림과 백업. 함수 도중에 부르는 `query()` 도 여기서 읽는다.
//!
//! # 왜 함수 안에서 바로 안 하나
//!
//! 스크립트는 할 일을 적기만 하고([[DEV-403]]), 실행은 함수가 끝난 뒤 여기서 한다. 셋 다
//! 그 덕을 본다:
//!
//! - 스크립트가 도는 동안 스크립트 때문에 길드가 바뀌지 않는다 — 함수가 고른 값이 자기 일로 변하지 않는다.
//! - **길드 잠금 안에서 자기를 기다리는 교착이 없다.** 전달은 잠금이 풀린 뒤에 돈다 — 보통 줄은 별도
//!   스레드에서, 끝날 때까지 기다리는 줄(`wait`)은 잠금이 풀리는 순간 부른 쪽에서(BUG-339 — 예전엔 이
//!   줄만 잠금 안에서 돌아 이 말이 틀렸다).
//! - 시간 제한·실패 기록이 한 곳에 모인다. 시험은 명령을 실행하지 않고 **기록만** 해서 본다.
//!
//! # 자기 자신에게 돌아오지 않는다
//!
//! 백업은 `backup.created` 를 낸다 — 그대로 두면 그 훅이 다시 불린다. [[DEV-401]] 의 "누가
//! 일으켰나" 를 달아 실행하므로 자기가 일으킨 일은 자기에게 안 온다.

use super::script::CommandKind;
use super::{Plugin, runtime::Delivery};
use crate::events::Event;
use serde_json::Value;
use std::sync::Arc;

/// 알림을 어디에 띄울지 — 컴포넌트가 정한다.
pub trait Notifier: Send + Sync {
    /// `plugin` 이 보낸 한 줄.
    fn notify(&self, plugin: &str, text: &str) -> Result<(), String>;

    /// 표시 시간 · 갈 곳이 있는 알림. 화면이 없는 곳(CLI · 서버)은 글만 남긴다.
    fn notify_with(
        &self,
        plugin: &str,
        text: &str,
        _options: &NoticeOptions,
    ) -> Result<(), String> {
        self.notify(plugin, text)
    }

    /// DEV-478: 플러그인이 돌다 실패했다. 화면이 있는 곳만 띄운다 — CLI · 서버는 따로 찍는다.
    fn problem(&self, _text: &str) {}
}

/// `notify(글, #{ … })` 의 옵션 — 값은 스크립트가 부를 때 이미 검사했다.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NoticeOptions {
    /// 표시 시간(초). 0 이면 닫을 때까지, 없으면 화면의 기본값.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// 누르면 열 문서 — `DEV-001` · `C-003` · `BOOK-002`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<String>,
    /// 그 문서의 댓글 번호까지 이동.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<i64>,
}

/// 기본 — 표준 오류로 한 줄. CLI 와 서버가 쓴다(서버 로그에 그대로 남는다).
pub struct Stderr;

impl Notifier for Stderr {
    fn notify(&self, plugin: &str, text: &str) -> Result<(), String> {
        eprintln!("[plugin:{plugin}] {text}");
        Ok(())
    }
}

/// 전달 구현을 감싸 길드 명령을 더한다.
pub struct WithGuild {
    inner: Arc<dyn Delivery>,
    store: crate::Store,
    notifier: Arc<dyn Notifier>,
    /// 길드 일을 돌릴 런타임. 전달 스레드는 런타임 밖이라 여기서 하나 만들어 쓴다 — 컴포넌트의
    /// 런타임(CLI 의 current_thread 등)을 빌리면 그쪽이 안 돌 때 멈춘다. 쓸 일이 있을 때 만든다.
    rt: std::sync::OnceLock<tokio::runtime::Runtime>,
}

impl WithGuild {
    pub fn new(inner: Arc<dyn Delivery>, store: crate::Store, notifier: Arc<dyn Notifier>) -> Self {
        Self {
            inner,
            store,
            notifier,
            rt: std::sync::OnceLock::new(),
        }
    }

    fn runtime(&self) -> Result<&tokio::runtime::Runtime, String> {
        if let Some(rt) = self.rt.get() {
            return Ok(rt);
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| {
                crate::tf!(
                    "길드 명령을 돌릴 런타임을 못 만들었습니다: {e}",
                    "could not create the runtime for guild commands: {e}"
                )
            })?;
        Ok(self.rt.get_or_init(|| rt))
    }

    /// 길드 일을 돌리고 끝날 때까지 기다린다. **런타임 밖의 스레드에서** 돌린다 — 시간 실행 · `wait` · `pre` 줄은 이미
    /// 런타임 안에서 도는데, 그 자리에서 `block_on` 하면 tokio 가 패닉한다(BUG-390).
    fn block_on<T: Send>(
        &self,
        work: impl std::future::Future<Output = T> + Send,
    ) -> Result<T, String> {
        let rt = self.runtime()?;
        std::thread::scope(|s| s.spawn(|| rt.block_on(work)).join()).map_err(|_| {
            crate::tf!(
                "길드 명령을 돌리다 패닉했습니다",
                "a guild command panicked"
            )
        })
    }
}

impl WithGuild {
    /// DEV-478: 알림함에 남긴다. 못 남겨도 알림은 띄운다 — 남기는 것은 덤이다.
    fn keep(&self, n: &crate::notices::Notice) {
        if let Err(e) = crate::notices::record(&self.store.paths, n) {
            eprintln!("[notices] {e:#}");
        }
    }
}

impl Drop for WithGuild {
    /// 플러그인을 다시 불러오면 비동기 작업 안에서 버려질 수 있다 — 그대로 버리면 런타임이 기다리려다 패닉한다.
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

impl Delivery for WithGuild {
    /// 목록은 index 를 읽는다 — 켜 둔 앱 · 서버는 밖에서 고친 파일을 모르니 함수의 첫 조회 때 맞춘다(BUG-389).
    /// 못 맞추면 낡은 목록으로 판단하지 않게 오류로 돌려준다.
    fn query(&self, of: &str, q: &str, fresh: bool) -> Result<Value, String> {
        let of = super::schedule::Of::parse(of)?;
        let store = self.store.clone();
        let q = q.to_string();
        self.block_on(async move {
            if fresh {
                crate::incremental::sync_if_changed(&store)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            super::schedule::items(&store, of, &q)
                .await
                .map(Value::Array)
                .map_err(|e| e.to_string())
        })?
    }

    fn deliver(
        &self,
        plugin: &Plugin,
        action: &super::Action,
        event: &Event,
        body: &Value,
        values: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), String> {
        self.inner.deliver(plugin, action, event, body, values)
    }

    fn guild_command(
        &self,
        plugin: &Plugin,
        kind: CommandKind,
        body: &Value,
        event: &Event,
    ) -> Result<(), String> {
        match kind {
            CommandKind::Notify => {
                let name = &plugin.def.name;
                if let Value::Object(fields) = body
                    && let Some(Value::String(text)) = fields.get("text")
                {
                    let options: NoticeOptions = serde_json::from_value(fields["options"].clone())
                        .map_err(|e| e.to_string())?;
                    let mut n =
                        crate::notices::Notice::now(crate::notices::Kind::Notify, Some(name), text);
                    n.open = options.open.clone();
                    n.comment = options.comment;
                    self.keep(&n);
                    return self.notifier.notify_with(name, text, &options);
                }
                let text = match body {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                self.keep(&crate::notices::Notice::now(
                    crate::notices::Kind::Notify,
                    Some(name),
                    &text,
                ));
                self.notifier.notify(name, &text)
            }
            CommandKind::Backup => {
                let store = self.store.clone();
                // DEV-401: 이 백업이 내는 `backup.created` 에 "이 플러그인이 일으켰다" 를 단다.
                let origin = event.origin.then(&plugin.def.name);
                self.block_on(crate::events::origin::scope(origin, async move {
                    crate::snapshot::create_snapshot(&store).await
                }))?
                .map(|_| ())
                .map_err(|e| {
                    crate::tf!(
                        "백업을 만들지 못했습니다: {e}",
                        "could not create a backup: {e}"
                    )
                })
            }
            // send/run 은 여기 오지 않는다 — 런타임이 `deliver` 로 보낸다.
            CommandKind::Send | CommandKind::Run => Err(crate::tf!(
                "{}: 길드 명령이 아닙니다",
                "{}: not a guild command",
                kind.verb()
            )),
        }
    }
}
