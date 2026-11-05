# 연결과 복구

## 공통

각 보관함에는 고유한 원격 root를 사용하세요. 예: `/ObsidianSync/personal/`.
앱이 만든 `vault-sync-v1` 내부는 수정하거나 삭제하지 마세요. 원격 이력에는 모든 파일 버전이 남습니다.
연결 대상 변경은 자동 동기화를 끈 뒤 수행하세요. 새 연결은 기존 연결과 별도 상태로 관리합니다.
동기화 전에 원격 root를 직접 생성하세요. NAS 마운트가 끊기면 재연결한 뒤 실행하세요.

## Synology WebDAV

DSM에서 WebDAV Server를 활성화하고 HTTPS 주소를 사용합니다. 일반적인 HTTPS 포트는 5006입니다.
공인 또는 기기에서 신뢰하는 인증서가 필요합니다. 인증서 검증을 비활성화하는 옵션은 제공하지 않습니다.

- backend: `webdav`
- endpoint: `https://nas.example.com:5006`
- root: `/공유폴더/ObsidianSync/`
- username / password: 해당 폴더 읽기·쓰기 권한이 있는 별도 계정

## Google Cloud Storage

- backend: `s3`
- endpoint: `https://storage.googleapis.com`
- region: `auto`
- bucket: 생성한 버킷 이름
- root: `/ObsidianSync/personal/`
- access_key_id / secret_access_key: Cloud Storage HMAC 키

서비스 계정에 대상 버킷 객체 생성·읽기·목록 권한이 필요합니다. 객체 삭제 권한은 이 엔진의 일반 동기화에 사용하지 않습니다.
Google Drive와 Google Cloud Storage는 서로 다른 서비스입니다.

## 네이버 클라우드 플랫폼

- backend: `s3`
- endpoint: 사용 중인 Object Storage 리전의 HTTPS endpoint (예: `https://kr.object.ncloudstorage.com`)
- region: 해당 리전 (예: `kr-standard`)
- bucket / root / access_key_id / secret_access_key 설정

새 Ncloud Storage 상품이나 다른 리전은 계정 콘솔에서 제공하는 endpoint를 사용하세요. MYBOX 계정으로는 연결되지 않습니다.

## OneDrive / Google Drive

Windows OneDrive 클라이언트를 이미 사용한다면 `fs` 연결의 root를 OneDrive 안의 전용 폴더로 지정할 수 있습니다.
API 연결은 각각 `onedrive`, `gdrive`를 선택하고 `root`, `access_token`을 입력합니다.
토큰은 해당 계정 파일 읽기·쓰기 권한이 필요하며 만료되면 동기화가 중단됩니다.
Windows 고급 JSON 또는 Android 입력 필드에 `refresh_token`, `client_id`, `client_secret`을 추가할 수 있습니다.
refresh_token을 입력하면 access_token보다 우선합니다. OneDrive는 client_id가 필요하고 네이티브 앱에서는 client_secret을 비워둡니다. Google Drive의 refresh-token 방식은 client_id와 client_secret이 모두 필요합니다. 이 앱은 OAuth 앱 등록이나 브라우저 로그인, 토큰 발급을 대신하지 않습니다.
사용자가 관리하는 자격 증명만 사용하고 설정 JSON/토큰을 GitHub에 업로드하지 마세요.

## Android

Android 11(API 30) 이상, arm64 또는 x86_64 기기를 지원합니다.
Obsidian과 동일한 공유 저장소 폴더에 접근하기 위해 '모든 파일 접근' 권한을 사용합니다.
예: `/storage/emulated/0/Documents/Obsidian`. SAF URI(`content://...`)나 다른 앱의 전용 저장소는 지원하지 않습니다.
현재 APK는 GitHub 직접 설치용 프리뷰입니다. Google Play의 모든 파일 접근 정책 승인을 받은 앱이 아닙니다.

실시간 동기화는 앱에서 직접 시작해야 하며 부팅 후 자동으로 시작하지 않습니다.
Android 15 이상은 dataSync 서비스의 백그라운드 실행을 24시간당 총 6시간으로 제한합니다.
앱은 한 세션을 최대 5시간으로 제한하고 timeout 콜백에서도 중지합니다. 백그라운드 주기 동기화는 별도로 켤 수 있습니다.

## 충돌과 복구

충돌 시 모든 내용 버전을 `.sync-conflicts`에 저장하고 동일한 이벤트 정렬 규칙으로 주 파일을 선택합니다.
삭제와 수정이 충돌하면 수정본을 보존합니다. 충돌을 해결하려면 주 파일을 원하는 내용으로 수정하고 동기화하세요.
각 복구 파일 옆의 `.json`에서 `original_path`를 확인합니다. 복구 대상에 복사한 뒤 동기화하면 새 수정본으로 전파됩니다.
원격 blobs와 events는 삭제하지 않으므로 로컬 상태가 없어도 빈 보관함에 다시 다운로드할 수 있습니다.
삭제 이벤트가 최신인 파일의 옛 버전은 원격 events의 경로와 blob 해시를 통해 수동 복구할 수 있습니다. 자동 버전 탐색 UI는 아직 없습니다.

## 오류가 날 때

앱 로그는 인증 정보 유출을 막기 위해 공급자 오류 본문을 표시하지 않습니다.
보관함/원격 폴더 권한, HTTPS 인증서, 토큰 만료, 파일 크기, 비호환 파일명, 원격 이력 손상을 차례로 확인하세요.
네트워크 중단 시 다음 실행이 재시도합니다. 원격 이력 누락이나 손상을 삭제로 간주하지 않습니다.
자동 동기화를 중지해도 이미 시작한 한 차례 작업은 완료될 수 있습니다.
