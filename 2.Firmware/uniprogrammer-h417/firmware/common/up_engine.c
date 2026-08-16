#include "up_engine.h"

#include <string.h>

#include "up_hal.h"

#define UP_FW_VERSION_MAJOR 0u
#define UP_FW_VERSION_MINOR 1u

typedef struct __attribute__((packed)) {
    uint16_t fw_major;
    uint16_t fw_minor;
    uint32_t features;
    uint16_t max_payload;
} up_info_response_t;

typedef struct __attribute__((packed)) {
    uint16_t seq;
    uint16_t cmd;
    uint32_t payload_len;
} up_echo_payload_t;

static uint16_t rd_u16(const uint8_t *p)
{
    return (uint16_t)((uint16_t)p[0] | ((uint16_t)p[1] << 8));
}

static uint32_t rd_u32(const uint8_t *p)
{
    return (uint32_t)p[0] |
           ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) |
           ((uint32_t)p[3] << 24);
}

static void wr_u16(uint8_t *p, uint16_t v)
{
    p[0] = (uint8_t)(v & 0xFFu);
    p[1] = (uint8_t)((v >> 8) & 0xFFu);
}

static void wr_u32(uint8_t *p, uint32_t v)
{
    p[0] = (uint8_t)(v & 0xFFu);
    p[1] = (uint8_t)((v >> 8) & 0xFFu);
    p[2] = (uint8_t)((v >> 16) & 0xFFu);
    p[3] = (uint8_t)((v >> 24) & 0xFFu);
}

static size_t make_response(uint8_t *out,
                            size_t out_capacity,
                            uint16_t seq,
                            uint16_t cmd,
                            uint32_t status,
                            const uint8_t *payload,
                            size_t payload_len)
{
    return up_frame_build(out, out_capacity,
                          UP_FRAME_TYPE_RESPONSE, seq, cmd,
                          payload, payload_len, &status);
}

static up_status_t handle_get_info(uint8_t *payload, size_t *payload_len)
{
    up_info_response_t info;
    info.fw_major = UP_FW_VERSION_MAJOR;
    info.fw_minor = UP_FW_VERSION_MINOR;
    info.features = UP_FEATURE_SPI | UP_FEATURE_QSPI_1LINE |
                    UP_FEATURE_FMC_NAND | UP_FEATURE_STREAMING;
    info.max_payload = UP_FRAME_MAX_PAYLOAD;

    if (*payload_len < sizeof(info)) {
        *payload_len = 0;
        return UP_STATUS_ERR_BAD_LEN;
    }
    memcpy(payload, &info, sizeof(info));
    *payload_len = sizeof(info);
    return UP_STATUS_OK;
}

void up_engine_handle(const uint8_t *frame,
                      size_t frame_len,
                      uint8_t *out,
                      size_t out_capacity,
                      size_t *out_len)
{
    up_status_t frame_status;
    uint16_t seq;
    uint16_t cmd;
    const uint8_t *payload;
    size_t payload_len;

    *out_len = 0;
    if (out == NULL || out_capacity < UP_FRAME_HEADER_SIZE + UP_FRAME_MAX_PAYLOAD + 8u) {
        return;
    }

    frame_status = up_frame_validate(frame, frame_len);
    if (frame_status != UP_STATUS_OK) {
        *out_len = make_response(out, out_capacity, 0, 0, frame_status, NULL, 0);
        return;
    }

    seq = rd_u16(&frame[6]);
    cmd = rd_u16(&frame[8]);
    payload_len = (size_t)rd_u32(&frame[10]);
    payload = &frame[UP_FRAME_HEADER_SIZE];

    switch ((up_cmd_t)cmd) {
    case UP_CMD_GET_INFO: {
        uint8_t resp[sizeof(up_info_response_t)];
        size_t resp_len = sizeof(resp);
        frame_status = handle_get_info(resp, &resp_len);
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status,
                                 resp, resp_len);
        break;
    }
    case UP_CMD_ECHO: {
        /* Payload is arbitrary; echo it back unchanged. */
        *out_len = make_response(out, out_capacity, seq, cmd, UP_STATUS_OK,
                                 payload, payload_len);
        break;
    }
    case UP_CMD_SPI_INIT: {
        if (payload_len < sizeof(uint8_t) + sizeof(up_spi_config_t)) {
            frame_status = UP_STATUS_ERR_BAD_LEN;
        } else {
            const uint8_t bus = payload[0];
            const up_spi_config_t *cfg = (const up_spi_config_t *)&payload[1];
            frame_status = up_hal_spi_init(bus, cfg);
        }
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    }
    case UP_CMD_SPI_XFER: {
        if (payload_len < 5u) {
            frame_status = UP_STATUS_ERR_BAD_LEN;
        } else {
            const uint8_t bus = payload[0];
            const size_t rx_len = rd_u16(&payload[1]);
            const size_t tx_len = payload_len - 3u;
            frame_status = up_hal_spi_xfer(bus, &payload[3], tx_len,
                                           out + UP_FRAME_HEADER_SIZE + 4u,
                                           rx_len);
            if (frame_status == UP_STATUS_OK) {
                frame_status = UP_STATUS_OK;
            }
        }
        if (frame_status != UP_STATUS_OK) {
            *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        } else {
            /* RX data is already after the response trailer area. */
            const size_t rx_len = (payload_len >= 5u) ? rd_u16(&payload[1]) : 0u;
            *out_len = make_response(out, out_capacity, seq, cmd, frame_status,
                                     out + UP_FRAME_HEADER_SIZE + 4u, rx_len);
        }
        break;
    }
    case UP_CMD_QSPI_INIT:
    case UP_CMD_QSPI_CMD:
    case UP_CMD_QSPI_READ:
    case UP_CMD_QSPI_WRITE:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO M4 */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    case UP_CMD_NAND_INIT:
    case UP_CMD_NAND_READ_ID:
    case UP_CMD_NAND_READ_PAGE:
    case UP_CMD_NAND_WRITE_PAGE:
    case UP_CMD_NAND_ERASE_BLOCK:
    case UP_CMD_NAND_READ_SPARE:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO M5 */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    case UP_CMD_GPIO_CONFIG:
    case UP_CMD_GPIO_READ:
    case UP_CMD_GPIO_WRITE:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO board bring-up */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    case UP_CMD_VCC_SET:
    case UP_CMD_VCC_MEASURE:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO M6 */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    case UP_CMD_STREAM_START:
    case UP_CMD_STREAM_DATA:
    case UP_CMD_STREAM_END:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO M2+ */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    case UP_CMD_BOOT_INFO:
    case UP_CMD_BOOT_UPDATE:
        frame_status = UP_STATUS_ERR_UNSUPPORTED; /* TODO M8 */
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    default:
        frame_status = UP_STATUS_ERR_CMD;
        *out_len = make_response(out, out_capacity, seq, cmd, frame_status, NULL, 0);
        break;
    }

    (void)wr_u16;
    (void)wr_u32;
}
