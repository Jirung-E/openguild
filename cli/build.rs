// DEV-169: Windows exe 에 앱 아이콘(gui/icons/icon.ico) 임베드.
// Windows 외 플랫폼에서는 no-op. rc.exe / 리소스 컴파일러가 없는 환경에서도
// 빌드가 막히지 않도록 실패는 warning 으로만 남긴다.
fn main() {
    #[cfg(windows)]
    {
        let icon = "../gui/icons/icon.ico";
        println!("cargo:rerun-if-changed={icon}");
        let mut res = winresource::WindowsResource::new();
        res.set_icon(icon);
        if let Err(e) = res.compile() {
            println!("cargo:warning=openguild-cli icon embed skipped: {e}");
        }
    }
}
