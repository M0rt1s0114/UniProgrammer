#ifndef UP_ENGINE_H
#define UP_ENGINE_H

#include <stddef.h>
#include <stdint.h>

#include "up_protocol.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Parse one validated incoming frame and write one response frame into
 * `out`. On success `out_len` receives the number of bytes to send.
 *
 * `out_capacity` must be at least:
 *   UP_FRAME_HEADER_SIZE + UP_FRAME_MAX_PAYLOAD + 8.
 */
void up_engine_handle(const uint8_t *frame,
                      size_t frame_len,
                      uint8_t *out,
                      size_t out_capacity,
                      size_t *out_len);

#ifdef __cplusplus
}
#endif

#endif /* UP_ENGINE_H */
