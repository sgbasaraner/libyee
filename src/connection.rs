use std::{
    convert::TryInto,
    fmt::Debug,
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

#[derive(Debug, Deserialize)]
pub struct StringVecResponse {
    id: i16,
    result: Vec<String>,
}

impl<'a> MethodCallResponse<'a> for StringVecResponse {
    fn id(&self) -> i16 {
        self.id
    }
}

impl BulbConnection {
    pub fn new(bulb: Bulb) -> Result<BulbConnection, Error> {
        return TcpStream::connect(&bulb.ip_address).map(|connection| BulbConnection {
            bulb: bulb,
            connection: Mutex::new(connection),
        });
    }

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

        let mut rng = rand::thread_rng();
        let id: i16 = rng.gen();
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

        self.call_method(Method::SetCtAbx, args)
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

        self.call_method(Method::SetCtAbx, params)
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
