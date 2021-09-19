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
}

impl Method {
    pub fn parse(str: &str) -> Option<Method> {
        match str {
            "get_prop" => Some(Method::GetProp),
            "set_default" => Some(Method::SetDefault),
            "set_power" => Some(Method::SetPower),
            "toggle" => Some(Method::Toggle),
            "set_bright" => Some(Method::SetBright),
            "start_cf" => Some(Method::StartCf),
            "stop_cf" => Some(Method::StopCf),
            "set_scene" => Some(Method::SetScene),
            "cron_add" => Some(Method::CronAdd),
            "cron_get" => Some(Method::CronGet),
            "cron_del" => Some(Method::CronDel),
            "set_ct_abx" => Some(Method::SetCtAbx),
            "set_rgb" => Some(Method::SetRgb),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
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
        }
    }
}
