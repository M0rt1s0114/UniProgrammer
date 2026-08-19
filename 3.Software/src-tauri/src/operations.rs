// Full-chip erase and blank-check operation logic, decoupled from the Tauri
// command layer. The command layer only translates progress callbacks into
// frontend events, so these operations can be driven from any UI transport.

use serde::Serialize;
use std::time::{Duration, Instant};

use crate::core::{
    nor_params, open_ch34x, open_ch34x_mode, parse_nand_bad_block_mode, prepare_bypass_if_needed,
    scan_nand_bad_blocks_for_mode, serprog_wait_ready, spi_4byte_mode, spi_read_status,
    spi_unprotect, spi_wait_ready, spi_write_disable, spi_write_enable, AppState, NorParams,
    NOR_BP_MASK_SR1,
};
use uni_devices::ch34x::Ch34xDevice;
use uni_hal::hal_router::{HalRouter, SidecarSelection};
use uni_hal::sidecar_nor::SidecarNor;
use uni_proto::protocols;

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

fn sidecar_selection(state: &AppState) -> Option<SidecarSelection> {
    match (&state.sidecar_adapter, &state.sidecar_device) {
        (Some(adapter), Some(device_id)) => Some(SidecarSelection {
            adapter: adapter.clone(),
            device_id: device_id.clone(),
        }),
        _ => None,
    }
}

pub fn chip_erase(
    state: &mut AppState,
    bad_block_mode: Option<&str>,
    erase_progress: &mut dyn FnMut(EraseProgress),
    bad_block_progress: &mut dyn FnMut(u32, u32),
    router: Option<&mut HalRouter>,
) -> Result<String, String> {
    if let (Some(selection), Some(router)) = (sidecar_selection(state), router) {
        return sidecar_erase_chip(router, &selection);
    }

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
    router: Option<&mut HalRouter>,
) -> Result<BlankCheckResult, String> {
    if let (Some(selection), Some(router)) = (sidecar_selection(state), router) {
        let data = sidecar_read_chip(router, &selection, size, progress)?;
        let first_non_blank = first_non_blank_byte(&data, start_addr);
        return Ok(BlankCheckResult {
            blank: first_non_blank.is_none(),
            checked: match first_non_blank {
                Some(pos) => pos - start_addr,
                None => data.len() as u64,
            },
            first_non_blank,
        });
    }

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

pub fn read_chip(
    state: &mut AppState,
    size: u64,
    start_addr: u64,
    bad_block_mode: Option<&str>,
    progress: &mut dyn FnMut(u64, u64),
    bad_block_progress: &mut dyn FnMut(u32, u32),
    router: Option<&mut HalRouter>,
) -> Result<Vec<u8>, String> {
    if let (Some(selection), Some(router)) = (sidecar_selection(state), router) {
        return sidecar_read_chip(router, &selection, size, progress);
    }

    if state.ch34x.is_none() {
        if let Some(ser) = state.serprog.as_mut() {
            // serprog path: two-phase write-then-read, unchanged.
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
            let total = size.saturating_sub(start_addr);
            let mut out = Vec::with_capacity(total as usize);
            let mut offset: u64 = 0;
            let read_chunk_max = ser.max_read_len().min(4096);
            while offset < total {
                let addr = start_addr + offset;
                let hdr = make_header(addr);
                let chunk = (total - offset).min(read_chunk_max as u64) as usize;
                let data = ser
                    .spi_command(&hdr, chunk)
                    .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;
                if data.len() != chunk {
                    return Err(format!(
                        "serprog 读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                        addr,
                        chunk,
                        data.len()
                    ));
                }
                out.extend_from_slice(&data);
                offset += chunk as u64;
                progress(offset, total);
            }
            return Ok(out);
        }
    }

    // Non-NOR protocols dispatch to the ported IMSProg command sequences.
    if let Some(info) = state.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::s95_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| progress(done, total),
                );
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::at45_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| progress(done, total),
                );
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let mode = parse_nand_bad_block_mode(bad_block_mode);
                let bad_blocks =
                    scan_nand_bad_blocks_for_mode(&dev, info, mode, &mut |done, total| {
                        bad_block_progress(done, total);
                    })?;
                prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                return protocols::nand_read(&dev, &params, size, &op_bad, &mut |done, total| {
                    progress(done, total);
                });
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::i2c_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| progress(done, total),
                );
            }
            "Microwire" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::mw_read(
                    &dev,
                    &params,
                    start_addr,
                    size as usize,
                    &mut |done, total| progress(done, total),
                );
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    }

    // CH34X path: IMPROG manual CS sequence.
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

    let total = size.saturating_sub(start_addr);
    let mut out = Vec::with_capacity(total as usize);
    let mut offset: u64 = 0;
    let chunk_limit = dev.spi_frame_limit();

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
            .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;

        let start = out.len();
        out.resize(start + chunk, 0xFF);
        dev.spi_rx(&mut out[start..start + chunk])
            .map_err(|e| format!("读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        offset += chunk as u64;
        progress(offset, total);
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }

    Ok(out)
}

#[allow(clippy::too_many_arguments)] // signature required by the sidecar-routing milestone
pub fn write_chip(
    state: &mut AppState,
    data: &[u8],
    start_addr: u64,
    force_segmented: Option<bool>,
    bad_block_mode: Option<&str>,
    progress: &mut dyn FnMut(u64, u64),
    bad_block_progress: &mut dyn FnMut(u32, u32),
    router: Option<&mut HalRouter>,
) -> Result<String, String> {
    if let (Some(selection), Some(router)) = (sidecar_selection(state), router) {
        return sidecar_write_chip(router, &selection, data, start_addr, progress);
    }

    let total = data.len();

    if state.ch34x.is_none() {
        if let Some(ser) = state.serprog.as_mut() {
            let sr1 = ser.spi_command(&[0x05], 1)?[0];
            if (sr1 & NOR_BP_MASK_SR1) != 0 {
                return Err(format!(
                    "SPI NOR 处于写保护状态（SR1=0x{:02X}）。请先执行“解除 NOR 写保护”",
                    sr1
                ));
            }
            let use_4b = (start_addr + total as u64) > 0x0100_0000;
            let make_header = |addr: u64| -> Vec<u8> {
                let mut h = vec![0x02u8];
                if use_4b {
                    h.push(((addr >> 24) & 0xFF) as u8);
                }
                h.push(((addr >> 16) & 0xFF) as u8);
                h.push(((addr >> 8) & 0xFF) as u8);
                h.push((addr & 0xFF) as u8);
                h
            };
            let mut offset: usize = 0;
            while offset < total {
                let addr = start_addr + offset as u64;
                let hdr = make_header(addr);
                let chunk = 256
                    .min(total - offset)
                    .min(ser.max_write_len().saturating_sub(hdr.len()).max(1));
                ser.spi_command(&[0x06], 0)?;
                let mut frame = Vec::with_capacity(hdr.len() + chunk);
                frame.extend_from_slice(&hdr);
                frame.extend_from_slice(&data[offset..offset + chunk]);
                ser.spi_command(&frame, 0)?;
                serprog_wait_ready(ser, 100)?;
                offset += chunk;
                progress(offset as u64, total as u64);
            }
            return Ok(format!("写入完成，共 {} 字节", total));
        }
    }

    // Non-NOR protocols dispatch to the ported IMSProg command sequences.
    if let Some(info) = state.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        match info.protocol.as_str() {
            "SPI_NOR" => {}
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::s95_write(
                    &dev,
                    &params,
                    data,
                    start_addr,
                    &mut |done, total| progress(done, total),
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::at45_write(
                    &dev,
                    &params,
                    data,
                    start_addr,
                    &mut |done, total| progress(done, total),
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                let mode = parse_nand_bad_block_mode(bad_block_mode);
                let bad_blocks =
                    scan_nand_bad_blocks_for_mode(&dev, info, mode, &mut |done, total| {
                        bad_block_progress(done, total);
                    })?;
                let bad_count = bad_blocks.len();
                let links = prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_write(
                    &dev,
                    &params,
                    data,
                    info.size,
                    &op_bad,
                    mode,
                    force_segmented.unwrap_or(false),
                    &mut |done, total| progress(done, total),
                )?;
                if !links.is_empty() {
                    return Ok(format!(
                        "写入完成，共 {} 字节（已写入 {} 条 BBM 坏块映射）",
                        total,
                        links.len()
                    ));
                }
                if bad_count > 0 {
                    return Ok(format!(
                        "写入完成，共 {} 字节（发现 {} 个坏块，已跳过）",
                        total, bad_count
                    ));
                }
                return Ok(format!("写入完成，共 {} 字节", total));
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::i2c_write(
                    &dev,
                    &params,
                    data,
                    start_addr,
                    &mut |done, total| progress(done, total),
                )
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            "Microwire" => {
                let dev = open_ch34x_mode(state, bus)?;
                let params = protocols::ChipParams::from_info(info);
                return protocols::mw_write(&dev, &params, data, start_addr, &mut |done, total| {
                    progress(done, total);
                })
                .map(|_| format!("写入完成，共 {} 字节", total));
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        }
    }

    // CH34X path: IMPROG page program sequence.
    let dev = open_ch34x(state)?;
    let params = match state.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: (start_addr + total as u64) > 0x0100_0000,
            alg: 0,
        },
    };

    spi_wait_ready(&dev, 2000)?;
    let sr1 = spi_read_status(&dev)?;
    if (sr1 & NOR_BP_MASK_SR1) != 0 {
        return Err(format!(
            "SPI NOR 处于写保护状态（SR1=0x{:02X}）。请先执行“解除 NOR 写保护”",
            sr1
        ));
    }
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let mut offset: usize = 0;
    while offset < total {
        let addr = start_addr + offset as u64;
        let chunk = params.page.min(total - offset);

        spi_wait_ready(&dev, 2000)?;
        spi_write_enable(&dev)?;

        dev.cs_low()?;
        let mut frame = Vec::with_capacity(5 + chunk);
        frame.push(0x02u8);
        if params.addr4b {
            frame.push(((addr >> 24) & 0xFF) as u8);
        }
        frame.push(((addr >> 16) & 0xFF) as u8);
        frame.push(((addr >> 8) & 0xFF) as u8);
        frame.push((addr & 0xFF) as u8);
        frame.extend_from_slice(&data[offset..offset + chunk]);
        dev.spi_tx(&frame)
            .map_err(|e| format!("写入失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;
        spi_wait_ready(&dev, 100)?;

        offset += chunk;
        progress(offset as u64, total as u64);
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }
    spi_write_disable(&dev)?;

    Ok(format!("写入完成，共 {} 字节", total))
}

pub fn verify_chip(
    state: &mut AppState,
    data: &[u8],
    start_addr: u64,
    bad_block_mode: Option<&str>,
    progress: &mut dyn FnMut(u64, u64),
    bad_block_progress: &mut dyn FnMut(u32, u32),
    router: Option<&mut HalRouter>,
) -> Result<String, String> {
    if let (Some(selection), Some(router)) = (sidecar_selection(state), router) {
        return sidecar_verify_chip(router, &selection, data, start_addr, progress);
    }

    let total = data.len() as u64;

    if state.ch34x.is_none() {
        if let Some(ser) = state.serprog.as_mut() {
            let use_4b = (start_addr + total) > 0x0100_0000;
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
            let mut offset: u64 = 0;
            let read_chunk_max = ser.max_read_len().min(4096);
            while offset < total {
                let addr = start_addr + offset;
                let hdr = make_header(addr);
                let chunk = (total - offset).min(read_chunk_max as u64) as usize;
                let buf = ser
                    .spi_command(&hdr, chunk)
                    .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;
                if buf.len() != chunk {
                    return Err(format!(
                        "serprog 读取长度不符 @ 0x{:08X}: 预期 {} 实际 {}",
                        addr,
                        chunk,
                        buf.len()
                    ));
                }
                for (i, actual) in buf.iter().enumerate() {
                    let expected = data[offset as usize + i];
                    if expected != *actual {
                        let fail_addr = format!("0x{:08X}", addr + i as u64);
                        return Err(format!(
                            "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                            fail_addr, expected, actual
                        ));
                    }
                }
                offset += chunk as u64;
                progress(offset, total);
            }
            return Ok("校验通过".to_string());
        }
    }

    // Non-NOR protocols: read back with the ported command sequences and
    // compare in-place.
    if let Some(info) = state.detected.as_ref() {
        let bus = protocols::bus_mode_for_protocol(&info.protocol);
        let params = protocols::ChipParams::from_info(info);
        let buf: Vec<u8> = match info.protocol.as_str() {
            "SPI_NOR" => Vec::new(),
            "SPI_EEPROM" => {
                let dev = open_ch34x_mode(state, bus)?;
                protocols::s95_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "SPI_DATA_45" => {
                let dev = open_ch34x_mode(state, bus)?;
                protocols::at45_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "SPI_NAND" => {
                let dev = open_ch34x_mode(state, bus)?;
                let mode = parse_nand_bad_block_mode(bad_block_mode);
                let bad_blocks =
                    scan_nand_bad_blocks_for_mode(&dev, info, mode, &mut |done, total| {
                        bad_block_progress(done, total);
                    })?;
                prepare_bypass_if_needed(&dev, info, mode, &bad_blocks)?;
                let op_bad = if mode == protocols::NandBadBlockMode::Bypass {
                    Vec::new()
                } else {
                    bad_blocks
                };
                protocols::nand_read(&dev, &params, total, &op_bad, &mut |_, _| {})?
            }
            "I2C" | "I2C_F-RAM" | "I2C_SPD" => {
                let dev = open_ch34x_mode(state, bus)?;
                protocols::i2c_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            "Microwire" => {
                let dev = open_ch34x_mode(state, bus)?;
                protocols::mw_read(&dev, &params, start_addr, data.len(), &mut |_, _| {})?
            }
            other => return Err(format!("协议 {} 暂未实现", other)),
        };

        if !buf.is_empty() {
            if buf.len() != data.len() {
                return Err(format!(
                    "校验读取长度不符: 预期 {} 实际 {}",
                    data.len(),
                    buf.len()
                ));
            }
            let mut offset = 0usize;
            while offset < buf.len() {
                let end = (offset + 4096).min(buf.len());
                for i in offset..end {
                    if buf[i] != data[i] {
                        let fail_addr = format!("0x{:08X}", start_addr + i as u64);
                        return Err(format!(
                            "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                            fail_addr, data[i], buf[i]
                        ));
                    }
                }
                offset = end;
                progress(offset as u64, total);
            }
            return Ok("校验通过".to_string());
        }
    }

    // CH34X path: read back and compare.
    let dev = open_ch34x(state)?;
    let params = match state.detected.as_ref() {
        Some(info) if info.protocol == "SPI_NOR" => nor_params(info),
        _ => NorParams {
            page: 256,
            _sector: 4096,
            _block: 64 * 1024,
            addr4b: (start_addr + total) > 0x0100_0000,
            alg: 0,
        },
    };

    spi_wait_ready(&dev, 1000)?;
    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, true)?;
    }

    let mut offset: u64 = 0;
    let chunk_limit = dev.spi_frame_limit();
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
            .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;

        let mut buf = vec![0xFFu8; chunk];
        dev.spi_rx(&mut buf)
            .map_err(|e| format!("校验读取失败 @ 0x{:08X}: {}", addr, e))?;
        dev.cs_high()?;

        for (i, actual) in buf.iter().enumerate() {
            let expected = data[offset as usize + i];
            if expected != *actual {
                let fail_addr = format!("0x{:08X}", addr + i as u64);
                return Err(format!(
                    "校验失败 @ {}: 期望 0x{:02X}, 读到 0x{:02X}",
                    fail_addr, expected, actual
                ));
            }
        }

        offset += chunk as u64;
        progress(offset, total);
    }

    if params.addr4b {
        spi_4byte_mode(&dev, params.alg, false)?;
    }

    Ok("校验通过".to_string())
}

/// Open a [`SidecarNor`], run `f`, and always close the session once before
/// returning. A failed close is deliberately ignored so it can never mask the
/// operation's own result.
#[allow(dead_code)] // not wired into a Tauri command yet; exercised by tests
fn with_sidecar_nor<'a, T>(
    router: &'a mut HalRouter,
    selection: &SidecarSelection,
    f: impl FnOnce(&mut SidecarNor<'a>) -> Result<T, String>,
) -> Result<T, String> {
    let mut nor = SidecarNor::open(router, &selection.adapter, &selection.device_id)?;
    let result = f(&mut nor);
    let _ = nor.close();
    result
}

#[allow(dead_code)] // not wired into a Tauri command yet; exercised by tests
pub fn sidecar_erase_chip(
    router: &mut HalRouter,
    selection: &SidecarSelection,
) -> Result<String, String> {
    with_sidecar_nor(router, selection, |nor| {
        nor.erase_chip()?;
        Ok("全片擦除完成（sidecar）".to_string())
    })
}

#[allow(dead_code)] // not wired into a Tauri command yet; exercised by tests
pub fn sidecar_read_chip(
    router: &mut HalRouter,
    selection: &SidecarSelection,
    size: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    with_sidecar_nor(router, selection, |nor| {
        let total = size;
        let mut out = Vec::with_capacity(total as usize);
        let mut offset: u64 = 0;
        while offset < total {
            let chunk = (total - offset).min(4096) as usize;
            let data = nor.read(offset as usize, chunk)?;
            out.extend_from_slice(&data);
            offset += chunk as u64;
            progress(offset, total);
        }
        Ok(out)
    })
}

#[allow(dead_code)] // not wired into a Tauri command yet; exercised by tests
pub fn sidecar_write_chip(
    router: &mut HalRouter,
    selection: &SidecarSelection,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<String, String> {
    with_sidecar_nor(router, selection, |nor| {
        let total = data.len();
        let mut offset: usize = 0;
        while offset < total {
            let chunk = 256.min(total - offset);
            let addr = start_addr + offset as u64;

            // Program full pages: the final partial page is padded with 0xFF
            // so the AND-program semantics leave the untouched tail unchanged.
            let mut page = [0xFFu8; 256];
            page[..chunk].copy_from_slice(&data[offset..offset + chunk]);
            nor.program_page(addr as usize, &page)?;

            offset += chunk;
            progress(offset as u64, total as u64);
        }
        Ok(format!("写入完成（sidecar），共 {} 字节", total))
    })
}

#[allow(dead_code)] // not wired into a Tauri command yet; exercised by tests
pub fn sidecar_verify_chip(
    router: &mut HalRouter,
    selection: &SidecarSelection,
    data: &[u8],
    start_addr: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<String, String> {
    with_sidecar_nor(router, selection, |nor| {
        let total = data.len() as u64;
        let mut offset: u64 = 0;
        while offset < total {
            let chunk = (total - offset).min(4096) as usize;
            let addr = start_addr + offset;
            let actual = nor.read(addr as usize, chunk)?;
            for (i, &read_byte) in actual.iter().enumerate() {
                let expected = data[offset as usize + i];
                if expected != read_byte {
                    return Err(format!(
                        "校验失败 @ 0x{:08X}: 期望 0x{:02X}, 读到 0x{:02X}",
                        addr + i as u64,
                        expected,
                        read_byte
                    ));
                }
            }
            offset += chunk as u64;
            progress(offset, total);
        }
        Ok("校验通过（sidecar）".to_string())
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
