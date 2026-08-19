use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::mem::size_of;

const MAGIC: u32 = 0x50494843;
const MAX_ID_LEN: usize = 16;

// ── 轻量混淆 ────────────────────────────────────────────────────────────────
// 参考 FFW_Decode_Tool 的逐字节算法：按位置异或掩码后循环左移。
// 只用于防止芯片库被直接 strings/hexdump 读取，不提供密码学强度。

fn obf_rol(value: u8, r: usize) -> u8 {
    let r = r % 8;
    let v = value as u16;
    (((v << r) | (v >> (8 - r))) & 0xFF) as u8
}

fn obf_ror(value: u8, r: usize) -> u8 {
    let r = r % 8;
    let v = value as u16;
    (((v >> r) | (v << (8 - r))) & 0xFF) as u8
}

fn obf_mask(i: usize) -> u8 {
    let mask = (1u32 << (i & 3)) ^ (1u32 << (i % 7)) ^ (1u32 << ((i % 13) + 4));
    (mask & 0xFF) as u8
}

fn obfuscate(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &byte)| obf_rol(byte ^ obf_mask(i), i % 8))
        .collect()
}

fn deobfuscate(data: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &byte)| obf_ror(byte, i % 8) ^ obf_mask(i))
        .collect()
}

#[repr(C)]
struct Header {
    magic: u32,
    version: u32,
    chip_count: u32,
    data_offset: u32,
    index_entry_size: u16,
    padding: u16,
}

#[repr(C)]
#[derive(Clone)]
struct IndexEntry {
    id: [u8; MAX_ID_LEN],
    data_offset: u32,
    data_len: u16,
    protocol: u16,
}

pub struct Chiplib {
    entries: Vec<IndexEntry>,
    data: Vec<u8>,
    version: u32,
}

#[derive(Serialize, Clone, Copy)]
pub struct ImportStats {
    pub dat_records: usize,
    pub matched_by_id: usize,
    pub matched_by_name: usize,
    pub entries_updated: usize,
}

#[derive(Clone)]
pub struct ChipInfo {
    pub id: String,
    pub vendor: String,
    pub model: String,
    pub protocol: String,
    pub size: u64,
    pub page: u32,
    /// Raw attribute bag from the data section. Self-describing key=value
    /// records keep the bin format forward compatible when new fields
    /// (sector, block, addr4bit, vcc, spare, besize, planes, timing, ...)
    /// are added later.
    pub attrs: HashMap<String, String>,
}

impl ChipInfo {
    #[allow(dead_code)] // used by the chip database importer / UI layers
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(|s| s.as_str())
    }

    pub fn attr_u32(&self, key: &str) -> Option<u32> {
        self.attrs.get(key).and_then(|v| v.parse().ok())
    }

    #[allow(dead_code)] // used by the chip database importer / UI layers
    pub fn attr_u64(&self, key: &str) -> Option<u64> {
        self.attrs.get(key).and_then(|v| v.parse().ok())
    }
}

impl Chiplib {
    pub fn load_auto(xml_path: &str, bin_path: &str) -> Result<Self, String> {
        if let Ok(lib) = Self::load_bin(bin_path) {
            return Ok(lib);
        }
        let lib = Self::load_xml(xml_path)?;
        let _ = lib.save_bin(bin_path);
        Ok(lib)
    }

    pub fn convert_xml_to_bin(xml_path: &str, bin_path: &str) -> Result<(), String> {
        let lib = Self::load_xml(xml_path)?;
        lib.save_bin(bin_path)
    }

    /// Enrich the library from IMSProg.Dat (68 bytes per record, EZP format).
    ///
    /// Matching order:
    /// 1. exact JEDEC ID (`%02X%02X%02X` of Dat bytes 0x32/0x31/0x30);
    /// 2. normalized model name (+ manufacturer when available).
    ///
    /// Fields are only filled when missing in chiplib.bin, so manual values
    /// already present are never overwritten. Added keys: `sector`, `block`,
    /// `addr4bit` (low nibble: 4B addressing, high nibble: algorithm),
    /// `timing`, `vcc`.
    pub fn import_imsprog_dat(&mut self, dat_path: &str) -> Result<ImportStats, String> {
        const REC_LEN: usize = 68;

        #[derive(Clone)]
        struct DatChip {
            id: String,
            model: String,
            manufacturer: String,
            sector: u32,
            block: u32,
            algorithm: u8,
            addr4bit: u8,
            timing: u16,
            vcc: Option<String>,
        }

        fn norm(s: &str) -> String {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        }

        let bytes = fs::read(dat_path).map_err(|e| e.to_string())?;
        let mut dat_chips: Vec<DatChip> = Vec::new();
        let mut pos = 0usize;
        while pos + REC_LEN <= bytes.len() {
            let rec = &bytes[pos..pos + REC_LEN];
            pos += REC_LEN;
            if rec[1] == 0 {
                break; // end record
            }

            let text_end = rec[..0x30].iter().position(|&b| b == 0).unwrap_or(0x30);
            let text = String::from_utf8_lossy(&rec[..text_end]);
            let mut fields = text.split(',');
            let _chip_type_txt = fields.next().unwrap_or("");
            let manufacturer = fields.next().unwrap_or("").trim().to_string();
            let model = fields.next().unwrap_or("").trim().to_string();
            if model.is_empty() {
                continue;
            }

            let man = rec[0x32];
            let dev = rec[0x31];
            let cap = rec[0x30];
            let id = if man == 0 && dev == 0 && cap == 0 {
                String::new()
            } else {
                format!("{:02X}{:02X}{:02X}", man, dev, cap)
            };

            let sector = rec[0x38] as u32 | ((rec[0x39] as u32) << 8);
            let algorithm = rec[0x3B];
            let addr4bit = rec[0x3E];
            let timing = rec[0x3C] as u16 | ((rec[0x3D] as u16) << 8);
            let block = (rec[0x3F] as u32) * 256 * 1024 + (rec[0x40] as u32) * 1024;
            let vcc = match rec[0x43] {
                0x00 => Some("3.3".to_string()),
                0x01 => Some("1.8".to_string()),
                0x02 => Some("5.0".to_string()),
                0x03 => Some("2.5".to_string()),
                _ => None,
            };

            dat_chips.push(DatChip {
                id,
                model,
                manufacturer,
                sector,
                block,
                algorithm,
                addr4bit,
                timing,
                vcc,
            });
        }

        let mut by_id: HashMap<String, DatChip> = HashMap::new();
        let mut by_model: HashMap<String, Vec<DatChip>> = HashMap::new();
        let mut matched_by_id = 0usize;
        for chip in &dat_chips {
            if !chip.id.is_empty() {
                by_id.entry(chip.id.clone()).or_insert_with(|| chip.clone());
            }
            let key = format!("{}|{}", norm(&chip.manufacturer), norm(&chip.model));
            by_model.entry(key).or_default().push(chip.clone());
        }

        let mut updated = 0usize;
        let mut matched_by_name = 0usize;

        for entry in &mut self.entries {
            let mut id_str = String::new();
            for &b in &entry.id {
                if b == 0 {
                    break;
                }
                id_str.push(b as char);
            }
            let attrs = Self::parse_data_attrs(
                &self.data[entry.data_offset as usize..][..entry.data_len as usize],
            );
            let model = attrs.get("model").cloned().unwrap_or_default();
            let vendor = attrs.get("vendor").cloned().unwrap_or_default();

            let candidate = if !id_str.is_empty() {
                if let Some(chip) = by_id.get(&id_str) {
                    matched_by_id += 1;
                    Some(chip.clone())
                } else {
                    None
                }
            } else {
                None
            };

            let candidate = candidate.or_else(|| {
                let key = format!("{}|{}", norm(&vendor), norm(&model));
                if let Some(list) = by_model.get(&key) {
                    if let Some(chip) = list.first() {
                        matched_by_name += 1;
                        return Some(chip.clone());
                    }
                }
                None
            });

            let Some(chip) = candidate else {
                continue;
            };

            let mut attrs = attrs;
            let mut changed = false;
            if chip.sector > 0 && !attrs.contains_key("sector") {
                attrs.insert("sector".into(), chip.sector.to_string());
                changed = true;
            }
            if chip.block > 0 && !attrs.contains_key("block") {
                attrs.insert("block".into(), chip.block.to_string());
                changed = true;
            }
            if !attrs.contains_key("algorithm") {
                attrs.insert("algorithm".into(), chip.algorithm.to_string());
                changed = true;
            }
            if !attrs.contains_key("addr4bit") {
                attrs.insert("addr4bit".into(), chip.addr4bit.to_string());
                changed = true;
            }
            if chip.timing > 0 && !attrs.contains_key("timing") {
                attrs.insert("timing".into(), chip.timing.to_string());
                changed = true;
            }
            if !attrs.contains_key("vcc") {
                if let Some(vcc) = &chip.vcc {
                    attrs.insert("vcc".into(), vcc.clone());
                    changed = true;
                }
            }

            if !changed {
                continue;
            }

            // Canonical serialization: vendor/model/protocol first, then the
            // remaining keys in sorted order. Trailing NUL matches the binary
            // writer in save_bin().
            let mut parts: Vec<String> = Vec::with_capacity(attrs.len());
            for key in ["vendor", "model", "protocol"] {
                if let Some(value) = attrs.get(key) {
                    parts.push(format!("{}={}", key, value));
                }
            }
            let mut rest: Vec<&String> = attrs
                .keys()
                .filter(|k| !["vendor", "model", "protocol"].contains(&k.as_str()))
                .collect();
            rest.sort();
            for key in rest {
                if let Some(value) = attrs.get(key) {
                    parts.push(format!("{}={}", key, value));
                }
            }
            let mut blob = parts.join("\0").into_bytes();
            blob.push(0);

            entry.data_offset = self.data.len() as u32;
            entry.data_len = blob.len() as u16;
            self.data.extend_from_slice(&blob);
            updated += 1;
        }

        self.version = 20260101;
        Ok(ImportStats {
            dat_records: dat_chips.len(),
            matched_by_id,
            matched_by_name,
            entries_updated: updated,
        })
    }

    pub fn find_by_id(&self, id_str: &str) -> Option<ChipInfo> {
        let mut key = [0u8; MAX_ID_LEN];
        let src = id_str.as_bytes();
        let len = src.len().min(MAX_ID_LEN);
        key[..len].copy_from_slice(&src[..len]);
        self.entries
            .binary_search_by(|e| e.id.cmp(&key))
            .ok()
            .map(|idx| {
                let entry = &self.entries[idx];
                let data_slice =
                    &self.data[entry.data_offset as usize..][..entry.data_len as usize];
                let attrs = Self::parse_data_attrs(data_slice);
                let vendor = attrs.get("vendor").cloned().unwrap_or_default();
                let model = attrs.get("model").cloned().unwrap_or_default();
                let protocol = attrs.get("protocol").cloned().unwrap_or_default();
                let size = attrs.get("size").and_then(|v| v.parse().ok()).unwrap_or(0);
                let page = attrs.get("page").and_then(|v| v.parse().ok()).unwrap_or(0);
                ChipInfo {
                    id: id_str.to_string(),
                    vendor,
                    model,
                    protocol,
                    size,
                    page,
                    attrs,
                }
            })
    }

    /// Look up a chip by its database coordinates (manual selection path for
    /// I2C / Microwire / EEPROM chips that have no JEDEC ID).
    pub fn find_by_model(&self, protocol: &str, vendor: &str, model: &str) -> Option<ChipInfo> {
        for entry in &self.entries {
            let data_slice = &self.data[entry.data_offset as usize..][..entry.data_len as usize];
            let attrs = Self::parse_data_attrs(data_slice);
            if attrs.get("protocol").map(String::as_str) != Some(protocol)
                || attrs.get("vendor").map(String::as_str) != Some(vendor)
                || attrs.get("model").map(String::as_str) != Some(model)
            {
                continue;
            }
            let id = attrs.get("id").cloned().unwrap_or_default();
            return Some(ChipInfo {
                id,
                vendor: vendor.to_string(),
                model: model.to_string(),
                protocol: protocol.to_string(),
                size: attrs.get("size").and_then(|v| v.parse().ok()).unwrap_or(0),
                page: attrs.get("page").and_then(|v| v.parse().ok()).unwrap_or(0),
                attrs,
            });
        }
        None
    }

    #[allow(dead_code)] // used by the chipdb_tool example
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of entries per protocol, ordered by the fixed protocol id.
    pub fn protocol_counts(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(u16, usize)> = (0..=11).map(|id| (id, 0)).collect();
        for entry in &self.entries {
            if (entry.protocol as usize) < counts.len() {
                counts[entry.protocol as usize].1 += 1;
            }
        }
        counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(id, count)| (protocol_id_to_name(id), count))
            .collect()
    }

    // 新增：列出所有协议类型（去重，按原数字顺序）
    pub fn list_protocols(&self) -> Vec<String> {
        let mut protos = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in &self.entries {
            let proto_name = protocol_id_to_name(entry.protocol);
            if seen.insert(proto_name.clone()) {
                protos.push(proto_name);
            }
        }
        protos
    }

    // 新增：列出某协议下的所有厂商
    pub fn list_vendors(&self, protocol_name: &str) -> Vec<String> {
        let proto_id = protocol_name_to_id(protocol_name);
        let mut vendors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in &self.entries {
            if entry.protocol == proto_id {
                let data_slice =
                    &self.data[entry.data_offset as usize..][..entry.data_len as usize];
                let attrs = Self::parse_data_attrs(data_slice);
                if let Some(vendor) = attrs.get("vendor") {
                    if seen.insert(vendor.clone()) {
                        vendors.push(vendor.clone());
                    }
                }
            }
        }
        vendors.sort();
        vendors
    }

    // 新增：列出某协议某厂商下的所有型号
    pub fn list_models(&self, protocol_name: &str, vendor_name: &str) -> Vec<String> {
        let proto_id = protocol_name_to_id(protocol_name);
        let mut models = Vec::new();
        for entry in &self.entries {
            if entry.protocol == proto_id {
                let data_slice =
                    &self.data[entry.data_offset as usize..][..entry.data_len as usize];
                let attrs = Self::parse_data_attrs(data_slice);
                if let Some(vendor) = attrs.get("vendor") {
                    if vendor == vendor_name {
                        if let Some(model) = attrs.get("model") {
                            models.push(model.clone());
                        }
                    }
                }
            }
        }
        models.sort();
        models
    }

    pub fn load_bin(path: &str) -> Result<Self, String> {
        let raw = fs::read(path).map_err(|e| e.to_string())?;
        // 兼容旧版明文 bin：以 CHIP magic 开头则直接解析，否则视为混淆数据。
        // 解码只在内存中进行，绝不写回明文文件。
        let file = if raw.starts_with(b"CHIP") {
            raw
        } else {
            deobfuscate(&raw)
        };
        if file.len() < size_of::<Header>() {
            return Err("bin 文件太小".into());
        }
        let header = unsafe { &*(file.as_ptr() as *const Header) };
        if header.magic != MAGIC {
            return Err("魔术不匹配".into());
        }
        if header.index_entry_size as usize != size_of::<IndexEntry>() {
            return Err("索引项大小不一致".into());
        }
        let count = header.chip_count as usize;
        let index_start = size_of::<Header>();
        let index_end = index_start + count * size_of::<IndexEntry>();
        if file.len() < index_end {
            return Err("索引区不完整".into());
        }
        let entries: Vec<IndexEntry> = unsafe {
            let ptr = file.as_ptr().add(index_start) as *const IndexEntry;
            std::slice::from_raw_parts(ptr, count).to_vec()
        };
        let data_start = header.data_offset as usize;
        let data = file[data_start..].to_vec();
        Ok(Chiplib {
            entries,
            data,
            version: header.version,
        })
    }

    pub fn load_xml(path: &str) -> Result<Self, String> {
        let raw = fs::read(path).map_err(|e| e.to_string())?;
        // 兼容旧版明文 XML；混淆后的 XML 先解码再解析，解码结果不落盘。
        let file_bytes = if raw.starts_with(b"<") {
            raw
        } else {
            let decoded = deobfuscate(&raw);
            if !decoded.starts_with(b"<") {
                return Err("XML 混淆数据无效".into());
            }
            decoded
        };
        let xml_data = String::from_utf8_lossy(&file_bytes).into_owned();
        let mut entries = Vec::new();
        let mut data_blob = Vec::new();
        let protocol_map: HashMap<&str, u16> = [
            ("SPI_EC", 0),
            ("SPI_DATA_45", 1),
            ("SPI_NAND", 2),
            ("SPI_NOR", 3),
            ("SPI_EEPROM", 4),
            ("SPI_F-RAM", 5),
            ("I2C", 6),
            ("I2C_F-RAM", 7),
            ("I2C_SPD", 8),
            ("Microwire", 9),
            ("AVR", 10),
            ("MCU", 11),
            ("PARALLEL_NAND", 12),
        ]
        .iter()
        .cloned()
        .collect();

        #[derive(Clone)]
        struct StackItem {
            tag_type: String,
            name: String,
        }
        let mut stack: Vec<StackItem> = Vec::new();
        let mut pos = 0;
        let bytes = xml_data.as_bytes();
        let len = bytes.len();

        while pos < len {
            if let Some(tag_start) = bytes[pos..].iter().position(|&b| b == b'<') {
                let tag_start = pos + tag_start;
                let inner_start = tag_start + 1;
                if let Some(tag_end) = bytes[inner_start..].iter().position(|&b| b == b'>') {
                    let tag_end = inner_start + tag_end;
                    let tag_content = &xml_data[inner_start..tag_end];

                    if tag_content.starts_with("!--") {
                        if let Some(comment_end) = xml_data[tag_start..].find("-->") {
                            pos = tag_start + comment_end + 3;
                            continue;
                        }
                    }

                    let is_closing = tag_content.starts_with('/');
                    let mut clean_content = if is_closing {
                        &tag_content[1..]
                    } else {
                        tag_content
                    };
                    clean_content = clean_content.trim();
                    let is_self_closing = clean_content.ends_with('/');
                    if is_self_closing {
                        clean_content = clean_content[..clean_content.len() - 1].trim();
                    }

                    let (tag_name, attr_str) =
                        if let Some(space_idx) = clean_content.find(char::is_whitespace) {
                            let name = &clean_content[..space_idx];
                            let rest = clean_content[space_idx..].trim();
                            (name, rest)
                        } else {
                            (clean_content, "")
                        };

                    if is_closing {
                        if let Some(top) = stack.last() {
                            if top.name == tag_name {
                                stack.pop();
                            }
                        }
                    } else {
                        if protocol_map.contains_key(tag_name) {
                            stack.push(StackItem {
                                tag_type: "protocol".into(),
                                name: tag_name.to_string(),
                            });
                        } else {
                            let current_protocol = stack
                                .iter()
                                .rev()
                                .find(|s| s.tag_type == "protocol")
                                .map(|s| s.name.clone());
                            let current_vendor = stack
                                .iter()
                                .rev()
                                .find(|s| s.tag_type == "vendor")
                                .map(|s| s.name.clone());
                            if let Some(proto) = current_protocol {
                                if attr_str.is_empty() {
                                    stack.push(StackItem {
                                        tag_type: "vendor".into(),
                                        name: tag_name.to_string(),
                                    });
                                } else {
                                    let attrs = parse_attr_string(attr_str);
                                    let mut id = String::new();
                                    let mut attr_pairs = String::new();
                                    let vendor_name = current_vendor.unwrap_or_default();
                                    attr_pairs.push_str(&format!("vendor={}\0", vendor_name));
                                    attr_pairs.push_str(&format!("model={}\0", tag_name));
                                    attr_pairs.push_str(&format!("protocol={}\0", proto));
                                    for (key, value) in &attrs {
                                        if key == "id" {
                                            id = value.clone();
                                        }
                                        attr_pairs.push_str(&format!("{}={}\0", key, value));
                                    }
                                    let mut id_bytes = [0u8; MAX_ID_LEN];
                                    let src = id.as_bytes();
                                    let len = src.len().min(MAX_ID_LEN);
                                    id_bytes[..len].copy_from_slice(&src[..len]);
                                    let data_offset = data_blob.len() as u32;
                                    let data_bytes = attr_pairs.into_bytes();
                                    let data_len = data_bytes.len() as u16;
                                    let proto_num =
                                        protocol_map.get(proto.as_str()).copied().unwrap_or(0xFF);
                                    entries.push(IndexEntry {
                                        id: id_bytes,
                                        data_offset,
                                        data_len,
                                        protocol: proto_num,
                                    });
                                    data_blob.extend(data_bytes);
                                }
                            }
                        }
                    }
                    pos = tag_end + 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        entries.sort_by_key(|a| a.id);
        Ok(Chiplib {
            entries,
            data: data_blob,
            version: 20250628,
        })
    }

    pub fn save_bin(&self, path: &str) -> Result<(), String> {
        let index_size = self.entries.len() * size_of::<IndexEntry>();
        let data_offset = (size_of::<Header>() + index_size) as u32;
        let header = Header {
            magic: MAGIC,
            version: self.version,
            chip_count: self.entries.len() as u32,
            data_offset,
            index_entry_size: size_of::<IndexEntry>() as u16,
            padding: 0,
        };

        // 先在内存中生成明文二进制，混淆后一次性写盘；工作目录中不会出现明文库。
        let mut plain = Vec::with_capacity(data_offset as usize + self.data.len());
        plain.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&header as *const Header as *const u8, size_of::<Header>())
        });
        for e in &self.entries {
            plain.extend_from_slice(unsafe {
                std::slice::from_raw_parts(
                    e as *const IndexEntry as *const u8,
                    size_of::<IndexEntry>(),
                )
            });
        }
        plain.extend_from_slice(&self.data);

        let sealed = obfuscate(&plain);
        fs::write(path, &sealed).map_err(|e| e.to_string())
    }

    /// Insert or replace one chip entry by JEDEC ID while keeping every
    /// other entry untouched. The previous data blob (when replacing) is
    /// intentionally left in place; save_bin() writes the new appended blob.
    #[allow(dead_code)] // used by the chipdb_tool example
    pub fn upsert_chip(
        &mut self,
        id: &str,
        vendor: &str,
        model: &str,
        protocol: &str,
        extra_attrs: &[(&str, &str)],
    ) -> Result<(), String> {
        let proto_id = protocol_name_to_id(protocol);
        if proto_id == 0xFF {
            return Err(format!("未知协议: {}", protocol));
        }
        if id.is_empty() || id.len() > MAX_ID_LEN {
            return Err(format!("无效 ID: {}", id));
        }

        let mut attrs: Vec<(String, String)> = vec![
            ("vendor".into(), vendor.to_string()),
            ("model".into(), model.to_string()),
            ("protocol".into(), protocol.to_string()),
        ];
        attrs.extend(
            extra_attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        if !attrs.iter().any(|(k, _)| k == "id") {
            attrs.push(("id".into(), id.to_string()));
        }
        let blob = Self::build_blob(&attrs)?;

        let mut id_bytes = [0u8; MAX_ID_LEN];
        id_bytes[..id.len()].copy_from_slice(id.as_bytes());
        let entry = IndexEntry {
            id: id_bytes,
            data_offset: self.data.len() as u32,
            data_len: blob.len() as u16,
            protocol: proto_id,
        };
        match self.entries.binary_search_by(|e| e.id.cmp(&id_bytes)) {
            Ok(idx) => self.entries[idx] = entry,
            Err(idx) => self.entries.insert(idx, entry),
        }
        self.data.extend_from_slice(&blob);
        Ok(())
    }

    /// Merge mode used by the SNANDer table importer: missing chips are
    /// inserted; existing chips only get the keys they don't already have,
    /// so enriched fields (sector/block/vcc/timing, ...) are never lost.
    #[allow(dead_code)] // used by the chipdb_tool example
    pub fn upsert_chip_merge(
        &mut self,
        id: &str,
        vendor: &str,
        model: &str,
        protocol: &str,
        extra_attrs: &[(&str, &str)],
    ) -> Result<(), String> {
        let mut id_bytes = [0u8; MAX_ID_LEN];
        id_bytes[..id.len()].copy_from_slice(id.as_bytes());

        let result = if let Ok(idx) = self.entries.binary_search_by(|e| e.id.cmp(&id_bytes)) {
            let entry = &self.entries[idx];
            let mut attrs = Self::parse_data_attrs(
                &self.data[entry.data_offset as usize..][..entry.data_len as usize],
            );
            if attrs.get("vendor").filter(|v| !v.is_empty()).is_none() && !vendor.is_empty() {
                attrs.insert("vendor".into(), vendor.to_string());
            }
            if attrs.get("model").filter(|v| !v.is_empty()).is_none() && !model.is_empty() {
                attrs.insert("model".into(), model.to_string());
            }
            for (key, value) in extra_attrs {
                if !attrs.contains_key(*key) && !value.is_empty() {
                    attrs.insert(key.to_string(), value.to_string());
                }
            }
            self.replace_entry_blob(idx, attrs)
        } else {
            self.upsert_chip(id, vendor, model, protocol, extra_attrs)
        };
        self.version = 20260816;
        result
    }

    fn build_blob(attrs: &[(String, String)]) -> Result<Vec<u8>, String> {
        // Canonical serialization: vendor/model/protocol first, remaining
        // keys sorted, trailing NUL (matches import_imsprog_dat).
        let mut parts: Vec<String> = Vec::with_capacity(attrs.len());
        for key in ["vendor", "model", "protocol"] {
            if let Some((_, value)) = attrs.iter().find(|(k, _)| k == key) {
                parts.push(format!("{}={}", key, value));
            }
        }
        let mut rest: Vec<&(String, String)> = attrs
            .iter()
            .filter(|(k, _)| !["vendor", "model", "protocol"].contains(&k.as_str()))
            .collect();
        rest.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, value) in rest {
            parts.push(format!("{}={}", key, value));
        }
        let mut blob = parts.join("\0").into_bytes();
        blob.push(0);
        if blob.len() > u16::MAX as usize {
            return Err("芯片属性块超过 64KB 上限".into());
        }
        Ok(blob)
    }

    fn replace_entry_blob(
        &mut self,
        idx: usize,
        attrs: HashMap<String, String>,
    ) -> Result<(), String> {
        let entry = &self.entries[idx];
        let all: Vec<(String, String)> =
            attrs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let blob = Self::build_blob(&all)?;
        let new_entry = IndexEntry {
            id: entry.id,
            data_offset: self.data.len() as u32,
            data_len: blob.len() as u16,
            protocol: entry.protocol,
        };
        self.entries[idx] = new_entry;
        self.data.extend_from_slice(&blob);
        Ok(())
    }

    fn parse_data_attrs(data: &[u8]) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let mut start = 0;
        while start < data.len() {
            if let Some(eq_pos) = data[start..].iter().position(|&b| b == b'=') {
                let key = String::from_utf8_lossy(&data[start..start + eq_pos]).to_string();
                start += eq_pos + 1;
                let val_end = data[start..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(data.len() - start);
                let val = String::from_utf8_lossy(&data[start..start + val_end]).to_string();
                map.insert(key, val);
                start += val_end + 1;
            } else {
                break;
            }
        }
        map
    }
}

fn parse_attr_string(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = String::from_utf8_lossy(&bytes[key_start..i]).to_string();
        while i < bytes.len() && (bytes[i] == b'=' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let val_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let value = String::from_utf8_lossy(&bytes[val_start..i]).to_string();
            i += 1;
            attrs.push((key, value));
        } else {
            let val_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            let value = String::from_utf8_lossy(&bytes[val_start..i]).to_string();
            attrs.push((key, value));
        }
    }
    attrs
}

fn protocol_id_to_name(id: u16) -> String {
    match id {
        0 => "SPI_EC".to_string(),
        1 => "SPI_DATA_45".to_string(),
        2 => "SPI_NAND".to_string(),
        3 => "SPI_NOR".to_string(),
        4 => "SPI_EEPROM".to_string(),
        5 => "SPI_F-RAM".to_string(),
        6 => "I2C".to_string(),
        7 => "I2C_F-RAM".to_string(),
        8 => "I2C_SPD".to_string(),
        9 => "Microwire".to_string(),
        10 => "AVR".to_string(),
        11 => "MCU".to_string(),
        12 => "PARALLEL_NAND".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn protocol_name_to_id(name: &str) -> u16 {
    match name {
        "SPI_EC" => 0,
        "SPI_DATA_45" => 1,
        "SPI_NAND" => 2,
        "SPI_NOR" => 3,
        "SPI_EEPROM" => 4,
        "SPI_F-RAM" => 5,
        "I2C" => 6,
        "I2C_F-RAM" => 7,
        "I2C_SPD" => 8,
        "Microwire" => 9,
        "AVR" => 10,
        "MCU" => 11,
        "PARALLEL_NAND" => 12,
        _ => 0xFF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Root directory that contains `chiplib.bin` and `chiplib.xml`.
    ///
    /// Cargo runs unit tests with the package root (`crates/uni-chipdb`) as
    /// the current directory, so the default is two levels up. Set
    /// `UNIPROG_ROOT` to point elsewhere when running a compiled test binary
    /// directly from a different directory.
    fn uniprog_root() -> PathBuf {
        if let Some(dir) = std::env::var_os("UNIPROG_ROOT") {
            return PathBuf::from(dir);
        }
        PathBuf::from("../../")
    }

    #[test]
    fn obfuscation_round_trip() {
        let original = b"CHIP-vendor=WINBOND-model=W25Q64-size=8388608";
        let sealed = obfuscate(original);
        assert_ne!(sealed.as_slice(), original.as_slice());
        assert!(!String::from_utf8_lossy(&sealed).contains("WINBOND"));
        assert_eq!(deobfuscate(&sealed), original);
    }

    #[test]
    fn xml_fallback_contains_new_chip() {
        let xml_path = uniprog_root().join("chiplib.xml");
        let xml_path = xml_path.to_str().expect("path must be UTF-8");
        let lib = Chiplib::load_xml(xml_path).expect("load chiplib.xml");
        assert_eq!(lib.entry_count(), 1429);
        let d40 = lib.find_by_id("5E3213").expect("ZB25D40B in XML fallback");
        assert_eq!(d40.vendor, "Zbit");
        assert_eq!(d40.model, "ZB25D40B");
        assert_eq!(d40.protocol, "SPI_NOR");
        assert_eq!(d40.page, 256);
        assert_eq!(d40.size, 512 * 1024);
        assert_eq!(d40.attr_u32("sector"), Some(4096));
        assert_eq!(d40.attr_u32("block"), Some(64 * 1024));
        assert_eq!(d40.attr("vcc"), Some("3.3"));

        let merged = lib
            .find_by_id("0B4018")
            .expect("XT25F128B in XML fallback after library expansion");
        assert_eq!(merged.vendor, "XTX");
        assert_eq!(merged.protocol, "SPI_NOR");
        assert_eq!(merged.size, 16 * 1024 * 1024);

        let ratchet_merged = lib
            .find_by_id("EF4021")
            .expect("W25Q01JV imported from ratchet chip database");
        assert_eq!(ratchet_merged.protocol, "SPI_NOR");
        assert_eq!(ratchet_merged.size, 128 * 1024 * 1024);
        assert_eq!(ratchet_merged.attr_u32("sector"), Some(4096));
        assert_eq!(ratchet_merged.attr_u32("block"), Some(65536));

        let prepend = lib
            .find_by_id("C8B1")
            .expect("GD5F1GQ4UC imported with dummy mode");
        assert_eq!(prepend.protocol, "SPI_NAND");
        assert_eq!(prepend.attr("dummyMode"), Some("prepend"));
        assert_eq!(prepend.attr("readMode"), Some("dual"));
        assert_eq!(prepend.attr_u32("feature"), Some(0));
    }

    #[test]
    fn load_enriched_bin() {
        let bin_path = uniprog_root().join("chiplib.bin");
        let bin_path = bin_path.to_str().expect("path must be UTF-8");
        let lib = Chiplib::load_bin(bin_path).expect("load chiplib.bin");
        assert_eq!(lib.entries.len(), 1429);

        let nor = lib.find_by_id("EF4018").expect("W25Q128 JEDEC");
        assert_eq!(nor.protocol, "SPI_NOR");
        assert_eq!(nor.vendor, "WINBOND");
        assert_eq!(nor.size, 16 * 1024 * 1024);
        assert!(nor.attr_u32("sector").is_some());
        assert!(nor.attr_u32("block").is_some());

        let ratchet_merged = lib
            .find_by_id("EF4021")
            .expect("W25Q01JV imported from ratchet chip database");
        assert_eq!(ratchet_merged.protocol, "SPI_NOR");
        assert_eq!(ratchet_merged.size, 128 * 1024 * 1024);
        assert_eq!(ratchet_merged.attr_u32("sector"), Some(4096));
        assert_eq!(ratchet_merged.attr_u32("block"), Some(65536));
        assert_eq!(ratchet_merged.attr("addr4bit"), Some("01"));

        let d40 = lib.find_by_id("5E3213").expect("Zbit ZB25D40B JEDEC");
        assert_eq!(d40.protocol, "SPI_NOR");
        assert_eq!(d40.vendor, "Zbit");
        assert_eq!(d40.model, "ZB25D40B");
        assert_eq!(d40.size, 512 * 1024);
        assert_eq!(d40.attr_u32("sector"), Some(4096));
        assert_eq!(d40.attr_u32("block"), Some(64 * 1024));

        let xtx = lib
            .find_by_id("0B4018")
            .expect("XT25F128B from library expansion");
        assert_eq!(xtx.protocol, "SPI_NOR");
        assert_eq!(xtx.vendor, "XTX");
        assert_eq!(xtx.model, "XT25F128B");
        assert_eq!(xtx.size, 16 * 1024 * 1024);

        let nand = lib
            .find_by_id("EFBA22")
            .expect("W25N02KWZEIR from library expansion");
        assert_eq!(nand.protocol, "SPI_NAND");
        assert_eq!(nand.vendor, "WINBOND");
        assert_eq!(nand.model, "W25N02KWZEIR");
        assert_eq!(nand.page, 2048);
        assert_eq!(nand.size, 256 * 1024 * 1024);
        assert_eq!(nand.attr_u32("spare"), Some(128));
        assert_eq!(nand.attr_u32("pagePerBlock"), Some(64));
        assert!(nand.attr("IsBMM").is_some());

        let dataflash = lib
            .find_by_id("1F2200")
            .expect("AT45DB011D binary from library expansion");
        assert_eq!(dataflash.protocol, "SPI_DATA_45");
        assert_eq!(dataflash.vendor, "ATMEL");
        assert_eq!(dataflash.model, "AT45DB011D_3V3_binary");
        assert_eq!(dataflash.page, 256);
        assert_eq!(dataflash.size, 128 * 1024);

        let i2c = lib
            .find_by_model("I2C", "Generic", "_24C02")
            .expect("24C02");
        assert_eq!(i2c.size, 256);
        assert!(i2c.attr("addrtype").is_some());

        let prepend = lib
            .find_by_id("C8B1")
            .expect("GD5F1GQ4UC imported with dummy mode");
        assert_eq!(prepend.protocol, "SPI_NAND");
        assert_eq!(prepend.attr("dummyMode"), Some("prepend"));
        assert_eq!(prepend.attr("readMode"), Some("dual"));
    }
}
