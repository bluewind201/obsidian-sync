# 저장소별 설정 안내

모든 기기에서 **같은 저장소, 같은 폴더**를 지정해야 노트가 서로 맞춰집니다.
볼트마다 다른 폴더를 쓰세요 (예: `ObsidianSync/개인`, `ObsidianSync/회사`).

## Synology NAS (WebDAV)

1. DSM → **패키지 센터**에서 *WebDAV Server* 설치 → 실행 후 **HTTPS 사용**(기본 포트 5006)을 켭니다.
2. 노트를 둘 공유폴더를 정합니다 (예: `home` 안의 `Obsidian` 폴더). 폴더가 없으면 앱이 만들어 줍니다.
3. 앱에 입력
   - WebDAV 주소: `https://NAS주소:5006/home/Obsidian`
     - 집 안에서만 쓰면 `https://192.168.0.10:5006/...`, 밖에서도 쓰려면 DDNS 주소(`https://내이름.synology.me:5006/...`)
   - NAS 아이디 / 비밀번호: 해당 폴더 읽기·쓰기 권한이 있는 계정
4. **연결 테스트**를 누릅니다.

> 인증서 오류가 나면 DSM → 보안 → 인증서에서 Let's Encrypt 인증서를 발급해 DDNS 주소로 접속하세요.
> 집 안 네트워크에서만 쓴다면 `http://NAS주소:5005/...`(HTTP)도 사용할 수 있지만 권장하지 않습니다.

## 폴더 / 공유 드라이브

Windows에서 NAS 공유폴더를 그대로 쓰거나, 다른 동기화 프로그램(Synology Drive Client, OneDrive 앱 등)이 관리하는 폴더를 지정합니다.

- 예: `\\NAS\home\Obsidian`, `Z:\Obsidian`, `D:\Backup\Obsidian`
- 볼트 폴더 **안쪽**이나 볼트를 **포함하는** 폴더는 지정하지 마세요.

## OneDrive

OneDrive는 Microsoft에 "앱 등록"을 한 번 해야 로그인 버튼을 쓸 수 있습니다 (무료, 5분).

1. https://entra.microsoft.com → **앱 등록** → *새 등록*
   - 이름: `Obsidian Sync` (아무거나)
   - 지원되는 계정 유형: **모든 조직 디렉터리의 계정 및 개인 Microsoft 계정**
   - 리디렉션 URI: 비워 둠
2. 만든 앱 → **인증** → *공용 클라이언트 흐름 허용* = **예** → 저장
3. **API 권한** → *Microsoft Graph* → *위임된 권한* → `Files.ReadWrite`, `offline_access` 추가
4. **개요**의 *애플리케이션(클라이언트) ID*를 복사해 앱의 **앱 Client ID**에 붙여넣기
5. **로그인** → 브라우저에서 표시된 코드 입력 → 계정 승인 → **연결 테스트**

노트는 OneDrive의 `ObsidianSync` 폴더(변경 가능)에 저장됩니다.
Windows에 OneDrive 앱이 이미 있다면 **폴더 / 공유 드라이브**로 OneDrive 폴더를 지정하는 방법도 있습니다.

## Google Drive

Google Cloud 콘솔에서 OAuth 클라이언트를 한 번 만들어야 합니다 (무료).

1. https://console.cloud.google.com → 새 프로젝트 생성
2. **API 및 서비스 → 라이브러리** → *Google Drive API* 사용 설정
3. **OAuth 동의 화면** → 외부 → 앱 이름/이메일 입력 → *테스트 사용자*에 본인 Gmail 추가
4. **사용자 인증 정보 → 사용자 인증 정보 만들기 → OAuth 클라이언트 ID**
   - 애플리케이션 유형: **TV 및 입력 제한 기기**
5. 나온 *클라이언트 ID*와 *클라이언트 보안 비밀*을 앱에 입력 → **로그인** → 코드 입력 → **연결 테스트**

- Windows와 Android에 **같은 Client ID**를 넣어야 서로의 파일이 보입니다 (보안상 이 앱이 만든 파일만 접근합니다).
- 노트는 내 드라이브의 `ObsidianSync` 폴더에 저장되며, 지운 파일은 Drive 휴지통으로 갑니다.
- 동의 화면이 "테스트" 상태이면 7일마다 다시 로그인해야 할 수 있습니다. 계속 쓰려면 *앱 게시*를 누르세요.

## 네이버 클라우드 / Google Cloud Storage

S3 호환 오브젝트 스토리지입니다. 앱에서 **네이버 클라우드** 또는 **Google Cloud** 버튼을 누르면 주소가 자동으로 채워집니다.

### 네이버 클라우드 플랫폼 Object Storage
1. 콘솔 → Object Storage → 버킷 생성
2. 마이페이지 → 인증키 관리 → API 인증키(Access Key / Secret Key) 확인
3. 앱 입력: 엔드포인트 `https://kr.object.ncloudstorage.com`, 리전 `kr-standard`, 버킷 이름, 키

> 네이버 MYBOX(개인 클라우드)는 공개 API가 없어 연결할 수 없습니다.

### Google Cloud Storage
1. 콘솔 → Cloud Storage → 버킷 생성
2. Cloud Storage → 설정 → **상호 운용성** → HMAC 키 생성
3. 앱 입력: 엔드포인트 `https://storage.googleapis.com`, 리전 `auto`, 버킷 이름, HMAC 키

## Android 참고

- 처음 실행 시 **모든 파일 접근 권한**을 허용해야 Obsidian 볼트 폴더를 읽고 쓸 수 있습니다.
- 볼트 폴더는 보통 `내장 저장공간/Documents/볼트이름` 입니다. **폴더 선택**으로 고르면 됩니다.
- 알림을 끄면 상태를 볼 수 없으니 알림은 허용해 주세요.
- 배터리 절약이 강한 기기(삼성 등)는 설정 → 애플리케이션 → Obsidian Sync → 배터리 → **제한 없음**.

## 문제가 생기면

| 메시지 | 확인할 것 |
| --- | --- |
| 아이디/비밀번호 또는 키가 맞지 않습니다 | 입력값, 계정 권한 |
| 주소나 버킷/폴더 이름을 찾을 수 없습니다 | 주소 오타, 버킷 이름 |
| 서버에 연결할 수 없습니다 | 인터넷, NAS 전원, 포트(5006), 외부 접속 설정 |
| 로그인이 만료되었습니다 | 설정 → 다시 로그인 |
| 원격 저장소가 비어 있습니다 / 많은 파일이 삭제될 예정 | 의도한 상황이면 `동기화 기록 초기화` 후 다시 시작 (파일은 지워지지 않고 양쪽이 합쳐집니다) |

설정 파일 위치: Windows `%APPDATA%\ObsidianSync\config.json` (exe 옆에 `portable.txt`를 두면 exe 폴더의 `data`에 저장).

## (관리자용) Android 서명 키 고정

APK를 항상 같은 키로 서명해야 새 버전이 기존 앱 위에 업데이트 설치됩니다. 한 번만 설정하면 됩니다.

```sh
keytool -genkeypair -keystore release.p12 -storetype PKCS12 -alias obsidiansync -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Obsidian Sync"
base64 -w0 release.p12   # Windows PowerShell: [Convert]::ToBase64String([IO.File]::ReadAllBytes("release.p12"))
```

GitHub 저장소 → Settings → Secrets and variables → Actions 에 추가:
`ANDROID_KEYSTORE_BASE64`(위 base64 결과), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`(`obsidiansync`).
키 파일은 저장소에 올리지 말고 안전한 곳에 보관하세요.
