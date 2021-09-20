use std::{
    convert::TryInto,
    io::{Error, Read, Write},
    net::TcpStream,
    sync::Mutex,
    time::Duration,
};

use crate::{
    bulb::{self, Bulb},
    method::Method,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct BulbConnection {
    bulb: Bulb,
    connection: Mutex<TcpStream>,
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
}

#[derive(Debug, Deserialize)]
pub struct MethodCallResponse {
    id: i16,
    result: Vec<Value>,
}

#[derive(Debug)]
pub struct StringVecResponse {
    id: i16,
    result: Vec<String>,
}

impl BulbConnection {
    pub fn new(bulb: Bulb) -> Result<BulbConnection, Error> {
        return TcpStream::connect(&bulb.ip_address).map(|connection| BulbConnection {
            bulb: bulb,
            connection: Mutex::new(connection),
        });
    }

    fn call_method(
        &mut self,
        method: Method,
        args: Vec<MethodArg>,
    ) -> Result<MethodCallResponse, MethodCallError> {
        if !self.bulb.support.contains(&method) {
            return Err(MethodCallError::UnsupportedMethod);
        }

        match self.connection.lock() {
            Ok(mut conn) => {
                let mut rng = rand::thread_rng();
                let id: i16 = rng.gen();
                let message = create_message(id, &method, args);

                let write_result = conn.write(message.as_bytes());
                if write_result.is_err() {
                    return Err(MethodCallError::IOError(write_result.unwrap_err()));
                }

                let mut buf = [0; 2048];
                match conn.read(&mut buf) {
                    Ok(_) => {
                        let rs = std::str::from_utf8(&buf)
                            .ok()
                            .map(|s| s.trim_end_matches(char::from(0)).trim_end())
                            .map(|s| serde_json::from_str::<MethodCallResponse>(s).ok())
                            .flatten()
                            .ok_or(MethodCallError::ParseError);

                        match rs {
                            Ok(rs) => {
                                if rs.id == id {
                                    Ok(rs)
                                } else {
                                    Err(MethodCallError::SynchronizationError)
                                }
                            }
                            Err(err) => Err(err),
                        }
                    }
                    Err(err) => Err(MethodCallError::IOError(err)),
                }
            }
            Err(_) => Err(MethodCallError::SynchronizationError),
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

        match self.call_method(Method::GetProp, args) {
            Ok(rs) => parse_string_vec(rs),
            Err(e) => Err(e),
        }
    }

    /// This method is used to change the color temperature of a smart LED.
    /// "ct_value" is the target color temperature. The type is integer and
    /// range is 1700 ~ 6500 (k).
    /// Smooth transition duration in milliseconds should be between 30 and i32::MAX.
    pub fn set_ct_abx(
        &mut self,
        ct_value: u16,
        mode: SetCtMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if ct_value > CT_MAX || ct_value < CT_MIN {
            return Err(MethodCallError::BadRequest);
        }

        let params = match mode {
            SetCtMode::Sudden => vec![
                MethodArg::Int(ct_value.into()),
                MethodArg::String("sudden"),
                MethodArg::Int(50),
            ],
            SetCtMode::Smooth(d) => {
                if d < MINIMUM_SET_CT_ABX_DURATION {
                    return Err(MethodCallError::BadRequest);
                }

                let millis: Option<i32> = d.as_millis().try_into().ok();
                if millis.is_none() {
                    return Err(MethodCallError::BadRequest);
                }

                vec![
                    MethodArg::Int(ct_value.into()),
                    MethodArg::String("smooth"),
                    MethodArg::Int(millis.unwrap()),
                ]
            }
        };

        match self.call_method(Method::SetCtAbx, params) {
            Ok(rs) => parse_string_vec(rs),
            Err(e) => Err(e),
        }
    }
}

fn parse_string_vec(rs: MethodCallResponse) -> Result<StringVecResponse, MethodCallError> {
    let strs: Vec<Option<&str>> = rs.result.iter().map(|v| v.as_str()).collect();
    if strs.iter().any(|s| s.is_none()) {
        Err(MethodCallError::ParseError)
    } else {
        Ok(StringVecResponse {
            id: rs.id,
            result: strs.iter().map(|s| s.unwrap().to_string()).collect(),
        })
    }
}

const MINIMUM_SET_CT_ABX_DURATION: Duration = Duration::from_millis(30);
const CT_MIN: u16 = 1700;
const CT_MAX: u16 = 6500;

pub enum SetCtMode {
    Sudden,
    Smooth(Duration),
}

fn create_message(id: i16, method: &Method, args: Vec<MethodArg>) -> String {
    let arg_strs: Vec<String> = args.iter().map(|a| a.to_str()).collect();
    let strs = [
        "{\"id\":",
        &id.to_string()[..],
        ",\"method\":\"",
        method.to_str(),
        "\",\"params\":[",
        &arg_strs.join(", "),
        "]}\r\n",
    ];
    strs.join("")
}
