use std::convert::TryFrom;

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Method {
    GetProp,
    SetDefault,
    SetPower,
    Toggle,
    SetBright,
    StartCf,
    StopCf,
    SetScene,
    CronAdd,
    CronGet,
    CronDel,
    SetCtAbx,
    SetRgb,
    SetHsv,
    SetAdjust,
    SetMusic,
}

impl TryFrom<&str> for Method {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "get_prop" => Ok(Method::GetProp),
            "set_default" => Ok(Method::SetDefault),
            "set_power" => Ok(Method::SetPower),
            "toggle" => Ok(Method::Toggle),
            "set_bright" => Ok(Method::SetBright),
            "start_cf" => Ok(Method::StartCf),
            "stop_cf" => Ok(Method::StopCf),
            "set_scene" => Ok(Method::SetScene),
            "cron_add" => Ok(Method::CronAdd),
            "cron_get" => Ok(Method::CronGet),
            "cron_del" => Ok(Method::CronDel),
            "set_ct_abx" => Ok(Method::SetCtAbx),
            "set_rgb" => Ok(Method::SetRgb),
            "set_hsv" => Ok(Method::SetHsv),
            "set_adjust" => Ok(Method::SetAdjust),
            "set_music" => Ok(Method::SetMusic),
            _ => Err("Doesn't match known methods."),
        }
    }
}

impl From<&Method> for &str {
    fn from(val: &Method) -> Self {
        match val {
            Method::GetProp => "get_prop",
            Method::SetDefault => "set_default",
            Method::SetPower => "set_power",
            Method::Toggle => "toggle",
            Method::SetBright => "set_bright",
            Method::StartCf => "start_cf",
            Method::StopCf => "stop_cf",
            Method::SetScene => "set_scene",
            Method::CronAdd => "cron_add",
            Method::CronGet => "cron_get",
            Method::CronDel => "cron_del",
            Method::SetCtAbx => "set_ct_abx",
            Method::SetRgb => "set_rgb",
            Method::SetHsv => "set_hsv",
            Method::SetAdjust => "set_adjust",
            Method::SetMusic => "set_music"
        }
    }
}
