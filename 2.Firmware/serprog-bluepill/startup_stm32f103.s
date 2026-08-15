/* Blue Pill (STM32F103C8T6) 最小启动文件 */
  .syntax unified
  .cpu cortex-m3
  .thumb

  .section .isr_vector, "a", %progbits
  .word _estack
  .word Reset_Handler
  .word NMI_Handler
  .word HardFault_Handler
  .word MemManage_Handler
  .word BusFault_Handler
  .word UsageFault_Handler
  .word 0
  .word 0
  .word 0
  .word 0
  .word SVC_Handler
  .word DebugMon_Handler
  .word 0
  .word PendSV_Handler
  .word SysTick_Handler

  .section .text
  .thumb_func
  .global Reset_Handler
Reset_Handler:
  /* .data 初始化：_sidata(Flash LMA) -> _sdata(RAM) */
  ldr r0, =_sidata
  ldr r1, =_sdata
  ldr r2, =_edata
  b 2f
1:
  ldr r3, [r0], #4
  str r3, [r1], #4
2:
  cmp r1, r2
  bcc 1b

  /* .bss 清零 */
  ldr r1, =_sbss
  ldr r2, =_ebss
  movs r0, #0
  b 4f
3:
  str r0, [r1], #4
4:
  cmp r1, r2
  bcc 3b

  bl main
  b .

  .thumb_func
  .weak NMI_Handler
NMI_Handler:
  b .
  .thumb_func
  .weak HardFault_Handler
HardFault_Handler:
  b .
  .thumb_func
  .weak MemManage_Handler
MemManage_Handler:
  b .
  .thumb_func
  .weak BusFault_Handler
BusFault_Handler:
  b .
  .thumb_func
  .weak UsageFault_Handler
UsageFault_Handler:
  b .
  .thumb_func
  .weak SVC_Handler
SVC_Handler:
  b .
  .thumb_func
  .weak DebugMon_Handler
DebugMon_Handler:
  b .
  .thumb_func
  .weak PendSV_Handler
PendSV_Handler:
  b .
  .thumb_func
  .weak SysTick_Handler
SysTick_Handler:
  b .
