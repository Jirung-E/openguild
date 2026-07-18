//! DEV-265 (Linux): 커스텀 타이틀바의 창 컨트롤 버튼을 손으로 그린
//! CSS 근사가 아니라 **실행 중인 시스템의 실제 GTK 아이콘 테마/버튼
//! 순서**를 조회해서 그린다.
//!
//! - 아이콘: `gtk::IconTheme` 로 `window-minimize-symbolic` 등 GTK 창
//!   데코레이션이 실제로 쓰는 심볼릭 아이콘 이름을 조회해 PNG data URL 로
//!   변환 — 어떤 아이콘 테마(Adwaita/Yaru/Breeze/Pop 등)든 이 이름들을
//!   구현하므로, 지금 실행 중인 테마 그대로 나온다.
//! - 순서: `gsettings get org.gnome.desktop.wm.preferences button-layout`
//!   파싱 — GNOME 계열이 아니거나 스키마가 없으면 조용히 실패, 프론트가
//!   기본 순서로 폴백.
//! - 간격/크기: 실제로 보이지 않는 `GtkOffscreenWindow` 안에 헤더바 버튼을
//!   만들어 레이아웃을 강제하고 그 결과 크기를 측정 — 특정 배포판 수치를
//!   추측/고정하지 않고 지금 시스템의 테마 CSS 가 계산한 값을 그대로 쓴다.
//!
//! GTK 호출은 GTK 메인 스레드에서만 안전(위젯이 `!Send`) 하므로 반드시
//! `app_handle.run_on_main_thread` 로 마샬링한다.
//!
//! 주의: 이 파일은 개발 환경(Windows)에서 컴파일 검증이 불가능했다 —
//! 실제 Linux 빌드에서 `gtk`/`gdk-pixbuf` 0.18 API 정합성 확인 필요
//! (Cargo.lock 에 이미 gtk 0.18.2 / gdk-pixbuf 0.18.5 가 wry/tao 의존성
//! 으로 잡혀있어 그 버전 기준으로 작성함).

#[cfg(target_os = "linux")]
use base64::Engine;
use serde::Serialize;

#[derive(Serialize, Clone, Default)]
pub struct NativeTitlebarStyle {
    #[serde(rename = "minIcon")]
    pub min_icon: Option<String>,
    #[serde(rename = "maxIcon")]
    pub max_icon: Option<String>,
    #[serde(rename = "restoreIcon")]
    pub restore_icon: Option<String>,
    #[serde(rename = "closeIcon")]
    pub close_icon: Option<String>,
    /// 창 컨트롤이 놓이는 쪽 — `"left"` | `"right"`. gsettings 의
    /// button-layout 에서 min/max/close 가 콜론 앞(왼쪽)에 있으면 "left".
    /// 조회 실패 시 "right"(가장 흔한 GNOME 기본).
    pub side: String,
    /// 왼쪽 → 오른쪽 순서. gsettings 조회 실패 시 GNOME 기본값.
    pub order: Vec<String>,
    #[serde(rename = "gapPx")]
    pub gap_px: Option<f64>,
    #[serde(rename = "buttonSizePx")]
    pub button_size_px: Option<f64>,
}

/// `gsettings get org.gnome.desktop.wm.preferences button-layout` 파싱.
/// button-layout 은 `"LEFT:RIGHT"` 형식(콜론 기준 왼쪽/오른쪽 배치):
///   - `':minimize,maximize,close'` → 오른쪽 배치(가장 흔함)
///   - `'close,minimize,maximize:'` → 왼쪽 배치(macOS 흉내 등)
/// min/max/close 가 들어있는 쪽을 창 컨트롤의 배치 쪽으로 보고, 그 순서를
/// 그대로 쓴다. 실패(비-GNOME 세션, 명령 없음 등)면 오른쪽·GNOME 기본순.
#[cfg(target_os = "linux")]
fn read_button_config() -> (&'static str, Vec<String>) {
    let fallback = (
        "right",
        vec!["minimize".to_string(), "maximize".to_string(), "close".to_string()],
    );
    let out = match std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return fallback,
    };
    let raw = String::from_utf8_lossy(&out.stdout);
    let trimmed = raw.trim().trim_matches('\'').trim_matches('"');

    let parse_side = |seg: &str| -> Vec<String> {
        seg.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| matches!(s.as_str(), "minimize" | "maximize" | "close"))
            .collect()
    };

    // 콜론 앞 = 왼쪽, 콜론 뒤 = 오른쪽. 콜론이 없으면 전체를 오른쪽으로.
    let (left_seg, right_seg) = match trimmed.split_once(':') {
        Some((l, r)) => (l, r),
        None => ("", trimmed),
    };
    let left_names = parse_side(left_seg);
    let right_names = parse_side(right_seg);

    // 창 컨트롤(min/max/close)이 있는 쪽을 배치 쪽으로. 양쪽에 다 있거나
    // (희귀) 아무 데도 없으면 오른쪽 폴백.
    if !left_names.is_empty() && right_names.is_empty() {
        ("left", left_names)
    } else if !right_names.is_empty() {
        ("right", right_names)
    } else {
        fallback
    }
}

/// 프론트가 쓰는 액션 키(`min`/`max`/`close`)로 변환.
#[cfg(target_os = "linux")]
fn to_action_key(name: &str) -> &'static str {
    match name {
        "minimize" => "min",
        "maximize" => "max",
        _ => "close",
    }
}

/// GTK 메인 스레드에서만 호출해야 한다. 아이콘 조회 + 헤더바 오프스크린
/// 측정. 어떤 단계든 실패하면 해당 필드만 None 으로 두고 계속 진행 —
/// 부분 실패를 전체 실패로 만들지 않는다.
#[cfg(target_os = "linux")]
fn build_style_on_gtk_thread() -> NativeTitlebarStyle {
    use gtk::prelude::*;

    let (side, order_raw) = read_button_config();
    let order = order_raw.iter().map(|n| to_action_key(n).to_string()).collect();

    // 아이콘: 심볼릭 SVG 파일 원문을 그대로 data URL 로 넘긴다(PNG 렌더
    // 아님). 프론트가 이를 CSS `mask-image` + `currentColor` 로 그려
    // 버튼 텍스트 색(= 앱 다크/라이트 테마)을 따라가게 한다 — 네이티브
    // GTK 가 심볼릭 아이콘을 테마 fg 로 recolor 하는 것과 동일한 결과.
    // (PNG 로 굽던 이전 방식은 색이 박혀 다크 테마에서 안 보였음.)
    let lookup_icon_svg_data_url = |icon_name: &str| -> Option<String> {
        let theme = gtk::IconTheme::default()?;
        let info = theme.lookup_icon(icon_name, 16, gtk::IconLookupFlags::empty())?;
        let path = info.filename()?;
        // 심볼릭 아이콘은 항상 .svg — 파일 원문을 그대로 읽는다.
        let bytes = std::fs::read(&path).ok()?;
        Some(format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    };

    let min_icon = lookup_icon_svg_data_url("window-minimize-symbolic");
    let max_icon = lookup_icon_svg_data_url("window-maximize-symbolic");
    let restore_icon = lookup_icon_svg_data_url("window-restore-symbolic")
        .or_else(|| lookup_icon_svg_data_url("window-unmaximize-symbolic"));
    let close_icon = lookup_icon_svg_data_url("window-close-symbolic");

    // 간격/버튼 크기: 화면에 안 보이는 GtkOffscreenWindow 안에 실제
    // `.titlebutton` 클래스가 붙은 버튼을 만들어 테마 CSS 가 계산한 자연
    // 크기를 읽는다 — 특정 배포판 수치를 하드코딩하지 않는다.
    //
    // 버튼 "크기": 아이콘을 넣은 titlebutton 의 preferred(자연) **가로**
    // 폭을 쓴다. 이전엔 HeaderBar 안 버튼의 allocation 에서 width.max(height)
    // 를 썼는데, HeaderBar 는 버튼 세로를 헤더바 높이(≈46)까지 늘려서
    // 그 값이 잡혀 원형이 과도하게 커졌다 — 실제 원형 지름은 가로 자연폭
    // (≈34). 그래서 세로 확장이 없는 단독 버튼의 preferred width 를 쓴다.
    //
    // 버튼 "간격": HeaderBar 안에 두 버튼을 넣었을 때의 실제 x 간격.
    let button_size_px = (|| -> Option<f64> {
        let offscreen = gtk::OffscreenWindow::new();
        let btn = gtk::Button::new();
        btn.style_context().add_class("titlebutton");
        let img = gtk::Image::from_icon_name(
            Some("window-close-symbolic"),
            gtk::IconSize::Menu,
        );
        btn.add(&img);
        offscreen.add(&btn);
        offscreen.show_all();
        while gtk::events_pending() {
            gtk::main_iteration();
        }
        let (_min, nat) = btn.preferred_size();
        if nat.width <= 0 {
            return None;
        }
        Some(nat.width as f64)
    })();

    let gap_px = (|| -> Option<f64> {
        let offscreen = gtk::OffscreenWindow::new();
        let header = gtk::HeaderBar::new();
        header.style_context().add_class("titlebar");
        let b1 = gtk::Button::new();
        b1.style_context().add_class("titlebutton");
        b1.style_context().add_class("minimize");
        let b2 = gtk::Button::new();
        b2.style_context().add_class("titlebutton");
        b2.style_context().add_class("close");
        header.pack_end(&b2);
        header.pack_end(&b1);
        offscreen.add(&header);
        offscreen.show_all();
        while gtk::events_pending() {
            gtk::main_iteration();
        }
        let alloc1 = b1.allocation();
        let alloc2 = b2.allocation();
        if alloc1.width() <= 0 || alloc2.width() <= 0 {
            return None;
        }
        Some((alloc2.x() - (alloc1.x() + alloc1.width())).abs() as f64)
    })();

    NativeTitlebarStyle {
        min_icon,
        max_icon,
        restore_icon,
        close_icon,
        side: side.to_string(),
        order,
        gap_px,
        button_size_px,
    }
}

#[cfg(target_os = "linux")]
pub fn get_native_titlebar_style_blocking(
    app: &tauri::AppHandle,
) -> Result<NativeTitlebarStyle, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let style = build_style_on_gtk_thread();
        let _ = tx.send(style);
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn get_native_titlebar_style_blocking(
    _app: &tauri::AppHandle,
) -> Result<NativeTitlebarStyle, String> {
    // 다른 플랫폼에선 이 command 자체가 호출되지 않지만(프론트가 리눅스
    // UA 일 때만 invoke), cfg 없이 항상 컴파일되어야 하므로 스텁 제공.
    Ok(NativeTitlebarStyle::default())
}

/// BUG-142: 리눅스 독바(Ubuntu Dock/GNOME Shell) 앱 아이콘 매칭.
///
/// 실기(Ubuntu/GNOME + NVIDIA) 검증 결과 이 데스크톱 환경은 창의
/// `_GTK_APPLICATION_ID` X11 프로퍼티가 (a) 존재하고 (b) 그 값과 일치하는
/// `<id>.desktop` 파일이 설치돼 있어야만 독바에 실제 앱 아이콘을 보여준다.
/// 없으면 `WM_CLASS`/`StartupWMClass` 매칭이 완전히 맞아도 일반 톱니바퀴
/// 아이콘으로 폴백 — WM_CLASS 매칭 경로는 이 환경에서 사실상 동작하지 않음
/// (여러 조합으로 재현/반증함).
///
/// Tauri 의 `app.enableGTKAppId` 설정은 이 프로퍼티에 `identifier`
/// (`io.openguild.desktop`) 를 그대로 쓰는데, tauri-bundler 의 리눅스
/// `.desktop` 파일명은 `productName`(`openguild`) 기준으로 고정 생성돼
/// (`{product_name}.desktop`) 서로 어긋난다 — `identifier` 를 바꾸면 기존
/// 베타 설치의 데이터 디렉토리/업데이터 식별자가 깨지므로 대신 여기서
/// 직접, 번들이 실제로 설치하는 이름(`openguild`)과 일치하는 값으로
/// `_GTK_APPLICATION_ID` 를 쓴다.
///
/// 타이밍이 중요 — GNOME Shell 은 창이 **처음 매핑(map)될 때** 앱을 한 번만
/// 식별하고 그 뒤로는 프로퍼티가 바뀌어도 재평가하지 않는 것으로 실측
/// 확인됨(이미 뜬 창에 `xprop -set` 으로 뒤늦게 넣어도 독바는 갱신 안 됨).
/// 그래서 이 창은 `tauri.linux.conf.json` 에서 `visible: false` 로 생성해
/// 아직 매핑 전 상태에서 프로퍼티를 먼저 쓰고, 그 다음에 호출자가 창을
/// `show()` 해야 한다.
#[cfg(target_os = "linux")]
pub fn set_gtk_application_id(gtk_window: &gtk::ApplicationWindow, app_id: &str) {
    use gtk::glib::Cast;
    use gtk::prelude::*;

    // 아직 realize 전이면(GdkWindow 없음) 강제로 realize — map 은 안 함.
    if !gtk_window.is_realized() {
        gtk_window.realize();
    }
    let Some(gdk_window) = gtk_window.window() else {
        eprintln!("[openguild-gui] warn: BUG-142 GdkWindow 없음 — _GTK_APPLICATION_ID 설정 skip");
        return;
    };
    let Ok(x11_window) = gdk_window.downcast::<gdkx11::X11Window>() else {
        eprintln!("[openguild-gui] warn: BUG-142 X11 세션 아님 — _GTK_APPLICATION_ID 설정 skip");
        return;
    };
    let xid = x11_window.xid();

    // SAFETY: 이 프로세스 전용의 새 Xlib 연결을 열어 프로퍼티 하나만 쓰고
    // 바로 닫는다 — GDK 의 기존 연결/이벤트 루프와 별개라 스레드/재진입
    // 문제 없음. xid 는 위에서 조회한 유효한 X11 윈도우 ID.
    unsafe {
        use x11::xlib;
        let display = xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            eprintln!("[openguild-gui] warn: BUG-142 XOpenDisplay 실패");
            return;
        }
        let prop_name = std::ffi::CString::new("_GTK_APPLICATION_ID").unwrap();
        let type_name = std::ffi::CString::new("UTF8_STRING").unwrap();
        let prop_atom = xlib::XInternAtom(display, prop_name.as_ptr(), 0);
        let type_atom = xlib::XInternAtom(display, type_name.as_ptr(), 0);
        xlib::XChangeProperty(
            display,
            xid,
            prop_atom,
            type_atom,
            8,
            xlib::PropModeReplace,
            app_id.as_ptr(),
            app_id.len() as i32,
        );
        xlib::XFlush(display);
        xlib::XCloseDisplay(display);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_gtk_application_id(_gtk_window: &(), _app_id: &str) {}
