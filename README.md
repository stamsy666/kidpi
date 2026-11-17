# kidpi

Подробная инструкция — в `ИНСТРУКЦИЯ.md`.

Обход ограничений для Discord и YouTube с интерфейсом. Внутри — zapret от Flowseal
(https://github.com/Flowseal/zapret-discord-youtube, MIT): kidpi запускает его
`winws.exe` со стратегиями из `general*.bat`, подбирает рабочую и обновляет zapret с GitHub.

## Telegram
«Ускорение Telegram» — прокси из https://github.com/Flowseal/tg-ws-proxy (MIT). kidpi запускает
его `proxy/tg_ws_proxy.py` на встроенном Python и обновляет код из релизов на GitHub.
Python и библиотеки в архив исходников не входят, их собирает команда ниже (в `src-tauri/resources/tgws/runtime`):
```
curl -o py.zip https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip
unzip py.zip -d src-tauri/resources/tgws/runtime
printf 'python312.zip\r\n.\r\nLib\\site-packages\r\n' > src-tauri/resources/tgws/runtime/python312._pth
pip download --dest wheels --platform win_amd64 --python-version 3.12 --implementation cp --only-binary=:all: "cryptography==46.0.5" certifi "httpx[http2]==0.28.1"
for w in wheels/*.whl; do unzip -o "$w" -d src-tauri/resources/tgws/runtime/Lib/site-packages; done
```

## Стек
- `src-tauri/` — Rust (Tauri 2): запуск winws.exe, подбор стратегии, обновления, трей, автозапуск.
- `src/` — интерфейс на React (Vite).

## Сборка установщика под Windows из Linux
```
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
sudo apt install nsis lld llvm clang libayatana-appindicator3-dev
npm install
TAURI_SIGNING_PRIVATE_KEY="$(cat kidpi-updater.key)" npx tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc
```
(ключ обновлений нужен, чтобы установщик подписался для автообновления; см. ИНСТРУКЦИЯ.md)
Установщик: `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`.

На Windows достаточно `npm install` и `npx tauri build`.

## Где kidpi хранит данные
`C:\ProgramData\kidpi` — копия zapret, `settings.json`, логи (`logs\kidpi.log`, `logs\winws.log`).
