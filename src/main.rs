use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

use aprs_parser::{AprsData, AprsPacket};
use geoutils::Location;
use serde::{Deserialize, Serialize};

mod secrets;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallSubscription {
    pub callsign: String,
    pub ssid: Option<String>,
    pub resource_id: String,
}

fn main() {
    let cfg_str = fs::read_to_string("config.json").expect("error reading config.json");
    let cfg: Vec<CallSubscription> = serde_json::from_str(&cfg_str).expect("error parsing config");

    let mut location_cache: HashMap<String, (f64, f64)> = HashMap::new();

    let mut stream =
        TcpStream::connect("noam.aprs2.net:14580").expect("couldn't connect to the server...");
    stream.set_nodelay(true).expect("set_nodelay call failed");

    stream
        .write(
            format!("user k9fgt pass {} filter r/42.93173958682743/-76.58727500008743/500\n", secrets::APRS_PASS).as_bytes(),
        )
        .expect("failed to send login");

    // stream
    //     .write("user k9fgt pass -1\n".as_bytes())
    //     .expect("failed to send login");

    println!("logged in, starting stream");

    let mut packet = BufReader::new(&stream);
    let mut line: String = Default::default();

    loop {
        line.clear();

        let rl_res = packet.read_line(&mut line);
        if rl_res.is_err() {
            continue;
        }

        // print!("{}", line);
        if line.starts_with("#") {
            continue;
        }

        let packet = match AprsPacket::decode_textual(line.as_bytes()) {
            Ok(packet) => packet,
            Err(_e) => continue,
        };

        // println!("{:?}", packet);
        let resource_sub = cfg.iter().find(|s| {
            s.callsign == packet.from.call().to_owned()
                && (s.ssid.is_none() || s.ssid
                    .as_ref()
                    .is_some_and(|ssid| packet.from.ssid().is_some_and(|pssid| pssid == ssid)))
        });

        if resource_sub.is_some() {
            // println!("{:?}", packet);
            println!("location recieved for {}", packet.from.to_string());

            match packet.data {
                AprsData::Position(curr_pos) => {
                    if location_cache.contains_key(&packet.from.call().to_owned()) {
                        let (l_lat, l_lon) =
                            location_cache.get(&packet.from.call().to_owned()).unwrap();
                        let last_location = Location::new(l_lat.to_owned(), l_lon.to_owned());
                        let curr_location =
                            Location::new(curr_pos.latitude.value(), curr_pos.longitude.value());
                        let dist = last_location.distance_to(&curr_location).unwrap();
                        println!(
                            "{} has moved {} feet",
                            &packet.from.call(),
                            dist.meters() * 3.28084
                        );
                        if dist.meters() < 20.0 {
                            continue;
                        }
                    }

                    // if the move is large or not in cache, update cache and get geocoded location
                    location_cache.insert(
                        packet.from.call().to_owned(),
                        (curr_pos.latitude.value(), curr_pos.longitude.value()),
                    );

                    let _body: String =
                        ureq::post("http://127.0.0.1:8080/api/v0/resources/location")
                            .set(
                                "Authorization",
                                "//",
                            )
                            .send_json(ureq::json!({
                                "id": resource_sub.unwrap().resource_id,
                                "lat": curr_pos.latitude.value().to_string(),
                                "lon": curr_pos.longitude.value().to_string()
                            }))
                            .unwrap()
                            .into_string()
                            .unwrap();
                }
                _ => continue,
            }
        }
    }
}
