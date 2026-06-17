use anyhow::Result;
use cli_table::WithTitle;
use crossbeam::channel::TryRecvError;
use crossbeam::{atomic, channel, thread};
use skymon::aircraft::{Aircraft, AircraftsBuilder};
use skymon::config::Config;
use skymon::utils::{AircraftTableRow, sleep_ms};
use skymon::{aircraft::Aircrafts, sbs1};
use std::io::{self, Write};
use std::sync::Arc;
use std::{net, time as std_time};
use time::UtcDateTime;

fn main() -> Result<()> {
    #[cfg(debug_assertions)]
    let config: Config = Config {
        sbs1_server: "192.168.20.18:30003".into(),
        ..Default::default()
    };

    #[cfg(not(debug_assertions))]
    let config = Config::new()?;

    let should_kill = Arc::new(atomic::AtomicCell::new(false));

    // listen for SIGINT
    let stop_signal = Arc::clone(&should_kill);
    ctrlc::set_handler(move || {
        stop_signal.store(true);
    })?;

    thread::scope(|s| {
        let (sbs1_frame_tx, sbs1_frame_rx) = channel::unbounded::<sbs1::Frame>();
        let (aircraft_tabrows_tx, aircraft_tabrows_rx) =
            channel::bounded::<Vec<AircraftTableRow>>(1); // we take a snapshot of the current state

        // sbs1 frame feeder thread
        let stop_signal = Arc::clone(&should_kill);
        let feeder_handle = s.spawn(move |_| {
            let timeout = std_time::Duration::from_millis(1000);
            let socket = net::TcpStream::connect(&config.sbs1_server)
                .expect("fail to connect to SBS1 server");
            _ = socket.set_nodelay(true);
            _ = socket.set_read_timeout(Some(timeout));

            let mut fetcher = sbs1::TcpFetcher::new(socket);

            println!("connected to SBS1 server: {}", config.sbs1_server);
            loop {
                if stop_signal.load() {
                    break;
                }

                if let Ok(frame) = fetcher.read_frame() {
                    _ = sbs1_frame_tx.send(frame);
                } else {
                    sleep_ms(100); // adding delay before retrying
                    continue;
                }
            }
        });

        // processing thread
        let aircrafts_proc_handle = s.spawn(move |_| {
            let flush_period = time::Duration::minutes(config.flush_period_mins as i64);
            let home_coord = config.home.parse().unwrap_or_default();
            let mut stdout = io::stdout();
            let mut last_flush_time = std_time::Instant::now();

            let mut aircrafts = AircraftsBuilder::new()
                .home(&home_coord)
                .radius(config.detection_dist)
                .persistence(&config.db_path)
                .build();

            if let Err(e) = aircrafts.import_aircrafts_metadata("assets/aircraft.csv.gz") {
                eprintln!("fail to import aircrafts metadata: {}", e);
            } else {
                println!("aircrafts metadata imported");
            }

            loop {
                // update aircrafts state by frames from feeder
                match sbs1_frame_rx.try_recv() {
                    Ok(sbs_frame) => {
                        aircrafts.feed(&sbs_frame);
                    }

                    // flush aircrafts state storage when idle
                    Err(TryRecvError::Empty) => {
                        let now = std_time::Instant::now();
                        if now - last_flush_time >= flush_period {
                            aircrafts.flush();

                            if config.slient {
                                stdout_print_recored_aircrafts_recent(&mut stdout, &aircrafts);
                                println!("flush took {} ms", now.elapsed().as_millis());
                            }

                            last_flush_time = now;
                        }

                        sleep_ms(450);
                    }

                    // channel closed, exit loop
                    _ => {
                        eprintln!("SBS1 frames feeder channel closed");
                        break;
                    }
                }

                if !config.slient {
                    let aircrafts_iter: Box<dyn Iterator<Item = &Aircraft>> = if config.disp_all {
                        Box::new(aircrafts.iter())
                    } else {
                        Box::new(aircrafts.iter_within_radius())
                    };

                    // we don't care about whether the current snapshot is passed to the display correctly,
                    // DON'T block here.
                    let current_snapshots_of_aircrafts: Vec<AircraftTableRow> =
                        aircrafts_iter.map(|a| a.into()).collect();
                    _ = aircraft_tabrows_tx.try_send(current_snapshots_of_aircrafts);
                }
            }
        });

        // display thread, only available when it's not in slient mode
        if !config.slient {
            let stop_signal = Arc::clone(&should_kill);
            let disp_handle = s.spawn(move |_| {
                let mut stdout = io::stdout();
                let disp_refresh_time_ms = config
                    .disp_refresh_rate_ms
                    .min(Config::minimum_refresh_rate_ms());

                loop {
                    if stop_signal.load() {
                        break;
                    }

                    match aircraft_tabrows_rx.try_recv() {
                        Ok(mut rows) => {
                            // clear terminal
                            _ = stdout.write_all("\x1B[H\x1B[2J\x1B[3J".as_bytes());
                            _ = stdout.flush();

                            rows.sort();

                            let tab = rows.with_title().display().unwrap().to_string();
                            _ = stdout.write_all(tab.as_bytes());
                            _ = stdout.flush();
                        }
                        _ => {}
                    };

                    // throttling the refresh rate
                    sleep_ms(disp_refresh_time_ms);
                }
            });

            _ = disp_handle.join();
        }

        _ = aircrafts_proc_handle.join();
        _ = feeder_handle.join();
    })
    .expect("fail to swpan threads");

    println!("exit");

    Ok(())
}

fn stdout_print_recored_aircrafts_recent(stdout: &mut io::Stdout, aircrafts: &Aircrafts) {
    // clear terminal
    _ = stdout.write_all("\x1B[H\x1B[2J\x1B[3J".as_bytes());
    _ = stdout.flush();

    // header
    let now = UtcDateTime::now().truncate_to_second();
    _ = stdout.write_all(format!("----{now}----\n",).as_bytes());

    let until_datetime = UtcDateTime::now();
    let from_datetime = until_datetime - time::Duration::hours(6);

    if let Ok(air_entries) = aircrafts.dump_seen_by_datetime(&from_datetime, &until_datetime) {
        let rows: Vec<AircraftTableRow> = air_entries.into_iter().map(|a| a.into()).collect();
        let tab = rows.with_title().display().unwrap().to_string();

        _ = stdout.write_all(tab.as_bytes());
        _ = stdout.flush();
    }
}
