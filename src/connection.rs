use std::{
    convert::TryInto,
    fmt::Debug,
    io::{self, Error, Read, Write},
    net::TcpStream,
    sync::Mutex,
    time::Duration,
};

use crate::{bulb::Bulb, lightmode::HSV, method::Method, power::Power, rgb::RGB};
use rand::{prelude::ThreadRng, Rng, RngCore};
use serde::Deserialize;

pub struct BulbConnection<T: Read + Write, R: RngCore> {
    bulb: Bulb,
    connection: Mutex<T>,
    rng: R,
}

enum MethodArg<'a> {
    String(&'a str),
    Int(i32),
}

impl MethodArg<'_> {
    fn to_str(&self) -> String {
        match self {
            MethodArg::String(str) => {
                let mut string = String::new();
                string.push_str("\"");
                string.push_str(str);
                string.push_str("\"");
                string
            }
            MethodArg::Int(int) => int.to_string(),
        }
    }
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
    id: i16,
    result: Vec<String>,
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

impl<C: Read + Write, R: RngCore> BulbConnection<C, R> {
    fn call_method<T>(&mut self, method: Method, args: Vec<MethodArg>) -> Result<T, MethodCallError>
    where
        for<'a> T: MethodCallResponse<'a>,
    {
        if !self.bulb.support.contains(&method) {
            return Err(MethodCallError::UnsupportedMethod);
        }

        let mut conn = self
            .connection
            .lock()
            .map_err(|_| MethodCallError::SynchronizationError)?;

        let id: i16 = self.rng.gen();
        let message = create_message(id, &method, args);

        conn.write(message.as_bytes())
            .map_err(|err| MethodCallError::IOError(err))?;

        let mut buf = [0; 2048];
        conn.read(&mut buf)
            .map_err(|err| MethodCallError::IOError(err))?;

        let rs = std::str::from_utf8(&buf)
            .map_err(|_| MethodCallError::ParseError)
            .map(|s| s.trim_end_matches(char::from(0)).trim_end())
            .map(|s| {
                serde_json::from_str::<T>(s).map_err(|_| {
                    let error = serde_json::from_str::<ErrorResponse>(s);
                    match error {
                        Ok(ers) => MethodCallError::ErrorResponse(ers),
                        Err(_) => MethodCallError::ParseError,
                    }
                })
            })??;

        if rs.id() == id {
            Ok(rs)
        } else {
            Err(MethodCallError::SynchronizationError)
        }
    }

    /// This method is used to retrieve current property of smart LED.
    /// The parameter is a list of property names and the response contains a
    /// list of corresponding property values. If the requested property name is not recognized by
    /// smart LED, then a empty string value ("") will be returned.
    pub fn get_prop(&mut self, props: &[&str]) -> Result<StringVecResponse, MethodCallError> {
        if props.is_empty() {
            return Err(MethodCallError::BadRequest);
        }

        let args = props.iter().map(|p| MethodArg::String(*p)).collect();

        self.call_method(Method::GetProp, args)
    }

    /// This method is used to change the color temperature of a smart LED.
    /// "ct_value" is the target color temperature. The type is integer and
    /// range is 1700 ~ 6500 (k).
    /// Smooth transition duration in milliseconds should be between 30 and i32::MAX.
    pub fn set_ct_abx(
        &mut self,
        ct_value: u16,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if ct_value > CT_MAX || ct_value < CT_MIN {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;

        self.call_method(
            Method::SetCtAbx,
            vec![MethodArg::Int(ct_value.into())]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn set_rgb(
        &mut self,
        rgb: &RGB,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        let args = mode.to_method_args()?;

        self.call_method(
            Method::SetRgb,
            vec![MethodArg::Int(u32::from(rgb) as i32)]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn set_hsv(
        &mut self,
        hsv: &HSV,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if !hsv.validate() {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;

        self.call_method(
            Method::SetHsv,
            vec![
                MethodArg::Int(hsv.hue as i32),
                MethodArg::Int(hsv.saturation as i32),
            ]
            .into_iter()
            .chain(args.into_iter())
            .collect(),
        )
    }

    pub fn set_bright(
        &mut self,
        brightness: u8,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if brightness > MAX_BRIGHTNESS {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;
        self.call_method(
            Method::SetBright,
            vec![MethodArg::Int(brightness as i32)]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn set_power(
        &mut self,
        power: Power,
        trans_mode: TransitionMode,
        power_mode: Option<PowerMode>,
    ) -> Result<StringVecResponse, MethodCallError> {
        let args = trans_mode.to_method_args()?;

        let mut args: Vec<MethodArg> = vec![MethodArg::String(power.into())]
            .into_iter()
            .chain(args.into_iter())
            .collect();

        if let Some(pm) = power_mode {
            args.push(MethodArg::Int(pm as i32));
        }

        self.call_method(Method::SetPower, args)
    }

    pub fn toggle(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::Toggle, vec![])
    }

    pub fn set_default(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::SetDefault, vec![])
    }

    pub fn start_cf(
        &mut self,
        count: u16,
        action: CfAction,
        flow: Vec<FlowTuple>,
    ) -> Result<StringVecResponse, MethodCallError> {
        let mut flow_vec: Vec<String> = Vec::with_capacity(4 * flow.len());

        for tuple in flow {
            let expr = tuple.to_expression()?;
            for ex in expr {
                flow_vec.push(ex.to_string());
            }
        }

        self.call_method(
            Method::StartCf,
            vec![
                MethodArg::Int(count as i32),
                MethodArg::Int(action as i32),
                MethodArg::String(&flow_vec.join(",")),
            ],
        )
    }

    pub fn stop_cf(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::StopCf, vec![])
    }
}

pub enum CfAction {
    Recover = 0,
    Stay = 1,
    TurnOff = 2,
}

pub struct FlowTuple {
    duration: Duration,
    mode: FlowTupleMode,
}

pub struct ColorFlowTupleMode {
    color: RGB,
    brightness: u8,
}

pub struct CtFlowTupleMode {
    ct: u16,
    brightness: u8,
}

pub enum FlowTupleMode {
    Color(ColorFlowTupleMode),
    Ct(CtFlowTupleMode),
    Sleep,
}

const MINIMUM_CF_DURATION: Duration = Duration::from_millis(50);

impl FlowTuple {
    fn to_expression(&self) -> Result<Vec<u32>, MethodCallError> {
        if self.duration < MINIMUM_CF_DURATION {
            return Err(MethodCallError::BadRequest);
        }

        let (second_arg, third_arg, fourth_arg) = match &self.mode {
            FlowTupleMode::Color(c) => {
                if c.brightness > MAX_BRIGHTNESS {
                    return Err(MethodCallError::BadRequest);
                }
                (1, u32::from(&c.color), c.brightness as u32)
            }
            FlowTupleMode::Ct(ct) => {
                if ct.brightness > MAX_BRIGHTNESS {
                    return Err(MethodCallError::BadRequest);
                }
                (2, ct.ct as u32, ct.brightness as u32)
            }
            FlowTupleMode::Sleep => (7, u32::MIN, u32::MIN),
        };

        return Ok(vec![
            self.duration.as_millis() as u32,
            second_arg,
            third_arg,
            fourth_arg,
        ]);
    }
}

pub enum PowerMode {
    Ct = 1,
    Rgb = 2,
    Hsv = 3,
    ColorFlow = 4,
    NightLight = 5,
}

const MAX_BRIGHTNESS: u8 = 100;
const MINIMUM_TRANSITION_DURATION: Duration = Duration::from_millis(30);
const CT_MIN: u16 = 1700;
const CT_MAX: u16 = 6500;

pub enum TransitionMode {
    Sudden,
    Smooth(Duration),
}

impl TransitionMode {
    fn to_method_args(&self) -> Result<Vec<MethodArg>, MethodCallError> {
        match self {
            TransitionMode::Sudden => Ok(vec![MethodArg::String("sudden"), MethodArg::Int(50)]),
            TransitionMode::Smooth(d) => {
                if d < &MINIMUM_TRANSITION_DURATION {
                    return Err(MethodCallError::BadRequest);
                }

                let millis: Option<i32> = d.as_millis().try_into().ok();
                if millis.is_none() {
                    return Err(MethodCallError::BadRequest);
                }

                Ok(vec![
                    MethodArg::String("smooth"),
                    MethodArg::Int(millis.unwrap()),
                ])
            }
        }
    }
}

fn create_message(id: i16, method: &Method, args: Vec<MethodArg>) -> String {
    let arg_strs: Vec<String> = args.iter().map(|a| a.to_str()).collect();
    let strs = [
        "{\"id\":",
        &id.to_string()[..],
        ",\"method\":\"",
        method.into(),
        "\",\"params\":[",
        &arg_strs.join(", "),
        "]}\r\n",
    ];
    strs.join("")
}

struct MockTcpConnection {
    when_written: String,
    return_val: String,
    written_val: Option<String>,
}

impl Read for MockTcpConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self
            .written_val
            .clone()
            .unwrap()
            .trim()
            .eq(self.when_written.trim())
        {
            let bytes = self.return_val.as_bytes();

            for (i, elem) in buf.iter_mut().enumerate() {
                if i >= bytes.len() {
                    break;
                }
                *elem = bytes[i];
            }

            return io::Result::Ok(usize::min(bytes.len(), buf.len()));
        }
        return io::Result::Ok(0);
    }
}

impl Write for MockTcpConnection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut str = String::new();
        let _ = buf.clone().read_to_string(&mut str);
        self.written_val = Some(str);
        println!("mock written: {}", self.written_val.as_ref().unwrap());
        println!("when written: {}", self.when_written);
        io::Result::Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::Result::Ok(())
    }
}

macro_rules! set {
    ( $( $x:expr ),* ) => {  // Match zero or more comma delimited items
        {
            let mut temp_set = std::collections::HashSet::new();  // Create a mutable HashSet
            $(
                temp_set.insert($x); // Insert each item matched into the HashSet
            )*
            temp_set // Return the populated HashSet
        }
    };
}

const TEST_OK_VAL: &str = "{\"id\":1, \"result\":[\"ok\"]}";

mod tests {
    use std::{sync::Mutex, time::Duration};

    use rand::rngs::mock::{self, StepRng};

    use crate::{
        bulb::Bulb,
        connection::{
            BulbConnection, ColorFlowTupleMode, CtFlowTupleMode, FlowTuple, FlowTupleMode,
            MockTcpConnection,
        },
        lightmode::{LightMode, HSV},
        method::Method,
        rgb::RGB,
    };

    use super::{MethodCallError, StringVecResponse, TransitionMode, TEST_OK_VAL};

    fn one_rng() -> StepRng {
        mock::StepRng::new(1, 0)
    }

    fn make_bulb_with_method(method: Method) -> Bulb {
        Bulb {
            id: "".to_string(),
            model: "".to_string(),
            fw_ver: "".to_string(),
            support: set![method],
            power: crate::power::Power::Off,
            bright: 0,
            color_mode: LightMode::ColorTemperature(8),
            name: "".to_string(),
            ip_address: "".to_string(),
        }
    }

    fn assert_ok_result(result: Result<StringVecResponse, MethodCallError>) {
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().result.first().unwrap().clone(),
            "ok".to_string()
        );
    }

    fn conn_with_method(
        method: Method,
        mock: MockTcpConnection,
    ) -> BulbConnection<MockTcpConnection, StepRng> {
        let mock_bulb = make_bulb_with_method(method);

        return BulbConnection {
            bulb: mock_bulb,
            connection: Mutex::new(mock),
            rng: one_rng(),
        };
    }

    #[test]
    fn get_prop_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"get_prop\",\"params\":[\"power\", \"not_exist\", \"bright\"]}".to_string(),
            return_val: "{\"id\":1, \"result\":[\"on\", \"\", \"100\"]}".to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::GetProp, mock);

        let result = conn.get_prop(&["power", "not_exist", "bright"]);
        assert!(result.is_ok());
        if let Ok(res) = result {
            assert_eq!(res.clone().result.first().unwrap(), "on");
            assert_eq!(res.clone().result.get(1).unwrap(), "");
            assert_eq!(res.clone().result.get(2).unwrap(), "100");
        }
    }

    #[test]
    fn set_ct_abx_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_ct_abx\",\"params\":[3500, \"smooth\", 500]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetCtAbx, mock);

        let result = conn.set_ct_abx(3500, TransitionMode::Smooth(Duration::from_millis(500)));
        assert_ok_result(result);
    }

    #[test]
    fn set_rgb_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_rgb\",\"params\":[255, \"smooth\", 500]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetRgb, mock);

        let result = conn.set_rgb(
            &RGB { r: 0, g: 0, b: 255 },
            TransitionMode::Smooth(Duration::from_millis(500)),
        );
        assert_ok_result(result);
    }

    #[test]
    fn set_hsv_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_hsv\",\"params\":[255, 45, \"smooth\", 500]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetHsv, mock);

        let result = conn.set_hsv(
            &HSV {
                hue: 255,
                saturation: 45,
            },
            TransitionMode::Smooth(Duration::from_millis(500)),
        );
        assert_ok_result(result);
    }

    #[test]
    fn set_bright_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_bright\",\"params\":[50, \"smooth\", 500]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetBright, mock);

        let result = conn.set_bright(50, TransitionMode::Smooth(Duration::from_millis(500)));
        assert_ok_result(result);
    }

    #[test]
    fn set_power_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"set_power\",\"params\":[\"on\", \"smooth\", 500]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetPower, mock);

        let result = conn.set_power(
            crate::power::Power::On,
            TransitionMode::Smooth(Duration::from_millis(500)),
            None,
        );
        assert_ok_result(result);
    }

    #[test]
    fn set_default_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_default\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetDefault, mock);

        assert_ok_result(conn.set_default());
    }

    #[test]
    fn start_cf_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"start_cf\",\"params\":[4, 2, \"1000,2,2700,100,500,1,255,10,5000,7,0,0,500,2,5000,1\"]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::StartCf, mock);

        let ctf_mode_1 = CtFlowTupleMode {
            ct: 2700,
            brightness: 100,
        };
        let cf_mode = ColorFlowTupleMode {
            color: RGB { r: 0, g: 0, b: 255 },
            brightness: 10,
        };
        let ctf_mode_2 = CtFlowTupleMode {
            ct: 5000,
            brightness: 1,
        };
        assert_ok_result(conn.start_cf(
            4,
            super::CfAction::TurnOff,
            vec![
                FlowTuple {
                    duration: Duration::from_millis(1000),
                    mode: FlowTupleMode::Ct(ctf_mode_1),
                },
                FlowTuple {
                    duration: Duration::from_millis(500),
                    mode: FlowTupleMode::Color(cf_mode),
                },
                FlowTuple {
                    duration: Duration::from_millis(5000),
                    mode: FlowTupleMode::Sleep,
                },
                FlowTuple {
                    duration: Duration::from_millis(500),
                    mode: FlowTupleMode::Ct(ctf_mode_2),
                },
            ],
        ));
    }

    #[test]
    fn stop_cf() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"stop_cf\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::StopCf, mock);

        println!("{:?}", conn.stop_cf());
    }
}
