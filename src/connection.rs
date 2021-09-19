use std::{
    io::{Error, Read, Write},
    net::TcpStream,
    sync::Mutex,
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
        method: &Method,
        args: Vec<MethodArg>,
    ) -> Result<MethodCallResponse, MethodCallError> {
        if !self.bulb.support.contains(method) {
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

    pub fn get_prop(&mut self, props: &[&str]) -> Result<StringVecResponse, MethodCallError> {
        if props.is_empty() {
            return Err(MethodCallError::BadRequest);
        }

        let args = props.iter().map(|p| MethodArg::String(*p)).collect();

        match self.call_method(&Method::GetProp, args) {
            Ok(rs) => {
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
            Err(e) => Err(e),
        }
    }
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
