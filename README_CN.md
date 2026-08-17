# UniProgrammer

> 涓€娆捐法骞冲彴 NAND/NOR SPI Flash 缂栫▼鍣紝鍏峰鍙彃鎷旂殑纭欢鎶借薄灞傦紙HAL锛夈€?
[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)
[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

[English](README.md)

> **鈿狅笍 椤圭洰灏氭湭瀹屾垚瀹炴満楠岃瘉锛岃璋ㄦ厧浣跨敤 / Most features have NOT been
> validated on real hardware; use with caution.**
>
> 2026-08-16锛欳H341A + SPI NOR 鍩虹鎿嶄綔宸插湪鍗曞彴娴嬭瘯鐜閫氳繃锛?> 鍏朵粬缂栫▼鍣?鑺墖缁勫悎浠嶉渶鎸夐獙璇佹竻鍗曢€愰」娴嬭瘯銆?
> **Alpha 鐗堟湰璀﹀憡** 鈥?涓嶈鐢ㄤ簬鍐呭鏃犳硶鎵垮彈涓㈠け鐨勮姱鐗囥€?
## 鐩綍

- [鑳屾櫙](#鑳屾櫙)
- [鍔熻兘鐗规€(#鍔熻兘鐗规€?
- [瀹夎](#瀹夎)
- [浣跨敤鏂规硶](#浣跨敤鏂规硶)
- [纭欢鍚庣](#纭欢鍚庣)
- [鑺墖鏁版嵁搴揮(#鑺墖鏁版嵁搴?
- [鍥轰欢](#鍥轰欢)
- [寮€鍙慮(#寮€鍙?
- [缁存姢鑰匽(#缁存姢鑰?
- [璐＄尞](#璐＄尞)
- [璁稿彲璇乚(#璁稿彲璇?

## 鑳屾櫙

UniProgrammer 鏄缁忓吀 CH341/CH347 缂栫▼宸ュ叿鐨勪竴娆＄幇浠ｅ寲閲嶅啓銆?鍗忚灞傜Щ妞嶈嚜 [flashrom](https://www.flashrom.org/) 涓?[IMSProg](https://github.com/bigbigmdm/IMSProg)锛屽湪鑺墖鍛戒护涓?USB/涓插彛浼犺緭涔嬮棿淇濇寔娓呮櫚鍒嗙銆?
## 鍔熻兘鐗规€?
- 缂栫▼鍣ㄦ敮鎸?  - CH341A銆丆H347T銆丆H347F
  - Serprog锛堜覆鍙ｅ崗璁級
  - HIDProg锛堥鐣欏崰浣嶏級
- 鍗忚锛歋PI NOR銆丼PI NAND銆両2C EEPROM銆丮icrowire EEPROM銆?  SPI EEPROM銆丏ataFlash AT45
- 璇?/ 鍐?/ 鎿﹂櫎 / 鏍￠獙锛屽疄鏃惰繘搴︽樉绀?- 鑺墖鏁版嵁搴撴敮鎸?JEDEC 鑷姩璇嗗埆涓庢墜鍔ㄩ€夊瀷
  锛堢鐩樹笂涓鸿交閲忔贩娣嗭紝瑙乕鑺墖鏁版嵁搴揮(#鑺墖鏁版嵁搴?锛?- 娣辫壊 / 娴呰壊 / 璺熼殢绯荤粺涓婚
- 璁剧疆瀵硅瘽妗嗭紝閰嶇疆鎸佷箙鍖栧埌 `Setting.set`锛圛NI锛夛紝鑷姩杩佺Щ鏃ф祻瑙堝櫒瀛樺偍
- 鐢靛帇璋冭妭闈㈡澘涓庢帴閫氱數婧愰珮鍗辩‘璁ゆ祦绋?- 鍏充簬瀵硅瘽妗嗭細鍔ㄦ€佺増鏈彿銆佽姱鐗囧簱鍒嗙被缁熻
- SPI NAND 鍧忓潡妯″紡锛圫kip / Bypass / Ignore锛夈€丅BM LUT 璇诲啓銆?  鐗囦笂 ECC 鎺у埗銆丱TP / 鍙傛暟椤佃鍙栵紝浠ュ強鎸夎姱鐗囬厤缃殑 dummy/plane/die 鏀寔
- Hex 缂栬緫鍣細缂栬緫銆佹挙閿€銆佹悳绱€佽烦杞€佸～鍏呫€佹牎楠屽拰
- Windows 鍘熺敓鏂囦欢瀵硅瘽妗嗭紱Linux 鏀寔寮€鍙戜腑

## 瀹夎

### 棰勬瀯寤轰骇鐗?
Alpha 鏋勫缓浜х墿浼氬彂甯冨湪 GitHub Releases锛屼粠 `v0.3.0-alpha.1` 寮€濮嬨€?
- Windows锛欳I 鐢熸垚 libusb 鍚庣鐨勪究鎼虹増銆傚嚭浜庤鍙瘉鍘熷洜锛屽畼鏂?  `CH34X.DLL` 涓嶉殢婧愮爜鍒嗗彂锛沇indows DLL 鍚庣闇€瑕佹湰鍦版斁缃巶鍟?DLL 鍚庢瀯寤恒€?- Linux锛氬熀浜?Ubuntu 24.04 + WebKitGTK 鏋勫缓鐨?`chip-validator`銆?
### 婧愮爜鏋勫缓

Windows锛圥owerShell锛夛細

```powershell
cd 3.Software
.\build-menu.ps1
```

Linux锛?
```bash
cd 3.Software
./build-menu.sh
```

## 浣跨敤鏂规硶

1. 杩炴帴缂栫▼鍣紝鍦ㄥ乏渚ч潰鏉块€夋嫨瀵瑰簲鍨嬪彿銆?2. 鐐瑰嚮 **杩炴帴**锛屽啀鐐瑰嚮 **妫€娴?*銆侸EDEC ID 浼氬埌鑺墖鏁版嵁搴撻噷鍖归厤銆?3. 杞藉叆浜岃繘鍒舵枃浠讹紝鎴栧皢鑺墖璇诲彇鍒?Hex 缂栬緫鍣ㄣ€?4. 鎸夐渶鎵ц **璇诲彇 / 鍐欏叆 / 鎿﹂櫎 / 鏍￠獙**銆?
娌℃湁 JEDEC ID 鐨勮姱鐗囷紙I2C銆丮icrowire锛夎鎵嬪姩閫夋嫨
绫诲瀷 鈫?鍘傚晢 鈫?鍨嬪彿銆?
## 纭欢鍚庣

`3.Software/src-tauri/src/ch34x.rs` 涓殑 HAL trait 鏄姱鐗囧崗璁笌
纭欢浼犺緭涔嬮棿鐨勮竟鐣屻€?
- `hal-dll`锛氬畼鏂?CH34X.DLL 鍚庣锛圵indows 榛樿锛?- `hal-libusb`锛歳usb/libusb 鍚庣锛圠inux 榛樿锛學indows 鍙€夛級

鍚庣閫夋嫨鏄紪璇戞湡 Cargo feature銆傛瀯寤哄懡浠よ
[CONTRIBUTING.md](CONTRIBUTING.md)銆?
## 鑺墖鏁版嵁搴?
`chiplib.bin` 鏄潈濞佹暟鎹簱锛岀鐩樹笂浣跨敤杞婚噺娣锋穯锛團FW 寮忛€愬瓧鑺傛帺鐮?寰幆绉讳綅锛夈€?`chiplib.xml` 鏄悓鏍锋贩娣嗙殑鍥為€€鏂囦欢锛屼笉鏄彲璇绘簮鐮侊紱涓よ€呴兘鍙湪鍐呭瓨涓В鐮侊紝
鍒嗗彂鍖呭拰宸ヤ綔鐩綍涓笉浼氱暀涓嬫槑鏂囪姱鐗囧簱銆?
缁存姢宸ュ叿锛堜篃鍙墽琛?`cargo run --example chipdb_tool -- help`锛夛細

```bash
# 鎵归噺鍚堝苟 TSV 鑺墖琛紙缂哄け鎻掑叆锛屽凡鏈夋潯鐩彧琛ョ己澶卞瓧娈碉級
cargo run --example chipdb_tool -- merge src-tauri/chiplib.bin chips.tsv

# 鎸?JEDEC ID 鏂板/鏇挎崲鍗曢鑺墖
cargo run --example chipdb_tool -- add src-tauri/chiplib.bin 5E3213 \
  Zbit ZB25D40B SPI_NOR page=256 size=524288 sector=4096 block=65536

# 浠?IMSProg.Dat 琛ュ叏瀛楁锛堝彧濉┖缂哄€硷級
cargo run --example chipdb_tool -- \
  src-tauri/chiplib.bin IMSProg.Dat --backup
```

## 鍥轰欢

`2.Firmware/serprog-bluepill` 鎻愪緵闈㈠悜 STM32F103 Blue Pill 鐨?瑁告満 serprog 鍥轰欢锛屽彲灏?Blue Pill 鍙樻垚 USART1 涓插彛 SPI 缂栫▼鍣ㄣ€?
## 寮€鍙?
- Rust + Node.js 22
- 鍚庣娴嬭瘯锛歚cargo test`
- 浠ｇ爜瑙勮寖锛氬湪 `3.Software` 涓嬭繍琛?`.\lint.ps1`锛圵indows锛夋垨 `./lint.sh`
  锛圠inux锛夛紝浼氫緷娆℃墽琛?ESLint銆丳rettier銆乣cargo fmt` 涓?`cargo clippy`锛?  CI 浼氬湪 `main` 鎺ㄩ€佸拰 Pull Request 鏃惰嚜鍔ㄦ墽琛屽悓鏍锋鏌?- 鍙戝竷绋冲畾鐗堝墠蹇呴』瀹屾垚鐪熸満楠岃瘉锛涙竻鍗曪細
  [涓枃](4.Docs/hardware-validation-nand-45_CN.md) /
  [English](4.Docs/hardware-validation-nand-45.md)

## 缁存姢鑰?
- [M0rt1s0114](https://github.com/M0rt1s0114)

## 璐＄尞

瑙?[CONTRIBUTING_CN.md](CONTRIBUTING_CN.md)锛堜腑鏂囷級/
[CONTRIBUTING.md](CONTRIBUTING.md)锛圗nglish锛夈€?
## 璁稿彲璇?
[GPL-3.0-or-later](LICENSE)
