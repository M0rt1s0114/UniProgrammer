//! Application business logic: chip operations, programmer autodetection and
//! the local glue that implements the HAL SPI traits. No Tauri dependency.

pub mod autodetect;
pub mod core;
pub mod operations;
pub mod spi_bus_impl;
