use crate::lightmode::LightMode;
use crate::method::Method;
use crate::power::Power;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug)]
pub struct Bulb {
    // The ID of a Yeelight WiFi LED device that uniquely identifies a Yeelight WiFi LED device.
    pub id: String,

    // The product model of a Yeelight smart device.
    pub model: String,

    // LED device's firmware version.
    pub fw_ver: String,

    // All the supported control methods.
    pub support: HashSet<Method>,

    // Current status of the device.
    pub power: Power,

    // Current brightness, it's the percentage of maximum brightness. Must be between 0 and 100.
    pub bright: u8,

    // Current light mode.
    pub color_mode: LightMode,

    // Name of the device. User can use “set_name” to store the name on the device.
    // The maximum length is 64 bytes. If none-ASCII character is used, it is suggested to
    // BASE64 the name first and then use “set_name” to store it on device.
    pub name: String,

    pub ip_address: String,
}

fn parse_to_hashmap(search_response: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    search_response
        .split("\r\n")
        .into_iter()
        .flat_map(|line| {
            let split_line: Vec<&str> = line.split(": ").collect();

            let key = split_line.first();
            if key.is_none() {
                return None;
            }

            let val = split_line.iter().skip(1).map(|s| *s).collect();
            Some((key.unwrap().clone(), val))
        })
        .for_each(|pair| {
            map.insert(pair.0.to_string(), pair.1);
        });
    return map;
}

impl Bulb {
    pub fn parse(search_response: &str) -> Option<Bulb> {
        let response_map = parse_to_hashmap(search_response);
        let id = response_map.get("id");
        let model = response_map.get("model");
        let fw_ver = response_map.get("fw_ver");
        let support = response_map.get("support").map(|s| {
            s.split(" ")
                .flat_map(|s| Method::parse(s))
                .collect::<HashSet<Method>>()
        });
        let power = response_map.get("power").map(|s| Power::parse(s)).flatten();
        let brightness = response_map
            .get("bright")
            .map(|s| s.parse::<u8>().ok())
            .flatten();

        let light_mode = LightMode::parse(&response_map);

        let name = response_map.get("name");

        let ip = response_map
            .get("Location")
            .map(|s| s.split("//").nth(1))
            .flatten();

        if let (
            Some(model),
            Some(id),
            Some(support),
            Some(power),
            Some(brightness),
            Some(light_mode),
            Some(fw_ver),
            Some(name),
            Some(ip),
        ) = (
            model, id, support, power, brightness, light_mode, fw_ver, name, ip,
        ) {
            let bulb = Bulb {
                bright: brightness,
                color_mode: light_mode,
                fw_ver: fw_ver.clone(),
                id: id.clone(),
                model: model.clone(),
                name: name.clone(),
                power: power,
                support: support,
                ip_address: ip.to_string(),
            };
            Some(bulb)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{bulb::Bulb, lightmode::LightMode, method::Method};

    #[test]
    fn bulb_parse_test() {
        let parse_test_str = "\
            HTTP/1.1 200 OK\r\n\
            Cache-Control: max-age=3600\r\n\
            Date:\r\n\
            Ext: \r\n\
            Location: yeelight://192.168.1.239:55443\r\n\
            Server: POSIX UPnP/1.0 YGLC/1\r\n\
            id: 0x000000000015243f\r\n\
            model: color\r\n\
            fw_ver: 18\r\n\
            support: get_prop set_default set_power toggle set_bright start_cf stop_cf set_scene cron_add cron_get cron_del set_ct_abx set_rgb\r\n\
            power: on\r\n\
            bright: 100\r\n\
            color_mode: 2\r\n\
            ct: 4000\r\n\
            rgb: 16711680\r\n\
            hue: 100\r\n\
            sat: 35\r\n\
            name: my_bulb\r\n\
            ";
        let bulb = Bulb::parse(parse_test_str);

        assert!(bulb.is_some());

        let bulb = bulb.unwrap();

        assert_eq!(bulb.ip_address, "192.168.1.239:55443");
        assert_eq!(bulb.id, "0x000000000015243f");
        assert_eq!(bulb.model, "color");
        assert_eq!(bulb.fw_ver, "18");
        assert_eq!(bulb.power, crate::power::Power::On);
        assert_eq!(bulb.color_mode, LightMode::ColorTemperature(4000));
        assert_eq!(bulb.name, "my_bulb");

        let methods = &[
            Method::GetProp,
            Method::SetDefault,
            Method::SetPower,
            Method::Toggle,
            Method::SetBright,
            Method::StartCf,
            Method::StopCf,
            Method::SetScene,
            Method::CronAdd,
            Method::CronGet,
            Method::CronDel,
            Method::SetCtAbx,
            Method::SetRgb,
        ];

        for method in methods {
            assert!(bulb.support.contains(&method));
        }

        
    }
}
