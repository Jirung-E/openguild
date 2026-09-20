//! REQ-028: 도서관 문서를 **파일 시스템으로 펼친다** — 폴더 구조 그대로.
//!
//! # 왜 펼쳐야 하나
//!
//! 도서관의 폴더는 **진짜 디렉터리가 아니다.** 문서는 전부 `.guild/library/BOOK-NNN.md` 로
//! 나란히 있고, "아키텍처/결정" 같은 폴더는 문서가 적어 둔 값(`path`)일 뿐이다. 그래서 파일
//! 탐색기로 가져가려면 그 구조를 **그때 만들어야** 한다 — 어딘가에 이미 있는 폴더를 복사하는
//! 일이 아니다.
//!
//! ```text
//! .guild/library/BOOK-003.md   (path = "아키텍처")   →   내보낸곳/아키텍처/라우터 설계.md
//! ```
//!
//! # 이름은 사람이 읽는 제목으로
//!
//! 관리번호(`BOOK-003.md`)로 내보내면 탐색기에서 무엇인지 알 수 없다. 제목을 파일 이름으로
//! 쓰되, 파일 이름에 못 쓰는 글자는 바꾸고 같은 이름이 겹치면 번호를 붙인다. 원문은 그대로
//! 둔다(frontmatter 포함) — 나중에 되돌려 넣을 때 필요한 것이 거기에 다 있다.

use crate::error::{AppError, AppResult};
use crate::store::Store;
use std::path::{Path, PathBuf};

/// 무엇을 내보내나.
#[derive(Debug, Clone)]
pub enum Pick {
    /// 문서 하나.
    Doc(String),
    /// 그 폴더와 **그 아래 전부**. 빈 글자면 도서관 전체.
    Folder(String),
}

/// 내보낸 파일 하나.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Exported {
    pub book_id: String,
    /// 내보낸 곳 기준 상대 경로 — `아키텍처/라우터 설계.md`.
    pub rel: String,
    /// 실제로 쓴 절대 경로.
    pub path: String,
    /// 이 문서와 함께 나간 첨부 수.
    pub attachments: usize,
}

/// 고른 것을 `dest` 아래에 펼친다. 이미 있는 파일은 덮지 않고 이름 뒤에 번호를 붙인다 —
/// 내보내기가 남의 파일을 지우면 안 된다.
pub async fn export(store: &Store, pick: &Pick, dest: &Path) -> AppResult<Vec<Exported>> {
    if !dest.is_dir() {
        return Err(AppError::BadRequest(crate::tf!(
            "내보낼 폴더가 없습니다: {}",
            "no such folder to export into: {}",
            dest.display()
        )));
    }
    let docs = match pick {
        Pick::Doc(id) => super::library::get_book(store, id)
            .await?
            .map(|d| vec![d])
            .ok_or_else(|| {
                AppError::NotFound(crate::tf!(
                    "도서관 문서 '{}' 없음",
                    "library doc '{}' not found",
                    id
                ))
            })?,
        Pick::Folder(folder) => {
            let all = super::library::list_books(store).await?;
            all.into_iter().filter(|d| under(&d.path, folder)).collect()
        }
    };

    let mut out = Vec::new();
    let mut used: Vec<PathBuf> = Vec::new();
    for d in docs {
        let book_id = d.book_id();
        // 폴더 하나만 고르면 그 폴더를 뿌리로 삼는다 — 고른 폴더 이름까지 통째로 나오는 게
        // 탐색기에서 자연스럽다(`아키텍처/결정` 을 고르면 `결정/…`).
        let rel_dir = match pick {
            Pick::Folder(f) if !f.is_empty() => trim_prefix(&d.path, f),
            _ => d.path.clone(),
        };
        let dir = dest.join(safe_rel(&rel_dir));
        std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;

        let name = unique(&dir, &safe_name(&d.title, &book_id), "md", &mut used);
        let src = store.paths.book_path(&book_id);
        let body = std::fs::read_to_string(&src).map_err(io_err(&src))?;
        std::fs::write(&name, &body).map_err(io_err(&name))?;

        // 첨부는 문서 옆 폴더로. 문서만 가져가고 첨부를 잃으면 조용한 손실이다.
        let atts = super::attachments::list_book_attachments(store, &book_id);
        let mut copied = 0;
        if !atts.is_empty() {
            let adir = name.with_extension("attachments");
            std::fs::create_dir_all(&adir).map_err(io_err(&adir))?;
            for a in &atts {
                let from = store.paths.dot_guild().join(&a.path);
                if !from.is_file() {
                    continue; // 파일이 사라진 첨부 — 기록만 남은 경우.
                }
                // 첨부 이름은 한 조각이다 — 경로 조각으로 쪼개지 않는다.
                let to = unique(&adir, &clean(&a.name), "", &mut Vec::new());
                std::fs::copy(&from, &to).map_err(io_err(&to))?;
                copied += 1;
            }
        }

        out.push(Exported {
            book_id,
            rel: name
                .strip_prefix(dest)
                .unwrap_or(&name)
                .to_string_lossy()
                .replace('\\', "/"),
            path: name.display().to_string(),
            attachments: copied,
        });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

/// 이 문서가 그 폴더(또는 그 아래)에 있나. 빈 폴더는 전체.
fn under(path: &str, folder: &str) -> bool {
    if folder.is_empty() {
        return true;
    }
    path == folder || path.starts_with(&format!("{folder}/"))
}

/// 고른 폴더를 뿌리로 — `아키텍처/결정` 을 고르면 그 아래 문서의 `rel` 은 `결정/…` 이 아니라
/// 고른 폴더의 **마지막 조각부터** 시작한다.
fn trim_prefix(path: &str, folder: &str) -> String {
    let leaf = folder.rsplit('/').next().unwrap_or(folder);
    match path.strip_prefix(folder) {
        Some(rest) => format!("{leaf}{rest}"),
        None => path.to_string(),
    }
}

/// 폴더 경로를 파일 시스템에 안전하게. 조각마다 이름 규칙을 적용하고 `..` 는 버린다.
fn safe_rel(path: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for seg in path.split('/') {
        let s = clean(seg);
        if s.is_empty() || s == "." || s == ".." {
            continue;
        }
        out.push(s);
    }
    out
}

/// 제목을 파일 이름으로. 비어 있으면 관리번호를 쓴다.
fn safe_name(title: &str, book_id: &str) -> String {
    let s = clean(title);
    if s.is_empty() { book_id.to_string() } else { s }
}

/// 파일 이름에 못 쓰는 글자를 바꾼다. 한글은 그대로 둔다 — 제목이 곧 이름이어야 한다.
fn clean(s: &str) -> String {
    let bad = ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0'];
    let out: String = s
        .chars()
        .map(|c| if bad.contains(&c) || (c as u32) < 0x20 { '_' } else { c })
        .collect();
    // 끝의 공백·점은 Windows 가 싫어한다.
    out.trim().trim_end_matches('.').to_string()
}

/// 안 겹치는 경로. 같은 이름이 있으면 ` (2)`, ` (3)` … 을 붙인다.
fn unique(dir: &Path, stem: &str, ext: &str, used: &mut Vec<PathBuf>) -> PathBuf {
    let make = |n: usize| -> PathBuf {
        let base = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem} ({n})")
        };
        dir.join(if ext.is_empty() {
            base
        } else {
            format!("{base}.{ext}")
        })
    };
    let mut n = 1;
    loop {
        let p = make(n);
        if !p.exists() && !used.contains(&p) {
            used.push(p.clone());
            return p;
        }
        n += 1;
    }
}

fn io_err(path: &Path) -> impl Fn(std::io::Error) -> AppError + '_ {
    move |e| {
        AppError::Internal(anyhow::anyhow!(crate::tf!(
            "{} 를 쓰지 못했습니다: {}",
            "failed to write {}: {}",
            path.display(),
            e
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn guild(label: &str) -> (Store, PathBuf) {
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("og-export-{label}-{ns}"));
        std::fs::create_dir_all(&dir).unwrap();
        crate::repo::seed_guild_dir(&dir).unwrap();
        let store = Store::open(&dir).await.unwrap();
        (store, dir)
    }

    async fn new_doc(store: &Store, title: &str, folder: &str, body: &str) -> String {
        super::super::library::create_book(store, title, body, folder)
            .await
            .unwrap()
            .book_id()
    }

    /// 폴더는 진짜 디렉터리가 아니다 — 내보낼 때 **그 구조를 만들어야** 한다.
    #[tokio::test]
    async fn folders_become_real_directories() {
        let (store, dir) = guild("tree").await;
        new_doc(&store, "라우터 설계", "아키텍처", "본문").await;
        new_doc(&store, "결정 기록", "아키텍처/결정", "본문2").await;
        new_doc(&store, "맨 위", "", "본문3").await;

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let out = export(&store, &Pick::Folder(String::new()), &dest)
            .await
            .unwrap();

        let rels: Vec<&str> = out.iter().map(|e| e.rel.as_str()).collect();
        assert!(rels.contains(&"아키텍처/라우터 설계.md"), "{rels:?}");
        assert!(rels.contains(&"아키텍처/결정/결정 기록.md"), "{rels:?}");
        assert!(rels.contains(&"맨 위.md"), "{rels:?}");
        // 원문 그대로 — frontmatter 까지(되돌려 넣을 때 필요하다).
        let body = std::fs::read_to_string(dest.join("아키텍처/라우터 설계.md")).unwrap();
        assert!(body.contains("본문"), "{body}");
        assert!(body.starts_with("---") || body.starts_with("+++"), "{body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 폴더 하나만 고르면 그 폴더가 뿌리다.
    #[tokio::test]
    async fn picking_a_folder_makes_it_the_root() {
        let (store, dir) = guild("pick").await;
        new_doc(&store, "안쪽", "아키텍처/결정", "x").await;
        new_doc(&store, "바깥", "다른곳", "y").await;

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let out = export(&store, &Pick::Folder("아키텍처".into()), &dest)
            .await
            .unwrap();
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].rel, "아키텍처/결정/안쪽.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 파일 이름은 제목이다 — 관리번호로 내보내면 탐색기에서 무엇인지 알 수 없다.
    /// 같은 제목이 둘이면 덮지 않고 번호를 붙인다.
    #[tokio::test]
    async fn titles_become_filenames_without_overwriting() {
        let (store, dir) = guild("name").await;
        new_doc(&store, "같은 제목", "", "첫째").await;
        new_doc(&store, "같은 제목", "", "둘째").await;
        // 파일 이름에 못 쓰는 글자.
        new_doc(&store, "a/b:c", "", "셋째").await;

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let out = export(&store, &Pick::Folder(String::new()), &dest)
            .await
            .unwrap();
        let rels: Vec<&str> = out.iter().map(|e| e.rel.as_str()).collect();
        assert!(rels.contains(&"같은 제목.md"), "{rels:?}");
        assert!(rels.contains(&"같은 제목 (2).md"), "{rels:?}");
        assert!(rels.contains(&"a_b_c.md"), "{rels:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 첨부는 문서 옆으로 — 문서만 가져가고 첨부를 잃으면 조용한 손실이다.
    #[tokio::test]
    async fn attachments_travel_with_their_document() {
        let (store, dir) = guild("att").await;
        let id = new_doc(&store, "첨부 있는 문서", "", "본문").await;
        let inside = dir.join(".guild/attachments");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::write(inside.join("spec.md"), "스펙").unwrap();
        super::super::attachments::add_book_attachment(&store, &id, "attachments/spec.md", "스펙.md")
            .await
            .unwrap();

        let dest = dir.join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let out = export(&store, &Pick::Doc(id), &dest).await.unwrap();
        assert_eq!(out[0].attachments, 1, "{out:?}");
        let att = dest.join("첨부 있는 문서.attachments/스펙.md");
        assert!(att.is_file(), "{att:?} 가 없다");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
