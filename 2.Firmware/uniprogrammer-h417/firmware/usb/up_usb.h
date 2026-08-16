#ifndef UP_USB_H
#define UP_USB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define UP_USB_EP_CMD_OUT 1u
#define UP_USB_EP_CMD_IN  1u

/*
 * Transport integration point.
 *
 * Target implementation: CH32H417EVT EXAM/USBHS/DEVICE/CH372Device,
 * reduced to EP0 control + EP1 OUT + EP1 IN bulk (512B HS / 64B FS).
 * Replace the example loopback in EP1 OUT/IN with the two functions below.
 */
void up_usb_init(void);
void up_usb_poll(void);

/* Called from the USB OUT endpoint handler when one packet arrived. */
void up_usb_on_receive(const uint8_t *data, uint16_t len);

/* Send one USB packet; returns 0 on success. */
int up_usb_send(const uint8_t *data, uint16_t len);

#ifdef __cplusplus
}
#endif

#endif /* UP_USB_H */
