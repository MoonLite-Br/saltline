# Saltline (Rust / Android)

FPS 3D chơi được: vào cảng, bắn, nhặt máu/đạn, cắt 3 relay. Không copy source Three.js.
Mọi mesh, texture, tiếng đều sinh trong code. Không có file asset.

Đã kiểm tra trên máy dev: `cargo test` — 9 test (tia, map, hộp máu, A/D không đảo, frustum, mưa, địch hồi sinh).
Chưa chạy được APK ở môi trường viết code này — máy bạn có Android SDK thì build theo dưới.

## Chạy thử desktop

```bash
# Linux: sudo apt install -y libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev libxcursor-dev
cargo run
```

Màn hình đầu: CHOI / Enter. WASD di chuyển, chuột nhìn, chuột trái bắn, R nạp, Space nhảy, E giữ để cắt relay.
Esc tạm dừng. Thua hoặc thắng thì CHOI LAI. Địch chết sẽ về chỗ cũ sau 14 giây nếu bạn đứng xa.
Hộp xanh là máu, hộp vàng là đạn. Chấm vàng quanh tâm ngắm chỉ relay gần nhất.

Trên điện thoại: nửa trái là joystick, nửa phải vuốt nhìn, nút BAN / NHAY / NAP / CAT.

Góc phải là bộ đếm FPS (trung bình 32 frame). Xanh khi ≥ 30, vàng khi ≥ 20, đỏ khi thấp hơn. Dòng dưới là ms và độ phân giải render nội bộ.

Hiệu ứng (trời chiều, container xếp, mặt đường ướt, đèn natri, mưa, chớp nòng, súng trên tay) nằm trong 3 lần vẽ: trời, map, batch động. Không shadow, không bloom. Mưa tự giảm 40 → 16 hạt khi máy phải hạ độ phân giải.

## Build APK (Linux hoặc macOS)

Chỉ `arm64-v8a` (Snapdragon 460, Helio G35). APK debug do `cargo quad-apk` ký sẵn, `adb install` được.

### 1. JDK 17 + Android SDK 34 + NDK r27

Linux:

```bash
sudo apt install -y openjdk-17-jdk unzip curl
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
```

macOS:

```bash
brew install openjdk@17
export JAVA_HOME="$(/usr/libexec/java_home -v 17)"
```

Cả hai:

```bash
mkdir -p "$HOME/Android/Sdk/cmdline-tools"
cd /tmp
# Linux: commandlinetools-linux. macOS: commandlinetools-mac.
curl -L -o cmdtools.zip https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip
unzip -q cmdtools.zip -d "$HOME/Android/Sdk/cmdline-tools"
mv "$HOME/Android/Sdk/cmdline-tools/cmdline-tools" "$HOME/Android/Sdk/cmdline-tools/latest"

export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools"

yes | sdkmanager --licenses
sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;27.2.12479018"

export NDK_HOME="$ANDROID_HOME/ndk/27.2.12479018"
export ANDROID_NDK_HOME="$NDK_HOME"
export ANDROID_NDK_ROOT="$NDK_HOME"
```

Nếu `sdkmanager` không thấy đúng bản NDK, chạy `sdkmanager --list` và lấy version r27/r28 mà nó in ra. rustc 1.99 sẽ báo nếu NDK quá cũ hoặc quá mới.

### 2. Rust target + cargo-quad-apk

```bash
rustup target add aarch64-linux-android
cargo install cargo-quad-apk
# Nếu bản crates.io (0.1.4) không nhận opengles_version_* hoặc android_version = 34:
# cargo install --git https://github.com/not-fl3/cargo-quad-apk cargo-quad-apk

```

### 3. Build

```bash
cd saltline
cargo quad-apk build --release
find target -name '*.apk' -ls
```

Profile release trong `Cargo.toml`: `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. Lần đầu có thể 10–20 phút.

`cargo-quad-apk` tự sinh `AndroidManifest.xml` từ `[package.metadata.android]`. File `android/AndroidManifest.xml` chỉ để đối chiếu (minSdk 24, targetSdk 34, landscape, GLES 3.0, không quyền mạng, `exported=true`).

### Docker (nhanh hơn nếu đã có Docker)

```bash
docker run --rm -v "$PWD":/root/src -w /root/src notfl3/cargo-apk \
  cargo quad-apk build --release
```

Image này đôi khi chỉ có SDK 30. Nếu nó từ chối `android_version = 34`, tạm sửa `android_version` và `target_sdk_version` xuống `30` rồi build lại. `min_sdk_version` giữ 24.

### Cài và ký

```bash
adb install -r path/toi/saltline.apk
```

APK debug đã được tool ký. Chỉ cần `apksigner` khi đưa lên CH Play:

```bash
keytool -genkeypair -keystore saltline.keystore -alias saltline \
  -keyalg RSA -keysize 2048 -validity 10000
"$ANDROID_HOME/build-tools/34.0.0/zipalign" -f 4 app.apk app.aligned.apk
"$ANDROID_HOME/build-tools/34.0.0/apksigner" sign --ks saltline.keystore \
  --ks-key-alias saltline --out saltline-release.apk app.aligned.apk
```

## Đo trên máy

```bash
# Dung lượng APK (file .apk, không phải .so đã giải nén)
ls -l saltline.apk

adb shell dumpsys meminfo com.saltline.fps | head -n 40
adb shell dumpsys gfxinfo com.saltline.fps reset
# chơi ~10 giây
adb shell dumpsys gfxinfo com.saltline.fps
```

PSS `TOTAL` (có mục Graphics) mới là RAM đỉnh cần ≤ 150 MB. `VmRSS` bỏ sót bộ nhớ GPU.

FPS: hàng percentile trong gfxinfo, hoặc số vẽ ở góc phải trong game. Mục tiêu ≥ 30, nội bộ đang cố định logic 30 Hz.
