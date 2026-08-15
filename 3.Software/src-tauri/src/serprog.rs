use std::io::{Read, Write};
use std::time::Duration;

// ── serprog 协议常量（flashrom Documentation/serprog-protocol.txt）──────────
const S_ACK: u8 = 0x06;
const S_NAK: u8 = 0x15;

const S_CMD_NOP: u8 = 0x00;
const S_CMD_Q_IFACE: u8 = 0x01; // 接口版本：ACK + 16位小端版本
const S_CMD_Q_CMDMAP: u8 = 0x02; // 支持的命令位图：ACK + 32字节
const S_CMD_Q_PGMNAME: u8 = 0x03; // 固件名：ACK + 16字节
const S_CMD_Q_SERBUF: u8 = 0x04; // 串口缓冲：ACK + 16位小端
const S_CMD_Q_BUSTYPE: u8 = 0x05; // 支持的总线：ACK + 32位小端
const S_CMD_Q_OPBUF: u8 = 0x07; // 操作缓冲：ACK + 16位小端
const S_CMD_Q_RDNMAXLEN: u8 = 0x0F; // 单次读取上限：ACK + 32位小端（可选）
const S_CMD_S_BUSTYPE: u8 = 0x10; // 设置总线类型
const S_CMD_O_SPIOP: u8 = 0x11; // SPI 操作：24位slen + 24位rlen + 写数据
const S_CMD_S_SPI_FREQ: u8 = 0x12; // 设置 SPI 频率：ACK + 32位小端实际值
const S_CMD_S_PIN_STATE: u8 = 0x13; // 设置 GPIO 状态

// bustype 位定义：bit0=并行 bit1=LPC bit2=FWH bit3=SPI
const BUS_SPI: u8 = 1 << 3; // 0x08

pub struct Serprog {
    port: Box<dyn serialport::SerialPort>,
    interface_version: u16,
    bustypes: u32,
    serbuf: usize,
    opbuf: usize,
    max_read_len: usize,
}

impl Serprog {
    /// 打开串口并初始化 serprog 设备：同步、查询能力、设置 SPI 总线。
    pub fn open(path: &str) -> Result<Self, String> {
        let port = serialport::new(path, 115_200)
            .timeout(Duration::from_secs(5))
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .open()
            .map_err(|e| format!("无法打开串口 {}: {}", path, e))?;
        let mut dev = Serprog {
            port,
            interface_version: 0,
            bustypes: 0,
            serbuf: 16,
            opbuf: 300,
            max_read_len: 4096,
        };

        // Arduino 类固件常用 DTR 脉冲复位；不支持的平台忽略即可
        dev.port.write_data_terminal_ready(true).ok();
        std::thread::sleep(Duration::from_millis(50));
        dev.port.write_data_terminal_ready(false).ok();
        dev.port.clear(serialport::ClearBuffer::All).ok();
        let _ = dev.nop();

        // 1) 同步：Q_IFACE 有应答说明在线
        let mut synced = false;
        for _ in 0..5 {
            match dev.command(S_CMD_Q_IFACE, &[], 2) {
                Ok(ver) if ver.len() == 2 => {
                    dev.interface_version = u16::from_le_bytes([ver[0], ver[1]]);
                    eprintln!(
                        "[serprog] 接口版本: {}.{}",
                        dev.interface_version >> 8,
                        dev.interface_version & 0xFF
                    );
                    synced = true;
                    break;
                }
                Ok(_) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    eprintln!("[serprog] 同步失败: {e}");
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if !synced {
            return Err("serprog 设备无响应，请检查固件和连线".into());
        }

        // 2) 查询能力；老固件可能不支持某些查询，失败则用保守默认值
        if let Ok(data) = dev.command(S_CMD_Q_BUSTYPE, &[], 4) {
            dev.bustypes = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        }
        if let Ok(data) = dev.command(S_CMD_Q_SERBUF, &[], 2) {
            dev.serbuf = u16::from_le_bytes([data[0], data[1]]) as usize;
        }
        if let Ok(data) = dev.command(S_CMD_Q_OPBUF, &[], 2) {
            dev.opbuf = u16::from_le_bytes([data[0], data[1]]) as usize;
        }
        if let Ok(data) = dev.command(S_CMD_Q_RDNMAXLEN, &[], 4) {
            let max = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
            if max > 0 {
                dev.max_read_len = max;
            }
        }

        // 3) 确认并选择 SPI 总线
        if dev.bustypes != 0 && (dev.bustypes as u8 & BUS_SPI) == 0 {
            return Err(format!(
                "serprog 固件不支持 SPI 总线（bustype=0x{:08X}）",
                dev.bustypes
            ));
        }
        dev.command(S_CMD_S_BUSTYPE, &[BUS_SPI], 0)
            .map_err(|e| format!("设置 SPI 总线失败: {e}"))?;

        let _ = dev.set_spi_freq(1_000_000); // 默认 1 MHz，固件可自行取舍
        let _ = dev.command_map();
        if let Some(name) = dev.program_name() {
            eprintln!("[serprog] 固件: {}", name);
        }

        eprintln!(
            "[serprog] bustype=0x{:08X} serbuf={} opbuf={} max_read={}",
            dev.bustypes, dev.serbuf, dev.opbuf, dev.max_read_len
        );
        Ok(dev)
    }

    /// 单次 SPI 操作可写出的最大字节数（命令+地址，不含读长度）。
    pub fn max_write_len(&self) -> usize {
        // O_SPIOP 请求 = 1 opcode + 6 长度字节 + slen 数据
        self.opbuf.saturating_sub(7).max(1)
    }

    /// 单次 SPI 操作可读入的最大字节数。
    pub fn max_read_len(&self) -> usize {
        self.max_read_len.min(self.serbuf).max(1)
    }

    /// 发送一个 serprog 命令：opcode + 命令专属参数；响应固定为
    /// ACK(0x06)/NAK(0x15)，ACK 后紧跟该命令规定长度的数据。
    fn command(&mut self, op: u8, params: &[u8], resp_len: usize) -> Result<Vec<u8>, String> {
        let mut frame = Vec::with_capacity(1 + params.len());
        frame.push(op);
        frame.extend_from_slice(params);
        self.port
            .write_all(&frame)
            .map_err(|e| format!("serprog 写入错误: {e}"))?;
        self.port
            .flush()
            .map_err(|e| format!("serprog 刷新错误: {e}"))?;

        let mut ack = [0u8; 1];
        self.port
            .read_exact(&mut ack)
            .map_err(|e| format!("serprog 读取应答错误: {e}"))?;
        if ack[0] == S_NAK {
            return Err(format!("serprog 命令 0x{:02X} 被设备拒绝 (NAK)", op));
        }
        if ack[0] != S_ACK {
            return Err(format!(
                "serprog 命令 0x{:02X} 应答异常: 0x{:02X}",
                op, ack[0]
            ));
        }

        let mut data = vec![0u8; resp_len];
        if resp_len > 0 {
            self.port
                .read_exact(&mut data)
                .map_err(|e| format!("serprog 读取数据错误: {e}"))?;
        }
        Ok(data)
    }

    /// 执行一次 SPI 操作（O_SPIOP = 0x11）：
    /// 写出 `write` 字节（指令码+地址），再读取 `read_len` 字节响应。
    /// 这是 flashrom 的“写 N + 读 M”两段式语义，不是全双工。
    pub fn spi_command(&mut self, write: &[u8], read_len: usize) -> Result<Vec<u8>, String> {
        if write.is_empty() && read_len == 0 {
            return Ok(Vec::new());
        }
        if write.len() > self.max_write_len() {
            return Err(format!(
                "SPI 写长度 {} 超过固件 opbuf 上限 {}",
                write.len(),
                self.max_write_len()
            ));
        }
        if read_len > self.max_read_len() {
            return Err(format!(
                "SPI 读长度 {} 超过固件 serbuf 上限 {}",
                read_len,
                self.max_read_len()
            ));
        }

        let slen = write.len() as u32;
        let rlen = read_len as u32;
        let mut params = Vec::with_capacity(6 + write.len());
        params.extend_from_slice(&slen.to_le_bytes()[..3]);
        params.extend_from_slice(&rlen.to_le_bytes()[..3]);
        params.extend_from_slice(write);

        self.command(S_CMD_O_SPIOP, &params, read_len)
    }

    /// 设置 SPI 频率（可选命令）。返回固件确认后的实际频率，失败不影响使用。
    pub fn set_spi_freq(&mut self, freq_hz: u32) -> Result<u32, String> {
        let data = self.command(S_CMD_S_SPI_FREQ, &freq_hz.to_le_bytes(), 4)?;
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// 设置设备 GPIO 状态（可选命令，用于固件调试）。
    #[allow(dead_code)]
    pub fn set_pin_state(&mut self, pin: u8, val: bool) -> Result<(), String> {
        self.command(S_CMD_S_PIN_STATE, &[pin, val as u8], 0)?;
        Ok(())
    }

    /// 查询固件名称（可选命令）。
    pub fn program_name(&mut self) -> Option<String> {
        self.command(S_CMD_Q_PGMNAME, &[], 16).ok().map(|d| {
            let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
            String::from_utf8_lossy(&d[..end]).to_string()
        })
    }

    /// 查询支持的命令位图（可选命令，调试用）。
    pub fn command_map(&mut self) -> Option<Vec<u8>> {
        self.command(S_CMD_Q_CMDMAP, &[], 32).ok()
    }

    /// 协议要求必须支持的空操作。
    pub fn nop(&mut self) -> Result<(), String> {
        self.command(S_CMD_NOP, &[], 0)?;
        Ok(())
    }
}
