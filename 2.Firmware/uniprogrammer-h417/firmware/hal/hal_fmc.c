#include "up_hal.h"

/*
 * TODO M5:
 * Port from CH32H417EVT EXAM/FMC/NAND.
 *
 * Planned FMC bank 3, 8-bit data:
 *   D0..D7   = PD14/PD15/PD0/PD1/PE7/PE8/PE9/PE10 (AF12)
 *   NOE/NWE  = PD4 / PD5 (AF12)
 *   NWAIT    = PD6 (AF12, pull-up)
 *   CLE/ALE  = PD11 / PD12 (AF12)
 *   CE       = PD7 (AF12)
 */
up_status_t up_hal_nand_init(const up_nand_config_t *cfg)
{
    (void)cfg;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_nand_read_id(uint8_t *id, size_t id_len)
{
    (void)id;
    (void)id_len;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_nand_read_page(uint32_t page,
                                  uint16_t column,
                                  uint8_t *data,
                                  size_t len)
{
    (void)page;
    (void)column;
    (void)data;
    (void)len;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_nand_write_page(uint32_t page,
                                   uint16_t column,
                                   const uint8_t *data,
                                   size_t len)
{
    (void)page;
    (void)column;
    (void)data;
    (void)len;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_nand_erase_block(uint32_t block)
{
    (void)block;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_nand_read_spare(uint32_t page, uint8_t *data, size_t len)
{
    (void)page;
    (void)data;
    (void)len;
    return UP_STATUS_ERR_UNSUPPORTED;
}
