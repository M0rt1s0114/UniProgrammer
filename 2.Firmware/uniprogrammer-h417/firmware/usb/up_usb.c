#include "up_usb.h"

#include <string.h>

#include "up_engine.h"
#include "up_protocol.h"

#define UP_USB_RX_CAPACITY 8192u
#define UP_USB_TX_CAPACITY 4096u

static uint8_t rx_buf[UP_USB_RX_CAPACITY];
static size_t rx_len;
static uint8_t tx_buf[UP_USB_TX_CAPACITY];

void up_usb_init(void)
{
    rx_len = 0;
    memset(rx_buf, 0, sizeof(rx_buf));
    memset(tx_buf, 0, sizeof(tx_buf));
    /* TODO M1/M2: configure USBHS CH372Device endpoints here. */
}

void up_usb_on_receive(const uint8_t *data, uint16_t len)
{
    if (len == 0 || data == NULL) {
        return;
    }
    if (rx_len + len > sizeof(rx_buf)) {
        /* Overflow: drop the oldest byte stream. The host must retry. */
        rx_len = 0;
    }
    memcpy(&rx_buf[rx_len], data, len);
    rx_len += len;
}

static void try_process_one_frame(void)
{
    size_t frame_len;
    size_t out_len = 0;

    if (rx_len < UP_FRAME_HEADER_SIZE + 4u) {
        return;
    }

    {
        const uint32_t payload_len =
            (uint32_t)rx_buf[10] |
            ((uint32_t)rx_buf[11] << 8) |
            ((uint32_t)rx_buf[12] << 16) |
            ((uint32_t)rx_buf[13] << 24);
        frame_len = UP_FRAME_HEADER_SIZE + (size_t)payload_len + 4u;
    }

    if (frame_len > sizeof(rx_buf) || frame_len > rx_len) {
        if (frame_len > sizeof(rx_buf)) {
            rx_len = 0; /* malformed length; resync */
        }
        return;
    }

    if (up_frame_validate(rx_buf, frame_len) != UP_STATUS_OK) {
        /* TODO: send a CRC/error response if header can be trusted. */
        memmove(rx_buf, &rx_buf[1], rx_len - 1);
        rx_len -= 1;
        return;
    }

    up_engine_handle(rx_buf, frame_len, tx_buf, sizeof(tx_buf), &out_len);
    if (out_len != 0) {
        (void)up_usb_send(tx_buf, (uint16_t)out_len);
    }

    memmove(rx_buf, &rx_buf[frame_len], rx_len - frame_len);
    rx_len -= frame_len;
}

void up_usb_poll(void)
{
    /* Process all complete frames currently buffered. */
    for (;;) {
        const size_t before = rx_len;
        try_process_one_frame();
        if (rx_len == before) {
            break;
        }
    }
}

int up_usb_send(const uint8_t *data, uint16_t len)
{
    (void)data;
    (void)len;
    /* TODO M2: replace with CH372Device EP1 IN DMA + ACK flow. */
    return -1;
}
