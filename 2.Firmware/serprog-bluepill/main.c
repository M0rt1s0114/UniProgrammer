/*
 * UniProgrammer serprog 测试固件 — Blue Pill (STM32F103C8T6)
 *
 * 功能：把 Blue Pill 变成一个 flashrom 兼容的 serprog SPI 编程器。
 *  - USART1  : PA9(TX) / PA10(RX) @115200 8N1，与 PC 上的 USB-TTL 连接
 *  - SPI1    : PA5(SCK) / PA6(MISO) / PA7(MOSI)，软件 CS 在 PA4
 *  - LED     : PC13 空闲心跳
 *
 * 裸寄存器实现，无 HAL/CMSIS 依赖。
 */

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

/* ── 寄存器基址 ──────────────────────────────────────────────────────────── */
#define RCC_BASE    0x40021000UL
#define FLASH_BASE  0x40022000UL
#define GPIOA_BASE  0x40010800UL
#define GPIOB_BASE  0x40010C00UL
#define GPIOC_BASE  0x40011000UL
#define SPI1_BASE   0x40013000UL
#define USART1_BASE 0x40013800UL

typedef struct { volatile uint32_t CR, CFGR, CIR, APB2RSTR, APB1RSTR,
                 AHBENR, APB2ENR, APB1ENR, BDCR, CSR; } RCC_TypeDef;
typedef struct { volatile uint32_t ACR; } FLASH_TypeDef;
typedef struct { volatile uint32_t CRL, CRH, IDR, ODR, BSRR, BRR, LCKR; } GPIO_TypeDef;
typedef struct { volatile uint32_t CR1, CR2, SR, DR, CRCPR, RXCRCR, TXCRCR,
                 I2SCFGR, I2SPR; } SPI_TypeDef;
typedef struct { volatile uint32_t SR, DR, BRR, CR1, CR2, CR3, GTPR; } USART_TypeDef;
typedef struct { volatile uint32_t CTRL, LOAD, VAL, CALIB; } SysTick_TypeDef;

#define RCC     ((RCC_TypeDef *)RCC_BASE)
#define FLASH   ((FLASH_TypeDef *)FLASH_BASE)
#define GPIOA   ((GPIO_TypeDef *)GPIOA_BASE)
#define GPIOB   ((GPIO_TypeDef *)GPIOB_BASE)
#define GPIOC   ((GPIO_TypeDef *)GPIOC_BASE)
#define SPI1    ((SPI_TypeDef *)SPI1_BASE)
#define USART1  ((USART_TypeDef *)USART1_BASE)
#define SYSTICK ((SysTick_TypeDef *)0xE000E010UL)

/* ── RCC ─────────────────────────────────────────────────────────────────── */
#define RCC_CR_HSEON     (1UL << 16)
#define RCC_CR_HSERDY    (1UL << 17)
#define RCC_CR_PLLON     (1UL << 24)
#define RCC_CR_PLLRDY    (1UL << 25)
#define RCC_CFGR_SW_PLL  2UL
#define RCC_CFGR_SWS_PLL (2UL << 2)
#define RCC_CFGR_PLLSRC  (1UL << 16)
#define RCC_CFGR_PLLMUL9 (7UL << 18)
#define RCC_APB2ENR_AFIOEN  (1UL << 0)
#define RCC_APB2ENR_IOPAEN  (1UL << 2)
#define RCC_APB2ENR_IOPBEN  (1UL << 3)
#define RCC_APB2ENR_IOPCEN  (1UL << 4)
#define RCC_APB2ENR_SPI1EN  (1UL << 12)
#define RCC_APB2ENR_USART1EN (1UL << 14)

/* ── USART ───────────────────────────────────────────────────────────────── */
#define USART_SR_TXE  (1UL << 7)
#define USART_SR_TC   (1UL << 6)
#define USART_SR_RXNE (1UL << 5)
#define USART_CR1_UE  (1UL << 13)
#define USART_CR1_TE  (1UL << 3)
#define USART_CR1_RE  (1UL << 2)

/* ── SPI ─────────────────────────────────────────────────────────────────── */
#define SPI_CR1_CPHA (1UL << 0)
#define SPI_CR1_CPOL (1UL << 1)
#define SPI_CR1_MSTR (1UL << 2)
#define SPI_CR1_BR0  (1UL << 3)
#define SPI_CR1_SPE  (1UL << 6)
#define SPI_CR1_SSI  (1UL << 8)
#define SPI_CR1_SSM  (1UL << 9)
#define SPI_SR_RXNE  (1UL << 0)
#define SPI_SR_TXE   (1UL << 1)
#define SPI_SR_BSY   (1UL << 7)

/* ── 引脚 / 常量 ──────────────────────────────────────────────────────────── */
#define CS_PIN      4
#define LED_PIN     13
#define SYS_CLK_HZ  72000000UL
#define UART_BAUD   115200UL
#define OPBUF_SIZE  1024
#define SERBUF_SIZE 4096

#define S_ACK 0x06
#define S_NAK 0x15

volatile uint32_t g_millis = 0;
static uint8_t txbuf[OPBUF_SIZE];
static uint8_t rxbuf[SERBUF_SIZE];

/* SysTick 1ms 中断 */
void SysTick_Handler(void) { g_millis++; }

/* ── 基础 IO ──────────────────────────────────────────────────────────────── */
static void uart_putc(uint8_t c)
{
    while (!(USART1->SR & USART_SR_TXE)) { }
    USART1->DR = c;
}

static void uart_write(const uint8_t *buf, uint32_t len)
{
    while (len--) uart_putc(*buf++);
}

/* 返回 -1 表示超时 */
static int32_t uart_getc(uint32_t timeout_ms)
{
    uint32_t start = g_millis;
    while (!(USART1->SR & USART_SR_RXNE)) {
        if ((g_millis - start) >= timeout_ms) return -1;
    }
    return (uint8_t)USART1->DR;
}

static void cs_set(bool high)
{
    if (high) GPIOA->BSRR = (1UL << CS_PIN);
    else      GPIOA->BRR  = (1UL << CS_PIN);
}

/* 全双工单字节交换 */
static uint8_t spi_xfer(uint8_t out)
{
    while (!(SPI1->SR & SPI_SR_TXE)) { }
    SPI1->DR = out;
    while (!(SPI1->SR & SPI_SR_RXNE)) { }
    return (uint8_t)SPI1->DR;
}

/* serprog O_SPIOP：写 slen 字节，再读 rlen 字节（读期间 MOSI=0xFF） */
static void spi_transaction(const uint8_t *tx, uint32_t slen,
                            uint8_t *rx, uint32_t rlen)
{
    cs_set(false);
    while (slen--) (void)spi_xfer(*tx++);
    while (rlen--) *rx++ = spi_xfer(0xFF);
    while (SPI1->SR & SPI_SR_BSY) { }
    cs_set(true);
}

/* 从 72MHz 里挑一个不超过请求值的最大频率 */
static uint32_t spi_set_freq(uint32_t requested_hz)
{
    static const uint32_t freqs[8] = {
        36000000UL, 18000000UL, 9000000UL, 4500000UL,
        2250000UL, 1125000UL, 562500UL, 281250UL
    };
    uint32_t br = 7;
    for (uint32_t i = 0; i < 8; i++) {
        if (freqs[i] <= requested_hz) { br = i; break; }
    }
    uint32_t cr1 = SPI1->CR1;
    cr1 &= ~(7UL << 3);
    cr1 |= (br << 3) | SPI_CR1_SPE;
    SPI1->CR1 = cr1;
    return freqs[br];
}

static void clock_init(void)
{
    /* HSE 8MHz 起振，PLL x9 = 72MHz，FLASH 2 等待周期 */
    RCC->CR |= RCC_CR_HSEON;
    while (!(RCC->CR & RCC_CR_HSERDY)) { }
    FLASH->ACR = (2UL) | (1UL << 4); /* LATENCY=2, PRFTBE=1 */
    RCC->CFGR |= RCC_CFGR_PLLSRC | RCC_CFGR_PLLMUL9;
    RCC->CR |= RCC_CR_PLLON;
    while (!(RCC->CR & RCC_CR_PLLRDY)) { }
    RCC->CFGR = (RCC->CFGR & ~3UL) | RCC_CFGR_SW_PLL;
    while ((RCC->CFGR & RCC_CFGR_SWS_PLL) != RCC_CFGR_SWS_PLL) { }

    SYSTICK->LOAD = SYS_CLK_HZ / 1000 - 1;
    SYSTICK->VAL = 0;
    SYSTICK->CTRL = 7; /* CLKSOURCE=processor, TICKINT, ENABLE */
}

static void gpio_init(void)
{
    RCC->APB2ENR |= RCC_APB2ENR_AFIOEN | RCC_APB2ENR_IOPAEN |
                    RCC_APB2ENR_IOPBEN | RCC_APB2ENR_IOPCEN |
                    RCC_APB2ENR_SPI1EN | RCC_APB2ENR_USART1EN;

    /* PA4 CS 推挽输出，50MHz；PA5/PA7 SPI 复用推挽；PA6 MISO 浮空输入 */
    uint32_t crl = GPIOA->CRL;
    crl &= ~((0xFUL << (4 * 4)) | (0xFUL << (5 * 4)) |
             (0xFUL << (6 * 4)) | (0xFUL << (7 * 4)));
    crl |= (3UL << (4 * 4));         /* PA4 通用输出 50MHz */
    crl |= (0xBUL << (5 * 4));       /* PA5 复用输出 50MHz */
    crl |= (0x4UL << (6 * 4));       /* PA6 浮空输入 */
    crl |= (0xBUL << (7 * 4));       /* PA7 复用输出 50MHz */
    GPIOA->CRL = crl;
    cs_set(true);

    /* PA9 TX 复用推挽；PA10 RX 浮空输入 */
    uint32_t crh = GPIOA->CRH;
    crh &= ~((0xFUL << ((9 - 8) * 4)) | (0xFUL << ((10 - 8) * 4)));
    crh |= (0xBUL << ((9 - 8) * 4));
    crh |= (0x4UL << ((10 - 8) * 4));
    GPIOA->CRH = crh;

    /* PC13 LED 推挽输出 2MHz */
    uint32_t crhc = GPIOC->CRH;
    crhc &= ~(0xFUL << ((LED_PIN - 8) * 4));
    crhc |= (2UL << ((LED_PIN - 8) * 4));
    GPIOC->CRH = crhc;
    GPIOC->BSRR = (1UL << LED_PIN);
}

static void usart_init(void)
{
    USART1->BRR = SYS_CLK_HZ / UART_BAUD; /* 625 = 0x271 */
    USART1->CR1 = USART_CR1_UE | USART_CR1_TE | USART_CR1_RE;
}

static void spi_init(void)
{
    /* 软件 NSS，主模式，Mode0，MSB，初始 /4 = 18MHz */
    SPI1->CR1 = SPI_CR1_SSM | SPI_CR1_SSI | SPI_CR1_MSTR |
                SPI_CR1_SPE | (1UL << 3);
}

/* ── serprog ─────────────────────────────────────────────────────────────── */
static void cmdmap(uint8_t map[32])
{
    static const uint8_t cmds[] = {
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x07, 0x0F, 0x10, 0x11, 0x12, 0x13
    };
    for (uint32_t i = 0; i < 32; i++) map[i] = 0;
    for (uint32_t i = 0; i < sizeof(cmds); i++) {
        uint8_t c = cmds[i];
        map[c >> 3] |= (uint8_t)(1UL << (c & 7));
    }
}

static void ack_write(const uint8_t *data, uint32_t len)
{
    uart_putc(S_ACK);
    if (len) uart_write(data, len);
}

static void nak(void) { uart_putc(S_NAK); }

/* 读固定长度参数，超时返回 false */
static bool read_params(uint8_t *buf, uint32_t len)
{
    for (uint32_t i = 0; i < len; i++) {
        int32_t c = uart_getc(200);
        if (c < 0) return false;
        buf[i] = (uint8_t)c;
    }
    return true;
}

static void serprog_loop(void)
{
    uint8_t params[8];
    uint32_t last_led = 0;

    for (;;) {
        uint32_t now = g_millis;
        if ((now - last_led) >= 500) {
            last_led = now;
            GPIOC->ODR ^= (1UL << LED_PIN);
        }

        int32_t cmd = uart_getc(1000);
        if (cmd < 0) continue;

        switch (cmd) {
        case 0x00: /* NOP */
            ack_write(NULL, 0);
            break;

        case 0x01: { /* Q_IFACE: 版本 1.0，低字节 minor，高字节 major */
            uint8_t ver[2] = { 0x00, 0x01 };
            ack_write(ver, 2);
            break;
        }

        case 0x02: { /* Q_CMDMAP */
            static uint8_t map[32];
            cmdmap(map);
            ack_write(map, 32);
            break;
        }

        case 0x03: { /* Q_PGMNAME */
            static const char name[16] = "uni-serprog";
            ack_write((const uint8_t *)name, 16);
            break;
        }

        case 0x04: { /* Q_SERBUF */
            uint8_t v[2] = { (uint8_t)(SERBUF_SIZE & 0xFF),
                             (uint8_t)((SERBUF_SIZE >> 8) & 0xFF) };
            ack_write(v, 2);
            break;
        }

        case 0x05: { /* Q_BUSTYPE: SPI = bit3 */
            uint8_t v[4] = { 0x08, 0x00, 0x00, 0x00 };
            ack_write(v, 4);
            break;
        }

        case 0x07: { /* Q_OPBUF */
            uint8_t v[2] = { (uint8_t)(OPBUF_SIZE & 0xFF),
                             (uint8_t)((OPBUF_SIZE >> 8) & 0xFF) };
            ack_write(v, 2);
            break;
        }

        case 0x0F: { /* Q_RDNMAXLEN */
            uint8_t v[4] = { (uint8_t)(SERBUF_SIZE & 0xFF),
                             (uint8_t)((SERBUF_SIZE >> 8) & 0xFF), 0, 0 };
            ack_write(v, 4);
            break;
        }

        case 0x10: /* S_BUSTYPE */
            if (!read_params(params, 1)) { nak(); break; }
            if (params[0] & 0x08) ack_write(NULL, 0);
            else nak();
            break;

        case 0x11: { /* O_SPIOP */
            uint8_t lenbuf[6];
            if (!read_params(lenbuf, 6)) { nak(); break; }
            uint32_t slen = lenbuf[0] | (lenbuf[1] << 8) | (lenbuf[2] << 16);
            uint32_t rlen = lenbuf[3] | (lenbuf[4] << 8) | (lenbuf[5] << 16);
            if (slen > OPBUF_SIZE || rlen > SERBUF_SIZE) { nak(); break; }
            if (!read_params(txbuf, slen)) { nak(); break; }
            spi_transaction(txbuf, slen, rxbuf, rlen);
            ack_write(rxbuf, rlen);
            break;
        }

        case 0x12: { /* S_SPI_FREQ */
            if (!read_params(params, 4)) { nak(); break; }
            uint32_t req = params[0] | (params[1] << 8) |
                           (params[2] << 16) | (params[3] << 24);
            uint32_t actual = spi_set_freq(req);
            uint8_t v[4] = { (uint8_t)(actual & 0xFF),
                             (uint8_t)((actual >> 8) & 0xFF),
                             (uint8_t)((actual >> 16) & 0xFF),
                             (uint8_t)((actual >> 24) & 0xFF) };
            ack_write(v, 4);
            break;
        }

        case 0x13: /* S_PIN_STATE */
            if (!read_params(params, 2)) { nak(); break; }
            if (params[0] == 0) cs_set(params[1] ? true : false);
            ack_write(NULL, 0);
            break;

        default:
            nak();
            break;
        }
    }
}

int main(void)
{
    clock_init();
    gpio_init();
    usart_init();
    spi_init();
    serprog_loop();
    return 0;
}
