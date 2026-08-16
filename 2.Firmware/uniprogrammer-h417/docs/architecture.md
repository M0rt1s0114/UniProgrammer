# UniProgrammer CH32H417 architecture (v0.1 draft)

## Goals

- Firmware is a thin, chip-library-free protocol engine.
- Chip database, UI, and policy remain in the UniProgrammer host app.
- Transport must be binary, checksummed, sequence-aware, and streaming for
  large reads/writes.
- First board targets: SPI NOR, QSPI NOR, parallel NAND (FMC), VCC control.

## Core assignment

| Core | Responsibility |
|---|---|
| V5F (400 MHz) | USB transport, frame parser, command engine, DMA coordination |
| V3F (150 MHz, later) | Hard real-time I/O: GPIO bit-bang, SWD, timing-sensitive FMC flows |

First release runs on **V5F only**; V3F is compiled as a minimal placeholder.

## Layers

```
+-----------------------------------------------+
| Host (UniProgrammer Tauri/Rust)               |
+-----------------------------------------------+
  binary framed protocol over USB bulk
+-----------------------------------------------+
| transport_usb  (CH372Device-style vendor bulk) |
+-----------------------------------------------+
| engine         (parse -> dispatch -> response) |
+-----------------------------------------------+
| hal_spi | hal_qspi | hal_fmc | hal_gpio       |
+-----------------------------------------------+
| WCH peripheral library (SPI/QSPI/FMC/GPIO)    |
+-----------------------------------------------+
```

## USB transport

- USB-HS first: vendor class 0xFF, EP1 OUT + EP1 IN bulk, 512-byte HS
  packets (FS fallback 64 bytes).
- USB3 and USBSS are intentionally deferred.
- The transport API is a small `up_usb` bridge around the EVT
  `CH372Device` endpoint model.

## Frame protocol

See `protocol-v0.1.md`.

## Real-time considerations

- All long operations (page program, erase) are non-blocking on the USB
  endpoint: command accepted, operation polled, completion sent later.
- Large data uses `STREAM_START / STREAM_DATA / STREAM_END`.
- VCC is always measured before enable; over-current/short-circuit shuts the
  rail down and reports a fault.

## Future hardware assumptions

- VIO18 GPIO bank for 1.2/1.8/2.5/3.3 V signal levels.
- External rail switches controlled by GPIO; ADC monitors target VCC.
- FMC bank 3 8-bit NAND (TSOP48), QSPI1 for NOR.
