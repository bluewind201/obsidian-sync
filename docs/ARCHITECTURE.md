# 설계

## 구성

| 경로 | 역할 |
| --- | --- |
| `crates/core` | 동기화 엔진. 설정(`config`), 저장소 플러그인(`backend/*`), OAuth 디바이스 로그인(`auth`), 3-way 비교(`engine`), 파일 감시 + 주기 동기화 서비스(`service`) |
| `crates/desktop` | Windows 앱 (egui). `ui.rs` 테마/위젯, `platform.rs` 트레이·자동 실행·단일 실행 |
| `crates/android` | JNI 함수 (`io.bluewind.obsidiansync.Native`). JSON 문자열로 주고받음 |
| `android/` | Java 화면(`MainActivity`), 상주 서비스(`SyncService`), 예비 주기 작업(`SyncWorker`), 부팅 시 시작(`BootReceiver`), 설정 암호화(`ConfigStore`, Android Keystore) |

## 저장소 플러그인

`backend::Backend` 트레이트(`list / download / upload / delete`)만 구현하면 새 저장소를 붙일 수 있습니다.
경로는 볼트 기준 상대경로(`/` 구분), `rev`는 내용이 바뀌면 바뀌는 값(ETag, md5, cTag 등)입니다.

| 저장소 | 구현 | rev |
| --- | --- | --- |
| 폴더 | 파일 복사 | 크기+수정시각 |
| WebDAV | PROPFIND(Depth 1 재귀), PUT, MKCOL, DELETE | ETag |
| S3 호환 | AWS Signature V4 직접 구현, ListObjectsV2 | ETag |
| OneDrive | Graph API 경로 주소, 4MB 초과 시 업로드 세션 | cTag |
| Google Drive | Drive v3, 폴더 ID 맵, 5MB 초과 시 resumable | md5Checksum |

HTTP는 `ureq` + rustls(webpki 루트 내장)라 Windows/Android 모두 시스템 인증서 설정 없이 동작합니다.

## 동기화 규칙 (3-way)

마지막 동기화 시점 상태(로컬 해시, 원격 rev)를 기기별로 저장하고 현재 로컬/원격과 비교합니다.

| 로컬 | 원격 | 동작 |
| --- | --- | --- |
| 바뀜 | 그대로 | 업로드 |
| 그대로 | 바뀜 | 다운로드 |
| 바뀜 | 바뀜 | 내용 비교 → 다르면 원격본을 원래 이름으로, 로컬본은 `(충돌 …)` 사본으로 보존 |
| 삭제 | 그대로 | 원격 삭제 (옵션) |
| 그대로 | 삭제 | 로컬을 `.trash`로 이동 (옵션) |
| 삭제 | 바뀜 / 바뀜 | 삭제 | 바뀐 쪽 우선 (복원) |

- 다운로드는 임시 파일 + rename. 스캔 이후 로컬이 바뀌었으면 덮어쓰지 않고 다음 회차로 미룹니다.
- 원격/볼트가 갑자기 비었거나, 기록된 파일의 절반 이상(20개 초과)이 삭제될 상황이면 중단합니다.
- 원격 rev가 형식만 바뀐 경우 내려받아 해시가 같으면 기록만 갱신합니다.

## 테스트

- `crates/core/tests/sync.rs`: 두 기기 왕복, 충돌, 삭제/휴지통, 빈 원격 안전장치
- S3 서명은 AWS 공식 예제 값으로 검증
- Android 에뮬레이터: JNI 왕복, 실시간 감시 업로드, 잘못된 설정 처리, 화면 실행
