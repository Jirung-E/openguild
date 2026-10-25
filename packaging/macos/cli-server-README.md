# openguild CLI / Server for macOS (Apple Silicon)

This archive contains prebuilt arm64 binaries. Rust and Cargo are not required.

## Install

```bash
sudo install -d /usr/local/bin
sudo install -m 755 openguild openguild-server /usr/local/bin/
```

If macOS blocks a binary downloaded from the internet, clear its quarantine
attribute and try again:

```bash
sudo xattr -d com.apple.quarantine /usr/local/bin/openguild
sudo xattr -d com.apple.quarantine /usr/local/bin/openguild-server
```

Verify the installation:

```bash
openguild --version
openguild-server --version
```

Run the server from a guild directory. It listens only on localhost by default:

```bash
cd /path/to/guild
openguild-server host
```

## Uninstall

```bash
./uninstall.sh              # the CLI and server only
./uninstall.sh --app        # + the desktop app
./uninstall.sh --settings   # + this machine's settings and app data
```

It lists what it will delete and asks first. Guild folders and
`~/.openguild/plugin-data` (files your plugins wrote) are never touched.

## 설치 (한국어)

이 압축 파일에는 미리 빌드된 Apple Silicon용 실행 파일이 들어 있습니다.
Rust와 Cargo는 필요하지 않습니다.

```bash
sudo install -d /usr/local/bin
sudo install -m 755 openguild openguild-server /usr/local/bin/
```

인터넷에서 받은 실행 파일을 macOS가 차단하면 격리 속성을 지운 뒤 다시
실행합니다.

```bash
sudo xattr -d com.apple.quarantine /usr/local/bin/openguild
sudo xattr -d com.apple.quarantine /usr/local/bin/openguild-server
```

```bash
openguild --version
openguild-server --version
```

서버는 길드 폴더에서 실행합니다. 기본값은 로컬호스트에만 바인딩됩니다.

```bash
cd /path/to/guild
openguild-server host
```

## 지우기

```bash
./uninstall.sh              # CLI · 서버만
./uninstall.sh --app        # + 데스크톱 앱
./uninstall.sh --settings   # + 이 기계의 설정 · 앱 데이터
```

지울 것을 먼저 보여 주고 묻습니다. 길드 폴더와 `~/.openguild/plugin-data`(플러그인이 남긴
기록)는 건드리지 않습니다.
