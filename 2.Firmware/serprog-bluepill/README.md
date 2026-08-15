# Blue Pill serprog 测试固件

把 Blue Pill（STM32F103C8T6，兼容常见 C6/C8 变体）变成一个
flashrom 兼容的 serprog SPI 编程器。PC 端 UniProgrammer 选择
`Serprog (Serial)`，串口 115200 8N1 即可连接。

## 接线

### PC <-> Blue Pill（通过 USB-TTL，例如 CH340）
| USB-TTL | Blue Pill | 说明 |
|---|---|---|
| RX | PA9 | Blue Pill TX |
| TX | PA10 | Blue Pill RX |
| GND | GND | 共地 |

### Blue Pill <-> SPI Flash
| Blue Pill | Flash 座 | 说明 |
|---|---|---|
| PA5 | SCK | SPI 时钟 |
| PA6 | MISO / SO | 主入从出 |
| PA7 | MOSI / SI | 主出从入 |
| PA4 | CS | 片选（低有效） |
| 3.3V | VCC | 供电 |
| GND | GND | 共地 |

LED：PC13，空闲时每秒闪两下（500ms 心跳）。

## 编译

需要 `arm-none-eabi-gcc`（Ubuntu: `sudo apt install gcc-arm-none-eabi`）。

```bash
make
```

产物：
- `serprog-bluepill.bin`（推荐烧录）
- `serprog-bluepill.hex`
- `serprog-bluepill.elf`

## 烧录

ST-Link（V2 克隆也可以）：

```bash
make flash          # st-flash write serprog-bluepill.bin 0x08000000
```

或手动：

```bash
st-flash write serprog-bluepill.bin 0x08000000
```

USB-TTL 串口模块只能通信，**不能**烧录 STM32；烧录用 ST-Link SWD（SWDIO/SWCLK/GND/3.3V）。

## 验证

1. 插上 USB-TTL，在 PC 上确认出现串口（Linux `/dev/ttyUSB0`，Windows `COMx`）
2. UniProgrammer → Programmer 选择 `Serprog (Serial)` → 填串口 → 连接
3. 日志应出现 `serprog 接口版本 1.0`、固件名 `uni-serprog`
4. 芯片座放 SPI NOR Flash 后点“检测芯片”

## 实现说明

- 72MHz 主频，硬件 SPI1（默认 18MHz，可按 `S_SPI_FREQ` 调到 36/18/9/4.5/…/281.25kHz）
- serprog 支持：NOP / Q_IFACE / Q_CMDMAP / Q_PGMNAME / Q_SERBUF /
  Q_BUSTYPE / Q_OPBUF / Q_RDNMAXLEN / S_BUSTYPE / O_SPIOP /
  S_SPI_FREQ / S_PIN_STATE
- 全裸寄存器实现，无 HAL 依赖，源码只有 `main.c` + 启动文件和链接脚本
