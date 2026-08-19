//! Frontend-independent HAL stack: sidecar protocol client, HAL router,
//! SPI bus abstraction and transport-agnostic SPI NOR primitives.
//!
//! This crate is intentionally Tauri-free.

pub mod hal_router;
pub mod nor_ops;
pub mod sidecar_nor;
pub mod spi_bus;
pub mod upt_hal;

pub use hal_router::{AdapterSession, HalRouter, LoadedAdapter, SidecarSelection};
pub use nor_ops::{
    addr24, erase_chip, erase_sector, nor_bp_bits, page_program, read, read_id, verify, wp_disable,
    wp_status, write_enable, NorWpStatus, ADDR_MASK_24, BP_MASK_SR1, PAGE_SIZE,
};
pub use sidecar_nor::SidecarNor;
pub use spi_bus::{SidecarSpiBus, SpiBus};
pub use upt_hal::{
    frame, spawn_sidecar, ChildTransport, SidecarClient, SidecarDevice, SidecarTransport,
    SIDECAR_PROTOCOL_VERSION,
};
