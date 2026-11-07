# Obsidian Sync

Obsidian 볼트(노트 폴더)와 사진 같은 일반 폴더를 **내 저장소**와 실시간으로 동기화하는 Windows · Android 앱입니다.
노트를 저장하면 몇 초 안에 올라가고, 다른 기기에서 바꾼 내용은 자동으로 내려받습니다.
**프로필**마다 `내 기기 폴더 ↔ 저장소 폴더`를 1:1로 연결하며, 프로필은 여러 개 만들 수 있습니다 (예: Obsidian 볼트 + 사진 백업).
Obsidian 공식 제품이 아닌 개인 프로젝트입니다.

## 다운로드

[Releases](https://github.com/bluewind201/obsidian-sync/releases/latest)에서 받으세요. `main`에 반영될 때마다 자동으로 새 버전이 올라옵니다.

| 기기 | 파일 | 설치 |
| --- | --- | --- |
| Windows 10/11 | `ObsidianSync-windows-x64.zip` | 압축 해제 후 `ObsidianSync.exe` 실행 (설치 필요 없음) |
| Android 11+ | `ObsidianSync-android.apk` | 파일을 열어 설치. 새 버전은 기존 앱 위에 그대로 업데이트됩니다 |

## 연결할 수 있는 저장소

| 저장소 | 연결 방법 | 필요한 것 |
| --- | --- | --- |
| **Synology NAS** | DSM File Station API (계정 로그인, 2단계 인증 지원) | NAS 주소, 아이디, 비밀번호 |
| **폴더 / 공유 드라이브** | 폴더 지정 | NAS 공유폴더(`\\NAS\...`), 매핑 드라이브, 외장 디스크 |
| **OneDrive** | 브라우저 로그인 | Microsoft 계정 + 앱 Client ID (최초 1회 등록) |
| **Google Drive** | 브라우저 로그인 | Google 계정 + OAuth Client ID/Secret (최초 1회 등록) |
| **네이버 클라우드** Object Storage | S3 호환 | 버킷, Access Key, Secret Key |
| **Google Cloud Storage** | S3 호환 (HMAC) | 버킷, HMAC 키 |

원격에는 노트가 **일반 파일 그대로** 저장됩니다. NAS나 드라이브에서 `.md` 파일을 바로 열어볼 수 있습니다.
저장소별 자세한 설정은 [설정 안내](docs/SETUP.md)를 보세요.

## 사용법 (3분)

1. 앱을 열면 **설정** 화면이 나옵니다. `＋ Obsidian 볼트` 또는 `＋ 일반 폴더 (사진 등)`로 프로필을 추가합니다.
2. **① 이름과 종류** → 프로필 이름(예: `사진`)과 종류를 정합니다.
3. **② 내 폴더** → 동기화할 폴더를 고릅니다.
4. **③ 저장소** → 카드 중 하나를 누르고 칸을 채웁니다. Synology는 NAS 주소, 계정, NAS 폴더(예: `/photo/휴대폰`)를 적습니다.
5. **④ 연결 확인** → `연결 테스트`. 2단계 인증을 쓰는 계정이면 OTP 코드를 한 번 입력합니다 (이후 기기를 기억).
6. **⑤ 동기화 방식** → Synology Cloud Sync와 같은 세 가지 방향 중 하나를 고릅니다.
   - *양방향*: 어느 쪽에서 바꿔도 맞춥니다. 양쪽이 모두 바뀌었을 때의 **데이터 기준**(둘 다 보존 / 내 기기 기준 / 저장소 기준)을 고를 수 있습니다.
   - *올리기만 (백업)*: 사진은 이것을 추천합니다 (휴대폰에서 지워도 NAS에는 남음).
   - *내려받기만*: NAS의 폴더를 이 기기로 받아 두기만 합니다. 이 기기에서 바꾼 내용은 올리지 않습니다.
7. **저장하고 시작** → 끝. 이후에는 **홈** 화면에서 프로필별 상태를 봅니다.

다른 기기에서도 **같은 저장소**를 연결하면 노트가 맞춰집니다. 두 번째 기기는 빈 폴더로 시작해도 됩니다.

- **Windows**: 창을 닫아도 트레이(시계 옆)에서 계속 동기화합니다. `설정 → 고급 설정`에서 *Windows 시작 시 자동 실행*을 켜 두세요.
- **Android**: 알림 영역에 "Obsidian 실시간 동기화"가 떠 있는 동안 동기화합니다. 휴대폰을 껐다 켜도 자동으로 다시 시작합니다.
  배터리 최적화가 강한 기기는 설정에서 이 앱의 배터리 사용을 *제한 없음*으로 바꿔 주세요.

## 안전장치

- **양쪽에서 같은 노트를 고친 경우**: 둘 다 남깁니다. 내 쪽 내용은 `노트 (충돌 기기이름 날짜).md`로 보존됩니다.
- **지워진 파일**: 다른 기기에서 지운 파일은 볼트의 `.trash` 폴더로 옮겨집니다 (Obsidian 휴지통과 같은 위치). Google Drive에서는 Drive 휴지통으로 갑니다.
- **대용량 파일**: 사진·동영상은 메모리에 올리지 않고 스트리밍으로 주고받습니다 (Synology, 폴더 저장소).
- **실수 방지**: 원격이나 볼트가 갑자기 비었거나 한 번에 많은 파일이 지워질 상황이면 동기화를 멈추고 알려줍니다.
- `.obsidian/workspace*.json`, `.trash`, `.git`, 임시 파일은 동기화하지 않습니다. 원하면 제외 목록을 추가할 수 있습니다.
- 처음 사용할 때는 볼트를 한 번 백업해 두세요.

## 개발

```
crates/core      공통 동기화 엔진 (Rust) - 저장소 연결, 3-way 비교, 파일 감시
crates/desktop   Windows 앱 (Rust, egui) - 설정 화면, 트레이, 자동 실행
crates/android   Android 연결부 (Rust JNI)
android/         Android 앱 (Java) - 화면, 상주 서비스, 부팅 시 시작
```

```sh
cargo test -p obsidian-sync-core -p obsidian-sync-android
cargo build --release -p obsidian-sync-desktop        # Windows 앱
# Android: NDK 27, cargo-ndk 필요
cargo ndk -t arm64-v8a -t x86_64 -o android/app/src/main/jniLibs build --release -p obsidian-sync-android
cd android && gradle assembleRelease
```

GitHub Actions가 PR마다 테스트(Windows 빌드, Android 에뮬레이터 실행 포함)를 돌리고, `main`에 반영되면 Windows ZIP과 Android APK를 자동으로 릴리즈합니다.
설계 설명은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)에 있습니다.
