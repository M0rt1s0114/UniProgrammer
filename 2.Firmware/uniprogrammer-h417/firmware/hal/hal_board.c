#include "up_hal.h"

/*
 * TODO M6 (VCC) and board bring-up (GPIO):
 *
 * VCC plan:
 *   - external rail switch controlled by GPIO
 *   - ADC measures target rail before enable
 *   - over-current/short-circuit latches a fault and disables the rail
 *
 * GPIO plan:
 *   - dedicated target nRST, BOOT0/SWDIO/SWCLK direction and level control
 */
up_status_t up_hal_vcc_set(uint16_t target_mv)
{
    (void)target_mv;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_vcc_measure(uint16_t *actual_mv, uint32_t *flags)
{
    (void)actual_mv;
    (void)flags;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_gpio_config(uint32_t pin, uint8_t mode, uint8_t pull)
{
    (void)pin;
    (void)mode;
    (void)pull;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_gpio_read(uint32_t pin_mask, uint32_t *values)
{
    (void)pin_mask;
    (void)values;
    return UP_STATUS_ERR_UNSUPPORTED;
}

up_status_t up_hal_gpio_write(uint32_t pin_mask, uint32_t values)
{
    (void)pin_mask;
    (void)values;
    return UP_STATUS_ERR_UNSUPPORTED;
}
