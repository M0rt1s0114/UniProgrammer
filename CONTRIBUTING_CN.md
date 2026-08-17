# 涓?UniProgrammer 鍋氳础鐚?
[English](CONTRIBUTING.md)

鎰熻阿浣犲 UniProgrammer 鐨勫叧娉紒UniProgrammer 鏄竴涓法骞冲彴 SPI Flash 缂栫▼鍣紝
甯︽湁鍙彃鎷旂殑纭欢鎶借薄灞傦紙HAL锛夈€傛湰鏂囨。璇存槑濡備綍鎼缓椤圭洰銆佷慨鏀逛唬鐮佷互鍙婃彁浜ゅ彉鏇淬€?
## 鍩烘湰瑙勫垯

- **鎻愪氦淇℃伅蹇呴』浣跨敤鑻辨枃銆?*
- 閬靛惊 [Conventional Commits](https://www.conventionalcommits.org/) 瑙勮寖銆?- 澶у瀷鍔熻兘璇峰厛鍦?issue 涓璁猴紝鍐嶆彁浜?Pull Request銆?- 鏈」鐩細鎿﹂櫎鍜屽啓鍏ョ湡瀹?Flash 鑺墖銆傛案杩滀笉瑕佸鍐呭鏃犳硶鎵垮彈涓㈠け鐨勮姱鐗?  鎵ц鍐欏叆鎴栨摝闄ゆ祴璇曘€?- 涓嶈鎻愪氦淇敼杩囩殑鍘傚晢浜岃繘鍒舵枃浠讹紙渚嬪 `CH34X.DLL`锛夈€傚巶鍟嗕簩杩涘埗鏂囦欢鍙兘
  鐢辩淮鎶よ€呭熀浜庡畼鏂瑰彂甯冪増鏈洿鏂般€?
## 寮€鍙戠幆澧?
闇€瑕佺殑宸ュ叿锛?
- Rust锛坰table锛変互鍙?`cargo`銆乣rustfmt`銆乣clippy`
- Node.js 22+ 涓?npm
- 骞冲彴渚濊禆
  - Windows锛歐ebView2锛堥€氬父宸查瑁咃級
  - Linux锛歐ebKitGTK 4.1銆丟TK 3銆乴ibusb-1.0銆乴ibudev锛屼互鍙?Tauri 鎵€闇€鐨?    鏍囧噯 Linux 渚濊禆

## 鏋勫缓

鏋勫缓鑿滃崟鑴氭湰浼氶€夋嫨 HAL 鍚庣骞舵墽琛屽畬鏁?release 鏋勫缓銆?
Windows锛圥owerShell锛夛細

```powershell
.\build-menu.ps1
```

Linux / macOS锛?
```bash
./build-menu.sh
```

鍚庣閫夋嫨瑙勫垯锛?
- 涓嶅惎鐢ㄤ换浣?feature锛氭寜骞冲彴榛樿锛坄Windows` -> `hal-dll`锛屽叾浠?-> `hal-libusb`锛?- `--features hal-libusb`锛氬己鍒朵娇鐢?rusb/libusb 鍚庣
- `--features hal-dll`锛氬己鍒朵娇鐢?CH34X.DLL 鍚庣锛堜粎 Windows锛?
## 鎻愪氦淇℃伅

鎵€鏈夋彁浜や娇鐢ㄨ嫳鏂?Conventional Commits锛?
```
type(scope): summary
```

绫诲瀷锛?
| Type | 鐢ㄩ€?|
| --- | --- |
| `feat` | 鏂板姛鑳?|
| `fix` | 缂洪櫡淇 |
| `docs` | 鏂囨。 |
| `style` | 浠呮牸寮忚皟鏁?|
| `refactor` | 涓嶆敼鍙樿涓虹殑浠ｇ爜閲嶆瀯 |
| `perf` | 鎬ц兘浼樺寲 |
| `test` | 娴嬭瘯 |
| `build` | 鏋勫缓绯荤粺鎴栦緷璧?|
| `ci` | CI 閰嶇疆 |
| `chore` | 缁存姢鎬т换鍔?|
| `revert` | 鍥炴粴鏌愭鎻愪氦 |

绀轰緥锛?
```
feat(hal): add Windows DLL backend
fix(serprog): correct S_BUSTYPE and O_SPIOP opcodes
refactor(chiplib): replace XML fallback with typed loader
chore(release): 0.3.0-alpha.1
```

鐮村潖鎬у彉鏇翠娇鐢?`!` 鎴?`BREAKING CHANGE:` 鑴氭敞锛?
```
feat(hal)!: split backends into separate compile features
```

## 鍒嗘敮涓?Pull Request

- 浠?`main` 鍒涘缓鐭嫳鏂囧垎鏀細
  `feat/...`銆乣fix/...`銆乣docs/...`銆乣chore/...`
- 姣忎釜 Pull Request 鍙叧娉ㄤ竴涓彉鏇淬€?- 涓哄彉鏇寸殑閫昏緫娣诲姞鎴栨洿鏂版祴璇曘€?- 纭繚鏈湴 `cargo check`銆乣cargo test` 浠ュ強
  [浠ｇ爜椋庢牸](#浠ｇ爜椋庢牸) 涓殑 lint 濂椾欢閫氳繃銆?
## 浠ｇ爜椋庢牸

鎺ㄩ€佸墠璇峰湪 `3.Software` 涓嬭繍琛屽畬鏁?lint 濂椾欢锛?
Windows锛圥owerShell锛夛細

```powershell
.\lint.ps1
```

Linux / macOS锛?
```bash
./lint.sh
```

璇ュ浠舵鏌ワ細

- 鍓嶇锛歚npm run lint`锛圗SLint 10 + typescript-eslint +
  eslint-plugin-vue锛変互鍙?`npm run format:check`锛圥rettier锛?- 鍚庣锛歚cargo fmt --check` 涓?`cargo lint`
  锛坄cargo clippy --all-targets -- -D warnings`锛涘弬瑙?  `3.Software/src-tauri/.cargo/config.toml`锛?
CI 閫氳繃 `.github/workflows/lint.yml` 鍦ㄦ瘡娆?`main` 鎺ㄩ€佸拰 Pull Request 涓?杩愯鍚屾牱鐨勬鏌ャ€?
鍗曠嫭鍛戒护锛?
```bash
# 鍓嶇锛堝湪 3.Software 涓嬭繍琛岋級
npm run lint
npm run lint:fix
npm run format:check
npm run format

# 鍚庣锛堝湪 3.Software/src-tauri 涓嬭繍琛岋級
cargo fmt --check
cargo lint
```

鍓嶇椋庢牸璇存槑锛?
- Vue 3 `<script setup lang="ts">`
- 鐢ㄦ埛鍙瀛楃涓叉斁鍦?`3.Software/src/i18n/index.ts`
- 鏂扮殑鍙鐢?UI 缁勪欢鏀惧湪 `3.Software/src/components`

## 娴嬭瘯

鑷冲皯杩愯锛?
```bash
cargo test
```

纭欢鐩稿叧鏀瑰姩闇€瑕佹墜鍔ㄩ獙璇侊紝鍙娇鐢ㄤ笅琛ㄤ綔涓烘鏌ユ竻鍗曪細

| 鍚庣 | 鑺墖 | 璇?ID | 鎿﹂櫎 | 璇诲彇 | 鍐欏叆 | 鏍￠獙 |
| --- | --- | --- | --- | --- | --- | --- |
| CH341A DLL | SPI NOR | | | | | |
| CH347T DLL | SPI NOR | | | | | |
| CH347F DLL | SPI NOR | | | | | |
| libusb | SPI NOR | | | | | |
| Serprog | SPI NOR | | | | | |

- 2026-08-16锛欳H341A DLL + SPI NOR 鍩虹璇?鍐?鎿?鏍￠獙宸插湪鍗曞彴娴嬭瘯鐜閫氳繃锛?  鍚庣画鎸夊疄闄呮祴璇曠户缁～鍐欍€?
## 鑺墖鏁版嵁搴?
- `chiplib.bin` 鏄潈濞佹暟鎹簱锛岀鐩樹笂浣跨敤杞婚噺娣锋穯銆?- `chiplib.xml` 鏄悓鏍锋贩娣嗙殑鍥為€€鏂囦欢锛屼笉鏄彲璇绘簮鏂囦欢銆?- 瑙ｇ爜鍙彂鐢熷湪鍐呭瓨涓紱涓嶅緱鎻愪氦鎴栧湪宸ヤ綔鐩綍鐣欎笅鏄庢枃鑺墖搴撱€?- 鎵归噺鏇存柊浼樺厛浣跨敤 `chipdb_tool merge <bin> <chips.tsv>`锛屽崟棰楄姱鐗囦娇鐢?  `chipdb_tool add ...`锛岄伩鍏嶈鐩栧凡鏈夎ˉ鍏ㄥ瓧娈点€?
## 鐗堟湰绠＄悊

- 璇箟鍖栫増鏈?2.0.0
- Git tag 浣跨敤 `v` 鍓嶇紑锛歚v0.3.0-alpha.1`
- 棰勫彂甯冪増鏈細`-alpha.N`銆乣-beta.N`銆乣-rc.N`
- 鎺ㄩ€?`v*` tag 浼氳Е鍙?release workflow

## 璁稿彲璇?
UniProgrammer 閲囩敤
[GPL-3.0-or-later](https://www.gnu.org/licenses/gpl-3.0.html) 璁稿彲銆?鍙備笌璐＄尞鍗宠〃绀轰綘鍚屾剰浣犵殑璐＄尞浠ョ浉鍚屾潯娆惧彂甯冦€?