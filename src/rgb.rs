use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<u32> for RGB {
    fn from(int: u32) -> Self {
        RGB {
            r: ((int >> 16) & 0xFF) as u8,
            g: ((int >> 8) & 0xFF) as u8,
            b: ((int >> 0) & 0xFF) as u8,
        }
    }
}

impl From<RGB> for u32 {
    fn from(rgb: RGB) -> Self {
        return 65536 * (rgb.r as u32) + 256 * (rgb.g as u32) + (rgb.b as u32);
    }
}

#[cfg(test)]
mod tests {
    use crate::rgb::RGB;

    #[test]
    fn rgb_u32_test() {
        let u32val: u32 = 0xFFFFFF;

        let rgb = RGB::from(u32val);

        assert_eq!(u32::from(rgb), u32val);
        assert_eq!(
            rgb,
            RGB {
                r: u8::MAX,
                g: u8::MAX,
                b: u8::MAX
            }
        );
    }
}

impl fmt::Display for RGB {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}, {}", self.r, self.g, self.b)
    }
}
