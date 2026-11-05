# Obsidian Sync

Windows와 Android에서 Obsidian 보관함을 사용자가 선택한 저장소로 동기화합니다.
Rust 공통 엔진, Windows 네이티브 GUI와 트레이, Android JNI 앱으로 구성했습니다.
Obsidian 공식 제품이 아닌 독립 프로젝트입니다.

## 다운로드

[Releases](https://github.com/bluewind201/obsidian-sync/releases)에서 Windows x64 ZIP 또는 Android APK를 받으세요.
첫 릴리즈는 **0.1 프리뷰**입니다. 원본 보관함을 백업한 후 복사본으로 먼저 확인하세요.
Windows EXE는 코드 서명되지 않았습니다. Android APK는 테스트용 debug 서명이며 Play Store 배포용이 아닙니다.
CI 재빌드 시 Android 서명 키가 바뀔 수 있으므로 업데이트에 기존 앱 제거가 필요할 수 있습니다.

## 지원 연결

| 대상 | 연결 방법 |
| --- | --- |
| 로컬 폴더, SMB/NAS, Windows OneDrive 동기화 폴더 | 폴더 어댑터 |
| Synology | HTTPS WebDAV |
| Google Cloud Storage | S3 호환 XML API + HMAC 키 |
| 네이버 클라우드 플랫폼 Object Storage | S3 API |
| OneDrive | Microsoft Graph 토큰 / OpenDAL 어댑터 |
| Google Drive | Google Drive 토큰 / OpenDAL 어댑터 |

네이버 MYBOX는 네이버 클라우드 플랫폼 Object Storage와 다르며 지원하지 않습니다.
OneDrive/Google Drive 브라우저 OAuth 로그인 마법사는 포함되지 않았습니다. 직접 발급한 토큰을 입력하거나 Windows에서 이미 연결된 OneDrive 폴더를 이용하세요.
원격 저장소는 일반 Markdown 미러가 아닌 **앱 전용 변경 이력 저장소**입니다. 같은 원격 root를 사용하는 기기끼리 노트를 복원합니다.

## 동작과 안전장치

- Windows 파일 변경 감시 + 기본 15초 원격 폴링. 감시 실패 시 폴링으로 확인합니다.
- Android 사용자 시작 서비스에서 15초 폴링. 최대 5시간 후 종료하며 OS가 더 일찍 중단할 수 있습니다.
- Android WorkManager는 최소 15분 주기이며 절전 정책에 따라 지연됩니다. 24시간 실시간 실행을 보장하지 않습니다.
- SHA-256 콘텐츠 검증, 불변 변경 이벤트, 동시 수정본 보존, 로컬 작업 잠금.
- 기본적으로 삭제를 전파하지 않고 삭제한 파일을 원격 버전으로 복원합니다.
- 삭제 전파를 양쪽 기기에서 켜면 삭제를 전파합니다. 한 번에 10개 초과 또는 10개 이상 보관함의 20% 초과 삭제는 차단합니다.
- 덮어쓰기 전 파일은 `.sync-backups`, 충돌본은 `.sync-conflicts`에 저장합니다. 함께 생성된 JSON에 원래 경로가 있습니다.
- `.git`, `.trash`, 복구 폴더, `.obsidian/workspace*`, `*.tmp`는 제외합니다. 나머지 `.obsidian` 설정은 동기화합니다.
- 경로 탈출, 심볼릭 링크, Windows 예약 이름, 대소문자 경로 충돌을 거부합니다.
- 단일 파일 최대 32 MiB, 원격 이력 항목 최대 100,000개. 이력 자동 정리는 아직 없습니다.
- 암호화는 로컬 자격 증명 저장에 적용합니다. 노트의 종단간 암호화는 제공하지 않습니다.

## 시작하기

1. 두 기기에 동일한 보관함 전용 원격 root를 설정합니다. 로컬 보관함과 원격 폴더가 겹치면 안 됩니다.
2. 첫 기기에서 보관함을 선택하고 동기화합니다.
3. 두 번째 기기에서 빈 보관함 폴더를 만들고 같은 원격 root를 연결합니다.
4. 수동 동기화 후 결과를 확인하고 자동 동기화를 켭니다.

자세한 공급자 설정, 복구 절차와 제한은 [설정 안내](docs/SETUP.md), 설계 및 조사 근거는 [설계 문서](docs/ARCHITECTURE.md)를 보세요.

## 빌드

Rust stable 및 Windows MSVC C++ Build Tools가 필요합니다.

```sh
cargo test -p vault-sync-core
cargo build --release -p obsidian-sync
```

Android: JDK 17, Gradle 8.11.1, Android SDK 35, NDK 27.2.12479018 및 cargo-ndk가 필요합니다.

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked
cargo ndk -t arm64-v8a -t x86_64 -o android/app/src/main/jniLibs build --release -p vault-sync-android
cd android
gradle assembleDebug lintDebug
```

GitHub Actions는 main/PR에서 테스트와 Windows/Android 빌드를 실행합니다.
`v*` 태그를 푸시하면 모든 작업 성공 후 ZIP, APK, SHA256SUMS를 GitHub 프리릴리즈에 게시합니다.
실제 클라우드 계정과 Android 실기기에서의 동작은 별도 검증이 필요합니다.
