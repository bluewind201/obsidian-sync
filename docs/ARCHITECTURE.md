# 설계

## 구성

| 경로 | 역할 |
| --- | --- |
| `crates/core` | 동기화 엔진. 설정(`config`: 프로필 목록), 저장소 플러그인(`backend/*`), OAuth 디바이스 로그인(`auth`), 3-way 비교(`engine`), 프로필별 파일 감시 + 주기 동기화 서비스(`service`) |
| `crates/desktop` | Windows 앱 (egui). `ui.rs` 테마/위젯, `platform.rs` 트레이·자동 실행·단일 실행 |
| `crates/android` | JNI 함수 (`io.bluewind.obsidiansync.Native`). JSON 문자열로 주고받음 |
| `android/` | Java 화면(`MainActivity`), 상주 서비스(`SyncService`), 예비 주기 작업(`SyncWorker`), 부팅 시 시작(`BootReceiver`), 설정 암호화(`ConfigStore`, Android Keystore) |

## 저장소 플러그인

`backend::Backend` 트레이트(`list / download / upload / delete`)만 구현하면 새 저장소를 붙일 수 있습니다.
대용량 파일용 `upload_file / download_to`(스트리밍)는 기본 구현이 있고, 저장소별로 재정의합니다 (Synology, 폴더).
경로는 프로필 폴더 기준 상대경로(`/` 구분), `rev`는 내용이 바뀌면 바뀌는 값(ETag, md5, cTag 등)입니다.

| 저장소 | 구현 | rev |
| --- | --- | --- |
| Synology | File Station API: `SYNO.API.Auth`(OTP·기기 토큰), `List`(폴더별 페이지 조회), `Upload`(multipart 스트리밍), `Download`, `Delete`. `SYNO.API.Info`로 경로/버전 확인 | 크기+수정시각 |
| 폴더 | 파일 복사 (스트리밍) | 크기+수정시각 |
| S3 호환 | AWS Signature V4 직접 구현, ListObjectsV2 | ETag |
| OneDrive | Graph API 경로 주소, 4MB 초과 시 업로드 세션 | cTag |
| Google Drive | Drive v3, 폴더 ID 맵, 5MB 초과 시 resumable | md5Checksum |

HTTP는 `ureq` + rustls(webpki 루트 내장)라 Windows/Android 모두 시스템 인증서 설정 없이 동작합니다.

## 설정과 프로필

`AppConfig { profiles: [Profile], poll_interval_secs, debounce_ms, watch_local, start_on_launch, device_name }`.
`Profile`은 `id, name, enabled, kind(obsidian|folder), local_path, backend, mode(two_way|upload_only), propagate_deletes, sync_obsidian_folder, ignore_patterns`.
이전 형식(`vault_path` + `backend`, WebDAV)은 `AppConfig::from_json`이 읽을 때 프로필 1개로 자동 변환합니다.
`SyncService`는 켜진 프로필마다 작업 스레드와 파일 감시를 하나씩 둡니다. 동기화 기록은 `로컬 폴더 + 저장소 id`별 파일에 저장됩니다.

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
- **백업 모드**(`upload_only`)는 로컬 → 원격 업로드만 합니다. 원격 변경·삭제는 무시하고 원격 파일을 지우지 않습니다. 첫 동기화 때 원격에 같은 크기 파일이 있으면 업로드 없이 기록만 합니다.

## 테스트

- `crates/core/tests/sync.rs`: 두 기기 왕복, 충돌, 삭제/휴지통, 빈 원격 안전장치, 여러 프로필, 백업 모드, 대용량 파일 스트리밍
- `backend/synology.rs`: 가짜 DSM 서버로 OTP·기기 토큰 로그인, 페이지 목록, multipart 업로드, 다운로드, 세션 만료 재로그인, 삭제 검증
- S3 서명은 AWS 공식 예제 값으로 검증
- Android 에뮬레이터: JNI 왕복, 실시간 감시 업로드, 잘못된 설정 처리, 화면 실행
