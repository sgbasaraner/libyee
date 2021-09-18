use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum Power {
    On,
    Off,
}

impl Power {
    pub fn parse(str: &str) -> Option<Power> {
        match str {
            "on" => Some(Power::On),
            "off" => Some(Power::Off),
            _ => None,
        }
    }
}

impl fmt::Display for Power {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Power::On => write!(f, "on"),
            Power::Off => write!(f, "off"),
        }
    }
}
