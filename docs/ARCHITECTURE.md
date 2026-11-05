# 설계와 조사

## 구성

- `crates/core`: Rust 동기화 엔진, OpenDAL 저장소 추상화, SHA-256, 로컬 상태와 잠금.
- `crates/desktop`: Rust eframe GUI, notify 파일 감시, Windows 트레이, Credential Manager.
- `crates/android`: Rust JNI 진입점. Java UI와 Android 서비스는 운영체제 연동만 담당합니다.
- `android`: Android Keystore 암호화 설정, dataSync foreground service, WorkManager.
- `.github/workflows/build.yml`: 코어 테스트, Windows ZIP, Android APK, 태그 기반 프리릴리즈.

## 프로토콜 v1

원격에 `vault-sync-v1/blobs/<sha256>`와 `vault-sync-v1/events/<uuid>.json`을 저장합니다.
이벤트는 경로, 부모 이벤트 목록, 파일 해시 또는 삭제(null)를 가집니다.
내용을 먼저 업로드하고 이벤트를 마지막에 게시합니다. 다른 기기의 이벤트를 덮어쓰지 않습니다.
경로별로 부모로 참조되지 않은 이벤트가 최신 head입니다. 기기의 기준 상태와 다른 로컬 파일만 새 이벤트로 게시합니다.
동시 편집은 동일한 부모에서 갈라진 복수 head로 탐지합니다. 수정 head를 삭제보다 우선하고 UUID 문자열 순서로 주 파일을 결정합니다.
이는 시계 기반 last-write-wins가 아닙니다. 모든 충돌 내용은 복구 폴더와 원격 blob에 보존됩니다.
다음 사용자 편집은 관측한 모든 head를 부모로 하여 충돌을 해소합니다.

## 안전성과 한계

원격 목록/이벤트/체크섬 오류는 실패로 처리합니다. 기존 상태의 head나 이벤트의 부모가 목록에 없으면 부분 목록으로 판단해 중단합니다.
로컬 기준 상태는 저장소 식별자와 보관함 경로로 분리합니다. 설정의 비밀번호/토큰 변경은 별도 저장소로 취급하지 않습니다.
파일 쓰기는 같은 디렉터리 임시 파일과 rename을 사용하고 쓰기 직전 해시를 다시 검사합니다.
외부 Obsidian 편집기와 공유 트랜잭션이 없으므로 마지막 검사와 파일 교체 사이의 극히 짧은 경쟁 구간은 완전히 제거하지 못합니다.
파일 접근 권한을 가진 악성 로컬 프로세스와 동시 디렉터리 교체 공격에 대한 보안 경계를 제공하지 않습니다.
원격 접근자는 노트 내용을 읽고 이력을 조작할 수 있습니다. E2EE/서명된 이벤트는 후속 과제입니다.
불변 이력은 저장 공간과 목록 조회 비용이 계속 증가합니다. 프리뷰에는 GC/압축과 대형 파일 스트리밍이 없습니다.
계정 토큰을 바꾸면서 같은 root를 지정할 때 다른 계정으로 바뀌었는지는 판별할 수 없지만, 기존 head 누락 검사로 잘못된 원격 재사용을 차단합니다.

## 조사 근거

- [Apache OpenDAL](https://opendal.apache.org/docs/rust/opendal/index.html): 공통 저장소 API, WebDAV/S3/OneDrive/Google Drive 어댑터.
- [GCS interoperability](https://docs.cloud.google.com/storage/docs/interoperability): S3 호환 XML API와 HMAC 인증.
- [NAVER Ncloud Storage](https://api.ncloud-docs.com/docs/en/storage-ncloudstorage): S3 호환 API. MYBOX와 별도 서비스.
- [Android foreground service timeout](https://developer.android.com/develop/background-work/services/fgs/timeout): Android 15 dataSync 시간 제한.

## 검증 구분

자동 테스트는 로컬 폴더를 원격으로 사용해 실제 파일 입출력, 다중 기기, 충돌, 삭제, 손상, 경로 검증을 수행합니다.
클라우드 어댑터는 라이브러리 기반 구현 및 컴파일 검증 대상입니다. 개인 계정/네트워크/실기기 검증 결과를 대신하지 않습니다.
프리뷰 APK는 일회성 debug 키를 사용합니다. 정식 릴리즈 전에 소유자가 관리하는 영구 서명 키와 Windows 코드 서명을 설정해야 합니다.
