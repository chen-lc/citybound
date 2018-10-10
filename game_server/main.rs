extern crate citybound_common;
use citybound_common::*;

extern crate rouille;
use rouille::{Response, extension_to_mime};

#[macro_use]
extern crate rust_embed_flag;

#[derive(RustEmbed)]
#[folder = "game_browser/dist/"]
struct Asset;

extern crate clap;
use clap::{Arg, App};

use kay::TypedID;
use transport::lane::{LaneID, SwitchLaneID};
use economy::households::household_kinds::family::FamilyID;
use economy::households::household_kinds::grocery_shop::GroceryShopID;
use economy::households::household_kinds::grain_farm::GrainFarmID;
use economy::households::household_kinds::cow_farm::CowFarmID;
use economy::households::household_kinds::mill::MillID;
use economy::households::household_kinds::bakery::BakeryID;
use economy::households::household_kinds::neighboring_town_trade::NeighboringTownTradeID;
use economy::households::tasks::TaskEndSchedulerID;
use construction::ConstructionID;

const VERSION: &str = include_str!("../.version");

fn main() {
    let arg_matches = App::new("citybound")
        .version(VERSION.trim())
        .author("ae play (Anselm Eickhoff)")
        .about("The city is us.")
        .arg(
            Arg::with_name("mode")
                .long("mode")
                .value_name("local/lan/internet")
                .display_order(0)
                .possible_values(&["local", "lan", "internet"])
                .default_value("local")
                .help("Where to expose the simulation. Sets defaults other settings."),
        ).arg(
            Arg::with_name("bind")
                .long("bind")
                .value_name("host:port")
                .default_value_ifs(&[
                    ("mode", Some("local"), "localhost:1234"),
                    ("mode", Some("lan"), "0.0.0.0:1234"),
                    ("mode", Some("internet"), "0.0.0.0:1234"),
                ]).help("Address and port to serve the browser UI from"),
        ).arg(
            Arg::with_name("bind-sim")
                .long("bind-sim")
                .value_name("host:port")
                .default_value_ifs(&[
                    ("mode", Some("local"), "localhost:9999"),
                    ("mode", Some("lan"), "0.0.0.0:9999"),
                    ("mode", Some("internet"), "0.0.0.0:9999"),
                ]).help("Address and port to accept connections to the simulation from"),
        ).arg(
            Arg::with_name("batch-msg-b")
                .long("batch-msg-bytes")
                .value_name("n-bytes")
                .default_value("5000")
                .help("How many bytes of simulation messages to batch"),
        ).arg(
            Arg::with_name("ok-turn-dist")
                .long("ok-turn-dist")
                .value_name("n-turns")
                .default_value_ifs(&[
                    ("mode", Some("local"), "2"),
                    ("mode", Some("lan"), "10"),
                    ("mode", Some("internet"), "30"),
                ]).help("How many network turns client/server can be behind before skipping"),
        ).arg(
            Arg::with_name("skip-ratio")
                .long("skip-ratio")
                .value_name("n-turns")
                .default_value("5")
                .help("How many network turns to skip if server/client are ahead"),
        ).get_matches();

    let serve_host_port = arg_matches.value_of("bind").unwrap().to_owned();
    let arg_matches_2 = arg_matches.clone();

    let my_host = format!(
        "{}:{}",
        match arg_matches.value_of("mode").unwrap() {
            "local" => "localhost",
            "lan" => "<your LAN IP>",
            "internet" => "<your public IP>",
            _ => unreachable!(),
        },
        serve_host_port.split(':').nth(1).unwrap(),
    );

    ::std::thread::spawn(move || {
        println!("  {: ^41}  ", format!("Citybound {}", VERSION.trim()));
        println!("  {: ^41}  ", "please connect with your browser");
        println!("╭───────────────────────────────────────────╮");
        println!("│ {: ^41} │", format!("http://{}", my_host));
        println!("╰───────────────────────────────────────────╯");
        println!("SIMULATION LOG:");

        rouille::start_server(serve_host_port, move |request| {
            if request.raw_url() == "/" {
                println!("{:?} loaded page", request.remote_addr());

                let template = std::str::from_utf8(
                    &Asset::get("index.html").expect("index.html should exist as asset"),
                ).unwrap()
                .to_owned();

                let rendered = template
                    .replace("CB_VERSION", VERSION.trim())
                    .replace(
                        "CB_BATCH_MESSAGE_BYTES",
                        arg_matches_2.value_of("batch-msg-b").unwrap(),
                    ).replace(
                        "CB_ACCEPTABLE_TURN_DISTANCE",
                        arg_matches_2.value_of("ok-turn-dist").unwrap(),
                    ).replace(
                        "CB_SKIP_TURNS_PER_TURN_AHEAD",
                        arg_matches_2.value_of("skip-ratio").unwrap(),
                    );

                Response::html(rendered)
            } else if let Some(asset) = Asset::get(&request.url()[1..]) {
                Response::from_data(
                    if request.url().ends_with(".wasm") {
                        "application/wasm"
                    } else {
                        extension_to_mime(request.url().split('.').last().unwrap_or(""))
                    },
                    asset,
                )
            } else {
                Response::html(format!("404 error. Not found: {}", request.url()))
                    .with_status_code(404)
            }
        });
    });

    util::init::ensure_crossplatform_proper_thread(move || {
        let mut system = Box::new(kay::ActorSystem::new(kay::Networking::new(
            0,
            vec![
                arg_matches.value_of("bind-sim").unwrap().to_owned(),
                "ws-client".to_owned(),
            ],
            arg_matches
                .value_of("batch-msg-b")
                .unwrap()
                .parse()
                .unwrap(),
            arg_matches
                .value_of("ok-turn-dist")
                .unwrap()
                .parse()
                .unwrap(),
            arg_matches.value_of("skip-ratio").unwrap().parse().unwrap(),
        )));

        setup_common(&mut system);

        let world = &mut system.world();

        system.networking_connect();

        let simulatables = vec![
            LaneID::local_broadcast(world).into(),
            SwitchLaneID::local_broadcast(world).into(),
            FamilyID::local_broadcast(world).into(),
            GroceryShopID::local_broadcast(world).into(),
            GrainFarmID::local_broadcast(world).into(),
            CowFarmID::local_broadcast(world).into(),
            MillID::local_broadcast(world).into(),
            BakeryID::local_broadcast(world).into(),
            NeighboringTownTradeID::local_broadcast(world).into(),
            TaskEndSchedulerID::local_first(world).into(),
            ConstructionID::global_first(world).into(),
        ];
        let simulation = simulation::spawn(world, simulatables);
        util::init::set_error_hook();

        let plan_manager = planning::spawn(world);
        construction::spawn(world);
        transport::spawn(world, simulation);
        economy::spawn(world, simulation, plan_manager);
        system.process_all_messages();

        let mut frame_counter = util::init::FrameCounter::new();
        let mut skip_turns = 0;

        loop {
            frame_counter.start_frame();

            system.process_all_messages();

            if system.shutting_down {
                break;
            }

            if skip_turns == 0 {
                simulation.progress(world);

                system.process_all_messages();
            }

            system.networking_send_and_receive();

            system.process_all_messages();

            if skip_turns > 0 {
                skip_turns -= 1;
            //println!("Skipping! {} left", skip_turns);
            } else {
                let maybe_should_skip = system.networking_finish_turn();
                if let Some(should_skip) = maybe_should_skip {
                    skip_turns = should_skip.min(100);
                }
            }

            //frame_counter.print_fps();
            frame_counter.sleep_if_faster_than(120);
        }
    });
}
