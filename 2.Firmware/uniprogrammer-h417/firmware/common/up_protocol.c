#include "up_protocol.h"

#include <string.h>

#define UP_CRC_POLY 0xEDB88320u

uint32_t up_crc32(const void *data, size_t len)
{
    const uint8_t *p = (const uint8_t *)data;
    uint32_t crc = 0xFFFFFFFFu;

    for (size_t i = 0; i < len; ++i) {
        crc ^= p[i];
        for (unsigned int bit = 0; bit < 8; ++bit) {
            if (crc & 1u) {
                crc = (crc >> 1) ^ UP_CRC_POLY;
            } else {
                crc >>= 1;
            }
        }
    }
    return crc ^ 0xFFFFFFFFu;
}

static void put_u16(uint8_t *p, uint16_t v)
{
    p[0] = (uint8_t)(v & 0xFFu);
    p[1] = (uint8_t)((v >> 8) & 0xFFu);
}

static void put_u32(uint8_t *p, uint32_t v)
{
    p[0] = (uint8_t)(v & 0xFFu);
    p[1] = (uint8_t)((v >> 8) & 0xFFu);
    p[2] = (uint8_t)((v >> 16) & 0xFFu);
    p[3] = (uint8_t)((v >> 24) & 0xFFu);
}

static uint16_t get_u16(const uint8_t *p)
{
    return (uint16_t)((uint16_t)p[0] | ((uint16_t)p[1] << 8));
}

static uint32_t get_u32(const uint8_t *p)
{
    return (uint32_t)p[0] |
           ((uint32_t)p[1] << 8) |
           ((uint32_t)p[2] << 16) |
           ((uint32_t)p[3] << 24);
}

size_t up_frame_build(uint8_t *out,
                      size_t out_capacity,
                      uint8_t type,
                      uint16_t seq,
                      uint16_t cmd,
                      const uint8_t *payload,
                      size_t payload_len,
                      const uint32_t *status)
{
    const size_t trailer_len = (status != NULL) ? 4u : 0u;
    const size_t frame_len = UP_FRAME_HEADER_SIZE + payload_len + trailer_len + 4u;

    if (payload_len > UP_FRAME_MAX_PAYLOAD || out_capacity < frame_len || out == NULL) {
        return 0;
    }

    out[0] = UP_MAGIC_0;
    out[1] = UP_MAGIC_1;
    out[2] = UP_MAGIC_2;
    out[3] = UP_MAGIC_3;
    out[4] = UP_PROTOCOL_VERSION;
    out[5] = type;
    put_u16(&out[6], seq);
    put_u16(&out[8], cmd);
    put_u32(&out[10], (uint32_t)payload_len);

    if (payload != NULL && payload_len != 0) {
        memcpy(&out[UP_FRAME_HEADER_SIZE], payload, payload_len);
    }
    if (status != NULL) {
        put_u32(&out[UP_FRAME_HEADER_SIZE + payload_len], *status);
    }

    const uint32_t crc = up_crc32(out, UP_FRAME_HEADER_SIZE + payload_len + trailer_len);
    put_u32(&out[UP_FRAME_HEADER_SIZE + payload_len + trailer_len], crc);
    return frame_len;
}

up_status_t up_frame_validate(const uint8_t *data, size_t len)
{
    if (data == NULL || len < UP_FRAME_HEADER_SIZE + 4u) {
        return UP_STATUS_ERR_BAD_LEN;
    }
    if (data[0] != UP_MAGIC_0 || data[1] != UP_MAGIC_1 ||
        data[2] != UP_MAGIC_2 || data[3] != UP_MAGIC_3) {
        return UP_STATUS_ERR_CMD;
    }
    if (data[4] != UP_PROTOCOL_VERSION) {
        return UP_STATUS_ERR_ARG;
    }
    if (data[5] != UP_FRAME_TYPE_REQUEST && data[5] != UP_FRAME_TYPE_RESPONSE) {
        return UP_STATUS_ERR_ARG;
    }

    const uint32_t payload_len = get_u32(&data[10]);
    const size_t trailer_len = (data[5] == UP_FRAME_TYPE_RESPONSE) ? 4u : 0u;
    const size_t frame_len = UP_FRAME_HEADER_SIZE + payload_len + trailer_len + 4u;
    if (frame_len != len) {
        return UP_STATUS_ERR_BAD_LEN;
    }

    const uint32_t expected = get_u32(&data[frame_len - 4u]);
    const uint32_t actual = up_crc32(data, frame_len - 4u);
    if (expected != actual) {
        return UP_STATUS_ERR_CRC;
    }
    (void)get_u16; /* kept for future header helpers */
    return UP_STATUS_OK;
}
