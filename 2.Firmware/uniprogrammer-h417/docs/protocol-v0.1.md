# UniProgrammer host-firmware protocol v0.1 (draft)

## Design rules

- Little-endian packed structures.
- Every frame ends with CRC32 (IEEE 802.3 polynomial).
- Host sends a request; firmware sends exactly one response per request
  (except streaming commands, which use an intermediate ACK).
- The host is the sequence source; responses echo the sequence number.

## Frame header (14 bytes)

| Offset | Size | Field | Notes |
|---|---:|---|---|
| 0 | 4 | magic | ASCII `UP01` = `55 50 30 31` |
| 4 | 1 | version | protocol version, currently `1` |
| 5 | 1 | type | `0x01` request, `0x02` response |
| 6 | 2 | seq | host-generated, echoed by firmware |
| 8 | 2 | cmd | command code |
| 10 | 4 | payload_len | payload length, little-endian |

Payload follows the header. CRC32 covers header + payload.

Maximum non-streaming payload: 4080 bytes. Stream data chunks are limited
by the transport packet size and re-assembled by the engine.

## Response header

| Offset | Size | Field |
|---|---:|---|
| 0 | 4 | magic |
| 4 | 1 | version |
| 5 | 1 | type = `0x02` |
| 6 | 2 | seq echo |
| 8 | 2 | cmd echo |
| 10 | 4 | payload_len |
| 14 | 4 | status (`up_status_t`, little-endian) |

Status payload follows if payload_len > 0.

## Status codes

| Code | Name | Meaning |
|---:|---|---|
| 0 | OK | success |
| 1 | ERR_CMD | unknown command |
| 2 | ERR_CRC | frame CRC mismatch |
| 3 | ERR_BAD_LEN | payload length invalid |
| 4 | ERR_ARG | invalid parameter |
| 5 | ERR_BUSY | operation in progress |
| 6 | ERR_HAL | peripheral error |
| 7 | ERR_UNSUPPORTED | not implemented on this firmware |
| 8 | ERR_FAULT | hardware fault (VCC/overcurrent/etc.) |

## Commands

| Code | Name | Direction/payload |
|---:|---|---|
| 0x0001 | GET_INFO | resp: firmware version, feature bitmap, max payload |
| 0x0002 | ECHO | echo request payload back |
| 0x0010 | SPI_INIT | mode, clock, bit order, data width |
| 0x0011 | SPI_XFER | tx bytes; resp rx bytes |
| 0x0020 | QSPI_INIT | flash size, clock mode |
| 0x0021 | QSPI_CMD | opcode + address + dummy + optional data |
| 0x0022 | QSPI_READ | address + length; resp data |
| 0x0023 | QSPI_WRITE | address + data |
| 0x0030 | NAND_INIT | page/block/spare size, timing profile |
| 0x0031 | NAND_READ_ID | resp JEDEC ID |
| 0x0032 | NAND_READ_PAGE | page + column + length |
| 0x0033 | NAND_WRITE_PAGE | page + column + data |
| 0x0034 | NAND_ERASE_BLOCK | block address |
| 0x0035 | NAND_READ_SPARE | page |
| 0x0040 | GPIO_CONFIG | pin, mode, pull |
| 0x0041 | GPIO_READ / GPIO_WRITE | pins |
| 0x0050 | VCC_SET | target mV |
| 0x0051 | VCC_MEASURE | resp mV and flags |
| 0x0100 | STREAM_START | stream id, total length, direction |
| 0x0101 | STREAM_DATA | stream id + chunk |
| 0x0102 | STREAM_END | stream id; resp completion status |
| 0x0200 | BOOT_INFO / BOOT_UPDATE | firmware update flow |

## Feature bitmap (GET_INFO)

| Bit | Feature |
|---:|---|
| 0 | SPI full-duplex |
| 1 | QSPI 1-line |
| 2 | QSPI 2/4-line |
| 3 | FMC NAND 8-bit |
| 4 | VCC control + measure |
| 5 | streaming |
| 6 | SWD bit-bang |
| 7 | firmware update |

## Streaming flow

1. `STREAM_START` -> `ACK` or error.
2. Zero or more `STREAM_DATA` frames (each ACK optional; host can pipeline
   with windowing).
3. `STREAM_END` -> final status.

The first `STREAM_DATA` payload for a write may carry op-specific context
(e.g. page/column); details are fixed per command family later.
