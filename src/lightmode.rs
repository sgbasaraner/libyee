use crate::rgb::RGB;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct HSV {
    // Current hue value. The range of this value is 0 to 359.
    pub hue: u16,

    // Current saturation value. The range of this value is 0 to 100.
    pub saturation: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LightMode {
    Color(RGB),
    // Current color temperature value.
    ColorTemperature(u32),
    Hsv(HSV),
}

impl fmt::Display for LightMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl LightMode {
    pub fn parse(response_map: &HashMap<String, String>) -> Option<LightMode> {
        response_map
            .get("color_mode")
            .map(|cm| cm.parse::<u8>().ok())
            .flatten()
            .map(|cm| match cm {
                1 => response_map
                    .get("rgb")
                    .map(|rgb| rgb.parse::<u32>().ok())
                    .flatten()
                    .map(|rgb| RGB::new(rgb))
                    .map(|rgb| LightMode::Color(rgb)),
                2 => response_map
                    .get("ct")
                    .map(|ct| ct.parse::<u32>().ok())
                    .flatten()
                    .map(|ct| LightMode::ColorTemperature(ct)),
                3 => response_map
                    .get("hue")
                    .map(|hue| {
                        response_map
                            .get("sat")
                            .map(|sat| {
                                hue.parse::<u16>().ok().map(|hue| {
                                    sat.parse::<u8>().ok().map(|sat| {
                                        LightMode::Hsv(HSV {
                                            hue: hue,
                                            saturation: sat,
                                        })
                                    })
                                })
                            })
                            .flatten()
                            .flatten()
                    })
                    .flatten(),
                _ => None,
            })
            .flatten()
    }
}
