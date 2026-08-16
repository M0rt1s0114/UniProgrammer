/*
 * UniProgrammer CH32H417 firmware - V5F application main (framework).
 *
 * Drop this file into a MounRiver project created from:
 *   CH32H417EVT\EXAM\USBHS\DEVICE\CH372Device\V5F\User
 *
 * Milestones:
 *   M1: USB-HS vendor bulk enumerates (CH372Device base).
 *   M2: wire USBHS endpoint handlers into up_usb_on_receive/up_usb_send.
 */
#include "ch32h417.h"
#include "debug.h"

#include "ch32h417_usbhs_device.h"
#include "up_engine.h"
#include "up_protocol.h"
#include "up_usb.h"

static void clock_init(void)
{
    /* Kept intentionally minimal; replace with the CH372Device template's
     * RCC/USBHS PLL setup. */
}

static void gpio_init(void)
{
    /* TODO board bring-up: LEDs, target nRST, VCC control GPIO. */
}

int main(void)
{
    NVIC_PriorityGroupConfig(NVIC_PriorityGroup_2);
    clock_init();
    gpio_init();

    /*
     * TODO M1:
     *   USBHS_RCC_Init();
     *   USBHS_Device_Init(ENABLE);
     * Then replace the template endpoint handlers with up_usb_* calls.
     */
    up_usb_init();

    while (1) {
        up_usb_poll();
        /* TODO M3+: poll async NAND/QSPI operations and voltage faults. */
    }
}
