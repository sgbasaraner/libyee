use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RGB {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB {
    pub fn new(int: u32) -> RGB {
        RGB {
            r: ((int >> 16) & 0xFF) as u8,
            g: ((int >> 8) & 0xFF) as u8,
            b: ((int >> 0) & 0xFF) as u8,
        }
    }
}

impl fmt::Display for RGB {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}, {}, {}", self.r, self.g, self.b)
    }
}