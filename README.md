# Obsidian Sync (v0.5.0) — Notion 스타일 마크다운 스튜디오 & 실시간 동기화

**Obsidian 볼트 호환 지식 베이스 + 네이버(NHN) TOAST UI Editor + 6종 클라우드/NAS 실시간 동기화**를 하나의 가벼운 Rust 앱(`ObsidianSync.exe`)으로 통합한 Windows · Android 프로젝트입니다.

노트를 편집하고 저장하면 몇 초 안에 내 저장소(Synology NAS, OneDrive, Google Drive, 네이버 클라우드 등)로 올라가고, 다른 기기에서 바꾼 내용은 자동으로 내려받습니다.
Obsidian 공식 제품이 아닌 개인 오픈소스 프로젝트입니다.

## 주요 기능 (v0.5.0)

### 1. Notion 감성의 워크스페이스 + TOAST UI Editor 탑재
- **위지윅(WYSIWYG) ↔ 마크다운 듀얼 모드**: NHN `@toast-ui/editor` 원본 탑재로, 숨겨진 문법 없이 상단 도구함에서 **표 생성·행/열 편집·셀 병합**, **굵게·기울임·밑줄·취소선**, **글자 색상/배경 형광펜**, **코드 구문 강조**, **콜아웃 박스**, **체크리스트**를 클릭 한 번으로 작성할 수 있습니다.
- **Notion 스타일 페이지 헤더 & 속성**: 페이지 아이콘(이모지), 커버 배너, YAML 프론트매터와 100% 호환되는 시각적 페이지 속성(태그, 날짜, 상태, 체크박스, 링크 등) 편집을 지원합니다.
- **다중 탭 · 우측 분할 보기 · 다크/라이트 테마 · 명령어 팔레트(`Ctrl+K`)** 지원.

### 2. Obsidian 볼트 완벽 호환 (순수 `.md` 로컬 파일)
- 기존 Obsidian 볼트 폴더를 변환 없이 그대로 열어 사용합니다.
- **`[[위키링크]]` 자동완성 & 백링크**: 연결된 언급(Linked Mentions)과 연결되지 않은 언급(Unlinked Mentions 원클릭 연결), 파일 이름 변경 시 볼트 전체 위키링크 자동 갱신을 지원합니다.
- **인터랙티브 그래프 뷰**: 전체 그래프 뷰 및 현재 문서 중심 로컬 그래프 뷰, 목차(Outline), 태그 탐색기, 전문 검색, 일일 노트(`Daily`), 템플릿(`Templates`), 이미지 붙여넣기(`attachments/`), `.trash` 휴지통 복원을 지원합니다.

### 3. 검증된 6종 저장소 실시간 동기화 (`obsidian-sync-core`)
**프로필**마다 `내 기기 폴더 ↔ 저장소 폴더`를 1:1로 연결하며, Obsidian 볼트뿐 아니라 사진·일반 문서 폴더 프로필도 여러 개 함께 운영할 수 있습니다.

| 저장소 | 연결 방법 | 필요한 것 |
| --- | --- | --- |
| **Synology NAS** | DSM File Station API (계정 로그인, 2단계 OTP 인증, 원격 폴더 탐색기 지원) | NAS 주소, 아이디, 비밀번호 |
| **폴더 / 공유 드라이브** | 폴더 지정 | NAS 공유폴더(`\\NAS\...`), 매핑 드라이브, 외장 디스크 |
| **OneDrive** | 브라우저 디바이스 코드 로그인 | Microsoft 계정 + 앱 Client ID (최초 1회 등록) |
| **Google Drive** | 브라우저 디바이스 코드 로그인 | Google 계정 + OAuth Client ID/Secret (최초 1회 등록) |
| **네이버 클라우드** Object Storage | S3 호환 | 버킷, Access Key, Secret Key |
| **Google Cloud Storage** | S3 호환 (HMAC) | 버킷, HMAC 키 |

원격에는 노트가 **일반 `.md` 파일 그대로** 저장됩니다. NAS나 드라이브, 모바일 Obsidian 앱에서 바로 열어볼 수 있습니다.
저장소별 자세한 설정은 [설정 안내](docs/SETUP.md)를 보세요.

## 다운로드

[Releases](https://github.com/bluewind201/obsidian-sync/releases/latest)에서 받으세요.

| 기기 | 파일 | 설치 |
| --- | --- | --- |
| Windows 10/11 | `ObsidianSync-windows-x64.zip` | 압축 해제 후 `ObsidianSync.exe` 단일 파일 실행 (설치 불필요, WebView2 내장 활용) |
| Android 11+ | `ObsidianSync-android.apk` | 파일을 열어 설치. 이후 앱 안에서 새 릴리즈 자동 업데이트 지원 |

## 동기화 설정 방법 (3분)

1. 좌측 사이드바 하단 또는 메뉴의 **🔄 동기화 센터 → 프로필 및 저장소 설정**을 엽니다. `＋ Obsidian 볼트` 또는 `＋ 일반 폴더 (사진 등)`로 프로필을 추가합니다.
2. **① 이름과 종류** → 프로필 이름과 종류를 정합니다.
3. **② 내 폴더** → 동기화할 로컬 폴더를 고릅니다.
4. **③ 저장소** → 카드 중 하나를 누르고 칸을 채웁니다. Synology는 **NAS에서 고르기…**로 원격 공유 폴더 목록에서 바로 고를 수 있습니다.
5. **④ 연결 확인** → `연결 테스트`. 2단계 인증을 쓰는 계정이면 OTP 코드를 한 번 입력합니다 (이후 이 PC를 기억).
6. **⑤ 동기화 방식** → 세 가지 방향 중 하나를 고릅니다.
   - *양방향*: 어느 쪽에서 바꿔도 맞춥니다. 충돌 시 **데이터 기준**(둘 다 보존 / 이 PC 기준 / 저장소 기준)을 고를 수 있습니다.
   - *올리기만 (백업)*: 사진 백업 등에 추천합니다 (내 기기에서 지워도 저장소에는 남음).
   - *내려받기만*: 저장소의 폴더를 이 기기로 받아 두기만 합니다.
7. **저장하고 동기화 시작** → 완료!

- **Windows**: 창을 닫아도 시스템 트레이(시계 옆)에서 계속 동기화합니다. `고급 설정`에서 *Windows 시작 시 자동 실행*을 켤 수 있습니다.
- **Android**: 알림 영역에 "Obsidian 실시간 동기화"가 떠 있는 동안 동기화하며, 재부팅 시 자동 시작됩니다.

## 안전장치

- **양쪽에서 같은 노트를 고친 경우**: 기본적으로 둘 다 남깁니다 (`노트 (충돌 기기이름 날짜).md`).
- **지워진 파일**: 볼트의 `.trash` 폴더로 안전하게 옮겨지며, 앱 내 휴지통에서 복원할 수 있습니다.
- **대용량 파일**: 사진·동영상은 메모리에 올리지 않고 스트리밍으로 주고받습니다.
- **대량 삭제 보호**: 원격이나 볼트가 갑자기 비었거나 한 번에 많은 파일이 지워질 상황이면 동기화를 보류하고 사용자 승인을 기다립니다.
- `.obsidian/workspace*.json`, `.trash`, `.git`, 임시 파일은 동기화하지 않으며 제외 패턴을 직접 추가할 수 있습니다.

## 개발 구조 및 빌드

```
crates/core          공통 동기화 엔진 (Rust) - 6종 저장소 연결, 3-way 비교, 실시간 파일 감시
crates/desktop       Windows 통합 앱 (Rust + Tao/Wry WebView2) - 볼트 엔진, 트레이, 자동 실행
crates/desktop/web   프론트엔드 소스 (TypeScript, Vite, Notion UI, @toast-ui/editor)
crates/android       Android 연결부 (Rust JNI)
android/             Android 앱 (Java) - 화면, 상주 서비스, 부팅 시 시작
```

```sh
# 프론트엔드 번들 빌드 (수정 시)
cd crates/desktop/web && npm install && npm run build

# 코어 / 데스크톱 테스트 및 Windows 단일 실행 파일 빌드
cargo test -p obsidian-sync-core -p obsidian-sync-android -p obsidian-sync-desktop
cargo build --release -p obsidian-sync-desktop
```

설계 설명은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)에 있습니다.
