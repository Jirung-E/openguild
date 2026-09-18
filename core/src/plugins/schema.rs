//! DEV-411: `plugin.toml` 의 **스키마 파일**. 편집기가 자동 완성과 오류 표시를 해 준다.
//!
//! # 왜 만들어 주나
//!
//! 이벤트 이름은 55개고, 줄이 받을 수 있는 `with` 는 대상 종류가 정한다. 그걸 외우거나 문서를
//! 왕복하며 쓰는 대신, 편집기가 목록을 띄우게 한다. 스키마는 **지금 이 버전이 아는 것**에서
//! 만들어지므로 문서처럼 낡지 않는다.
//!
//! ```bash
//! openguild plugin schema --out .guild/plugins/plugin.schema.json
//! ```
//!
//! 그리고 `plugin.toml` 첫 줄에 이렇게 적으면 Taplo(VS Code 의 Even Better TOML)가 집어 든다.
//!
//! ```toml
//! #:schema ../plugin.schema.json
//! ```
//!
//! # 스키마가 못 하는 것
//!
//! "이 줄의 `with` 에는 이 이벤트가 주는 것만" 처럼 **줄 안에서 값끼리 걸리는 규칙**은 JSON
//! Schema 로 깔끔하게 못 적는다. 그래서 목록은 합집합으로 주고, 진짜 검사는 적재와
//! [`super::check`] 가 한다. 스키마는 오타를 줄이는 도구지 검사기가 아니다.

use serde_json::{Value, json};

/// 파일에 쓸 스키마 한 덩이.
pub fn plugin_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "openguild plugin.toml",
        "type": "object",
        "required": ["name", "scope", "handlers"],
        "additionalProperties": false,
        "properties": {
            "name": {
                "type": "string",
                "description": "플러그인 이름. 길드 안에서 하나뿐이어야 한다 — 동의도 화면도 이 이름으로 구분한다."
            },
            "description": {
                "type": "string",
                "description": "무슨 일을 하는지 사람 말로. 허용 화면 맨 위에 나온다."
            },
            "scope": {
                "type": "array",
                "minItems": 1,
                "items": { "enum": ["cli", "gui", "server"] },
                "description": "어느 컴포넌트에서 도는가. 비어 있으면 적재하지 않는다."
            },
            "scripts": {
                "type": "array",
                "items": { "type": "string", "pattern": "\\.rhai$" },
                "description": "스크립트 파일들(폴더 기준 상대 경로). 적힌 파일의 함수는 한 공간에 모인다."
            },
            "permissions": {
                "type": "array",
                "items": { "enum": super::PERMISSIONS },
                "description": "스크립트가 길드에 시킬 수 있는 일. 밝히지 않은 것을 부르면 그 줄만 실패한다."
            },
            "actions": {
                "type": "object",
                "additionalProperties": { "$ref": "#/$defs/action" },
                "description": "이름 붙인 동작. 스크립트는 이름만 알고, 주소는 여기에만 있다."
            },
            "handlers": {
                "type": "array",
                "minItems": 1,
                "items": { "$ref": "#/$defs/handler" },
                "description": "언제 무엇을 할지 — 적힌 순서대로 돈다."
            },
            "inputs": {
                "type": "array",
                "items": { "$ref": "#/$defs/input" },
                "description": "사용자에게 받아야 하는 값들(설정 화면이 그린다)."
            }
        },
        "$defs": {
            "handler": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "id": { "type": "string", "description": "줄 이름. 화면과 오류 메시지가 이 줄을 가리킬 때 쓴다." },
                    "pre": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/preEvent" },
                        "description": "바뀌기 **전**에 볼 이벤트. 막거나 값을 바꿀 수 있고, 언제나 기다린다. `post` 와 둘 중 하나만."
                    },
                    "post": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/postEvent" },
                        "description": "바뀐 **뒤**에 볼 이벤트. `pre` 와 둘 중 하나만."
                    },
                    "call": { "type": "string", "description": "부를 스크립트 함수. `action` 과 둘 중 하나만." },
                    "action": {
                        "description": "바로 실행할 동작 — `[actions]` 의 이름이거나 이 자리에 적은 동작.",
                        "anyOf": [{ "type": "string" }, { "$ref": "#/$defs/action" }]
                    },
                    "with": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/with" },
                        "description": "함수가 이벤트 뒤에 받을 연결 데이터. 받을 수 있는 것은 이벤트 대상의 종류가 정한다."
                    },
                    "when": {
                        "type": "object",
                        "description": "이 줄이 불릴 조건 — 전부 만족해야 한다. 경로는 `change.to` 처럼 점으로 잇는다.",
                        "additionalProperties": {
                            "anyOf": [
                                { "type": ["string", "number", "boolean"] },
                                { "type": "array", "items": { "type": ["string", "number", "boolean"] } }
                            ]
                        }
                    },
                    "wait": { "type": "boolean", "description": "이 줄이 끝날 때까지 기다린다(기본: 안 기다림)." },
                    "timeout_ms": { "type": "integer", "minimum": 1, "description": "이 줄에 줄 시간(ms)." },
                    "on_timeout": { "enum": ["continue", "block"], "description": "시간이 넘었을 때. `block` 은 `pre` 줄만." },
                    "on_error": { "enum": ["continue", "block"], "description": "스크립트가 던졌을 때. `block` 은 `pre` 줄만." }
                }
            },
            "action": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "post": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["url"],
                        "properties": {
                            "url": { "type": "string" },
                            "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                            "body_env": {
                                "type": "object",
                                "additionalProperties": { "type": "string" },
                                "description": "본문에 끼워 넣을 환경변수 — `{ chat_id = \"TELEGRAM_CHAT_ID\" }`. 값이 아니라 **이름**을 적는다."
                            },
                            "timeout_ms": { "type": "integer", "minimum": 1 }
                        }
                    },
                    "run": {
                        "type": "object",
                        "required": ["command"],
                        "properties": {
                            "command": { "type": "string" },
                            "args": { "type": "array", "items": { "type": "string" } },
                            "timeout_ms": { "type": "integer", "minimum": 1 },
                            "windows": { "$ref": "#/$defs/osRun" },
                            "macos": { "$ref": "#/$defs/osRun" },
                            "linux": { "$ref": "#/$defs/osRun" }
                        }
                    }
                }
            },
            "osRun": {
                "type": "object",
                "additionalProperties": false,
                "required": ["command"],
                "properties": {
                    "command": { "type": "string" },
                    "args": { "type": "array", "items": { "type": "string" } }
                }
            },
            "input": {
                "type": "object",
                "required": ["key"],
                "properties": {
                    "key": { "type": "string" },
                    "label": { "type": "string" },
                    "type": { "enum": ["text", "checkbox", "select", "number"] },
                    "help": { "type": "string" },
                    "secret": { "type": "boolean" },
                    "default": {},
                    "options": { "type": "array" }
                }
            },
            // 이름은 `quest.*` 처럼 별표로도 쓸 수 있으므로 **enum 이 아니라** 목록 + 자유 문자열이다.
            // 편집기는 `examples` 를 자동 완성으로 띄운다.
            "postEvent": { "type": "string", "examples": crate::events::names::ALL },
            "preEvent": { "type": "string", "examples": crate::events::names::PRE_CAPABLE },
            "with": { "type": "string", "enum": with_names() }
        }
    })
}

/// 줄이 받을 수 있는 것 전부(대상 종류들의 합집합).
fn with_names() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (_, rels) in super::related::RELATIONS {
        for r in *rels {
            if !out.contains(r) {
                out.push(r);
            }
        }
    }
    out
}

/// 파일에 쓸 글자.
pub fn schema_text() -> String {
    serde_json::to_string_pretty(&plugin_schema()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 스키마는 **지금 이 버전이 아는 것**에서 나온다 — 이벤트를 더하면 스키마도 따라온다.
    #[test]
    fn the_schema_carries_this_versions_lists() {
        let s = plugin_schema();
        let ex = s["$defs"]["postEvent"]["examples"].as_array().unwrap();
        assert_eq!(ex.len(), crate::events::names::ALL.len());
        assert!(ex.iter().any(|v| v == "quest.status_changed"));
        let pre = s["$defs"]["preEvent"]["examples"].as_array().unwrap();
        assert_eq!(pre.len(), crate::events::names::PRE_CAPABLE.len());
        let perms = s["properties"]["permissions"]["items"]["enum"].as_array().unwrap();
        assert_eq!(perms.len(), super::super::PERMISSIONS.len());
        let with = s["$defs"]["with"]["enum"].as_array().unwrap();
        assert!(with.iter().any(|v| v == "subject") && with.iter().any(|v| v == "prereqs"));
    }

    /// 스키마가 **모델보다 뒤처지지 않게** — 정의·줄에 칸을 더하면 여기서 걸린다.
    /// (JSON Schema 검사기를 들이는 대신, 드리프트가 실제로 나는 지점만 붙잡는다.)
    #[test]
    fn the_schema_knows_every_field_the_model_has() {
        let s = plugin_schema();
        let declared = |v: &Value| -> Vec<String> {
            v.as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default()
        };
        let root = declared(&s["properties"]);
        for f in ["name", "description", "scope", "scripts", "permissions", "actions", "handlers", "inputs"] {
            assert!(root.contains(&f.to_string()), "스키마에 `{f}` 가 없다");
        }
        let h = declared(&s["$defs"]["handler"]["properties"]);
        for f in ["id", "pre", "post", "call", "action", "with", "when", "wait", "timeout_ms", "on_timeout", "on_error"] {
            assert!(h.contains(&f.to_string()), "스키마의 줄에 `{f}` 가 없다");
        }
    }

    /// 저장소에 넣어 둔 스키마 파일이 낡지 않았나. 예제들이 `#:schema` 로 이 파일을 가리키므로,
    /// 낡으면 편집기가 **틀린 것을 맞다고** 말하게 된다.
    #[test]
    fn the_checked_in_schema_file_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/plugins/plugin.schema.json");
        let on_disk = std::fs::read_to_string(&path).expect("스키마 파일");
        assert_eq!(
            on_disk.trim(),
            schema_text().trim(),
            "{} 가 낡았습니다 — `openguild plugin schema --out {}` 로 다시 만드세요",
            path.display(),
            path.display()
        );
    }

    /// 우리가 쓴 예제 정의의 칸이 전부 스키마에 있어야 한다 — 없으면 편집기가 빨간 줄을 긋는다.
    #[test]
    fn the_shipped_examples_only_use_known_keys() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/plugins");
        let s = plugin_schema();
        let known = |at: &Value| -> Vec<String> {
            at.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
        };
        let top = known(&s["properties"]);
        let line = known(&s["$defs"]["handler"]["properties"]);
        let mut seen = 0;
        for e in std::fs::read_dir(&root).expect("예제 폴더").flatten() {
            let manifest = e.path().join(super::super::MANIFEST);
            if !manifest.is_file() {
                continue;
            }
            let raw = std::fs::read_to_string(&manifest).unwrap();
            let v: Value = toml::from_str(&raw).expect("예제 TOML");
            for k in v.as_object().unwrap().keys() {
                assert!(top.contains(k), "{}: 스키마가 모르는 칸 `{k}`", manifest.display());
            }
            for h in v["handlers"].as_array().unwrap_or(&Vec::new()) {
                for k in h.as_object().unwrap().keys() {
                    assert!(line.contains(k), "{}: 줄에 모르는 칸 `{k}`", manifest.display());
                }
            }
            seen += 1;
        }
        assert!(seen >= 3, "예제를 못 찾았다: {seen}");
    }
}
