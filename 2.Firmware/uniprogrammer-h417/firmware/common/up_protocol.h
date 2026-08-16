#ifndef UP_PROTOCOL_H
#define UP_PROTOCOL_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define UP_MAGIC_0 'U'
#define UP_MAGIC_1 'P'
#define UP_MAGIC_2 '0'
#define UP_MAGIC_3 '1'

#define UP_PROTOCOL_VERSION 1u

#define UP_FRAME_HEADER_SIZE 14u
#define UP_FRAME_MAX_PAYLOAD 4080u

#define UP_FRAME_TYPE_REQUEST  0x01u
#define UP_FRAME_TYPE_RESPONSE 0x02u

/* Status codes */
typedef enum {
    UP_STATUS_OK = 0,
    UP_STATUS_ERR_CMD = 1,
    UP_STATUS_ERR_CRC = 2,
    UP_STATUS_ERR_BAD_LEN = 3,
    UP_STATUS_ERR_ARG = 4,
    UP_STATUS_ERR_BUSY = 5,
    UP_STATUS_ERR_HAL = 6,
    UP_STATUS_ERR_UNSUPPORTED = 7,
    UP_STATUS_ERR_FAULT = 8,
} up_status_t;

/* Command codes */
typedef enum {
    UP_CMD_GET_INFO = 0x0001,
    UP_CMD_ECHO = 0x0002,

    UP_CMD_SPI_INIT = 0x0010,
    UP_CMD_SPI_XFER = 0x0011,

    UP_CMD_QSPI_INIT = 0x0020,
    UP_CMD_QSPI_CMD = 0x0021,
    UP_CMD_QSPI_READ = 0x0022,
    UP_CMD_QSPI_WRITE = 0x0023,

    UP_CMD_NAND_INIT = 0x0030,
    UP_CMD_NAND_READ_ID = 0x0031,
    UP_CMD_NAND_READ_PAGE = 0x0032,
    UP_CMD_NAND_WRITE_PAGE = 0x0033,
    UP_CMD_NAND_ERASE_BLOCK = 0x0034,
    UP_CMD_NAND_READ_SPARE = 0x0035,

    UP_CMD_GPIO_CONFIG = 0x0040,
    UP_CMD_GPIO_READ = 0x0041,
    UP_CMD_GPIO_WRITE = 0x0042,

    UP_CMD_VCC_SET = 0x0050,
    UP_CMD_VCC_MEASURE = 0x0051,

    UP_CMD_STREAM_START = 0x0100,
    UP_CMD_STREAM_DATA = 0x0101,
    UP_CMD_STREAM_END = 0x0102,

    UP_CMD_BOOT_INFO = 0x0200,
    UP_CMD_BOOT_UPDATE = 0x0201,
} up_cmd_t;

/* Feature bitmap reported by GET_INFO */
typedef enum {
    UP_FEATURE_SPI = 1u << 0,
    UP_FEATURE_QSPI_1LINE = 1u << 1,
    UP_FEATURE_QSPI_MULTI = 1u << 2,
    UP_FEATURE_FMC_NAND = 1u << 3,
    UP_FEATURE_VCC = 1u << 4,
    UP_FEATURE_STREAMING = 1u << 5,
    UP_FEATURE_SWD = 1u << 6,
    UP_FEATURE_BOOT_UPDATE = 1u << 7,
} up_feature_t;

typedef struct __attribute__((packed)) {
    uint8_t magic[4];
    uint8_t version;
    uint8_t type;
    uint16_t seq;
    uint16_t cmd;
    uint32_t payload_len;
} up_frame_header_t;

/* Response-only trailer, located after payload and before CRC. */
typedef struct __attribute__((packed)) {
    uint32_t status;
} up_frame_response_trailer_t;

uint32_t up_crc32(const void *data, size_t len);

/*
 * Serialize a complete frame into `out` (capacity `out_capacity`).
 * Returns total frame size, or 0 when the frame does not fit.
 *
 * For responses, pass `status` (non-NULL) so the response trailer is added.
 */
size_t up_frame_build(uint8_t *out,
                      size_t out_capacity,
                      uint8_t type,
                      uint16_t seq,
                      uint16_t cmd,
                      const uint8_t *payload,
                      size_t payload_len,
                      const uint32_t *status);

/*
 * Validate a buffer:
 *  - returns UP_STATUS_OK on success;
 *  - fills optional pointers with payload begin/length.
 * The CRC is verified over the entire buffer including the trailer bytes.
 */
up_status_t up_frame_validate(const uint8_t *data, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* UP_PROTOCOL_H */
