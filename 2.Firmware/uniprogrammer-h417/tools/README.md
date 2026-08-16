# Integration helper notes

The framework intentionally does not commit the WCH SDK. Use this layout in
MounRiver Studio:

## Project include paths (relative to project root)

```
../EXAM/SRC/Core
../EXAM/SRC/Debug
../EXAM/SRC/Peripheral/inc
../EXAM/SRC/Peripheral/src
../EXAM/STARTUP
../EXAM/SRC/Ld/V5F        (V5F project)
../EXAM/SRC/Ld/V3F        (V3F project)
Common/up
Common/up_usb
```

## Source folders to add

- `firmware/common` -> project `Common/up`
- `firmware/hal`   -> project `Common/up`
- `firmware/usb`   -> project `Common/up_usb`
- `firmware/v5f`   -> replace `V5F/User`
- `firmware/v3f`   -> replace `V3F/User`

## USBHS base

Start from:

```
CH32H417EVT/EXAM/USBHS/DEVICE/CH372Device
```

Keep only EP0 + EP1 OUT/IN at first. In the template endpoint handler:

```c
/* EP1 OUT complete */
up_usb_on_receive(packet, len);
```

```c
/* main loop */
up_usb_poll();
```

Replace `up_usb_send()` internals with the template's EP1 IN DMA + ACK flow.
