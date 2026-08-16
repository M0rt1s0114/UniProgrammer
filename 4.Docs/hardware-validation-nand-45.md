# NAND / DataFlash Hardware Validation Checklist

[中文](hardware-validation-nand-45_CN.md)

The features below are implemented and compile locally, but their command
sequences come from public datasheets and standard protocol references.
**Each item must be validated on real hardware before release.**

## Preparation

- Programmer: CH341A / CH347T / CH347F (CH347 preferred)
- Suggested chips:
  - Winbond `W25N01GV` / `W25N02KV` (bad blocks, ECC, BBM LUT)
  - GigaDevice `GD5F1GQ4UExxG` (parameter page, OTP)
  - Adesto/Dialog `AT45DB321E` (45-series page mode)
- Target board or socket adapter; back up all important data

## SPI NAND

| # | Item | Action | Expected result | Risk |
|---|---|---|---|---|
| 1 | Bad block scan | Detect the chip, then click "Scan Bad Blocks" | Log lists block number / main-area address / address including OOB; count matches factory marks | Read-only |
| 2 | Skip read | Select Skip mode and read the whole chip | Bad block positions appear as 0xFF holes, other data intact | Read-only |
| 3 | Skip write | Erase, then write a small file | Bad blocks are skipped and physical pages shift; read-back verifies | Write; back up first |
| 4 | Ignore mode | Read/write/erase | Full-area operation as before; errors may appear in bad block areas | Write; may corrupt data |
| 5 | Bypass write | Select Bypass mode and write | Log shows "N BBM mappings written"; logical addresses stay continuous afterwards | Writes chip-internal LUT; highly irreversible |
| 6 | BBM LUT read | Click "Read BBM LUT" | 20 LBA/PBA/status entries match the A5h response | Read-only |
| 7 | Read UID | Click "Read UID" | 64-byte unique ID matches the datasheet | Read-only |
| 8 | Parameter page | Click "Read Parameter Page" | 256-byte ONFI parameter page; signature/vendor/page size readable | Read-only |
| 9 | ECC toggle | Enable/disable on-die ECC | B0h bit 4 toggles; writes/reads report no ECC errors when enabled; OOB ECC area is FF when disabled | Register write |
| 10 | OTP read | Enter page number, click "Read OTP Page" | Main + OOB data returned; B0h configuration restored afterwards | Read-only |

## 45-Series DataFlash

| # | Item | Action | Expected result | Risk |
|---|---|---|---|---|
| 1 | Read page mode | Click "Read Page Mode" | Status register bit 0 reports binary vs standard page | Read-only |
| 2 | Switch to binary page | Click "Binary Page" and confirm | 3D/2A/80/A6 results in power-of-two page size | Nonvolatile config |
| 3 | Switch to DataFlash page | Click "DataFlash Page" and confirm | 3D/2A/80/A7 restores the standard page | Nonvolatile config |
| 4 | Read/write after switching | Read/write/erase/verify after switching | Operations honor the current page size | Write |

## Completion criteria

- Every item passes on at least one real chip;
- Record chip model, ID, bad block count, backend, and clock used;
- Keep logs (including raw hex) for any anomaly so behavior can be analyzed
  later.
