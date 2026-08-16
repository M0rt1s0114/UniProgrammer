#include "up_hal.h"

/*
 * TODO M4:
 * Port from CH32H417EVT EXAM/QSPI/QSPI_FLASH.
 *
 * Planned pins on the first board:
 *   QSPI1_SCK  = PB2 (AF9)
 *   QSPI1_SCSN = PB6 (AF10)
 *   SIO0..SIO3 = PF8/PF9/PF7/PF6 (AF10)
 */
up_status_t up_hal_qspi_init(uint8_t bus, const up_qspi_config_t *cfg)
{
    (void)bus;
    (void)cfg;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_qspi_cmd(uint8_t bus,
                            uint8_t opcode,
                            uint32_t address,
                            uint8_t addr_len,
                            uint8_t dummy_cycles,
                            uint8_t data_lines,
                            const uint8_t *tx,
                            size_t tx_len,
                            uint8_t *rx,
                            size_t rx_len)
{
    (void)bus;
    (void)opcode;
    (void)address;
    (void)addr_len;
    (void)dummy_cycles;
    (void)data_lines;
    (void)tx;
    (void)tx_len;
    (void)rx;
    (void)rx_len;
    return UP_STATUS_ERR_UNSUPPORTED;
}
