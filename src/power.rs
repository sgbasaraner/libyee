use std::{convert::TryFrom, fmt};

#[derive(Debug, PartialEq, Eq)]
pub enum Power {
    On,
    Off,
}

impl TryFrom<&String> for Power {
    type Error = &'static str;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        let value = value.as_str();
        match value {
            "on" => Ok(Power::On),
            "off" => Ok(Power::Off),
            _ => Err("Doesn't match any power option."),
        }
    }
}

impl From<Power> for &str {
    fn from(pow: Power) -> Self {
        match pow {
            Power::On => "on",
            Power::Off => "off",
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
