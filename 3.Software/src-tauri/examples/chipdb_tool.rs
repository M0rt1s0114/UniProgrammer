//! Offline maintenance tool for chiplib.bin.
//!
//! Usage:
//!   cargo run --example chipdb_tool -- <chiplib.bin> <IMSProg.Dat> [--backup]
//!   cargo run --example chipdb_tool -- add <chiplib.bin> <id> <vendor> <model> <protocol> [key=value ...]
//!
//! The first form enriches chiplib.bin with IMSProg.Dat fields (sector,
//! block, addr4bit, timing, vcc) without overwriting values already present.
//! The `add` form inserts or replaces a single chip by JEDEC ID, which is
//! useful for chips reported by the programmer as unknown IDs.

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
