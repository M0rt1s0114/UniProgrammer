/*
 * UniProgrammer CH32H417 firmware - V3F real-time core placeholder.
 *
 * First milestone runs V5F only. V3F is reserved for:
 *   - timing-sensitive GPIO bit-bang
 *   - SWD
 *   - VCC fault handling
 *   - HSEM/IPC message responder
 */
#include "ch32h417.h"
#include "debug.h"

int main(void)
{
    NVIC_PriorityGroupConfig(NVIC_PriorityGroup_2);

    /*
     * TODO M7:
     *   - take HSEM gate and release V5F (see EVT dual-core examples)
     *   - register IPC channels for command offload
     *   - enter low-power wait loop
     */
    while (1) {
        __WFI();
    }
}
