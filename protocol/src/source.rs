//! Utilities to get a mapping from the source name to the source id
//! Most of this logic was translated from the cordova app

use super::DeviceInfo;

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
pub enum Source {
    NotInstalled,
    Analog,
    Toslink,
    Spdif,
    Usb,
    Aesebu,
    Rca,
    Xlr,
    Lan,
    I2S,
    Bluetooth,
    Hdmi,
}

impl Source {
    /// Gets the available input sources with their numerical mapping for a given device
    pub fn mapping(device_info: &DeviceInfo) -> &'static [(Source, u8)] {
        use Source::*;
        match device_info.hw_id {
            2 | 11 => &[(Toslink, 0), (Spdif, 1)],
            1 | 4 | 5 => &[(Spdif, 0), (Toslink, 1), (Aesebu, 2)],
            10 if matches!(device_info.dsp_version, 99..=101) => {
                &[(Analog, 0), (Toslink, 1), (Usb, 2)]
            }
            10 => &[(I2S, 0), (Toslink, 1), (Usb, 2)],
            14 => &[
                (Toslink, 0),
                (Spdif, 1),
                (Aesebu, 2),
                (Rca, 3),
                (Xlr, 4),
                (Usb, 5),
                (Lan, 6),
            ],
            17 | 18 => &[(Toslink, 0), (Spdif, 1), (Aesebu, 2), (Usb, 3), (Lan, 4)],
            27 => &[
                (Analog, 0),
                (Toslink, 1),
                (Spdif, 2),
                (Usb, 3),
                (Bluetooth, 4),
            ],
            32 => &[(Analog, 0), (Toslink, 1), (Spdif, 2), (Usb, 3), (Hdmi, 4)],
            _ => &[(NotInstalled, 0)],
        }
    }

    pub fn from_id(id: u8, device_info: &DeviceInfo) -> Self {
        for (src, src_id) in Self::mapping(device_info) {
            if *src_id == id {
                return *src;
            }
        }
        Source::NotInstalled
    }

    pub fn to_id(self, device_info: &DeviceInfo) -> u8 {
        for &(src, src_id) in Self::mapping(device_info) {
            if src == self {
                return src_id;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_analog_instead_of_i2s_for_2x4hd_48khz() {
        for dsp_version in [99, 100, 101] {
            let info = DeviceInfo {
                hw_id: 10,
                dsp_version,
                serial: 911111,
                fw_major: 1,
                fw_minor: 86,
            };
            for (source, id) in [(Source::Analog, 0), (Source::Toslink, 1), (Source::Usb, 2)] {
                assert_eq!(Source::from_id(id, &info), source);
                assert_eq!(source.to_id(&info), id);
            }
        }
        let unknown = DeviceInfo {
            hw_id: 10,
            dsp_version: 98,
            serial: 911111,
            fw_major: 1,
            fw_minor: 86,
        };
        assert_eq!(Source::from_id(0, &unknown), Source::I2S);
    }
}
