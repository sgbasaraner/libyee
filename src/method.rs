use enum_iterator::IntoEnumIterator;
use std::convert::TryFrom;

#[derive(Debug, Hash, PartialEq, Eq, IntoEnumIterator)]
pub enum Method {
    GetProp,
    SetPower,
    CronAdd,
    CronGet,
    CronDel,
    SetRgb,
    SetHsv,
    SetCtAbx,
    StartCf,
    StopCf,
    SetScene,
    SetDefault,
    SetBright,
    SetAdjust,
    Toggle,
    AdjustBright,
    AdjustCt,
    AdjustColor,
    BgSetRgb,
    BgSetHsv,
    BgSetCtAbx,
    BgStartCf,
    BgStopCf,
    BgSetScene,
    BgSetDefault,
    BgSetBright,
    BgSetAdjust,
    BgToggle,
    BgAdjustBright,
    BgAdjustCt,
    BgAdjustColor,
    BgSetPower,
    SetMusic,
    SetName,
    DevToggle,
}

impl TryFrom<&str> for Method {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let err: &'static str = "Doesn't match known methods.";
        Method::into_enum_iter()
            .find_map(|m| {
                let method_str: &str = (&m).into();

                if method_str == value {
                    Some(m)
                } else {
                    None
                }
            })
            .ok_or(err)
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
            Method::SetMusic => "set_music",
            Method::SetName => "set_name",
            Method::AdjustBright => "adjust_bright",
            Method::AdjustCt => "adjust_ct",
            Method::AdjustColor => "adjust_color",
            Method::BgSetRgb => "bg_set_rgb",
            Method::BgSetHsv => "bg_set_hsv",
            Method::BgSetCtAbx => "bg_set_ct_abx",
            Method::BgStartCf => "bg_start_cf",
            Method::BgStopCf => "bg_stop_cf",
            Method::BgSetScene => "bg_set_scene",
            Method::BgSetDefault => "bg_set_default",
            Method::BgSetBright => "bg_set_bright",
            Method::BgSetAdjust => "bg_set_adjust",
            Method::BgToggle => "bg_toggle",
            Method::BgAdjustBright => "bg_adjust_bright",
            Method::BgAdjustCt => "bg_adjust_ct",
            Method::BgAdjustColor => "bg_adjust_color",
            Method::DevToggle => "dev_toggle",
            Method::BgSetPower => "bg_set_power",
        }
    }
}
