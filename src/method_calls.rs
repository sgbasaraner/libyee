use std::{
    convert::TryInto,
    io::{self, Read, Write},
};

use rand::{Rng, RngCore};

use crate::{
    connection::{
        AdjustAction, AdjustableProp, BulbConnection, CfAction, ColorFlow, Cron, CronResponse,
        CronType, ErrorResponse, FlowTuple, FlowTupleMode, MethodCallError, MethodCallResponse,
        MusicMode, PowerMode, Scene, StringVecResponse, TransitionMode, CT_MAX, CT_MIN,
        MAX_BRIGHTNESS, MINIMUM_CF_DURATION, MINIMUM_TRANSITION_DURATION,
        MIN_AUTO_DELAY_OFF_MINUTES,
    },
    lightmode::HSV,
    method::Method,
    power::Power,
    rgb::RGB,
};

enum MethodArg {
    String(String),
    Int(i32),
}

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

impl CfAction {
    const fn val(&self) -> i32 {
        match self {
            CfAction::Recover => 0,
            CfAction::Stay => 1,
            CfAction::TurnOff => 2,
        }
    }
}

impl ColorFlow {
    fn params(&self) -> Result<Vec<MethodArg>, MethodCallError> {
        let mut flow_vec: Vec<String> = Vec::with_capacity(4 * self.sequence.len());

        for tuple in &self.sequence {
            let expr = tuple.to_expression()?;
            for ex in expr {
                flow_vec.push(ex.to_string());
            }
        }

        Ok(vec![
            MethodArg::Int(self.count as i32),
            MethodArg::Int(self.action.val()),
            MethodArg::String(flow_vec.join(",")),
        ])
    }
}

impl<'a, 'b> Scene<'a, 'b> {
    const fn val(&self) -> &str {
        match self {
            Scene::Color(_, _) => "color",
            Scene::HSV(_, _) => "hsv",
            Scene::Ct(_, _) => "ct",
            Scene::Cf(_) => "cf",
            Scene::AutoDelayOff(_, _) => "auto_delay_off",
        }
    }

    fn params(&self) -> Result<Vec<MethodArg>, MethodCallError> {
        match self {
            Scene::Color(rgb, brightness) => Ok(vec![
                MethodArg::String(self.val().to_string()),
                MethodArg::Int(u32::from(*rgb) as i32),
                MethodArg::Int(*brightness as i32),
            ]),
            Scene::HSV(hsv, brightness) => Ok(vec![
                MethodArg::String(self.val().to_string()),
                MethodArg::Int(hsv.hue as i32),
                MethodArg::Int(hsv.saturation as i32),
                MethodArg::Int(*brightness as i32),
            ]),
            Scene::Ct(ct, brightness) => Ok(vec![
                MethodArg::String(self.val().to_string()),
                MethodArg::Int(*ct as i32),
                MethodArg::Int(*brightness as i32),
            ]),
            Scene::Cf(cf) => cf.params().map(|p| {
                let mut args = vec![MethodArg::String(self.val().to_string())];

                for param in p {
                    args.push(param);
                }

                args
            }),
            Scene::AutoDelayOff(brightness, duration_min) => {
                if *duration_min < MIN_AUTO_DELAY_OFF_MINUTES as u16 {
                    return Err(MethodCallError::BadRequest);
                }

                if *brightness > MAX_BRIGHTNESS {
                    return Err(MethodCallError::BadRequest);
                }

                Ok(vec![
                    MethodArg::String(self.val().to_string()),
                    MethodArg::Int(*brightness as i32),
                    MethodArg::Int(*duration_min as i32),
                ])
            }
        }
    }
}

impl TransitionMode {
    fn to_method_args(&self) -> Result<Vec<MethodArg>, MethodCallError> {
        match self {
            TransitionMode::Sudden => Ok(vec![
                MethodArg::String("sudden".to_string()),
                MethodArg::Int(50),
            ]),
            TransitionMode::Smooth(d) => {
                if d < &MINIMUM_TRANSITION_DURATION {
                    return Err(MethodCallError::BadRequest);
                }

                let millis: Option<i32> = d.as_millis().try_into().ok();
                if millis.is_none() {
                    return Err(MethodCallError::BadRequest);
                }

                Ok(vec![
                    MethodArg::String("smooth".to_string()),
                    MethodArg::Int(millis.unwrap()),
                ])
            }
        }
    }
}

impl MethodArg {
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

        let args = props
            .iter()
            .map(|p| MethodArg::String(p.to_string()))
            .collect();

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

    pub fn start_cf(&mut self, cf: &ColorFlow) -> Result<StringVecResponse, MethodCallError> {
        cf.params()
            .and_then(|p| self.call_method(Method::StartCf, p))
    }

    pub fn stop_cf(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::StopCf, vec![])
    }

    pub fn set_scene(&mut self, scene: &Scene) -> Result<StringVecResponse, MethodCallError> {
        scene
            .params()
            .and_then(|p| self.call_method(Method::SetScene, p))
    }

    /// Usage: This method is used to start a timer job on the smart LED.
    pub fn cron_add(&mut self, cron: &Cron) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(
            Method::CronAdd,
            vec![MethodArg::Int(0), MethodArg::Int(cron.minutes as i32)],
        )
    }

    pub fn cron_get(&mut self, cron_type: &CronType) -> Result<CronResponse, MethodCallError> {
        self.call_method(Method::CronGet, vec![MethodArg::Int(0)])
    }

    pub fn cron_del(&mut self, cron_type: &CronType) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::CronDel, vec![MethodArg::Int(0)])
    }

    pub fn set_adjust(
        &mut self,
        prop: &AdjustableProp,
        action: &AdjustAction,
    ) -> Result<StringVecResponse, MethodCallError> {
        let action_str: &str = action.into();
        let prop_str: &str = prop.into();
        self.call_method(
            Method::SetAdjust,
            vec![
                MethodArg::String(action_str.to_string()),
                MethodArg::String(prop_str.to_string()),
            ],
        )
    }

    pub fn set_music(&mut self, mode: MusicMode) -> Result<StringVecResponse, MethodCallError> {
        let method = Method::SetMusic;
        match mode {
            MusicMode::On(ip_address, port) => self.call_method(
                method,
                vec![
                    MethodArg::Int(1),
                    MethodArg::String(ip_address.to_string()),
                    MethodArg::Int(port as i32),
                ],
            ),
            MusicMode::Off => self.call_method(method, vec![MethodArg::Int(0)]),
        }
    }

    pub fn set_name(&mut self, name: &str) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::SetName, vec![MethodArg::String(name.to_string())])
    }

    pub fn bg_set_ct_abx(
        &mut self,
        ct_value: u16,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if ct_value > CT_MAX || ct_value < CT_MIN {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;

        self.call_method(
            Method::BgSetCtAbx,
            vec![MethodArg::Int(ct_value.into())]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn bg_set_rgb(
        &mut self,
        rgb: &RGB,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        let args = mode.to_method_args()?;

        self.call_method(
            Method::BgSetRgb,
            vec![MethodArg::Int(u32::from(rgb) as i32)]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn bg_set_hsv(
        &mut self,
        hsv: &HSV,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if !hsv.validate() {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;

        self.call_method(
            Method::BgSetHsv,
            vec![
                MethodArg::Int(hsv.hue as i32),
                MethodArg::Int(hsv.saturation as i32),
            ]
            .into_iter()
            .chain(args.into_iter())
            .collect(),
        )
    }

    pub fn bg_set_bright(
        &mut self,
        brightness: u8,
        mode: TransitionMode,
    ) -> Result<StringVecResponse, MethodCallError> {
        if brightness > MAX_BRIGHTNESS {
            return Err(MethodCallError::BadRequest);
        }

        let args = mode.to_method_args()?;
        self.call_method(
            Method::BgSetBright,
            vec![MethodArg::Int(brightness as i32)]
                .into_iter()
                .chain(args.into_iter())
                .collect(),
        )
    }

    pub fn bg_set_power(
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

        self.call_method(Method::BgSetPower, args)
    }

    pub fn bg_toggle(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::BgToggle, vec![])
    }

    pub fn bg_set_default(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::BgSetDefault, vec![])
    }

    pub fn bg_start_cf(&mut self, cf: &ColorFlow) -> Result<StringVecResponse, MethodCallError> {
        cf.params()
            .and_then(|p| self.call_method(Method::BgStartCf, p))
    }

    pub fn bg_stop_cf(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::BgStopCf, vec![])
    }

    pub fn bg_set_scene(&mut self, scene: &Scene) -> Result<StringVecResponse, MethodCallError> {
        scene
            .params()
            .and_then(|p| self.call_method(Method::BgSetScene, p))
    }

    pub fn bg_set_adjust(
        &mut self,
        prop: &AdjustableProp,
        action: &AdjustAction,
    ) -> Result<StringVecResponse, MethodCallError> {
        let action_str: &str = action.into();
        let prop_str: &str = prop.into();
        self.call_method(
            Method::BgSetAdjust,
            vec![
                MethodArg::String(action_str.to_string()),
                MethodArg::String(prop_str.to_string()),
            ],
        )
    }

    pub fn dev_toggle(&mut self) -> Result<StringVecResponse, MethodCallError> {
        self.call_method(Method::DevToggle, vec![])
    }
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
    use std::{result, sync::Mutex, time::Duration};

    use rand::rngs::mock::{self, StepRng};

    use crate::{
        bulb::Bulb,
        connection::{
            BulbConnection, ColorFlow, ColorFlowTupleMode, CtFlowTupleMode, FlowTuple,
            FlowTupleMode,
        },
        lightmode::{LightMode, HSV},
        method::Method,
        rgb::RGB,
    };

    use super::{
        Cron, CronType, MethodCallError, MockTcpConnection, MusicMode, Scene, StringVecResponse,
        TransitionMode, TEST_OK_VAL,
    };

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
        assert_ok_result(conn.start_cf(&ColorFlow {
            count: 4,
            action: super::CfAction::TurnOff,
            sequence: vec![
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
        }));
    }

    #[test]
    fn stop_cf_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"stop_cf\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::StopCf, mock);

        assert_ok_result(conn.stop_cf());
    }

    #[test]
    fn set_scene_color_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_scene\",\"params\":[\"color\", 65280, 70]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetScene, mock);

        assert_ok_result(conn.set_scene(&Scene::Color(&RGB { r: 0, g: 255, b: 0 }, 70)));
    }

    #[test]
    fn set_scene_hsv_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_scene\",\"params\":[\"hsv\", 300, 70, 100]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetScene, mock);

        assert_ok_result(conn.set_scene(&Scene::HSV(
            &HSV {
                hue: 300,
                saturation: 70,
            },
            100,
        )));
    }

    #[test]
    fn set_scene_ct_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_scene\",\"params\":[\"ct\", 5400, 100]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetScene, mock);

        assert_ok_result(conn.set_scene(&Scene::Ct(5400, 100)));
    }

    #[test]
    fn set_scene_cf_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_scene\",\"params\":[\"cf\", 0, 0, \"1000,2,2700,100,500,1,255,10,5000,7,0,0,500,2,5000,1\"]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

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

        let cf = ColorFlow {
            count: 0,
            action: super::CfAction::Recover,
            sequence: vec![
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
        };

        let mut conn = conn_with_method(Method::SetScene, mock);

        assert_ok_result(conn.set_scene(&Scene::Cf(&cf)));
    }

    #[test]
    fn set_scene_auto_delay_off_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"set_scene\",\"params\":[\"auto_delay_off\", 50, 5]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetScene, mock);

        assert_ok_result(conn.set_scene(&Scene::AutoDelayOff(50, 5)));
    }

    #[test]
    fn cron_add_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"cron_add\",\"params\":[0, 14]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::CronAdd, mock);

        assert_ok_result(conn.cron_add(&Cron {
            cron_type: CronType::PowerOff,
            minutes: 14,
        }));
    }

    #[test]
    fn cron_get_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"cron_get\",\"params\":[0]}".to_string(),
            return_val: "{\"id\":1,\"result\":[{\"type\": 0, \"delay\": 15, \"mix\": 0}]}"
                .to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::CronGet, mock);

        let result = conn.cron_get(&CronType::PowerOff);

        assert!(result.is_ok());

        let result = result.unwrap();

        assert_eq!(result.result.first().unwrap().cron_type, 0);
        assert_eq!(result.result.first().unwrap().mix, 0);
        assert_eq!(result.result.first().unwrap().delay, 15);
    }

    #[test]
    fn cron_del_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"cron_del\",\"params\":[0]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::CronDel, mock);

        assert_ok_result(conn.cron_del(&CronType::PowerOff));
    }

    #[test]
    fn set_adjust_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_adjust\",\"params\":[\"increase\", \"ct\"]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetAdjust, mock);

        assert_ok_result(
            conn.set_adjust(&super::AdjustableProp::Ct, &super::AdjustAction::Increase),
        );
    }

    #[test]
    fn set_music_on_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"set_music\",\"params\":[1, \"192.168.0.2\", 54321]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetMusic, mock);

        assert_ok_result(conn.set_music(MusicMode::On("192.168.0.2", 54321)));
    }

    #[test]
    fn set_music_off_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_music\",\"params\":[0]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetMusic, mock);

        assert_ok_result(conn.set_music(MusicMode::Off));
    }

    #[test]
    fn set_name_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"set_name\",\"params\":[\"my_bulb\"]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::SetName, mock);

        assert_ok_result(conn.set_name("my_bulb"));
    }

    #[test]
    fn bg_set_adjust_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_adjust\",\"params\":[\"increase\", \"ct\"]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetAdjust, mock);

        assert_ok_result(
            conn.bg_set_adjust(&super::AdjustableProp::Ct, &super::AdjustAction::Increase),
        );
    }

    #[test]
    fn bg_set_ct_abx_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_ct_abx\",\"params\":[3500, \"smooth\", 500]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetCtAbx, mock);

        let result = conn.bg_set_ct_abx(3500, TransitionMode::Smooth(Duration::from_millis(500)));
        assert_ok_result(result);
    }

    #[test]
    fn bg_set_rgb_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_set_rgb\",\"params\":[255, \"smooth\", 500]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetRgb, mock);

        let result = conn.bg_set_rgb(
            &RGB { r: 0, g: 0, b: 255 },
            TransitionMode::Smooth(Duration::from_millis(500)),
        );
        assert_ok_result(result);
    }

    #[test]
    fn bg_set_hsv_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_hsv\",\"params\":[255, 45, \"smooth\", 500]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetHsv, mock);

        let result = conn.bg_set_hsv(
            &HSV {
                hue: 255,
                saturation: 45,
            },
            TransitionMode::Smooth(Duration::from_millis(500)),
        );
        assert_ok_result(result);
    }

    #[test]
    fn bg_set_bright_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_bright\",\"params\":[50, \"smooth\", 500]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetBright, mock);

        let result = conn.bg_set_bright(50, TransitionMode::Smooth(Duration::from_millis(500)));
        assert_ok_result(result);
    }

    #[test]
    fn bg_set_power_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_power\",\"params\":[\"on\", \"smooth\", 500]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetPower, mock);

        let result = conn.bg_set_power(
            crate::power::Power::On,
            TransitionMode::Smooth(Duration::from_millis(500)),
            None,
        );
        assert_ok_result(result);
    }

    #[test]
    fn bg_set_default_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_set_default\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetDefault, mock);

        assert_ok_result(conn.bg_set_default());
    }

    #[test]
    fn bg_start_cf_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_start_cf\",\"params\":[4, 2, \"1000,2,2700,100,500,1,255,10,5000,7,0,0,500,2,5000,1\"]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgStartCf, mock);

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
        assert_ok_result(conn.bg_start_cf(&ColorFlow {
            count: 4,
            action: super::CfAction::TurnOff,
            sequence: vec![
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
        }));
    }

    #[test]
    fn bg_stop_cf_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_stop_cf\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgStopCf, mock);

        assert_ok_result(conn.bg_stop_cf());
    }

    #[test]
    fn bg_set_scene_color_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_scene\",\"params\":[\"color\", 65280, 70]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetScene, mock);

        assert_ok_result(conn.bg_set_scene(&Scene::Color(&RGB { r: 0, g: 255, b: 0 }, 70)));
    }

    #[test]
    fn bg_set_scene_hsv_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_scene\",\"params\":[\"hsv\", 300, 70, 100]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetScene, mock);

        assert_ok_result(conn.bg_set_scene(&Scene::HSV(
            &HSV {
                hue: 300,
                saturation: 70,
            },
            100,
        )));
    }

    #[test]
    fn bg_set_scene_ct_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_set_scene\",\"params\":[\"ct\", 5400, 100]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetScene, mock);

        assert_ok_result(conn.bg_set_scene(&Scene::Ct(5400, 100)));
    }

    #[test]
    fn bg_set_scene_cf_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_set_scene\",\"params\":[\"cf\", 0, 0, \"1000,2,2700,100,500,1,255,10,5000,7,0,0,500,2,5000,1\"]}"
                .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

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

        let cf = ColorFlow {
            count: 0,
            action: super::CfAction::Recover,
            sequence: vec![
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
        };

        let mut conn = conn_with_method(Method::BgSetScene, mock);

        assert_ok_result(conn.bg_set_scene(&Scene::Cf(&cf)));
    }

    #[test]
    fn bg_set_scene_auto_delay_off_test() {
        let mock = MockTcpConnection {
            when_written:
                "{\"id\":1,\"method\":\"bg_set_scene\",\"params\":[\"auto_delay_off\", 50, 5]}"
                    .to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgSetScene, mock);

        assert_ok_result(conn.bg_set_scene(&Scene::AutoDelayOff(50, 5)));
    }

    #[test]
    fn bg_toggle_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"bg_toggle\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::BgToggle, mock);

        assert_ok_result(conn.bg_toggle());
    }

    #[test]
    fn toggle_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"toggle\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::Toggle, mock);

        assert_ok_result(conn.toggle());
    }

    #[test]
    fn dev_toggle_test() {
        let mock = MockTcpConnection {
            when_written: "{\"id\":1,\"method\":\"dev_toggle\",\"params\":[]}".to_string(),
            return_val: TEST_OK_VAL.to_string(),
            written_val: None,
        };

        let mut conn = conn_with_method(Method::DevToggle, mock);

        assert_ok_result(conn.dev_toggle());
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
