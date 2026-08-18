//! Offline maintenance tool for chiplib.bin.
//!
//! Usage:
//!   cargo run --example chipdb_tool -- <chiplib.bin> <IMSProg.Dat> [--backup]
//!   cargo run --example chipdb_tool -- add <chiplib.bin> <id> <vendor> <model> <protocol> [key=value ...]
//!   cargo run --example chipdb_tool -- merge <chiplib.bin> <chips.tsv>
//!   cargo run --example chipdb_tool -- xml2bin <chiplib.xml> <chiplib.bin>
//!
//! The first form enriches chiplib.bin with IMSProg.Dat fields (sector,
//! block, addr4bit, timing, vcc) without overwriting values already present.
//! The `add` form inserts or replaces a single chip by JEDEC ID, which is
//! useful for chips reported by the programmer as unknown IDs.
//! The `merge` form imports a TSV table (header + one chip per line) with
//! merge semantics: new chips are inserted, existing chips only receive
//! missing attributes. The `xml2bin` form rebuilds chiplib.bin from the
//! (possibly obfuscated) XML source without writing any plaintext to disk.

#[path = "../src/chiplib.rs"]
#[allow(dead_code)]
mod chiplib;

use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "add" {
        run_add(&args);
        return;
    }
    if args.len() >= 2 && args[1] == "merge" {
        run_merge(&args);
        return;
    }
    if args.len() >= 2 && args[1] == "xml2bin" {
        run_xml_to_bin(&args);
        return;
    }

    if args.len() < 3 {
        eprintln!("用法: chipdb_tool <chiplib.bin> <IMSProg.Dat> [--backup]");
        eprintln!(
            "      chipdb_tool add <chiplib.bin> <id> <vendor> <model> <protocol> [key=value ...]"
        );
        std::process::exit(2);
    }
    let bin_path = &args[1];
    let dat_path = &args[2];
    let backup = args.iter().any(|a| a == "--backup");

    if backup {
        let bak = format!("{}.bak", bin_path);
        fs::copy(bin_path, &bak).unwrap_or_else(|e| {
            eprintln!("备份失败 {} -> {}: {}", bin_path, bak, e);
            std::process::exit(1);
        });
        println!("已备份: {}", bak);
    }

    let mut lib = match chiplib::Chiplib::load_bin(bin_path) {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("加载 {} 失败: {}", bin_path, e);
            std::process::exit(1);
        }
    };

    let stats = match lib.import_imsprog_dat(dat_path) {
        Ok(stats) => stats,
        Err(e) => {
            eprintln!("导入 {} 失败: {}", dat_path, e);
            std::process::exit(1);
        }
    };

    if let Err(e) = lib.save_bin(bin_path) {
        eprintln!("保存 {} 失败: {}", bin_path, e);
        std::process::exit(1);
    }

    println!(
        "IMSProg.Dat 记录: {}\n按 ID 匹配: {}\n按型号匹配: {}\n更新条目: {}\n已写回: {}",
        stats.dat_records,
        stats.matched_by_id,
        stats.matched_by_name,
        stats.entries_updated,
        bin_path
    );
}

fn run_xml_to_bin(args: &[String]) {
    if args.len() < 4 {
        eprintln!("用法: chipdb_tool xml2bin <chiplib.xml> <chiplib.bin>");
        std::process::exit(2);
    }
    let xml_path = &args[2];
    let bin_path = &args[3];
    match chiplib::Chiplib::convert_xml_to_bin(xml_path, bin_path) {
        Ok(()) => {
            let lib = chiplib::Chiplib::load_bin(bin_path).unwrap_or_else(|e| {
                eprintln!("回读校验 {} 失败: {}", bin_path, e);
                std::process::exit(1);
            });
            println!(
                "已由 {} 重建 {}，共 {} 条记录",
                xml_path,
                bin_path,
                lib.entry_count()
            );
        }
        Err(e) => {
            eprintln!("重建 {} -> {} 失败: {}", xml_path, bin_path, e);
            std::process::exit(1);
        }
    }
}

fn run_merge(args: &[String]) {
    if args.len() < 4 {
        eprintln!("用法: chipdb_tool merge <chiplib.bin> <chips.tsv>");
        std::process::exit(2);
    }
    let bin_path = &args[2];
    let tsv_path = &args[3];
    let text = match fs::read_to_string(tsv_path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("读取 {} 失败: {}", tsv_path, e);
            std::process::exit(1);
        }
    };

    let mut lib = match chiplib::Chiplib::load_bin(bin_path) {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("加载 {} 失败: {}", bin_path, e);
            std::process::exit(1);
        }
    };

    let mut imported = 0usize;
    let mut merged = 0usize;
    for (line_no, line) in text.lines().enumerate() {
        if line_no == 0 || line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 5 {
            eprintln!("第 {} 行字段不足: {}", line_no + 1, line);
            std::process::exit(1);
        }
        let id = fields[0].trim();
        let vendor = fields[1].trim();
        let model = fields[2].trim();
        let protocol = fields[3].trim();
        let mut attrs: Vec<(String, String)> = Vec::new();
        for (idx, raw) in fields.iter().enumerate().skip(4) {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let key = match idx {
                4 => "page",
                5 => "size",
                6 => "block",
                7 => "spare",
                8 => "sector",
                9 => "dummyMode",
                10 => "readMode",
                11 => "writeMode",
                12 => "feature",
                _ => "",
            };
            if key.is_empty() {
                continue;
            }
            attrs.push((key.to_string(), raw.to_string()));
        }

        let before = lib.entry_count();
        let attr_refs: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if let Err(e) = lib.upsert_chip_merge(id, vendor, model, protocol, &attr_refs) {
            eprintln!("导入 {} {} 失败: {}", vendor, model, e);
            std::process::exit(1);
        }
        if lib.entry_count() != before {
            imported += 1;
        } else {
            merged += 1;
        }
    }

    if let Err(e) = lib.save_bin(bin_path) {
        eprintln!("保存 {} 失败: {}", bin_path, e);
        std::process::exit(1);
    }

    println!(
        "已导入 TSV：新增 {} 条、合并属性 {} 条，共 {} 条记录 -> {}",
        imported,
        merged,
        lib.entry_count(),
        bin_path
    );
}

fn run_add(args: &[String]) {
    if args.len() < 7 {
        eprintln!(
            "用法: chipdb_tool add <chiplib.bin> <id> <vendor> <model> <protocol> [key=value ...]"
        );
        std::process::exit(2);
    }
    let bin_path = &args[2];
    let id = &args[3];
    let vendor = &args[4];
    let model = &args[5];
    let protocol = &args[6];

    let mut attrs: Vec<(&str, &str)> = Vec::new();
    for arg in &args[7..] {
        let Some((key, value)) = arg.split_once('=') else {
            eprintln!("无效属性（应为 key=value）: {}", arg);
            std::process::exit(2);
        };
        attrs.push((key, value));
    }

    let mut lib = match chiplib::Chiplib::load_bin(bin_path) {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("加载 {} 失败: {}", bin_path, e);
            std::process::exit(1);
        }
    };

    if let Err(e) = lib.upsert_chip(id, vendor, model, protocol, &attrs) {
        eprintln!("添加芯片失败: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = lib.save_bin(bin_path) {
        eprintln!("保存 {} 失败: {}", bin_path, e);
        std::process::exit(1);
    }

    println!(
        "已写入 {}: {} {} {} ({} 条记录)",
        bin_path,
        vendor,
        model,
        id,
        lib.entry_count()
    );
}
