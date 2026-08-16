# UniProgrammer CH32H417 Firmware (framework)

Firmware framework for the UniProgrammer self-designed programmer based on
WCH CH32H417 (RISC-V dual core, USB2 HS/FS + USB3, 4x SPI, 2x QSPI, FMC).

## Status

- Framework / protocol skeleton only.
- No vendor SDK files are committed here. You must point the build at the
  official WCH CH32H417 EVT (currently kept outside the repository).
- First milestone is single-core V5F with USB-HS vendor bulk transport.

## Directory layout

```
docs/                  Architecture and host protocol
firmware/common/       Portable protocol + command engine (no WCH headers)
firmware/hal/          SPI / QSPI / FMC driver skeletons
firmware/usb/          USB transport bridge (integration point)
firmware/v5f/          V5F application main
firmware/v3f/          V3F application main (reserved for real-time I/O)
```

## Integration with the official EVT

1. Obtain `CH32H417EVT` from WCH and put it at
   `example/wch/CH32H417EVT` (not committed).
2. In MounRiver Studio create a project from the EVT
   `USBHS/DEVICE/CH372Device` example as the base template.
3. Replace/add:
   - `firmware/common/*` and `firmware/hal/*` as project `Common/up/*`
   - `firmware/usb/*` as `Common/up_usb/*`
   - `firmware/v5f/main.c` as `V5F/User/main.c`
4. Keep the EVT `SRC` include paths. No vendor file is modified here.

## Milestones

- [ ] M1: V5F runs, USB-HS vendor bulk enumerates (CH372Device base)
- [ ] M2: frame protocol GET_INFO / ECHO over USB
- [ ] M3: SPI full-duplex pass-through
- [ ] M4: QSPI 1/2/4-line operations
- [ ] M5: FMC parallel NAND page/read/write/erase
- [ ] M6: VCC/IO rail control and measurement
- [ ] M7: V3F offload + HSEM/IPC
- [ ] M8: SWD GPIO bit-bang

See `docs/architecture.md` and `docs/protocol-v0.1.md`.
