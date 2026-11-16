//! Resolves a `Device` struct using a hardware id + dsp version

use crate::DeviceInfo;

#[derive(Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(
    feature = "use_serde",
    derive(
        strum::EnumString,
        strum::Display,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )
)]
#[cfg_attr(feature = "use_serde", strum(serialize_all = "lowercase"))]
#[derive(Default)]
pub enum DeviceKind {
    #[default]
    Generic,
    #[cfg(feature = "device_4x10hd")]
    M4x10Hd,
    #[cfg(feature = "device_10x10hd")]
    M10x10Hd,
    #[cfg(feature = "device_msharc4x8")]
    MSharc4x8,
    #[cfg(feature = "device_2x4hd")]
    M2x4Hd,
    #[cfg(feature = "device_2x4hd48")]
    M2x4Hd48,
    #[cfg(feature = "device_shd")]
    Shd,
    #[cfg(feature = "device_ddrc24")]
    DDRC24,
    #[cfg(feature = "device_ddrc88bm")]
    DDRC88BM,
    #[cfg(feature = "device_nanodigi2x8")]
    Nanodigi2x8,
    #[cfg(feature = "device_c8x12v2")]
    C8x12v2,
    #[cfg(feature = "device_m2x4")]
    M2x4,
    #[cfg(feature = "device_flex")]
    Flex,
    #[cfg(feature = "device_flexhtx")]
    FlexHtx,
}

/// Attempts to get a `&Device` from a DeviceInfo
pub fn probe(device_info: &DeviceInfo) -> &'static super::Device {
    by_kind(probe_kind(device_info))
}

pub fn probe_kind(device_info: &DeviceInfo) -> DeviceKind {
    use DeviceKind::*;
    match (device_info.hw_id, device_info.dsp_version) {
        #[cfg(feature = "device_10x10hd")]
        (1, 51) => M10x10Hd,
        #[cfg(feature = "device_4x10hd")]
        (1, _) => M4x10Hd,
        #[cfg(feature = "device_msharc4x8")]
        (4, _) => MSharc4x8,
        #[cfg(feature = "device_2x4hd")]
        (10, 100) => M2x4Hd,
        #[cfg(feature = "device_2x4hd48")]
        (10, 99) => M2x4Hd48,
        #[cfg(feature = "device_ddrc24")]
        (10, 101) => DDRC24,
        #[cfg(feature = "device_ddrc88bm")]
        (6, 95) => DDRC88BM,
        #[cfg(feature = "device_shd")]
        (14, _) => Shd,
        #[cfg(feature = "device_nanodigi2x8")]
        (2, 54) => Nanodigi2x8,
        #[cfg(feature = "device_c8x12v2")]
        (11, 97) => C8x12v2,
        // TODO: Figure out hw id and dsp version
        #[cfg(feature = "device_m2x4")]
        (2, 22) => M2x4,
        #[cfg(feature = "device_flex")]
        (27, 100) => Flex,

        #[cfg(feature = "device_flexhtx")]
        (32, 115) => FlexHtx, //get from `minidsp probe`

        _ => Generic,
    }
}

pub fn by_kind(kind: DeviceKind) -> &'static super::Device {
    use DeviceKind::*;
    match kind {
        Generic => &super::GENERIC,

        #[cfg(feature = "device_4x10hd")]
        M4x10Hd => &super::m4x10hd::DEVICE,

        #[cfg(feature = "device_msharc4x8")]
        MSharc4x8 => &super::msharc4x8::DEVICE,

        #[cfg(feature = "device_2x4hd")]
        M2x4Hd => &super::m2x4hd::DEVICE,

        #[cfg(feature = "device_2x4hd48")]
        M2x4Hd48 => &super::m2x4hd48::DEVICE,

        #[cfg(feature = "device_ddrc24")]
        DDRC24 => &super::ddrc24::DEVICE,

        #[cfg(feature = "device_ddrc88bm")]
        DDRC88BM => &super::ddrc88bm::DEVICE,

        #[cfg(feature = "device_shd")]
        Shd => &super::shd::DEVICE,

        #[cfg(feature = "device_nanodigi2x8")]
        Nanodigi2x8 => &super::nanodigi2x8::DEVICE,

        #[cfg(feature = "device_c8x12v2")]
        C8x12v2 => &super::c8x12v2::DEVICE,

        #[cfg(feature = "device_m2x4")]
        M2x4 => &super::m2x4::DEVICE,

        #[cfg(feature = "device_10x10hd")]
        M10x10Hd => &super::m10x10hd::DEVICE,

        #[cfg(feature = "device_flex")]
        Flex => &super::flex::DEVICE,

        #[cfg(feature = "device_flexhtx")]
        FlexHtx => &super::flexhtx::DEVICE,
    }
}

#[cfg(all(test, feature = "device_2x4hd48"))]
mod tests {
    use super::*;

    fn info(hw_id: u8, dsp_version: u8) -> DeviceInfo {
        DeviceInfo {
            hw_id,
            dsp_version,
            serial: 911111,
            fw_major: 1,
            fw_minor: 86,
        }
    }

    #[test]
    fn identifies_2x4hd_48khz_by_hardware_and_dsp_id() {
        let info = info(10, 99);
        assert_eq!(probe_kind(&info), DeviceKind::M2x4Hd48);
        let profile = probe(&info);
        assert_eq!(profile.product_name, "2x4HD (48kHz)");
        assert_eq!(profile.internal_sampling_rate, 48000);
        assert_eq!(
            profile.sources,
            &[
                crate::Source::Analog,
                crate::Source::Toslink,
                crate::Source::Usb
            ]
        );
        assert!(core::ptr::eq(profile, by_kind(DeviceKind::M2x4Hd48)));
    }

    #[test]
    fn does_not_assume_unknown_dsp_ids_are_the_48khz_profile() {
        for info in [info(10, 98), info(10, 199), info(27, 99)] {
            assert_eq!(probe_kind(&info), DeviceKind::Generic);
        }
        #[cfg(feature = "device_2x4hd")]
        assert_eq!(probe_kind(&info(10, 100)), DeviceKind::M2x4Hd);
        #[cfg(feature = "device_ddrc24")]
        assert_eq!(probe_kind(&info(10, 101)), DeviceKind::DDRC24);
    }
}
