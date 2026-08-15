PREFIX  ?= arm-none-eabi-
CC      := $(PREFIX)gcc
AS      := $(PREFIX)gcc
LD      := $(PREFIX)gcc
OBJCOPY := $(PREFIX)objcopy
SIZE    := $(PREFIX)size

TARGET  := serprog-bluepill
CFLAGS  := -mcpu=cortex-m3 -mthumb -Os -g -Wall -Wextra \
           -ffunction-sections -fdata-sections -std=c11 -fno-builtin
LDFLAGS := -mcpu=cortex-m3 -mthumb -T stm32f103.ld -nostdlib \
           -Wl,--gc-sections -Wl,-Map=$(TARGET).map

OBJS := main.o startup_stm32f103.o

all: $(TARGET).elf $(TARGET).bin $(TARGET).hex

$(TARGET).elf: $(OBJS)
	$(LD) $(LDFLAGS) -o $@ $(OBJS)
	$(SIZE) $@

main.o: main.c
	$(CC) $(CFLAGS) -c $< -o $@

startup_stm32f103.o: startup_stm32f103.s
	$(AS) $(CFLAGS) -c $< -o $@

%.bin: %.elf
	$(OBJCOPY) -O binary $< $@

%.hex: %.elf
	$(OBJCOPY) -O ihex $< $@

flash: $(TARGET).bin
	st-flash write $(TARGET).bin 0x08000000

clean:
	rm -f $(OBJS) $(TARGET).elf $(TARGET).bin $(TARGET).hex $(TARGET).map

.PHONY: all flash clean
