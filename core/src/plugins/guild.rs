//! DEV-406: 스크립트가 시킨 **길드 일**을 실제로 한다 — 알림과 백업.
//!
//! # 왜 함수 안에서 바로 안 하나
//!
//! 스크립트는 할 일을 적기만 하고([[DEV-403]]), 실행은 함수가 끝난 뒤 여기서 한다. 셋 다
//! 그 덕을 본다:
//!
//! - 스크립트가 도는 동안 길드가 안 바뀐다 — 판단하던 값이 발밑에서 변하지 않는다.
//! - **길드 잠금 안에서 자기를 기다리는 교착이 없다.** 전달은 mutation 이 끝난 뒤 별도
//!   스레드에서 돌므로 잠금은 이미 풀려 있다.
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
            .map_err(|e| format!("길드 명령을 돌릴 런타임을 못 만들었습니다: {e}"))?;
        Ok(self.rt.get_or_init(|| rt))
    }
}

impl Delivery for WithGuild {
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
                let text = match body {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                self.notifier.notify(&plugin.def.name, &text)
            }
            CommandKind::Backup => {
                let rt = self.runtime()?;
                let store = self.store.clone();
                // DEV-401: 이 백업이 내는 `backup.created` 에 "이 플러그인이 일으켰다" 를 단다.
                let origin = event.origin.then(&plugin.def.name);
                rt.block_on(crate::events::origin::scope(origin, async move {
                    crate::snapshot::create_snapshot(&store).await
                }))
                .map(|_| ())
                .map_err(|e| format!("백업을 만들지 못했습니다: {e}"))
            }
            // send/run 은 여기 오지 않는다 — 런타임이 `deliver` 로 보낸다.
            CommandKind::Send | CommandKind::Run => {
                Err(format!("{}: 길드 명령이 아닙니다", kind.verb()))
            }
        }
    }
}
