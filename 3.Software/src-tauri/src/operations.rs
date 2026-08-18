// Full-chip erase and blank-check operation logic, decoupled from the Tauri
// command layer. The command layer only translates progress callbacks into
// frontend events, so these operations can be driven from any UI transport.

use serde::Serialize;
use std::time::{Duration, Instant};

use crate::ch34x::Ch34xDevice;
use crate::core::{
    nor_params, open_ch34x, open_ch34x_mode, parse_nand_bad_block_mode, prepare_bypass_if_needed,
    scan_nand_bad_blocks_for_mode, serprog_wait_ready, spi_4byte_mode, spi_unprotect,
    spi_wait_ready, spi_write_disable, spi_write_enable, AppState, NorParams,
};
use crate::protocols;

#[derive(Debug, Clone, Serialize)]
pub struct EraseProgress {
    pub done: u64,
    pub total: u64,
    pub phase: String,
    pub message: String,
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlankCheckResult {
    pub blank: bool,
    pub checked: u64,
    pub first_non_blank: Option<u64>,
}

pub fn chip_erase(
    state: &mut AppState,
    bad_block_mode: Option<&str>,
    erase_progress: &mut dyn FnMut(EraseProgress),
    bad_block_progress: &mut dyn FnMut(u32, u32),
) -> Result<String, String> {
    if state.ch34x.is_some() {
        let detected = state.detected.clone();
        let info = match detected.as_ref() {
            Some(info) => info.clone(),
            None => {
                // No detection cached: default to NOR behaviour.
                let dev = open_ch34x(state)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "等待芯片就绪...".to_string(),
                    elapsed_ms: None,
                });
                spi_wait_ready(&dev, 2000)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "写使能 (WREN)...".to_string(),
                    elapsed_ms: None,
                });
                spi_write_enable(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "解除写保护...".to_string(),
                    elapsed_ms: None,
                });
                spi_unprotect(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "再次写使能 (WREN)...".to_string(),
                    elapsed_ms: None,
                });
                spi_write_enable(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "已发送全片擦除命令 (C7h)，芯片内部擦除中...".to_string(),
                    elapsed_ms: None,
                });
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready_with_progress_cb(
                    &dev,
                    120_000,
                    "erase",
                    "全片擦除中",
                    erase_progress,
                )?;
                spi_write_disable(&dev)?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
                return Ok("全片擦除完成".to_string());
            }
        };
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {
                let dev = open_ch34x_mode(state, bus)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "等待芯片就绪...".to_string(),
                    elapsed_ms: None,
                });
                spi_wait_ready(&dev, 2000)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "写使能 (WREN)...".to_string(),
                    elapsed_ms: None,
                });
                spi_write_enable(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "解除写保护...".to_string(),
                    elapsed_ms: None,
                });
                spi_unprotect(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "prepare".to_string(),
                    message: "再次写使能 (WREN)...".to_string(),
                    elapsed_ms: None,
                });
                spi_write_enable(&dev)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "已发送全片擦除命令 (C7h)，芯片内部擦除中...".to_string(),
                    elapsed_ms: None,
                });
                dev.cs_low()?;
                dev.spi_tx(&[0xC7])?;
                dev.cs_high()?;
                spi_wait_ready_with_progress_cb(
                    &dev,
                    120_000,
                    "erase",
                    "全片擦除中",
                    erase_progress,
                )?;
                spi_write_disable(&dev)?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
            }
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(state, bus)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "EEPROM 全片擦除中...".to_string(),
                    elapsed_ms: None,
                });
                protocols::s95_erase(&dev)?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(state, bus)?;
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "DataFlash 全片擦除中...".to_string(),
                    elapsed_ms: None,
                });
                protocols::at45_erase(&dev)?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                let mode = parse_nand_bad_block_mode(bad_block_mode);
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "bad_block".to_string(),
                    message: "正在扫描坏块...".to_string(),
                    elapsed_ms: None,
                });
                let bad_blocks =
                    scan_nand_bad_blocks_for_mode(&dev, &info, mode, &mut |done, total| {
                        bad_block_progress(done, total);
                    })?;
                let links = prepare_bypass_if_needed(&dev, &info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_erase(&dev, &params, info.size, &op_bad, &mut |done, total| {
                    erase_progress(EraseProgress {
                        done: done as u64,
                        total: total as u64,
                        phase: "erase".to_string(),
                        message: format!("SPI NAND 块擦除 {}/{}", done, total),
                        elapsed_ms: None,
                    });
                })?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
                if !links.is_empty() {
                    return Ok(format!(
                        "全片擦除完成（已写入 {} 条 BBM 坏块映射）",
                        links.len()
                    ));
                }
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "I2C 芯片擦除中（写 0xFF）...".to_string(),
                    elapsed_ms: None,
                });
                protocols::i2c_erase(&dev, &params, info.size, &mut |done, total| {
                    erase_progress(EraseProgress {
                        done,
                        total,
                        phase: "erase".to_string(),
                        message: format!("I2C 擦除 {}/{} 字节", done, total),
                        elapsed_ms: None,
                    });
                })?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
            }
            "Microwire" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(&info);
                erase_progress(EraseProgress {
                    done: 0,
                    total: 0,
                    phase: "erase".to_string(),
                    message: "Microwire 全片擦除中...".to_string(),
                    elapsed_ms: None,
                });
                protocols::mw_erase(&dev, &params)?;
                erase_progress(EraseProgress {
                    done: 1,
                    total: 1,
                    phase: "done".to_string(),
                    message: "全片擦除完成".to_string(),
                    elapsed_ms: None,
                });
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    } else if let Some(ser) = &mut state.serprog {
        erase_progress(EraseProgress {
            done: 0,
            total: 0,
            phase: "prepare".to_string(),
            message: "写使能 (WREN)...".to_string(),
            elapsed_ms: None,
        });
        ser.spi_command(&[0x06], 0)?;
        erase_progress(EraseProgress {
            done: 0,
            total: 0,
            phase: "erase".to_string(),
            message: "已发送全片擦除命令 (C7h)，芯片内部擦除中...".to_string(),
            elapsed_ms: None,
        });
        ser.spi_command(&[0xC7], 0)?;
        serprog_wait_ready(ser, 120_000)?;
        erase_progress(EraseProgress {
            done: 1,
            total: 1,
            phase: "done".to_string(),
            message: "全片擦除完成".to_string(),
            elapsed_ms: None,
        });
    } else {
        return Err("没有可用的编程器，请先初始化".into());
    }

    Ok("全片擦除完成".to_string())
}

/// Return the absolute address of the first byte that is not 0xFF.
fn first_non_blank_byte(data: &[u8], base: u64) -> Option<u64> {
    data.iter()
        .position(|&b| b != 0xFF)
        .map(|index| base + index as u64)
}

/// Stream a blank check: read the requested range in chunks and stop at the
/// first non-0xFF byte. Never materialises the whole chip in memory.
pub fn blank_check(
    state: &mut AppState,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<&str>,
    progress: &mut dyn FnMut(u64, u64),
    bad_block_progress: &mut dyn FnMut(u32, u32),
) -> Result<BlankCheckResult, String> {
    let total = size.saturating_sub(start_addr);
    if total == 0 {
        return Ok(BlankCheckResult {
            blank: true,
            checked: 0,
            first_non_blank: None,
        });
    }

    // serprog path: stream reads through O_SPIOP.
    if state.ch34x.is_none() {
        if let Some(ser) = state.serprog.as_mut() {
            let use_4b = size > 0x0100_0000;
            let cmd_read: u8 = if use_4b { 0x13 } else { 0x03 };
            let make_header = |addr: u64| -> Vec<u8> {
                let mut h = vec![cmd_read];
                if use_4b {
                    h.push(((addr >> 24) & 0xFF) as u8);
                }
                h.push(((addr >> 16) & 0xFF) as u8);
                h.push(((addr >> 8) & 0xFF) as u8);
                h.push((addr & 0xFF) as u8);
                h
            };
            let chunk_max = ser.max_read_len().min(4096);
            let mut offset: u64 = 0;
            while offset < total {
                let addr = start_addr + offset;
                let chunk = (total - offset).min(chunk_max as u64) as usize;
                let data = ser
                    .spi_command(&make_header(addr), chunk)
                    .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
                if data.len() != chunk {
                    return Err(format!(
                        "查空读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                        addr,
                        chunk,
                        data.len()
                    ));
                }
                if let Some(pos) = first_non_blank_byte(&data, addr) {
                    return Ok(BlankCheckResult {
                        blank: false,
                        checked: offset + pos - addr,
                        first_non_blank: Some(pos),
                    });
                }
                offset += chunk as u64;
                progress(offset, total);
            }
            return Ok(BlankCheckResult {
                blank: true,
                checked: total,
                first_non_blank: None,
            });
        }
    }

    // Small / non-streaming protocols reuse the existing read paths and scan
    // the returned buffer. Capped so a pathological request cannot OOM.
    if let Some(info) = state.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            other => {
                if size > 256 * 1024 * 1024 {
                    return Err(format!("{} 协议查空暂不支持超过 256 MiB", other));
                }
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let data = match other {
                    "SPI_EEPROM" => protocols::s95_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| progress(done, total),
                    )?,
                    "SPI_DATA_45" => protocols::at45_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| progress(done, total),
                    )?,
                    "SPI_NAND" => {
                        let mode = parse_nand_bad_block_mode(bad_block_mode);
                        let bad_blocks =
                            scan_nand_bad_blocks_for_mode(&dev, info, mode, &mut |done, total| {
                                bad_block_progress(done, total)
                            })?;
                        let links = prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                        let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                            Vec::new()
                        } else {
                            bad_blocks
                        };
                        let data = protocols::nand_read(
                            &dev,
                            &params,
                            size,
                            &op_bad,
                            &mut |done, total| progress(done, total),
                        )?;
                        let _ = links;
                        data
                    }
                    "I2C" | "I2C_F-RAM" | "I2C_SPD" => protocols::i2c_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| progress(done, total),
                    )?,
                    "Microwire" => protocols::mw_read(
                        &dev,
                        &params,
                        start_addr,
                        size as usize,
                        &mut |done, total| progress(done, total),
                    )?,
                    other => return Err(format!("协议 {} 暂未实现查空", other)),
                };
                if let Some(pos) = first_non_blank_byte(&data, start_addr) {
                    return Ok(BlankCheckResult {
                        blank: false,
                        checked: pos - start_addr,
                        first_non_blank: Some(pos),
                    });
                }
                return Ok(BlankCheckResult {
                    blank: true,
                    checked: data.len() as u64,
                    first_non_blank: None,
                });
            }
        }
    }

    // CH34X SPI NOR: manual CS read, chunked, no whole-image buffer.
    let params = match state.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: size > 0x0100_0000,
            alg: 0,
        },
    };
    let dev = open_ch34x(state)?;
    spi_wait_ready(&dev, 1000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let chunk_limit = dev.spi_frame_limit();
    let mut offset: u64 = 0;
    while offset < total {
        let addr = start_addr + offset;
        let hdr_len = if params.addr4b { 5 } else { 4 };
        let chunk = (total - offset).min((chunk_limit.saturating_sub(hdr_len)) as u64) as usize;
        dev.cs_low()?;
        let mut hdr = vec![0x03u8];
        if params.addr4b {
            hdr.push(((addr >> 24) & 0xFF) as u8);
        }
        hdr.push(((addr >> 16) & 0xFF) as u8);
        hdr.push(((addr >> 8) & 0xFF) as u8);
        hdr.push((addr & 0xFF) as u8);
        dev.spi_tx(&hdr)
            .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
        let mut buf = vec![0xFFu8; chunk];
        dev.spi_rx(&mut buf)
            .map_err(|e| format!("查空读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        if let Some(pos) = first_non_blank_byte(&buf, addr) {
            if params.addr4b {
                spi_4byte_mode(&dev, params.alg, false)?;
            }
            return Ok(BlankCheckResult {
                blank: false,
                checked: offset + (pos - addr),
                first_non_blank: Some(pos),
            });
        }
        offset += chunk as u64;
        progress(offset, total);
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }
    Ok(BlankCheckResult {
        blank: true,
        checked: total,
        first_non_blank: None,
    })
}

/// Same as `spi_wait_ready`, but reports elapsed time through the erase
/// progress callback every 250 ms so a long full-chip erase never looks frozen.
fn spi_wait_ready_with_progress_cb(
    dev: &Ch34xDevice,
    timeout_ms: u64,
    phase: &str,
    message_prefix: &str,
    erase_progress: &mut dyn FnMut(EraseProgress),
) -> Result<(), String> {
    let start = Instant::now();
    let mut last_report = start;
    loop {
        let status = crate::core::spi_read_status(dev)?;
        if (status & 0x01 | status & 0x20 | status & 0x02) == 0 {
            return Ok(());
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms > timeout_ms {
            return Err("等待闪存就绪超时".into());
        }
        if last_report.elapsed() >= Duration::from_millis(250) {
            erase_progress(EraseProgress {
                done: 0,
                total: 0,
                phase: phase.to_string(),
                message: format!(
                    "{} · 最长等待 {:.0}s",
                    message_prefix,
                    timeout_ms as f64 / 1000.0
                ),
                elapsed_ms: Some(elapsed_ms),
            });
            last_report = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
