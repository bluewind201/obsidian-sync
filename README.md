# Onyx Studio (v1.3.0) — 100% 순수 네이티브 Rust 스튜디오(Windows) · Android 스튜디오 & 실시간 동기화

**Windows 100% 순수 네이티브 Rust GUI(`eframe`/`egui` + `glow`, Zero WebView2 · Zero 로컬 포트)에서 WebView Notion 스튜디오와 1:1 동일한 디자인·구조를 구현하고, Android 고가독성 스튜디오 · 벡터 플로우차트 엔진 · A4 PDF/인쇄 · 3종 클러스터 그래프 · 실시간 동기화**를 **Windows(`OnyxStudio.exe`)와 Android(`OnyxStudio-android.apk`)**에서 하나의 통합 생태계로 제공합니다.

> 기존 `Obsidian Sync` (`v0.7.0` 이하)에서 **`Onyx Studio` (`v0.8.0+`)** 로 이름이 새롭게 변경되었습니다. 기존 설정과 동기화 상태는 첫 실행 시 자동으로 `OnyxStudio` 폴더로 마이그레이션되며, Android 앱(`io.bluewind.obsidiansync`)도 앱 내 업데이트로 바로 이어집니다.

노트를 편집하고 저장하면 몇 초 안에 내 저장소(Synology NAS, OneDrive, Google Drive, 네이버 클라우드 등)로 올라가고, 다른 기기에서 바꾼 내용은 자동으로 내려받습니다.
Obsidian 공식 제품이 아닌 개인 오픈소스 프로젝트입니다.

## 주요 기능 (v1.3.0)

### 0. v1.3.0 핵심 개선 사항 (WebView 1:1 노션 스튜디오 순수 네이티브 Rust UI 통합)
- **WebView Notion 스튜디오 1:1 동일 순수 네이티브 Rust UI (`eframe`/`egui` + `glow`, Zero `WebView2Loader.dll`)**:
  - 좌측 Notion `#F7F7F5` 사이드바(`[O]` 블루 아바타 `#2383E2`, 일일노트 `⚙️` 폴더 설정, 드래그 앤 드롭 폴더/루트 이동, 틴트 커스텀 벡터 폴더 아이콘, `⊞/⊟` 전체 접기/펼치기, 태그 기본 숨김).
  - 상단 3단 크롬(탭 바 38px + 브레드크럼/3모드 세그먼트 `[👁️ 뷰어 | 위지윅 모드 | 마크다운 분할]` 44px + 50% 슬림 고밀도 도킹 툴바 8개 그룹 & 찾기/바꾸기 바).
  - `#FFFFFF` A4 용지/전체폭 스테이지, 상단 룰러 배지, 좌우 여백 희미한 경계 가이드(`guide_line`), 테두리 없는 28px 노션 제목, **단일 문서 인플레이스 블록 위지윅 편집기**, `#CBD5E1` 테두리 직접 클릭 편집 표, `10pt`(`13.33px`) 균일 본문 가독성.
- **고화질 벡터 플로우차트 엔진 & 비주얼 빌더 (`flowchart.rs`)**:
  - Mermaid `flowchart TB/TD/BT/LR/RL`, `graph`, `sequenceDiagram`, `mindmap`을 Sugiyama 계층 레이아웃 · 5종 테마 · 곡선/직선/직각 라우팅 · 독립 SVG 출력으로 선명하게 렌더링하며, 인터랙티브 **플로우차트 빌더 모달**을 지원합니다.
- **선명한 표 · A4 인쇄 및 PDF 내보내기 (`print_pdf.rs`)**:
  - `WebView2Loader.dll` 없이 A4 여백/방향, `#CBD5E1`/`#94A3B8` 표 테두리, 인라인 SVG 플로우차트, 첨부 이미지를 1:1 유지하여 A4 인쇄 및 PDF 저장(브라우저 인쇄 엔진 + 순수 Rust `%PDF-1.4` 폴백)을 수행합니다.
- **3종 레이아웃 클러스터 그래프 (`Cluster` / `Radial` / `Hierarchy`) & 동기화 센터 개선**:
  - 폴더 클러스터 후광·배지, 홉 동심원, 방향성 계층 뷰, 오른쪽 선택 노드 인스펙터 카드를 제공하며, 동기화 센터는 **Obsidian 볼트 전용 연결 해제(`🔌 연결 해제`) · 폴더 변경 초기화 · 1클릭 클라우드 드라이브(Google Drive/OneDrive/Synology Drive/Dropbox/iCloud) 바로 연결**을 지원합니다.
- **v1.2.0 기반 유지**: `WebView2Loader.dll` 영구 제거(로컬 포트 0개), 외부 웹/워드 복사 시 HTML 태그·제로폭 공백 차단 및 이미지 보존 클린 붙여넣기(`win_clipboard.rs` / `pasteClean.ts`).

### 0-1. v1.1.0 주요 개선 사항
- **스크롤 위치 고정 (위로 튀는 현상 완전 해결)**: 긴 노트 편집·툴바 클릭·자동 저장·동기화 센터 폴링·오른쪽 인스펙터 갱신 중에도 스크롤 위치가 맨 위로 되돌아가지 않습니다.
- **깔끔한 시작 사이드바**: 앱 시작 및 볼트 전환 시 모든 폴더를 기본적으로 접은 상태로 표시하며, 상단 `⊞/⊟` 버튼으로 전체 폴더를 한 번에 펼치거나 접을 수 있습니다.
- **폴더별 아이콘 · 색상 커스터마이징**: 폴더 우클릭(`⋯`) → **🎨 아이콘 · 색상 변경…**에서 커스텀 이모지와 13종 팔레트/직접 색상 선택을 지정할 수 있으며 `.obsidian/onyx-folder-styles.json`으로 기기 간 동기화됩니다.
- **PC 재시작 시 자동 실행 (동기화 자동 시작)**: 동기화 센터 홈·트레이 메뉴·`Ctrl+K`에서 1클릭으로 Windows 자동 시작(`--minimized`)을 설정하고 구버전 경로도 자동 정리합니다.
- **GitHub 자동 업데이트 (Windows & Android 통합)**: 새 GitHub 릴리즈가 출시되면 백그라운드에서 자동 확인·SHA-256 검증 다운로드하며, 상단 바 및 동기화 센터 홈에서 1클릭 재시작 업데이트를 지원합니다.

### 1. Windows · Android 동일 스튜디오 경험 (편집 + 동기화 올인원)
- **Android 앱 전면 통합**: 기존 동기화 설정 전용 UI에서 벗어나, Android에서도 Windows와 동일한 **Notion 스타일 워크스페이스 · 50% 슬림 확장 TOAST UI 에디터(위지윅/마크다운 분할) · 그래프 뷰(터치/핀치 줌) · 암호화 · PDF 인쇄 출력 · 실시간 동기화 센터**를 그대로 사용할 수 있습니다.
- **모바일·태블릿 반응형 레이아웃**: 스마트폰 화면에서는 100% 가독성 편집 캔버스와 슬라이드 서랍 사이드바/인스펙터, 하드웨어 뒤로 가기 제스처, Android 저장소 폴더 선택기, 시스템 인쇄/PDF 내보내기를 네이티브로 지원합니다.

### 2. Notion 감성 워크스페이스 + 2배 이상 확장된 TOAST UI Editor (초슬림 도구함)
- **아이콘 50% 축소 컴팩트 도구함**: 상단 툴바 아이콘을 기존 대비 약 50% 크기로 정밀 축소·고밀도 배치하여, 작은 노트 화면에서도 **50여 종의 편집 도구(80+ 액션)** 를 한눈에 사용할 수 있습니다.
- **위지윅(WYSIWYG) ↔ 마크다운 듀얼 모드**:
  - **히스토리 & 찾기/바꾸기**: 실행 취소(`Ctrl+Z`) / 다시 실행(`Ctrl+Y`), 본문 내 찾기·바꾸기 바(`Ctrl+F` / `Ctrl+H`)
  - **제목 · 글자 · 서식**: `H1~H6` 빠른 제목, 글자 크기(`11~32px`), 굵게, 기울임, **밑줄**, 취소선, 위첨자(`x²`), 아래첨자(`H₂O`), 키보드 키(`Ctrl`), 서식 지우기
  - **색상 & 정렬**: 16색 글자 색상 팔레트, 6색 배경 형광펜 + `==마크다운 형광펜==`, 문단 정렬(왼쪽/가운데/오른쪽/양쪽)
  - **블록 & 레이아웃**: 12종 Notion 스타일 콜아웃(`Note/Tip/Info/Warning/Danger` 등), 접기/펼치기 토글(`<details>`), **2단/3단 다단 컬럼**, 명시적 페이지 나누기, 자동 목차(`TOC`), 인용구, 목록, 체크리스트, 들여쓰기/내어쓰기
  - **표 · 미디어 · 전문 요소**: 표 생성·행/열 편집·셀 병합, 링크, `[[위키링크]]`, 각주(`[^1]`), 이미지 및 **일반 파일 첨부(`📎` — PDF/Office/ZIP 등 볼트 `attachments/` 저장)**, 인라인/블록 코드 하이라이트, LaTeX 수식 및 28종 수학·그리스 기호, Mermaid 다이어그램
  - **생산성 유틸리티**: 날짜/시간/D-Day 삽입, 상태 배지(`진행중/완료/보류/긴급`), 진행률 바, 한글 사무용 기호(`※ ▶ ● ★ ☐ ☑`), 대소문자·공백 변환, 업무용 템플릿, 마크다운 복사, 확대/축소, 스크롤 동기화

### 3. 태그 기본 숨김 & 필요 시 토글 보기
- 평소에는 깔끔한 문서 집중 화면을 위해 사이드바 태그 클라우드와 노트 상단 `tags` 속성 행이 **기본으로 숨겨져** 있습니다.
- 필요할 때 상단 툴바 `🏷️`, 노트 속성 `🏷️ 태그 보기`, 사이드바 `보기/숨기기`, `Ctrl+K`로 즉시 켜고 끌 수 있습니다.

### 4. ISO A4 용지 작성 캔버스 & 1:1 인쇄 / PDF 내보내기 (사용자 지정 여백)
- **A4 용지 기준 작성 화면**: ISO A4(`210×297mm` 세로 / `297×210mm` 가로) 용지 비율 위에 작성하며, 화면 자동 배율 맞춤 및 `297mm` 페이지 구분 가이드를 제공합니다. (스마트폰 화면에서는 터치 편집 가독성을 위해 자동 100% 폭으로 표시되고, 인쇄/PDF 출력 시 A4 설정이 그대로 적용됩니다.)
- **사용자 지정 여백**: 기본 여백 **상단 `2.0 cm` · 하단 `1.0 cm` · 좌우 `3.0 cm`** (`0.0 ~ 6.0 cm` 커스텀)으로 화면·인쇄·PDF가 일치합니다.
- **네이티브 인쇄 & PDF 출력**: Windows는 `Ctrl+P` 및 순수 Rust HTML/SVG·`%PDF-1.4` 인쇄 엔진(`print_pdf.rs`, Zero WebView2), Android는 시스템 인쇄/`PDF로 저장`과 직접 연결됩니다.

### 5. 볼트별 일일 문서(`Daily`) 저장 폴더 설정
- 일일 문서(`Ctrl+D`) 옆 **`⚙️` 설정 버튼**에서 오늘의 일일 노트가 생성될 폴더를 **볼트별로 개별 지정**하거나 새 폴더(`일일업무` 등)를 즉시 생성할 수 있습니다.

### 6. 개별 문서 및 전체 볼트 암호화 (`AES-256-GCM`)
- Windows와 Android 모두에서 `PBKDF2-HMAC-SHA256`(210,000회) + `AES-256-GCM` 인증 암호화를 지원하며, 표준 `.md` 아머 포맷으로 저장되어 6종 동기화와 100% 호환됩니다.

### 7. 검증된 6종 저장소 실시간 동기화 (`obsidian-sync-core`)
| 저장소 | 연결 방법 | 필요한 것 |
| --- | --- | --- |
| **Synology NAS** | DSM File Station API (계정 로그인, 2단계 OTP 인증, 원격 폴더 탐색기 지원) | NAS 주소, 아이디, 비밀번호 |
| **폴더 / 공유 드라이브** | 폴더 지정 | NAS 공유폴더(`\\NAS\...`), 매핑 드라이브, 휴대폰 저장소 |
| **OneDrive** | 브라우저 디바이스 코드 로그인 | Microsoft 계정 + 앱 Client ID |
| **Google Drive** | 브라우저 디바이스 코드 로그인 | Google 계정 + OAuth Client ID/Secret |
| **네이버 클라우드** Object Storage | S3 호환 | 버킷, Access Key, Secret Key |
| **Google Cloud Storage** | S3 호환 (HMAC) | 버킷, HMAC 키 |

저장소별 자세한 설정은 [설정 안내](docs/SETUP.md)를 보세요.

## 다운로드

[Releases](https://github.com/bluewind201/obsidian-sync/releases/latest)에서 받으세요.

| 기기 | 파일 | 설치 |
| --- | --- | --- |
| Windows 10/11 | `OnyxStudio-windows-x64.zip` / `OnyxStudio.exe` | 압축 해제 후 `OnyxStudio.exe` 단일 파일 실행 (설치·외부 DLL·WebView2 일절 불필요) |
| Android 11+ | `OnyxStudio-android.apk` | 파일을 열어 설치. Notion/TOAST UI 편집 스튜디오 + 상주 실시간 동기화 모두 포함, 앱 내 자동 업데이트 지원 |

## 안전장치
- **양쪽에서 같은 노트를 고친 경우**: 기본적으로 둘 다 남깁니다 (`노트 (충돌 기기이름 날짜).md`).
- **지워진 파일**: 볼트의 `.trash` 폴더로 안전하게 옮겨지며, 앱 내 휴지통에서 복원할 수 있습니다.
- **대용량 파일**: 사진·동영상은 메모리에 올리지 않고 스트리밍으로 주고받습니다.
- **대량 삭제 보호**: 원격이나 볼트가 갑자기 비었거나 한 번에 많은 파일이 지워질 상황이면 동기화를 보류하고 사용자 승인을 기다립니다.

## 개발 구조 및 빌드

```
crates/core          공통 동기화 엔진 (Rust) - 6종 저장소 연결, 3-way 비교, 실시간 파일 감시
crates/desktop       Windows 통합 앱 & 공통 스튜디오 백엔드 (Rust library + Win binary)
crates/desktop/web   공통 프론트엔드 (TypeScript, Vite, Notion UI, @toast-ui/editor, 반응형)
crates/android       Android JNI 연결부 (Rust) - 동기화 서비스 + 공통 스튜디오 API/에셋 브리지
android/             Android 앱 (Java) - WebView 호스트, SAF 선택기, PrintManager, 상주 서비스
```

```sh
# 1) 공통 프론트엔드 번들 빌드 (Windows & Android 공용)
cd crates/desktop/web && npm install && npm run build

# 2) 코어 / 데스크톱 / 안드로이드 Rust 테스트 및 Windows 단일 실행 파일 빌드
cargo test --offline --locked -p obsidian-sync-core -p obsidian-sync-desktop -p obsidian-sync-android
cargo build --offline --locked --release -p obsidian-sync-desktop
```

설계 설명은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)에 있습니다.
