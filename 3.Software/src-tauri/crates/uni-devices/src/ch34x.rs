//! CH341A / CH347T / CH347F programmer transport.
//!
//! CH341A and CH347T are ported from IMSProg and talk libusb directly:
//!
//! * CH341A : VID 1A86 / PID 5512, interface 0, EP OUT 0x02 / IN 0x82.
//!   Stream protocol (flashrom's ch341a_spi): every byte is bit-reversed
//!   (the CH341 shifts LSB first), each 32-byte USB packet carries a 0xA8
//!   command byte followed by up to 31 SPI bytes. CS is controlled with the
//!   UIO stream (0xAB) by enabling/disabling the output pins.
//! * CH347T : VID 1A86 / PID 55DB, interface 2, EP OUT 0x06 / IN 0x86.
//!   Vendor protocol from ch347-nor-prog: packet = [cmd, len_lo, len_hi, data].
//!   Commands: 0xCA read config, 0xC0 write SPI config, 0xC1 CS control,
//!   0xC2 full-duplex, 0xC3 block read, 0xC4 block write. Max payload 507.
//!
//! CH347F has no IMSProg/libusb reference implementation, so it keeps the
//! official vendor CH34X.DLL path (CH347OpenDevice / CH347SPI_Init /
//! CH347StreamSPI4), the same mechanism the original project used.
//!
//! Like IMSProg, the higher layer drives CS manually around each command:
//! `cs_low -> spi_tx(opcode/addr) -> spi_rx(data) -> cs_high`.

#[cfg(hal_backend_libusb)]
use rusb::{Device, DeviceHandle, GlobalContext};
use std::time::Duration;

#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const USB_TIMEOUT: Duration = Duration::from_secs(1);

// ── CH341 ────────────────────────────────────────────────────────────────────
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_VID: u16 = 0x1A86;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_PID: u16 = 0x5512;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_IFACE: u8 = 0;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_EP_OUT: u8 = 0x02;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_EP_IN: u8 = 0x82;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_PACKET_LEN: usize = 32;

#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_CMD_I2C_STREAM: u8 = 0xAA; // used for stream configuration
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_CMD_UIO_STREAM: u8 = 0xAB; // GPIO/CS control stream
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_CMD_SPI_STREAM: u8 = 0xA8; // SPI data packet marker

#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_STM_SET: u8 = 0x60;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_STM_END: u8 = 0x00;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_UIO_STM_IN: u8 = 0x00;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_UIO_OUT: u8 = 0x80;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_UIO_DIR: u8 = 0x40;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH341_UIO_END: u8 = 0x20;

// ── CH347 ────────────────────────────────────────────────────────────────────
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_VID: u16 = 0x1A86;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_PID: u16 = 0x55DB;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_IFACE: u8 = 2;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_EP_OUT: u8 = 0x06;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_EP_IN: u8 = 0x86;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_MAX_TRX: usize = 507;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_BUF_LEN: usize = 510; // 3-byte packet header + 507 payload

#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_CMD_SPI_INIT: u8 = 0xC0;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_CMD_SPI_CONTROL: u8 = 0xC1;
#[allow(dead_code)] // kept for the full-duplex path, documented protocol parity
const CH347_CMD_SPI_RD_WR: u8 = 0xC2;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_CMD_SPI_BLCK_RD: u8 = 0xC3;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_CMD_SPI_BLCK_WR: u8 = 0xC4;
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
const CH347_CMD_INFO_RD: u8 = 0xCA;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipKind {
    Ch341A,
    Ch347T,
    Ch347F,
}

/// Bus mode the device is opened in (IMPROG initialises the CH34x
/// differently per chip type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceMode {
    Spi,
    I2c,
    Microwire,
}

/// User-selectable programmer settings (IMPROG semantics).
#[derive(Clone, Debug)]
pub struct Ch34xSettings {
    pub kind: ChipKind,
    /// SPI mode 0..=3. IMSProg uses mode 3; keep it configurable in the UI.
    pub spi_mode: u8,
    /// Requested SPI clock in kHz (CH347 only; CH341 ignores it).
    /// IMSProg: 30 MHz for CH347T v1.0, 15 MHz for v1.1.
    pub freq_khz: u32,
    /// Target rail voltage in mV. VCC supply and SPI/IO signal level are bound
    /// to the same target rail (1200 / 1800 / 2500 / 3300).
    /// 当前硬件 HAL 尚未切换电平，先保留参数供后续驱动使用。
    #[allow(dead_code)]
    pub io_level_mv: u32,
    /// CH34X.DLL device index (0 = first device). Auto-detection fills the
    /// index of the specific device the user selected.
    #[allow(dead_code)] // only read by the DLL HAL build
    pub device_index: u32,
    /// libusb backend: select a specific device by bus/address when several
    /// identical CH34X devices are connected. None = first matching device.
    #[allow(dead_code)] // only read by the libusb HAL build
    pub usb_bus: Option<u8>,
    #[allow(dead_code)] // only read by the libusb HAL build
    pub usb_address: Option<u8>,
}

impl Default for Ch34xSettings {
    fn default() -> Self {
        Self {
            kind: ChipKind::Ch341A,
            spi_mode: 3,
            freq_khz: 15_000,
            io_level_mv: 3300,
            device_index: 0,
            usb_bus: None,
            usb_address: None,
        }
    }
}

/// 硬件抽象层（HAL）。
///
/// 上层协议（protocols.rs）只依赖这个接口，不关心底层是 CH34X.DLL 还是
/// libusb/rusb。新后端（例如 HIDProg）只需要实现本 trait。
pub trait ProgrammerHal: Send {
    fn cs_low(&self) -> Result<(), String>;
    fn cs_high(&self) -> Result<(), String>;
    fn spi_tx(&self, data: &[u8]) -> Result<(), String>;
    fn spi_rx(&self, data: &mut [u8]) -> Result<(), String>;
    fn i2c_write(&self, data: &[u8]) -> Result<(), String>;
    fn i2c_read(&self, data: &mut [u8]) -> Result<usize, String>;
    fn gpio_setbits(&self, bits: u8) -> Result<(), String>;
    fn gpio_getbits(&self) -> Result<u8, String>;
    /// 单个 CS 会话（命令+数据）的最大帧长度。
    fn spi_frame_limit(&self) -> usize;
    /// CH347 系列（I2C 响应有状态头偏移）。
    fn is_ch347(&self) -> bool;
}

/// Swaps the bit order of one byte (CH341 is LSB-first on the wire).
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
fn swap_byte(x: u8) -> u8 {
    let x = ((x >> 1) & 0x55) | ((x << 1) & 0xAA);
    let x = ((x >> 2) & 0x33) | ((x << 2) & 0xCC);
    ((x >> 4) & 0x0F) | ((x << 4) & 0xF0)
}

/// Pure part of CH341 StreamSPI4 framing: 32 zero bytes + per-packet
/// [0xA8][up to 31 payload bytes], with all data bits reversed.
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
fn ch341_build_stream(write: &[u8], read_len: usize) -> Vec<u8> {
    let total = write.len() + read_len;
    let packets = if total == 0 {
        0
    } else {
        (total + (CH341_PACKET_LEN - 2)) / (CH341_PACKET_LEN - 1)
    };

    let mut out = Vec::with_capacity(CH341_PACKET_LEN + packets + total);
    out.resize(CH341_PACKET_LEN, 0);

    let mut w_left = write.len();
    let mut r_left = read_len;
    let mut w_pos = 0;
    for _ in 0..packets {
        out.push(CH341_CMD_SPI_STREAM);
        let w_now = w_left.min(CH341_PACKET_LEN - 1);
        for i in 0..w_now {
            out.push(swap_byte(write[w_pos + i]));
        }
        w_pos += w_now;
        w_left -= w_now;
        let r_now = r_left.min(CH341_PACKET_LEN - 1 - w_now);
        out.resize(out.len() + r_now, 0xFF);
        r_left -= r_now;
    }
    out
}

/// Extract the read phase from a CH341 response buffer and reverse the bits.
#[cfg_attr(not(hal_backend_libusb), allow(dead_code))]
fn ch341_extract_read(write_len: usize, read: &mut [u8], response: &[u8]) {
    debug_assert!(response.len() >= write_len + read.len());
    for (i, byte) in read.iter_mut().enumerate() {
        *byte = swap_byte(response[write_len + i]);
    }
}

// ═══════════════════════════════════════ CH341 ═══════════════════════════════

#[cfg(hal_backend_libusb)]
fn open_device_matching(
    vid: u16,
    pid: u16,
    bus: Option<u8>,
    address: Option<u8>,
) -> Option<Device<GlobalContext>> {
    let devices = rusb::devices().ok()?;
    for device in devices.iter() {
        let Ok(desc) = device.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() != vid || desc.product_id() != pid {
            continue;
        }
        if let Some(want) = bus {
            if device.bus_number() != want {
                continue;
            }
        }
        if let Some(want) = address {
            if device.address() != want {
                continue;
            }
        }
        return Some(device);
    }
    None
}

#[cfg(hal_backend_libusb)]
pub fn enumerate_libusb_devices() -> Vec<(ChipKind, u8, u8)> {
    let mut found = Vec::new();
    let Ok(devices) = rusb::devices() else {
        return found;
    };
    for device in devices.iter() {
        let Ok(desc) = device.device_descriptor() else {
            continue;
        };
        if desc.vendor_id() != CH347_VID && desc.vendor_id() != CH341_VID {
            continue;
        }
        let kind = match desc.product_id() {
            CH341_PID => ChipKind::Ch341A,
            CH347_PID => ChipKind::Ch347T,
            // CH347F has no libusb transport in this project yet.
            _ => continue,
        };
        found.push((kind, device.bus_number(), device.address()));
    }
    found
}

#[cfg(hal_backend_libusb)]
pub(crate) struct Ch341 {
    handle: DeviceHandle<GlobalContext>,
}

#[cfg(hal_backend_libusb)]
impl Ch341 {
    fn open(settings: &Ch34xSettings, mode: DeviceMode) -> Result<Self, String> {
        let device =
            open_device_matching(CH341_VID, CH341_PID, settings.usb_bus, settings.usb_address)
                .ok_or_else(|| "未找到 CH341A 设备 (1A86:5512)".to_string())?;
        let handle = device
            .open()
            .map_err(|e| format!("打开 CH341A 设备失败: {e}"))?;
        handle
            .set_auto_detach_kernel_driver(true)
            .map_err(|e| format!("设置内核驱动自动分离失败: {e}"))?;
        handle
            .claim_interface(CH341_IFACE)
            .map_err(|e| format!("占用 CH341A 接口失败: {e}"))?;

        let dev = Ch341 { handle };
        match mode {
            DeviceMode::Spi => {
                dev.config_stream(0x03)?;
                dev.enable_pins(false)?;
            }
            DeviceMode::I2c => {
                dev.config_stream(0x01)?; // 100 kHz standard speed
            }
            DeviceMode::Microwire => {
                dev.config_stream(0x01)?;
                dev.enable_pins(false)?;
                dev.gpio_setdir()?;
            }
        }
        Ok(dev)
    }

    /// IMSProg ch341a_spi.c config_stream(): select the SPI/I2C stream.
    /// Speed bits are I2C related; SPI uses 0x03 (0x63), I2C uses 0..3.
    fn config_stream(&self, speed: u8) -> Result<(), String> {
        let buf = [
            CH341_CMD_I2C_STREAM,
            CH341_STM_SET | (speed & 0x07),
            CH341_STM_END,
        ];
        self.write_bulk(&buf)
            .map_err(|e| format!("CH341 配置流模式失败: {e}"))?;
        Ok(())
    }

    /// IMSProg ch341a_spi.c enable_pins(): CS/SCK/MOSI control via UIO stream.
    /// enable=false deselects the chip (the last byte is DIR=0x00).
    fn enable_pins(&self, enable: bool) -> Result<(), String> {
        let buf = [
            CH341_CMD_UIO_STREAM,
            CH341_UIO_OUT | 0x37, // CS high, SCK=0, DO=1
            CH341_UIO_OUT | 0x37,
            CH341_UIO_OUT | 0x37,
            CH341_UIO_OUT | 0x37,
            CH341_UIO_OUT | 0x37,
            CH341_UIO_OUT | 0x36, // CS low
            CH341_UIO_DIR | if enable { 0x3F } else { 0x00 },
            CH341_UIO_END,
        ];
        self.write_bulk(&buf)
            .map_err(|e| format!("CH341 切换 CS 失败: {e}"))?;
        Ok(())
    }

    fn write_bulk(&self, buf: &[u8]) -> Result<usize, String> {
        self.handle
            .write_bulk(CH341_EP_OUT, buf, USB_TIMEOUT)
            .map_err(|e| format!("CH341 写入失败: {e}"))
    }

    fn read_chunks(&self, buf: &mut [u8]) -> Result<(), String> {
        let mut done = 0;
        while done < buf.len() {
            // flashrom queues IN transfers of at most 31 bytes because each
            // 32-byte bulk packet carries at most 31 data bytes.
            let chunk_len = (buf.len() - done).min(CH341_PACKET_LEN - 1);
            let n = self
                .handle
                .read_bulk(CH341_EP_IN, &mut buf[done..done + chunk_len], USB_TIMEOUT)
                .map_err(|e| format!("CH341 读取失败: {e}"))?;
            if n == 0 {
                return Err("CH341 读取失败: 设备返回 0 字节".into());
            }
            done += n;
        }
        Ok(())
    }

    /// Port of ch341a_spi_send_command(writecnt, readcnt):
    /// one 32-byte zero block, then per packet [0xA8][payload].
    /// Reads back writecnt+readcnt bytes; response region for the read phase
    /// starts at `writecnt` (the write echo is discarded).
    fn send_command(&self, write: &[u8], read: &mut [u8]) -> Result<(), String> {
        let out = ch341_build_stream(write, read.len());
        self.write_bulk(&out)?;

        let mut in_buf = vec![0u8; write.len() + read.len()];
        self.read_chunks(&mut in_buf)?;
        ch341_extract_read(write.len(), read, &in_buf);
        Ok(())
    }

    fn cs_low(&self) -> Result<(), String> {
        self.enable_pins(true)
    }

    fn cs_high(&self) -> Result<(), String> {
        self.enable_pins(false)
    }

    fn spi_tx(&self, data: &[u8]) -> Result<(), String> {
        self.send_command(data, &mut [])
    }

    fn spi_rx(&self, data: &mut [u8]) -> Result<(), String> {
        self.send_command(&[], data)
    }

    /// I2C stream write (opcode 0xAA packets, same bulk endpoint).
    fn i2c_write(&self, data: &[u8]) -> Result<(), String> {
        self.write_bulk(data).map(|_| ())
    }

    fn i2c_read(&self, data: &mut [u8]) -> Result<(), String> {
        let n = self
            .handle
            .read_bulk(CH341_EP_IN, data, USB_TIMEOUT)
            .map_err(|e| format!("CH341 I2C 读取失败: {e}"))?;
        if n != data.len() {
            return Err(format!(
                "CH341 I2C 读取长度不符: 期望 {} 实际 {}",
                data.len(),
                n
            ));
        }
        Ok(())
    }

    fn gpio_setdir(&self) -> Result<(), String> {
        let buf = [CH341_CMD_UIO_STREAM, CH341_UIO_DIR | 0x3F, CH341_UIO_END];
        self.write_bulk(&buf)
            .map_err(|e| format!("CH341 GPIO 方向设置失败: {e}"))?;
        Ok(())
    }

    fn gpio_setbits(&self, bits: u8) -> Result<(), String> {
        let buf = [CH341_CMD_UIO_STREAM, CH341_UIO_OUT | bits, CH341_UIO_END];
        self.write_bulk(&buf)
            .map_err(|e| format!("CH341 GPIO 写位失败: {e}"))?;
        Ok(())
    }

    fn gpio_getbits(&self) -> Result<u8, String> {
        let buf = [CH341_CMD_UIO_STREAM, CH341_UIO_STM_IN, CH341_UIO_END];
        self.write_bulk(&buf)?;
        let mut data = [0u8; 1];
        self.handle
            .read_bulk(CH341_EP_IN, &mut data, USB_TIMEOUT)
            .map_err(|e| format!("CH341 GPIO 读位失败: {e}"))?;
        Ok(data[0])
    }
}

#[cfg(hal_backend_libusb)]
impl Drop for Ch341 {
    fn drop(&mut self) {
        let _ = self.enable_pins(false);
        let _ = self.handle.release_interface(CH341_IFACE);
    }
}

#[cfg(hal_backend_libusb)]
impl ProgrammerHal for Ch341 {
    fn cs_low(&self) -> Result<(), String> {
        self.cs_low()
    }

    fn cs_high(&self) -> Result<(), String> {
        self.cs_high()
    }

    fn spi_tx(&self, data: &[u8]) -> Result<(), String> {
        self.spi_tx(data)
    }

    fn spi_rx(&self, data: &mut [u8]) -> Result<(), String> {
        self.spi_rx(data)
    }

    fn i2c_write(&self, data: &[u8]) -> Result<(), String> {
        self.i2c_write(data)
    }

    fn i2c_read(&self, data: &mut [u8]) -> Result<usize, String> {
        self.i2c_read(data)?;
        Ok(data.len())
    }

    fn gpio_setbits(&self, bits: u8) -> Result<(), String> {
        self.gpio_setbits(bits)
    }

    fn gpio_getbits(&self) -> Result<u8, String> {
        self.gpio_getbits()
    }

    fn spi_frame_limit(&self) -> usize {
        4096
    }

    fn is_ch347(&self) -> bool {
        false
    }
}

// ═══════════════════════════════════════ CH347 ═══════════════════════════════

#[cfg(hal_backend_libusb)]
pub(crate) struct Ch347 {
    handle: DeviceHandle<GlobalContext>,
    cfg: [u8; 40],
}

#[cfg(hal_backend_libusb)]
impl Ch347 {
    fn open(settings: &Ch34xSettings, mode: DeviceMode) -> Result<Self, String> {
        let device =
            open_device_matching(CH347_VID, CH347_PID, settings.usb_bus, settings.usb_address)
                .ok_or_else(|| "未找到 CH347T 设备 (1A86:55DB)".to_string())?;
        let handle = device
            .open()
            .map_err(|e| format!("打开 CH347T 设备失败: {e}"))?;
        handle
            .set_auto_detach_kernel_driver(true)
            .map_err(|e| format!("设置内核驱动自动分离失败: {e}"))?;
        handle
            .claim_interface(CH347_IFACE)
            .map_err(|e| format!("占用 CH347T 接口失败: {e}"))?;

        let mut dev = Ch347 {
            handle,
            cfg: [0u8; 40],
        };
        match mode {
            DeviceMode::Spi => {
                dev.read_hw_config()?;
                dev.setup_spi(settings.spi_mode, false)?;
                dev.set_freq(settings.freq_khz)?;
            }
            DeviceMode::I2c => {
                dev.set_i2c_stream(0x01)?;
                dev.set_sda_scl_high()?;
            }
            DeviceMode::Microwire => {
                return Err("CH347T 不支持 Microwire 协议（与 IMSProg 一致）".into());
            }
        }
        Ok(dev)
    }

    /// IMSProg ch347setI2Cstream(): 0xAA stream with 0x60|speed.
    fn set_i2c_stream(&self, speed: u8) -> Result<(), String> {
        let buf = [0xAA, 0x60 | (speed & 0x03), 0x00];
        self.write_bulk(&buf)?;
        Ok(())
    }

    /// IMSProg ch347setSDAandSCLHighlevels().
    fn set_sda_scl_high(&self) -> Result<(), String> {
        let buf = [0xAA, 0x12, 0x00];
        self.write_bulk(&buf)?;
        Ok(())
    }

    fn write_bulk(&self, buf: &[u8]) -> Result<usize, String> {
        self.handle
            .write_bulk(CH347_EP_OUT, buf, USB_TIMEOUT)
            .map_err(|e| format!("CH347 写入失败: {e}"))
    }

    /// CH347 packet writer: [cmd, len_lo, len_hi] + payload. If the payload
    /// exceeds CH347_MAX_TRX the remainder is sent as a raw follow-up transfer.
    fn write_packet(&self, cmd: u8, data: &[u8]) -> Result<(), String> {
        let first = data.len().min(CH347_MAX_TRX);
        let mut buf = Vec::with_capacity(first + 3);
        buf.push(cmd);
        buf.push((data.len() & 0xFF) as u8);
        buf.push(((data.len() >> 8) & 0xFF) as u8);
        buf.extend_from_slice(&data[..first]);
        self.write_bulk(&buf)?;
        if first < data.len() {
            self.write_bulk(&data[first..])?;
        }
        Ok(())
    }

    /// CH347 packet reader: first read CH347_BUF_LEN bytes, validate cmd and
    /// declared length, then read any remaining payload directly.
    fn read_packet(&self, cmd: u8, rx: &mut [u8]) -> Result<usize, String> {
        let mut tmp = vec![0u8; CH347_BUF_LEN];
        let transferred = self
            .handle
            .read_bulk(CH347_EP_IN, &mut tmp, USB_TIMEOUT)
            .map_err(|e| format!("CH347 读取失败: {e}"))?;
        if transferred < 3 || tmp[0] != cmd {
            return Err(format!(
                "CH347 响应头错误: 期望命令 0x{:02X}, 收到 {} 字节",
                cmd, transferred
            ));
        }
        let rxlen = tmp[1] as usize | ((tmp[2] as usize) << 8);
        if rxlen > rx.len() {
            return Err(format!(
                "CH347 响应过长: 声明 {} 字节, 缓冲区 {} 字节",
                rxlen,
                rx.len()
            ));
        }
        let mut done = 0;
        let first = (transferred - 3).min(rxlen);
        rx[..first].copy_from_slice(&tmp[3..3 + first]);
        done += first;
        while done < rxlen {
            let n = self
                .handle
                .read_bulk(CH347_EP_IN, &mut rx[done..rxlen], USB_TIMEOUT)
                .map_err(|e| format!("CH347 读取剩余数据失败: {e}"))?;
            if n == 0 {
                return Err("CH347 读取剩余数据失败: 设备返回 0 字节".into());
            }
            done += n;
        }
        Ok(rxlen)
    }

    fn read_hw_config(&mut self) -> Result<(), String> {
        self.write_packet(CH347_CMD_INFO_RD, &[0x01])?;
        let mut cfg = [0u8; 40];
        let got = self.read_packet(CH347_CMD_INFO_RD, &mut cfg)?;
        if got != cfg.len() {
            return Err(format!(
                "CH347 硬件配置长度不符: 期望 {} 字节, 实际 {} 字节",
                cfg.len(),
                got
            ));
        }
        self.cfg = cfg;
        Ok(())
    }

    fn put_u16(cfg: &mut [u8], off: usize, val: u16) {
        cfg[off] = (val & 0xFF) as u8;
        cfg[off + 1] = ((val >> 8) & 0xFF) as u8;
    }

    /// ch347_setup_spi(): mode 0..=3, MSB first, software CS, full duplex 8 bit.
    fn setup_spi(&mut self, spi_mode: u8, _lsb_first: bool) -> Result<(), String> {
        let mode = spi_mode & 0x03;
        let cpol = if mode & 2 != 0 { 0x0002u16 } else { 0x0000u16 };
        let cpha = if mode & 1 != 0 { 0x0001u16 } else { 0x0000u16 };

        Self::put_u16(&mut self.cfg, 0, 0x0000); // 2 lines full duplex
        Self::put_u16(&mut self.cfg, 2, 0x0104); // master
        Self::put_u16(&mut self.cfg, 4, 0x0000); // 8 bit
        Self::put_u16(&mut self.cfg, 6, cpol);
        Self::put_u16(&mut self.cfg, 8, cpha);
        Self::put_u16(&mut self.cfg, 10, 0x0200); // software CS
                                                  // baud prescaler at offset 12: set_freq overwrites it
        Self::put_u16(&mut self.cfg, 14, 0x0000); // MSB first
                                                  // CRC polynomial (offset 16) untouched
        Self::put_u16(&mut self.cfg, 18, 0x0000); // write/read interval
        self.cfg[20] = 0x00; // MOSI default data
                             // OtherCfg: keep I2C bits, clear both CS polarities (active low)
        self.cfg[21] &= 0x3F;

        self.commit_settings()
    }

    /// ch347_set_spi_freq(): prescaler = x*8; x=0 -> 60 MHz, each step halves.
    fn set_freq(&mut self, freq_khz: u32) -> Result<(), String> {
        let mut freq = 60_000u32;
        let mut prescaler = 0u16;
        while prescaler < 7 && freq > freq_khz {
            freq /= 2;
            prescaler += 1;
        }
        Self::put_u16(&mut self.cfg, 12, prescaler * 8);
        self.commit_settings()
    }

    fn commit_settings(&self) -> Result<(), String> {
        self.write_packet(CH347_CMD_SPI_INIT, &self.cfg)?;
        let mut ack = [0u8; 1];
        self.read_packet(CH347_CMD_SPI_INIT, &mut ack)?;
        Ok(())
    }

    fn cs(&self, cs: u8, val: bool) -> Result<(), String> {
        let pkt_val: u8 = if val { 0xC0 } else { 0x80 };
        let mut buf = [0u8; 10];
        if cs == 0 {
            buf[0] = pkt_val;
        } else {
            buf[5] = pkt_val;
        }
        self.write_packet(CH347_CMD_SPI_CONTROL, &buf)
    }

    fn spi_tx(&self, data: &[u8]) -> Result<(), String> {
        for chunk in data.chunks(CH347_MAX_TRX) {
            self.write_packet(CH347_CMD_SPI_BLCK_WR, chunk)?;
            let mut ack = [0u8; 1];
            self.read_packet(CH347_CMD_SPI_BLCK_WR, &mut ack)?;
        }
        Ok(())
    }

    fn spi_rx(&self, data: &mut [u8]) -> Result<(), String> {
        let len_bytes = (data.len() as u32).to_le_bytes();
        self.write_packet(CH347_CMD_SPI_BLCK_RD, &len_bytes)?;
        let mut done = 0;
        while done < data.len() {
            let chunk_len = (data.len() - done).min(CH347_MAX_TRX);
            let got = self.read_packet(CH347_CMD_SPI_BLCK_RD, &mut data[done..done + chunk_len])?;
            if got == 0 {
                return Err("CH347 块读取失败: 设备返回 0 字节".into());
            }
            done += got;
        }
        Ok(())
    }

    /// I2C stream write through the same bulk endpoint.
    fn i2c_write(&self, data: &[u8]) -> Result<(), String> {
        self.write_bulk(data).map(|_| ())
    }

    /// I2C stream read; CH347 firmware echoes status bytes which the caller
    /// offsets past (IMPROG skips 3 or 4 header bytes).
    fn i2c_read(&self, data: &mut [u8]) -> Result<usize, String> {
        let n = self
            .handle
            .read_bulk(CH347_EP_IN, data, USB_TIMEOUT)
            .map_err(|e| format!("CH347 I2C 读取失败: {e}"))?;
        Ok(n)
    }

    fn cs_low(&self) -> Result<(), String> {
        self.cs(0, false)
    }

    fn cs_high(&self) -> Result<(), String> {
        self.cs(0, true)
    }
}

#[cfg(hal_backend_libusb)]
impl Drop for Ch347 {
    fn drop(&mut self) {
        let _ = self.cs_high();
        let _ = self.handle.release_interface(CH347_IFACE);
    }
}

#[cfg(hal_backend_libusb)]
impl ProgrammerHal for Ch347 {
    fn cs_low(&self) -> Result<(), String> {
        self.cs_low()
    }

    fn cs_high(&self) -> Result<(), String> {
        self.cs_high()
    }

    fn spi_tx(&self, data: &[u8]) -> Result<(), String> {
        self.spi_tx(data)
    }

    fn spi_rx(&self, data: &mut [u8]) -> Result<(), String> {
        self.spi_rx(data)
    }

    fn i2c_write(&self, data: &[u8]) -> Result<(), String> {
        self.i2c_write(data)
    }

    fn i2c_read(&self, data: &mut [u8]) -> Result<usize, String> {
        self.i2c_read(data)
    }

    fn gpio_setbits(&self, _bits: u8) -> Result<(), String> {
        Err("CH347T 不支持 Microwire GPIO".into())
    }

    fn gpio_getbits(&self) -> Result<u8, String> {
        Err("CH347T 不支持 Microwire GPIO".into())
    }

    fn spi_frame_limit(&self) -> usize {
        4096
    }

    fn is_ch347(&self) -> bool {
        true
    }
}

// ═════════════════════════ CH347F via official CH34X.DLL ═════════════════════
//
// ═══════════════════ CH34X.DLL HAL（Windows 默认后端） ════════════════════════
//
// Windows 上普通用户安装 WCH 官方驱动即可使用，不需要 WinUSB/Zadig。
// CH341 / CH347T / CH347F 三种芯片统一走官方 DLL：
//   CH341  : CH341OpenDevice / CH341SetStream / CH341StreamSPI4
//   CH347T/F: CH347OpenDevice / CH347SPI_Init / CH347StreamSPI4
// I2C / Microwire GPIO 使用 DLL 的原始 WriteData/ReadData 收发流包。

#[cfg(hal_backend_dll)]
mod dll_hal {
    use super::{Ch34xSettings, ChipKind, DeviceMode, ProgrammerHal};
    use std::cell::RefCell;
    use std::ffi::CString;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;

    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::Foundation::{FreeLibrary, HMODULE};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    type FnOpen = unsafe extern "system" fn(u32) -> *mut std::ffi::c_void;
    type FnGetProc = unsafe extern "system" fn() -> isize;
    type FnClose = unsafe extern "system" fn(u32) -> i32;
    type FnStream = unsafe extern "system" fn(u32, u32, u32, *mut u8) -> i32;
    type FnInit = unsafe extern "system" fn(u32, *const SpiCfg) -> i32;
    type FnGetChipType = unsafe extern "system" fn(u32) -> u8;
    type FnSetStream = unsafe extern "system" fn(u32, u32) -> i32;
    type FnSetDataBits = unsafe extern "system" fn(u32, u8) -> i32;
    type FnWriteData = unsafe extern "system" fn(u32, *mut std::ffi::c_void, *mut u32) -> i32;
    type FnReadData = unsafe extern "system" fn(u32, *mut std::ffi::c_void, *mut u32) -> i32;

    const CHIP_TYPE_CH341: u8 = 0;
    const CHIP_TYPE_CH347T: u8 = 1;
    const CHIP_TYPE_CH347F: u8 = 2;
    const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = usize::MAX as *mut _;

    /// 旧项目验证过的 DLL 缓冲上限：CH341 FullSpeed 512 字节包，
    /// CH347/CH347F HighSpeed 4096 字节包（各留 4 字节帧头余量）。
    const DLL_FRAME_LIMIT_CH341: usize = 508;
    const DLL_FRAME_LIMIT_CH347: usize = 4092;

    #[repr(C, packed)]
    struct SpiCfg {
        i_mode: u8,
        i_clock: u8,
        i_byte_order: u8,
        i_spi_write_read_interval: u16,
        i_spi_out_default_data: u8,
        i_chip_select: u32,
        cs1_polarity: u8,
        cs2_polarity: u8,
        i_is_auto_deactive_cs: u16,
        i_active_delay: u16,
        i_delay_deactive: u32,
    }

    pub struct DllHal {
        lib: HMODULE,
        #[allow(dead_code)] // open-handle sentinel; DLL tracks device by index
        handle: *mut std::ffi::c_void,
        idx: u32,
        kind: ChipKind,
        close: FnClose,
        stream: FnStream,
        write_data: FnWriteData,
        read_data: FnReadData,
        pending: RefCell<Vec<u8>>,
        frame_limit: usize,
    }

    unsafe impl Send for DllHal {}
    unsafe impl Sync for DllHal {}

    fn freq_to_clock_index(freq_khz: u32) -> u8 {
        match freq_khz {
            60_000..=u32::MAX => 0,
            30_000..=59_999 => 1,
            15_000..=29_999 => 2,
            7_500..=14_999 => 3,
            3_750..=7_499 => 4,
            1_875..=3_749 => 5,
            937..=1_874 => 6,
            _ => 7,
        }
    }

    impl DllHal {
        fn find_dll(filename: &str) -> Result<PathBuf, String> {
            let mut paths = Vec::new();
            if let Ok(cwd) = std::env::current_dir() {
                paths.push(cwd.clone());
                let src_tauri = cwd.join("src-tauri");
                if src_tauri.exists() {
                    paths.push(src_tauri);
                }
            }
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    paths.push(dir.to_path_buf());
                }
            }
            for base in &paths {
                let full = base.join(filename);
                if full.exists() {
                    return Ok(full);
                }
            }
            Err(format!("未找到 {}，已搜索: {:?}", filename, paths))
        }

        pub fn open(settings: &Ch34xSettings, mode: DeviceMode) -> Result<Self, String> {
            let path = Self::find_dll("CH34X.DLL")?;
            // 路径可能包含中文等非 ASCII 字符：LoadLibraryA 会按系统 ANSI
            // 代码页解释字节，UTF-8 路径会失败，因此必须使用宽字符版本。
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let lib = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) }
                .map_err(|e| format!("加载 CH34X.DLL 失败: {e}"))?;

            let prefix = match settings.kind {
                ChipKind::Ch341A => "CH341",
                ChipKind::Ch347T | ChipKind::Ch347F => "CH347",
            };

            macro_rules! get_fn {
                ($name:expr) => {{
                    let fname = CString::new($name.as_str()).unwrap();
                    let ptr = GetProcAddress(lib, PCSTR(fname.as_ptr() as *const u8));
                    if ptr.is_none() {
                        return Err(format!("CH34X.DLL 缺少导出函数 {}", $name));
                    }
                    ptr.unwrap()
                }};
            }

            let open: FnOpen =
                unsafe { std::mem::transmute(get_fn!(&format!("{}OpenDevice", prefix))) };
            let close: FnClose =
                unsafe { std::mem::transmute(get_fn!(&format!("{}CloseDevice", prefix))) };
            let stream: FnStream =
                unsafe { std::mem::transmute(get_fn!(&format!("{}StreamSPI4", prefix))) };
            let write_data: FnWriteData =
                unsafe { std::mem::transmute(get_fn!(&format!("{}WriteData", prefix))) };
            let read_data: FnReadData =
                unsafe { std::mem::transmute(get_fn!(&format!("{}ReadData", prefix))) };

            // 官方头文件：CH347F 可设置 SPI 数据位宽（0=8bit, 1=16bit）。
            // 旧 DLL 可能没有该导出，因此按可选函数处理。
            let set_data_bits: Option<FnSetDataBits> = {
                let name = CString::new("CH347SPI_SetDataBits").unwrap();
                unsafe { GetProcAddress(lib, PCSTR(name.as_ptr() as *const u8)) }
                    .map(|ptr| unsafe { std::mem::transmute::<FnGetProc, FnSetDataBits>(ptr) })
            };

            // 芯片类型函数：新 DLL 用 CH347GetChipType，旧纯 CH341 DLL 只有 CH341GetChipType
            let get_type: FnGetChipType = {
                let name = format!("{}GetChipType", prefix);
                match unsafe {
                    GetProcAddress(
                        lib,
                        PCSTR(CString::new(name.as_str()).unwrap().as_ptr() as *const u8),
                    )
                } {
                    Some(ptr) => unsafe { std::mem::transmute::<FnGetProc, FnGetChipType>(ptr) },
                    None => {
                        let fname = CString::new("CH347GetChipType").unwrap();
                        let ptr =
                            unsafe { GetProcAddress(lib, PCSTR(fname.as_ptr() as *const u8)) }
                                .ok_or_else(|| format!("CH34X.DLL 缺少导出函数 {}", name))?;
                        unsafe { std::mem::transmute::<FnGetProc, FnGetChipType>(ptr) }
                    }
                }
            };

            let handle = unsafe { open(settings.device_index) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(format!(
                    "打开 {:?} 设备 {} 失败：请检查设备与驱动",
                    settings.kind, settings.device_index
                ));
            }

            let chip_type = unsafe { get_type(settings.device_index) };
            let expected = match settings.kind {
                ChipKind::Ch341A => CHIP_TYPE_CH341,
                ChipKind::Ch347T => CHIP_TYPE_CH347T,
                ChipKind::Ch347F => CHIP_TYPE_CH347F,
            };
            if chip_type != expected {
                return Err(format!(
                    "设备芯片类型为 {}，与所选 {:?}（类型 {}）不符",
                    chip_type, settings.kind, expected
                ));
            }

            // 初始化各芯片的 SPI 控制器
            match settings.kind {
                ChipKind::Ch341A => {
                    if mode != DeviceMode::I2c {
                        let set_name = CString::new("CH341SetStream").unwrap();
                        let set_ptr =
                            unsafe { GetProcAddress(lib, PCSTR(set_name.as_ptr() as *const u8)) };
                        if let Some(ptr) = set_ptr {
                            let set_stream: FnSetStream = unsafe { std::mem::transmute(ptr) };
                            if unsafe { set_stream(settings.device_index, 0x80) } == 0 {
                                return Err("CH341 SetStream(SPI) 失败".into());
                            }
                        }
                    }
                    if mode == DeviceMode::Microwire {
                        let buf = [0xABu8, 0x40 | 0x3F, 0x20];
                        let mut len = buf.len() as u32;
                        if unsafe {
                            write_data(settings.device_index, buf.as_ptr() as *mut _, &mut len)
                        } == 0
                        {
                            return Err("CH341 GPIO 方向设置失败".into());
                        }
                    }
                }
                ChipKind::Ch347T | ChipKind::Ch347F => {
                    if mode == DeviceMode::Microwire {
                        return Err("CH347T/CH347F 不支持 Microwire".into());
                    }
                    let init_name = CString::new("CH347SPI_Init").unwrap();
                    let init_ptr =
                        unsafe { GetProcAddress(lib, PCSTR(init_name.as_ptr() as *const u8)) }
                            .ok_or("CH34X.DLL 缺少导出函数 CH347SPI_Init")?;
                    let init: FnInit = unsafe { std::mem::transmute(init_ptr) };
                    let cfg = SpiCfg {
                        i_mode: settings.spi_mode,
                        i_clock: freq_to_clock_index(settings.freq_khz),
                        i_byte_order: 1,
                        i_spi_write_read_interval: 0,
                        i_spi_out_default_data: 0xFF,
                        i_chip_select: 0x80,
                        cs1_polarity: 0,
                        cs2_polarity: 0,
                        i_is_auto_deactive_cs: 0,
                        i_active_delay: 0,
                        i_delay_deactive: 0,
                    };
                    let cfg_copy = cfg;
                    if unsafe { init(settings.device_index, &cfg_copy as *const SpiCfg) } == 0 {
                        return Err(format!("{:?} SPI 初始化失败", settings.kind));
                    }
                    // 官方 API：CH347F 需要显式设置 8bit 数据位宽，避免
                    // 默认 16bit 模式导致 Flash 收发数据全部错位。
                    if settings.kind == ChipKind::Ch347F {
                        let set_bits = set_data_bits
                            .ok_or("CH34X.DLL 缺少导出函数 CH347SPI_SetDataBits（CH347F 必需）")?;
                        if unsafe { set_bits(settings.device_index, 0) } == 0 {
                            return Err("CH347F SPI 数据位宽设置失败".into());
                        }
                    }
                }
            }

            let frame_limit = match settings.kind {
                ChipKind::Ch341A => DLL_FRAME_LIMIT_CH341,
                ChipKind::Ch347T | ChipKind::Ch347F => DLL_FRAME_LIMIT_CH347,
            };

            Ok(DllHal {
                lib,
                handle,
                idx: settings.device_index,
                kind: settings.kind,
                close,
                stream,
                write_data,
                read_data,
                pending: RefCell::new(Vec::new()),
                frame_limit,
            })
        }

        /// Enumerate CH34X devices through the official DLL without
        /// initialising SPI. Returns `(device_index, chip_kind)` pairs.
        pub fn enumerate() -> Result<Vec<(u32, ChipKind)>, String> {
            let path = Self::find_dll("CH34X.DLL")?;
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let lib = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) }
                .map_err(|e| format!("加载 CH34X.DLL 失败: {e}"))?;

            let mut found: Vec<(u32, ChipKind)> = Vec::new();
            let mut seen_index = [false; 8];
            // 新版 WCH DLL 同时导出 CH341* 和 CH347* 两套 API。CH347* 能
            // 正确报告 CH341/CH347T/CH347F 类型，因此优先用它；CH341* 只
            // 用来兜底旧版纯 CH341 DLL，避免同一台 CH347T 被重复识别成
            // “CH341A + CH347T”两个候选。
            for prefix in ["CH347", "CH341"] {
                let open_name = CString::new(format!("{}OpenDevice", prefix)).unwrap();
                let close_name = CString::new(format!("{}CloseDevice", prefix)).unwrap();
                let type_name = CString::new(format!("{}GetChipType", prefix)).unwrap();

                let Some(open_ptr) =
                    (unsafe { GetProcAddress(lib, PCSTR(open_name.as_ptr() as *const u8)) })
                else {
                    continue;
                };
                let Some(close_ptr) =
                    (unsafe { GetProcAddress(lib, PCSTR(close_name.as_ptr() as *const u8)) })
                else {
                    continue;
                };
                let open: FnOpen = unsafe { std::mem::transmute::<FnGetProc, FnOpen>(open_ptr) };
                let close: FnClose =
                    unsafe { std::mem::transmute::<FnGetProc, FnClose>(close_ptr) };
                let get_type: Option<FnGetChipType> =
                    unsafe { GetProcAddress(lib, PCSTR(type_name.as_ptr() as *const u8)) }
                        .map(|ptr| unsafe { std::mem::transmute::<FnGetProc, FnGetChipType>(ptr) });

                for index in 0..8u32 {
                    let handle = unsafe { open(index) };
                    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                        continue;
                    }
                    let chip_type = match get_type {
                        Some(get) => unsafe { get(index) },
                        None if prefix == "CH341" => CHIP_TYPE_CH341,
                        None => CHIP_TYPE_CH347T,
                    };
                    unsafe {
                        close(index);
                    }
                    let kind = match chip_type {
                        CHIP_TYPE_CH341 => ChipKind::Ch341A,
                        CHIP_TYPE_CH347T => ChipKind::Ch347T,
                        CHIP_TYPE_CH347F => ChipKind::Ch347F,
                        _ => continue, // CH339W and unknown WCH chips are not programmers
                    };
                    let idx = index as usize;
                    if prefix == "CH341" && seen_index[idx] {
                        continue;
                    }
                    seen_index[idx] = true;
                    found.push((index, kind));
                }
            }

            unsafe {
                let _ = FreeLibrary(lib);
            }
            found.sort_by_key(|(idx, _)| *idx);
            found.dedup_by_key(|(idx, kind)| (*idx, *kind));
            Ok(found)
        }

        /// StreamSPI4 每次调用都会自己完成片选，因此把
        /// `cs_low -> tx... -> cs_high` 的整段序列延迟合并成一次原子调用。
        fn flush(&self, read_len: usize, read: Option<&mut [u8]>) -> Result<(), String> {
            let mut pending = self.pending.borrow_mut();
            let mut frame = std::mem::take(&mut *pending);
            if frame.is_empty() && read_len == 0 {
                return Ok(());
            }
            if frame.len() + read_len > self.frame_limit {
                return Err(format!(
                    "DLL SPI 帧过长: {} 字节，上限 {} 字节",
                    frame.len() + read_len,
                    self.frame_limit
                ));
            }
            let write_len = frame.len();
            frame.resize(write_len + read_len, 0xFF);
            let ret =
                unsafe { (self.stream)(self.idx, 0x80, frame.len() as u32, frame.as_mut_ptr()) };
            if ret == 0 {
                return Err("DLL SPI 传输失败".into());
            }
            if let Some(out) = read {
                out.copy_from_slice(&frame[write_len..write_len + out.len()]);
            }
            Ok(())
        }

        fn raw_write(&self, data: &[u8]) -> Result<(), String> {
            let mut len = data.len() as u32;
            let ret = unsafe { (self.write_data)(self.idx, data.as_ptr() as *mut _, &mut len) };
            if ret == 0 {
                return Err("DLL WriteData 失败".into());
            }
            Ok(())
        }

        fn raw_read(&self, data: &mut [u8]) -> Result<usize, String> {
            let mut len = data.len() as u32;
            let ret = unsafe { (self.read_data)(self.idx, data.as_mut_ptr() as *mut _, &mut len) };
            if ret == 0 {
                return Err("DLL ReadData 失败".into());
            }
            Ok(len as usize)
        }
    }

    impl ProgrammerHal for DllHal {
        fn cs_low(&self) -> Result<(), String> {
            Ok(()) // 由 flush() 原子完成片选
        }

        fn cs_high(&self) -> Result<(), String> {
            self.flush(0, None)
        }

        fn spi_tx(&self, data: &[u8]) -> Result<(), String> {
            self.pending.borrow_mut().extend_from_slice(data);
            Ok(())
        }

        fn spi_rx(&self, data: &mut [u8]) -> Result<(), String> {
            self.flush(data.len(), Some(data))
        }

        fn i2c_write(&self, data: &[u8]) -> Result<(), String> {
            self.raw_write(data)
        }

        fn i2c_read(&self, data: &mut [u8]) -> Result<usize, String> {
            self.raw_read(data)
        }

        fn gpio_setbits(&self, bits: u8) -> Result<(), String> {
            if self.kind != ChipKind::Ch341A {
                return Err("当前设备不支持 Microwire GPIO".into());
            }
            let buf = [0xABu8, 0x80 | bits, 0x20];
            self.raw_write(&buf)
        }

        fn gpio_getbits(&self) -> Result<u8, String> {
            if self.kind != ChipKind::Ch341A {
                return Err("当前设备不支持 Microwire GPIO".into());
            }
            self.raw_write(&[0xAB, 0x00, 0x20])?;
            let mut data = [0u8; 1];
            let got = self.raw_read(&mut data)?;
            if got != 1 {
                return Err(format!("DLL GPIO 读取长度异常: {}", got));
            }
            Ok(data[0])
        }

        fn spi_frame_limit(&self) -> usize {
            self.frame_limit
        }

        fn is_ch347(&self) -> bool {
            matches!(self.kind, ChipKind::Ch347T | ChipKind::Ch347F)
        }
    }

    impl Drop for DllHal {
        fn drop(&mut self) {
            unsafe {
                (self.close)(self.idx);
                let _ = FreeLibrary(self.lib);
            }
        }
    }
}

#[cfg(hal_backend_dll)]
pub use dll_hal::DllHal;

// ═════════════════════════════════ Public device wrapper ═════════════════════

/// Enumerate CH34X devices through the official DLL (Windows default HAL).
#[cfg(hal_backend_dll)]
pub fn enumerate_dll_devices() -> Result<Vec<(u32, ChipKind)>, String> {
    dll_hal::DllHal::enumerate()
}

/// HAL 门面：`Box<dyn ProgrammerHal>`，上层只面对这个类型。
pub struct Ch34xDevice(Box<dyn ProgrammerHal>);

impl std::ops::Deref for Ch34xDevice {
    type Target = dyn ProgrammerHal;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl Ch34xDevice {
    /// Open and configure the programmer. Dropping the returned value closes
    /// the USB device (per-operation lifecycle, like IMSProg).
    pub fn open(settings: &Ch34xSettings) -> Result<Self, String> {
        Self::open_with_mode(settings, DeviceMode::Spi)
    }

    pub fn open_with_mode(settings: &Ch34xSettings, mode: DeviceMode) -> Result<Self, String> {
        #[cfg(hal_backend_dll)]
        {
            Ok(Ch34xDevice(Box::new(dll_hal::DllHal::open(
                settings, mode,
            )?)))
        }

        #[cfg(hal_backend_libusb)]
        {
            let device: Box<dyn ProgrammerHal> = match settings.kind {
                ChipKind::Ch341A => Box::new(Ch341::open(settings, mode)?),
                ChipKind::Ch347T => Box::new(Ch347::open(settings, mode)?),
                ChipKind::Ch347F => {
                    return Err("CH347F 没有 libusb 实现，请使用 Windows DLL 后端".into())
                }
            };
            Ok(Ch34xDevice(device))
        }

        #[cfg(not(any(hal_backend_dll, hal_backend_libusb)))]
        {
            let _ = (settings, mode);
            compile_error!(
                "没有可用的 HAL 后端（build.rs 应设置 hal_backend_dll 或 hal_backend_libusb）"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_swap_round_trip() {
        for byte in 0u8..=255 {
            assert_eq!(swap_byte(swap_byte(byte)), byte);
        }
    }

    #[test]
    fn ch341_frame_layout_matches_imsprog() {
        // 5-byte read command (1 cmd + 4 addr) -> 1 packet: 32 + 1 + 5 bytes
        let write = [0x03u8, 0x12, 0x34, 0x56, 0x78];
        let frame = ch341_build_stream(&write, 0);
        assert_eq!(frame.len(), CH341_PACKET_LEN + 1 + write.len());
        assert!(frame[..CH341_PACKET_LEN].iter().all(|&b| b == 0));
        assert_eq!(frame[CH341_PACKET_LEN], CH341_CMD_SPI_STREAM);
        assert_eq!(frame[CH341_PACKET_LEN + 1], swap_byte(0x03));

        // 32 data bytes need two packets (31 + 1)
        let write2 = [0xAAu8; 32];
        let frame2 = ch341_build_stream(&write2, 0);
        assert_eq!(frame2.len(), CH341_PACKET_LEN + 2 + 32);
        assert_eq!(frame2[CH341_PACKET_LEN + 32], CH341_CMD_SPI_STREAM);

        // Read region: after the write bytes, payload is 0xFF placeholders
        let write3 = [0x03u8; 2];
        let frame3 = ch341_build_stream(&write3, 4);
        assert_eq!(frame3.len(), CH341_PACKET_LEN + 1 + 6);
        assert_eq!(&frame3[CH341_PACKET_LEN + 1 + 2..], &[0xFF; 4]);
    }

    #[test]
    fn ch341_response_extraction() {
        let mut response = vec![0x00u8; 6];
        response[2] = swap_byte(0x5A);
        response[3] = swap_byte(0xA5);
        let mut read = [0u8; 4];
        ch341_extract_read(2, &mut read, &response);
        assert_eq!(read, [0x5A, 0xA5, 0x00, 0x00]);
    }
}
