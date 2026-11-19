param(
    [string]$VersionName = "",
    [switch]$SkipPublish,
    [switch]$NoBump
)
$ErrorActionPreference = 'Stop'
. "D:\BuildToolset\setenv.ps1"

if (Test-Path "C:\mingw64\bin") {
    $env:Path = "C:\mingw64\bin;$env:Path"
}
$env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-gnu"
$env:CARGO_TARGET_DIR = "D:\BuildToolset\tmp\onyx-target-gnu"

$RepoDir = "D:\Dev_project\obsidian-sync"
$GhWrapper = "D:\Dev_project\JUMPGATE\scripts\with-github.ps1"
$UseGhDirect = -not (Test-Path $GhWrapper)
$VcFile = "D:\BuildToolset\signing\version-codes.json"

function Invoke-Gh {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]]$GhArgs)
    if ($UseGhDirect) {
        & gh.exe @GhArgs
    } else {
        & $GhWrapper @GhArgs
    }
    if ($LASTEXITCODE -ne 0) {
        throw "gh command failed ($LASTEXITCODE): $($GhArgs -join ' ')"
    }
}

function Copy-PlainDirectory([string]$SourceDir, [string]$TargetDir, [string[]]$ExcludeDirs) {
    if (Test-Path -LiteralPath "\\?\$TargetDir") {
        & cmd.exe /c "rd /s /q `"\\?\$TargetDir`"" | Out-Null
        if (Test-Path -LiteralPath "\\?\$TargetDir") {
            $emptyDir = Join-Path ([System.IO.Path]::GetTempPath()) "empty_purge_$([Guid]::NewGuid().ToString('N'))"
            [System.IO.Directory]::CreateDirectory($emptyDir) | Out-Null
            & robocopy.exe $emptyDir $TargetDir /MIR /NFL /NDL /NJH /NJS /NC /NS /NP | Out-Null
            [System.IO.Directory]::Delete($emptyDir, $true)
            [System.IO.Directory]::Delete($TargetDir, $true)
        }
    }
    [System.IO.Directory]::CreateDirectory($TargetDir) | Out-Null
    $stack = New-Object 'System.Collections.Generic.Stack[string]'
    $stack.Push("")
    while ($stack.Count -gt 0) {
        $rel = $stack.Pop()
        $curSrc = if ($rel) { Join-Path $SourceDir $rel } else { $SourceDir }
        $curDst = if ($rel) { Join-Path $TargetDir $rel } else { $TargetDir }
        foreach ($dir in [System.IO.Directory]::EnumerateDirectories($curSrc)) {
            $name = [System.IO.Path]::GetFileName($dir)
            if ($ExcludeDirs -contains $name) { continue }
            $subRel = if ($rel) { Join-Path $rel $name } else { $name }
            [System.IO.Directory]::CreateDirectory((Join-Path $TargetDir $subRel)) | Out-Null
            $stack.Push($subRel)
        }
        foreach ($file in [System.IO.Directory]::EnumerateFiles($curSrc)) {
            $dstFile = Join-Path $curDst ([System.IO.Path]::GetFileName($file))
            [System.IO.File]::Copy($file, $dstFile, $true)
            [System.IO.File]::SetAttributes($dstFile, [System.IO.FileAttributes]::Normal)
        }
    }
}

if (-not $VersionName) {
    $cargoToml = Get-Content (Join-Path $RepoDir "Cargo.toml") -Raw
    if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
        $VersionName = $matches[1]
    } else {
        throw "Could not parse version from Cargo.toml"
    }
}

$vcObj = if (Test-Path $VcFile) { Get-Content $VcFile -Raw | ConvertFrom-Json } else { [pscustomobject]@{ SplitTesla = 150; KIDS_Photo = 232; ObsidianSync = 100 } }
if ($NoBump) {
    $nextVc = [Math]::Max([int]$vcObj.ObsidianSync, 100)
} else {
    $nextVc = [Math]::Max([int]$vcObj.ObsidianSync + 1, 101)
    $vcObj.ObsidianSync = $nextVc
    [IO.File]::WriteAllText($VcFile, ($vcObj | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
}
$env:BUILD_NUMBER = "$nextVc"
$env:VERSION_NAME = "$VersionName-b$nextVc"

Write-Host "[OnyxStudio] Building Windows x64 + Android APK on local PC (v$VersionName, versionCode=$nextVc)..." -ForegroundColor Cyan
Push-Location $RepoDir
try {
    # 1. Build Web Studio bundle -> crates/desktop/web-dist
    Write-Host "[1/6] Typechecking & building Web Studio frontend..." -ForegroundColor Yellow
    Push-Location (Join-Path $RepoDir "crates\desktop\web")
    try {
        & npx.cmd tsc --noEmit
        if ($LASTEXITCODE -ne 0) { throw "TypeScript check failed" }
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { throw "Vite web build failed" }
    } finally {
        Pop-Location
    }

    # 2. Run Rust workspace unit & integration tests
    Write-Host "[2/6] Running Rust workspace tests..." -ForegroundColor Yellow
    & cargo.exe test -p obsidian-sync-core -p obsidian-sync-desktop -p obsidian-sync-android
    if ($LASTEXITCODE -ne 0) { throw "Cargo workspace tests failed" }

    # 3. Build Windows Native Release (OnyxStudio.exe)
    Write-Host "[3/6] Building Windows x64 OnyxStudio.exe..." -ForegroundColor Yellow
    & cargo.exe build --release -p obsidian-sync-desktop
    if ($LASTEXITCODE -ne 0) { throw "Windows OnyxStudio.exe release build failed" }

    $builtExe = Join-Path $env:CARGO_TARGET_DIR "release\OnyxStudio.exe"
    if (-not (Test-Path $builtExe)) { throw "Built executable not found: $builtExe" }

    # Gate: Verify no external WebView2Loader.dll PE import (embedded shim only -> single standalone EXE)
    if (Test-Path "C:\mingw64\bin\objdump.exe") {
        $dllImports = & "C:\mingw64\bin\objdump.exe" -p $builtExe | Select-String "DLL Name:"
        Write-Host "PE DLL Imports:" -ForegroundColor DarkGray
        $dllImports | ForEach-Object { Write-Host "  $($_.Line.Trim())" -ForegroundColor DarkGray }
        if ($dllImports | Where-Object { $_.Line -match "WebView2Loader" }) {
            throw "FATAL: External WebView2Loader.dll import detected in $builtExe! Embedded shim required for single-EXE portability."
        }
        Write-Host "Verified standalone PE imports (embedded WebView2Loader shim, 0 external WebView2Loader.dll import)" -ForegroundColor Green
    }

    # Optional Authenticode code signing if matching local CodeSigning cert exists
    $signCert = Get-ChildItem Cert:\CurrentUser\My -CodeSigningCert -ErrorAction SilentlyContinue |
        Where-Object { $_.Subject -match 'Obsidian|Onyx|Bluewind' } |
        Select-Object -First 1
    if ($signCert) {
        Write-Host "Signing OnyxStudio.exe with certificate $($signCert.Subject)..." -ForegroundColor DarkCyan
        $sigRes = Set-AuthenticodeSignature -FilePath $builtExe -Certificate $signCert -HashAlgorithm SHA256
        Write-Host "Authenticode status: $($sigRes.Status)" -ForegroundColor DarkCyan
    }

    # 4. Build Android JNI libraries (arm64-v8a + x86_64)
    Write-Host "[4/6] Building Android JNI libraries via cargo ndk..." -ForegroundColor Yellow
    $jniStageDir = "D:\BuildToolset\tmp\onyx-jniLibs"
    New-Item -ItemType Directory -Force -Path $jniStageDir | Out-Null
    & cargo.exe ndk -t arm64-v8a -t x86_64 -o $jniStageDir build --release -p obsidian-sync-android
    if ($LASTEXITCODE -ne 0) { throw "cargo ndk Android JNI build failed" }

    # Copy JNI libs back to repo android/app/src/main/jniLibs as well
    foreach ($abi in @("arm64-v8a", "x86_64")) {
        $srcSo = Join-Path $jniStageDir "$abi\libobsidian_sync.so"
        $dstDir = Join-Path $RepoDir "android\app\src\main\jniLibs\$abi"
        New-Item -ItemType Directory -Force -Path $dstDir | Out-Null
        [System.IO.File]::Copy($srcSo, (Join-Path $dstDir "libobsidian_sync.so"), $true)
    }

    # 5. Stage Android project outside ReparsePoint root & assemble signed Release APK
    Write-Host "[5/6] Assembling signed Android Release APK..." -ForegroundColor Yellow
    $gradleStageDir = "D:\BuildToolset\tmp\onyx-gradle-stage\android"
    Copy-PlainDirectory (Join-Path $RepoDir "android") $gradleStageDir @('build', '.gradle', '.cxx')

    Push-Location $gradleStageDir
    try {
        & "D:\BuildToolset\gradle-8.11.1\bin\gradle.bat" assembleRelease --no-daemon
        if ($LASTEXITCODE -ne 0) { throw "Gradle assembleRelease failed" }
    } finally {
        Pop-Location
    }

    $apkPath = Join-Path $gradleStageDir "app\build\outputs\apk\release\app-release.apk"
    if (-not (Test-Path $apkPath)) { throw "Signed Release APK not found: $apkPath" }

    & "D:\BuildToolset\sdk\build-tools\35.0.0\apksigner.bat" verify --print-certs $apkPath
    if ($LASTEXITCODE -ne 0) { throw "APK signature verification failed" }

    & powershell.exe -ExecutionPolicy Bypass -File "D:\BuildToolset\tools\inspect-android.ps1" -Mode apk -ApkPath $apkPath
    if ($LASTEXITCODE -ne 0) { throw "ADR-007 Android APK policy check failed" }

    # 6. Stage release artifacts & SHA256SUMS.txt
    Write-Host "[6/6] Packaging release artifacts..." -ForegroundColor Yellow
    $stageDir = "D:\BuildToolset\tmp\obsidian-sync-release"
    if (Test-Path $stageDir) { Remove-Item $stageDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $stageDir | Out-Null

    $outExe = Join-Path $stageDir "OnyxStudio.exe"
    $outZip = Join-Path $stageDir "OnyxStudio-windows-x64.zip"
    $outOnyxApk = Join-Path $stageDir "OnyxStudio-android.apk"
    $outLegacyApk = Join-Path $stageDir "ObsidianSync-android.apk"

    Copy-Item $builtExe $outExe -Force
    Copy-Item $apkPath $outOnyxApk -Force
    Copy-Item $apkPath $outLegacyApk -Force

    $winPkgDir = "D:\BuildToolset\tmp\onyx-win-pkg"
    if (Test-Path $winPkgDir) { Remove-Item $winPkgDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $winPkgDir | Out-Null
    Copy-Item $builtExe (Join-Path $winPkgDir "OnyxStudio.exe") -Force
    Copy-Item (Join-Path $RepoDir "README.md") (Join-Path $winPkgDir "README.md") -Force
    if (Test-Path (Join-Path $RepoDir "docs\SETUP.md")) {
        Copy-Item (Join-Path $RepoDir "docs\SETUP.md") (Join-Path $winPkgDir "SETUP.md") -Force
    }
    Compress-Archive -Path "$winPkgDir\*" -DestinationPath $outZip -Force

    $exeHash = (Get-FileHash $outExe -Algorithm SHA256).Hash.ToLowerInvariant()
    $zipHash = (Get-FileHash $outZip -Algorithm SHA256).Hash.ToLowerInvariant()
    $apkHash = (Get-FileHash $outOnyxApk -Algorithm SHA256).Hash.ToLowerInvariant()

    $sumsLines = @(
        "$exeHash  OnyxStudio.exe",
        "$zipHash  OnyxStudio-windows-x64.zip",
        "$apkHash  OnyxStudio-android.apk",
        "$apkHash  ObsidianSync-android.apk"
    ) -join "`n"
    $sumsFile = Join-Path $stageDir "SHA256SUMS.txt"
    [IO.File]::WriteAllText($sumsFile, "$sumsLines`n", [Text.UTF8Encoding]::new($false))

    $exeSize = (Get-Item $outExe).Length
    $zipSize = (Get-Item $outZip).Length
    $apkSize = (Get-Item $outOnyxApk).Length
    $commitShort = (& git rev-parse --short HEAD).Trim()
    $publishedIso = (Get-Date).ToString("yyyy-MM-ddTHH:mm:sszzz")

    $latestMeta = [ordered]@{
        versionCode = $nextVc
        versionName = "$VersionName-b$nextVc"
        tag         = "v$VersionName"
        apkName     = "OnyxStudio-android.apk"
        downloadUrl = "https://github.com/bluewind201/obsidian-sync/releases/download/v$VersionName/OnyxStudio-android.apk"
        sha256      = $apkHash
        size        = $apkSize
        publishedAt = $publishedIso
    }
    $latestJsonFile = Join-Path $stageDir "latest.json"
    [IO.File]::WriteAllText($latestJsonFile, ($latestMeta | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))

    Write-Host "Artifacts staged at $stageDir`:" -ForegroundColor Green
    Write-Host "  OnyxStudio.exe             ($exeSize bytes) sha256=$exeHash"
    Write-Host "  OnyxStudio-windows-x64.zip ($zipSize bytes) sha256=$zipHash"
    Write-Host "  OnyxStudio-android.apk     ($apkSize bytes) sha256=$apkHash"

    if (-not $SkipPublish) {
        $vTag = "v$VersionName"
        $notesPath = Join-Path $stageDir "release-notes.md"
        $notesBody = @"
## Onyx Studio $vTag 주요 개선 사항

### 1. WebView Notion 스튜디오 UI 완전 복원 & 단일 파일 실행 보장 (`WebView2Loader.dll` 내장)
- **WebView (`tao` + `wry` + TOAST UI) 원본 디자인 복원**: `v1.2.0`~`v1.3.0`의 `eframe/egui` UI를 제거하고 기존 Notion 감성 WebView2 스튜디오(50% 초슬림 툴바 · A4 작성 캔버스/PDF · 벡터 다이어그램 · 외부 글 순수 텍스트 붙여넣기 & 이미지 보존)로 되돌렸습니다.
- **단일 실행 파일 동작**: `WebView2Loader.dll`을 `OnyxStudio.exe` 내부에 내장해 별도 DLL 파일 없이 단독 실행 및 인앱 자동 업데이트가 정상 동작합니다.

### 2. 외부 텍스트 복사·붙여넣기 시 순수 일반 텍스트 붙여넣기 & 이미지 보존 (유지됨)
- **원인 해결**: 외부 웹페이지·블로그·워드·메신저 등에서 글을 복사해 붙여넣을 때 클립보드 HTML 속성(`<span style="color:...; font-size:...">`, `<div>`, `<b>`, `<a href>`, `<mark>`, 제로폭 공백 `\u200B` 등)이 함께 파싱되어 글자 앞뒤로 HTML/마크다운 코드가 붙던 현상을 해결했습니다.
- **일반 텍스트 + 그림 동시 만족**:
  - 외부 글(텍스트)은 위지윅·마크다운 모든 모드에서 **순수 일반 텍스트**로만 깔끔하게 붙여넣어집니다.
  - 화면 캡처·브라우저 **이미지 복사**·파일 이미지 붙여넣기는 기존처럼 **볼트 첨부 이미지**로 바로 삽입됩니다.
  - 글과 그림(`<img>`)이 함께 섞인 외부 웹 게시글을 복사해 붙여넣을 때도 **그림은 그림으로 보존(인라인 base64 이미지는 볼트 첨부파일로 자동 분리 저장)**하고, **글은 속성 코드 없는 일반 텍스트**로 붙여넣습니다.
- **단축키 충돌 해결 및 우클릭 메뉴 강화**: `Ctrl+S` 저장 시 TOAST UI 기본 취소선(`~~~~`) 단축키가 함께 실행되던 문제를 차단하고, 에디터 우클릭 메뉴에 **`📥 붙여넣기 — 일반 텍스트 · 이미지 (Ctrl+V)`** 항목을 추가했습니다.

---

## 받는 법
- **Windows**: `OnyxStudio.exe` 단일 실행 파일 실행 (또는 `OnyxStudio-windows-x64.zip` 압축 해제 후 실행)
- **Android**: `OnyxStudio-android.apk` 설치 (구버전 자동 업데이트 호환용 `ObsidianSync-android.apk` 동일 파일 포함)

---

## 빌드 검증 및 SHA-256 체크섬 (로컬 `D:\BuildToolset` 정식 빌드 · 커밋 `$commitShort` · Android `versionCode=$nextVc`)

| 파일명 | 크기 | SHA-256 |
| --- | ---: | --- |
| `OnyxStudio.exe` | $exeSize bytes | ``$exeHash`` |
| `OnyxStudio-windows-x64.zip` | $zipSize bytes | ``$zipHash`` |
| `OnyxStudio-android.apk` | $apkSize bytes | ``$apkHash`` |
| `ObsidianSync-android.apk` | $apkSize bytes | ``$apkHash`` |
"@
        [IO.File]::WriteAllText($notesPath, $notesBody, [Text.UTF8Encoding]::new($false))

        # Publish / update rolling `latest-apk` tag (non-latest so /releases/latest stays semver vX.Y.Z for auto-updater)
        Write-Host "[OnyxStudio] Updating rolling latest-apk release..." -ForegroundColor Cyan
        $existingJson = if ($UseGhDirect) { & gh.exe release list --repo bluewind201/obsidian-sync --limit 50 --json tagName } else { & $GhWrapper release list --repo bluewind201/obsidian-sync --limit 50 --json tagName }
        $existingTags = @()
        if ($existingJson) {
            $existingTags = @(($existingJson | ConvertFrom-Json) | ForEach-Object { $_.tagName })
        }
        if ($existingTags -contains "latest-apk") {
            Invoke-Gh release upload "latest-apk" $outOnyxApk $outLegacyApk $latestJsonFile --clobber --repo bluewind201/obsidian-sync
            Invoke-Gh release edit "latest-apk" --title "Onyx Studio Latest Android APK ($vTag #$nextVc)" --notes "최신 Android APK ($vTag, versionCode=$nextVc)`n- SHA-256: ``$apkHash``" --latest=false --repo bluewind201/obsidian-sync
        } else {
            Invoke-Gh release create "latest-apk" $outOnyxApk $outLegacyApk $latestJsonFile --title "Onyx Studio Latest Android APK ($vTag #$nextVc)" --notes "최신 Android APK ($vTag, versionCode=$nextVc)`n- SHA-256: ``$apkHash``" --latest=false --repo bluewind201/obsidian-sync
        }

        # Publish / update semantic version release `$vTag` as `--latest`
        Write-Host "[OnyxStudio] Publishing $vTag release (--latest)..." -ForegroundColor Green
        if ($existingTags -contains $vTag) {
            Invoke-Gh release upload $vTag $outExe $outZip $outOnyxApk $outLegacyApk $sumsFile --clobber --repo bluewind201/obsidian-sync
            Invoke-Gh release edit $vTag --title "Onyx Studio $vTag — WebView Notion 스튜디오 복원 · 단일 파일 실행 완성" --notes-file $notesPath --latest --repo bluewind201/obsidian-sync
        } else {
            Invoke-Gh release create $vTag $outExe $outZip $outOnyxApk $outLegacyApk $sumsFile --target main --title "Onyx Studio $vTag — WebView Notion 스튜디오 복원 · 단일 파일 실행 완성" --notes-file $notesPath --latest --repo bluewind201/obsidian-sync
        }
    }
} finally {
    Pop-Location
}
