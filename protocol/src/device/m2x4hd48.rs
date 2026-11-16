//! miniDSP 2x4 HD running the 48 kHz firmware (hardware ID 10, DSP ID 99).
//!
//! The common EEPROM controls are supported. DSP parameter addresses must be
//! verified against a 48 kHz configuration before enabling channel controls.
//! In particular, do not assume the 96 kHz profile's parameter layout is valid.

use super::*;

pub const DEVICE: Device = Device {
    product_name: "2x4HD (48kHz)",
    sources: &[Analog, Toslink, Usb],
    inputs: &[],
    outputs: &[],
    fir_max_taps: 0,
    internal_sampling_rate: 48000,
    dialect: Dialect {
        addr_encoding: AddrEncoding::AddrLen3,
        float_encoding: FloatEncoding::Float32LE,
    },
    #[cfg(feature = "symbols")]
    symbols: &[],
};
