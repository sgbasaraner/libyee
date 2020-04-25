use crate::BulbMap;

#[derive(Debug, Clone)]
pub struct Bulb {
    pub id: String,
    pub ip: String
}

impl Bulb {
    pub(crate) fn new(map: &BulbMap) -> Option<Bulb> {
        map.get("id")
            .and_then(|id| map.get("Location")
            .and_then(|loc| loc.split("//").nth(1)
            .and_then(|ip| Some(Bulb { id: id.to_string(), ip: ip.to_string() }))
        ))
    }
}