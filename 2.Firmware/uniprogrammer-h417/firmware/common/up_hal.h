#ifndef UP_HAL_H
#define UP_HAL_H

#include <stdint.h>
#include <stddef.h>

#include "up_protocol.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Bus-level abstraction. Concrete drivers live in `hal_*.[ch]`.
 * The engine only talks to these functions; it never touches WCH registers.
 */

typedef struct {
    uint8_t mode;      /* SPI mode 0..3 */
    uint32_t clock_hz; /* requested clock */
    uint8_t bit_order; /* 0 = MSB first, 1 = LSB first */
    uint8_t data_bits; /* 8 or 16 */
} up_spi_config_t;

typedef struct {
    uint32_t flash_size; /* bytes */
    uint8_t clock_mode;  /* QSPI CKMode 0..3 */
} up_qspi_config_t;

typedef struct {
    uint16_t page_size;
    uint16_t spare_size;
    uint32_t block_size; /* bytes */
    uint32_t total_size; /* bytes */
    uint8_t row_cycles;
    uint8_t col_cycles;
} up_nand_config_t;

up_status_t up_hal_spi_init(uint8_t bus, const up_spi_config_t *cfg);
up_status_t up_hal_spi_xfer(uint8_t bus,
                            const uint8_t *tx,
                            size_t tx_len,
                            uint8_t *rx,
                            size_t rx_len);

up_status_t up_hal_qspi_init(uint8_t bus, const up_qspi_config_t *cfg);
up_status_t up_hal_qspi_cmd(uint8_t bus,
                            uint8_t opcode,
                            uint32_t address,
                            uint8_t addr_len,
                            uint8_t dummy_cycles,
                            uint8_t data_lines,
                            const uint8_t *tx,
                            size_t tx_len,
                            uint8_t *rx,
                            size_t rx_len);

up_status_t up_hal_nand_init(const up_nand_config_t *cfg);
up_status_t up_hal_nand_read_id(uint8_t *id, size_t id_len);
up_status_t up_hal_nand_read_page(uint32_t page,
                                  uint16_t column,
                                  uint8_t *data,
                                  size_t len);
up_status_t up_hal_nand_write_page(uint32_t page,
                                   uint16_t column,
                                   const uint8_t *data,
                                   size_t len);
up_status_t up_hal_nand_erase_block(uint32_t block);
up_status_t up_hal_nand_read_spare(uint32_t page, uint8_t *data, size_t len);

/* Board-level helpers (voltage, GPIO, protection). */
up_status_t up_hal_vcc_set(uint16_t target_mv);
up_status_t up_hal_vcc_measure(uint16_t *actual_mv, uint32_t *flags);
up_status_t up_hal_gpio_config(uint32_t pin, uint8_t mode, uint8_t pull);
up_status_t up_hal_gpio_read(uint32_t pin_mask, uint32_t *values);
up_status_t up_hal_gpio_write(uint32_t pin_mask, uint32_t values);

#ifdef __cplusplus
}
#endif

#endif /* UP_HAL_H */
