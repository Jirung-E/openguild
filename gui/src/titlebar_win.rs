//! DEV-265 (Windows): decorations:false 커스텀 타이틀바에서도 최대화 버튼
//! 호버 시 진짜 OS Snap Layout 플라이아웃이 뜨도록 `WM_NCHITTEST` 를
//! 가로챈다.
//!
//! 배경: WinUI3 의 `ExtendsContentIntoTitleBar` 조차 프레임 제거 후 캡션
//! 버튼 자체는 앱이 그린다(MS 공식 문서) — DWM 이 버튼 픽셀을 그려주는
//! API 는 존재하지 않는다. 그래서 "네이티브"로 개선 가능한 건 아이콘
//! 픽셀이 아니라 *동작* 이다: 이 모듈은 tauri-plugin-frame /
//! tauri-plugin-decoration 이 쓰는 것과 동일한 히트테스트 트릭 —
//! `WM_NCHITTEST` 에서 우리 최대화 버튼 영역이면 `HTMAXBUTTON` 을 리턴해
//! OS 의 Snap Layout 판단 로직이 그 영역을 "진짜 최대화 버튼"으로 인식하게
//! 만든다. 버튼 아이콘 자체는 여전히 프론트(Segoe Fluent Icons 폰트
//! 글리프)가 그린다.
//!
//! 프론트(`TitleBar.svelte`)가 최대화 버튼의 클라이언트 좌표(물리 픽셀,
//! devicePixelRatio 반영)를 `set_maximize_hit_rect` invoke 로 알려주면,
//! 이 좌표를 저장해두고 WM_NCHITTEST 가 그 사각형 안이면 HTMAXBUTTON 을
//! 반환한다. 나머지 메시지는 원래 wndproc 으로 그대로 넘긴다.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, HTMAXBUTTON, WM_NCHITTEST,
};

#[derive(Clone, Copy, Default)]
struct ClientRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl ClientRect {
    fn contains(&self, px: i32, py: i32) -> bool {
        self.w > 0
            && self.h > 0
            && px >= self.x
            && py >= self.y
            && px < self.x + self.w
            && py < self.y + self.h
    }
}

type RawWndProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;

struct WindowState {
    original_proc: RawWndProc,
    max_rect: ClientRect,
}

fn registry() -> &'static Mutex<HashMap<isize, WindowState>> {
    static REG: OnceLock<Mutex<HashMap<isize, WindowState>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 프론트에서 리사이즈/호버 때마다 호출 — 최대화 버튼의 클라이언트 좌표
/// 사각형(물리 픽셀)을 저장한다. 해당 HWND 를 처음 보는 경우에만 wndproc
/// 을 서브클래싱한다.
pub fn set_maximize_hit_rect(hwnd_isize: isize, x: i32, y: i32, w: i32, h: i32) {
    let hwnd = hwnd_isize as HWND;
    let mut reg = match registry().lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if let Some(state) = reg.get_mut(&hwnd_isize) {
        state.max_rect = ClientRect { x, y, w, h };
        return;
    }
    // SAFETY: hwnd 는 Tauri 가 준 유효한 최상위 창 핸들. GWLP_WNDPROC 교체는
    // 이 창이 살아있는 동안만 의미가 있고, 창 파괴는 OS 가 처리한다. 반환된
    // 이전 프로시저 포인터를 저장해두고 우리가 처리하지 않는 메시지는 그
    // 프로시저로 그대로 위임한다.
    let original_ptr = unsafe {
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wnd_proc_trampoline as *const () as isize)
    };
    if original_ptr == 0 {
        return; // 실패 — 서브클래싱 없이 그냥 포기(호버 개선만 못 받음).
    }
    // isize 함수 포인터 → 실제 함수 포인터. Win32 WNDPROC 은 항상 이 시그니처.
    let original_proc: RawWndProc = unsafe { std::mem::transmute(original_ptr) };
    reg.insert(
        hwnd_isize,
        WindowState {
            original_proc,
            max_rect: ClientRect { x, y, w, h },
        },
    );
}

/// 서브클래싱된 창들의 공통 진입점. WM_NCHITTEST 만 가로채고 나머지는
/// 원래 프로시저로 위임.
unsafe extern "system" fn wnd_proc_trampoline(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        let reg = registry().lock().ok();
        if let Some(reg) = reg
            && let Some(state) = reg.get(&(hwnd as isize))
        {
            // lParam 은 스크린 좌표(x = low word, y = high word, 부호 있음).
            let x = (lparam & 0xFFFF) as u16 as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
            let mut pt = POINT { x, y };
            // SAFETY: hwnd 는 살아있는 창(이 메시지를 받고 있으므로).
            if unsafe { ScreenToClient(hwnd, &mut pt) } != 0
                && state.max_rect.contains(pt.x, pt.y)
            {
                return HTMAXBUTTON as LRESULT;
            }
            let original = state.original_proc;
            drop(reg);
            return unsafe { CallWindowProcW(Some(original), hwnd, msg, wparam, lparam) };
        }
    }
    let original = {
        let reg = registry().lock().ok();
        reg.and_then(|r| r.get(&(hwnd as isize)).map(|s| s.original_proc))
    };
    match original {
        Some(orig) => unsafe { CallWindowProcW(Some(orig), hwnd, msg, wparam, lparam) },
        // 등록 안 된(레이스) 창 — DefWindowProc 대신 원본 프로시저를 못 찾은
        // 경우이므로 표준 기본 처리로 넘긴다.
        None => unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DefWindowProcW(
                hwnd, msg, wparam, lparam,
            )
        },
    }
}
