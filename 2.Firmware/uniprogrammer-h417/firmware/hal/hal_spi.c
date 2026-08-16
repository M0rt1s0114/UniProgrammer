#include "up_hal.h"

/*
 * TODO M3:
 * Port from CH32H417EVT EXAM/SPI/2Lines_FullDuplex.
 *
 * Planned pins on the first board:
 *   SPI2_SCK  = PB13 (AF5)
 *   SPI2_MOSI = PC1  (AF5)
 *   SPI2_MISO = PC2  (AF5)
 *   CS        = PB12 (GPIO, software controlled)
 */
up_status_t up_hal_spi_init(uint8_t bus, const up_spi_config_t *cfg)
{
    (void)bus;
    (void)cfg;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_spi_xfer(uint8_t bus,
                            const uint8_t *tx,
                            size_t tx_len,
                            uint8_t *rx,
                            size_t rx_len)
{
    (void)bus;
    (void)tx;
    (void)tx_len;
    (void)rx;
    (void)rx_len;
    return UP_STATUS_ERR_UNSUPPORTED;
}
