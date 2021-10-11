use std::{
    convert::TryFrom,
    fmt::Debug,
    io::{Error, Read, Write},
    net::TcpStream,
    sync::Mutex,
    time::Duration,
};

use crate::{bulb::Bulb, lightmode::HSV, rgb::RGB};
use rand::{prelude::ThreadRng, RngCore};
use serde::Deserialize;

pub struct BulbConnection<T: Read + Write, R: RngCore> {
    pub bulb: Bulb,
    pub connection: Mutex<T>,
    pub rng: R,
}

#[derive(Debug)]
pub enum MethodCallError {
    BadRequest,
    UnsupportedMethod,
    IOError(std::io::Error),
    ParseError,
    SynchronizationError,
    ErrorResponse(ErrorResponse),
}

pub trait MethodCallResponse<'a>: Deserialize<'a> + Debug {
    fn id(&self) -> i16;
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    id: i16,
    error: BulbErrorResponse,
}

#[derive(Debug, Deserialize)]
pub struct BulbErrorResponse {
    code: i32,
    message: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StringVecResponse {
    pub id: i16,
    pub result: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CronResult {
    #[serde(rename(serialize = "type", deserialize = "type"))]
    pub cron_type: i32,
    pub delay: u16,
    pub mix: i32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CronResponse {
    pub id: i16,
    pub result: Vec<CronResult>,
}

impl TryFrom<CronResult> for Cron {
    type Error = String;

    fn try_from(value: CronResult) -> Result<Self, Self::Error> {
        if value.cron_type != 0 {
            return Err("Unsupported cron type.".to_string());
        }
        Ok(Cron {
            cron_type: CronType::PowerOff,
            minutes: value.delay,
        })
    }
}

impl<'a> MethodCallResponse<'a> for CronResponse {
    fn id(&self) -> i16 {
        self.id
    }
}

impl<'a> MethodCallResponse<'a> for StringVecResponse {
    fn id(&self) -> i16 {
        self.id
    }
}

pub type TcpConnection = BulbConnection<TcpStream, ThreadRng>;

impl TcpConnection {
    pub fn new(bulb: Bulb) -> Result<Self, Error> {
        return TcpStream::connect(&bulb.ip_address).map(|connection| BulbConnection {
            bulb: bulb,
            connection: Mutex::new(connection),
            rng: rand::thread_rng(),
        });
    }
}

pub enum MusicMode<'a> {
    On(&'a str, usize),
    Off,
}

pub enum AdjustableProp {
    Brightness,
    Ct,
    Color,
}

impl From<&AdjustableProp> for &str {
    fn from(a: &AdjustableProp) -> Self {
        match a {
            AdjustableProp::Brightness => "bright",
            AdjustableProp::Ct => "ct",
            AdjustableProp::Color => "color",
        }
    }
}

pub enum AdjustAction {
    Increase,
    Decrease,
    Circle,
}

impl From<&AdjustAction> for &str {
    fn from(a: &AdjustAction) -> Self {
        match a {
            AdjustAction::Increase => "increase",
            AdjustAction::Decrease => "decrease",
            AdjustAction::Circle => "circle",
        }
    }
}

pub enum CronType {
    PowerOff,
}

pub enum CfAction {
    Recover,
    Stay,
    TurnOff,
}

pub enum Scene<'a, 'b> {
    Color(&'a RGB, Brightness),
    HSV(&'a HSV, Brightness),
    Ct(Ct, Brightness),
    Cf(&'b ColorFlow),

    /// brightness, minutes
    AutoDelayOff(Brightness, u16),
}

pub const MIN_AUTO_DELAY_OFF_MINUTES: u8 = 1;

pub struct ColorFlow {
    pub count: u16,
    pub action: CfAction,
    pub sequence: Vec<FlowTuple>,
}

pub struct Cron {
    pub cron_type: CronType,
    pub minutes: u16,
}

pub struct FlowTuple {
    pub duration: Duration,
    pub mode: FlowTupleMode,
}

pub struct ColorFlowTupleMode {
    pub color: RGB,
    pub brightness: Brightness,
}

pub type Ct = u16;
pub type Brightness = u8;

pub struct CtFlowTupleMode {
    pub ct: Ct,
    pub brightness: Brightness,
}

pub enum FlowTupleMode {
    Color(ColorFlowTupleMode),
    Ct(CtFlowTupleMode),
    Sleep,
}

pub const MINIMUM_CF_DURATION: Duration = Duration::from_millis(50);

pub enum PowerMode {
    Ct = 1,
    Rgb = 2,
    Hsv = 3,
    ColorFlow = 4,
    NightLight = 5,
}

pub const MAX_BRIGHTNESS: u8 = 100;
pub const MINIMUM_TRANSITION_DURATION: Duration = Duration::from_millis(30);
pub const CT_MIN: u16 = 1700;
pub const CT_MAX: u16 = 6500;

pub enum TransitionMode {
    Sudden,
    Smooth(Duration),
}
