// See the "macOS permissions note" in README.md before running this on macOS
// Big Sur or later.

use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, CentralEvent, ValueNotification};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::error::Error;
use std::time::Duration;
use tokio::{time};
use uuid::Uuid;
use chrono;
use futures::StreamExt;

#[macro_use]
extern crate log;

/// Only devices whose name contains this string will be tried.
const PERIPHERAL_NAME_MATCH_FILTER: &str = "OxySmart";
/// UUID of the characteristic for which we should subscribe to notifications to receive new bytes
const NUS_CHARACTERISTIC_RX_UUID: Uuid = Uuid::from_u128(0x6e400003_b5a3_f393_e0a9_e50e24dcca9e);

async fn find_device(manager: &Manager) -> Result<(Adapter, Peripheral, btleplug::api::Characteristic), Box<dyn Error>> {
    let adapter_list = manager.adapters().await?;
    if adapter_list.is_empty() {
        error!("No Bluetooth adapters found");
        return Err("No adapters found".into());
    }

    for adapter in adapter_list.iter() {
        info!("Starting scan...");
        adapter
            .start_scan(ScanFilter::default())
            .await
            .expect("Can't scan BLE adapter for connected devices...");
        time::sleep(Duration::from_secs(2)).await;
        let peripherals = adapter.peripherals().await?;

        if peripherals.is_empty() {
            error!("->>> BLE peripheral devices were not found, sorry. Exiting...");
            return Err("No BLE peripheral devices found".into());
        }

        // All peripheral devices in range.
        for peripheral in peripherals.iter() {
            let properties = peripheral.properties().await?.unwrap();
            let is_connected = peripheral.is_connected().await?;
            let local_name = properties
                .local_name
                .unwrap_or(String::from(properties.address.to_string()));
            // Check if it's the peripheral we want.
            if !local_name.contains(PERIPHERAL_NAME_MATCH_FILTER) {
                continue;
            }

            info!("Found matching peripheral {:?}...", &local_name);
            if !is_connected {
                // Connect if we aren't already connected.
                if let Err(err) = peripheral.connect().await {
                    error!("Error connecting to peripheral, skipping: {}", err.to_string());
                    continue;
                }
            }
            let is_connected = peripheral.is_connected().await?;
            info!("Now connected ({:?}) to peripheral {:?}.", is_connected, &local_name);
            if !is_connected {
                error!("Couldn't connect to peripheral, skipping {:?}.", &local_name);
                continue;
            }

            debug!("Discover peripheral {:?} services...", local_name);
            peripheral.discover_services().await?;
            let characteristics = peripheral.characteristics();
            let characteristic_rx = characteristics.iter().find(|c| {
                c.uuid == NUS_CHARACTERISTIC_RX_UUID &&
                    c.properties.contains(CharPropFlags::NOTIFY)
            });
            if characteristic_rx.is_none() {
                error!("Couldn't find characteristic, skipping {:?}.", &local_name);
                continue;
            }
            return Ok((adapter.to_owned(), peripheral.to_owned(), characteristic_rx.unwrap().to_owned()));
        }
    }
    Err("No matching peripheral found".into())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    let manager = Manager::new().await?;
    println!("time,spo2,heartrate");

    loop {
        match find_device(&manager).await {
            Ok((adaptor, peripheral, characteristic_rx)) => {
                peripheral.subscribe(&characteristic_rx).await?;
                let mut notification_stream = peripheral.notifications().await?;
                let mut disconnect_stream = adaptor.events().await?;
                // Process while the BLE connection is not broken or stopped.


                loop {
                    tokio::select! {
                        msg = notification_stream.next() => {
                            match msg {
                                Some(ValueNotification { uuid: _, value }) => {
                                    trace!("Got raw data: {:?}", value);
                                    if value.len() >= 7 && value[..5] == vec! {0xaa, 0x55, 0x0f, 0x08, 0x01} {
                                        let time_iso8601 = chrono::offset::Utc::now().to_rfc3339();
                                        let (spo2, hr) = (value[5], value[6]);
                                        if spo2 == 0 && hr == 0 {
                                            debug!("Suppressing null data");
                                            continue;
                                        }
                                        println!("{},{},{}", time_iso8601, spo2, hr);
                                    }
                                },
                                _ => break
                            }
                        },
                        msg = disconnect_stream.next() => {
                            match msg {
                                Some(CentralEvent::DeviceDisconnected(periph_id)) if periph_id == peripheral.id() => {
                                    info!("Disconnected from peripheral, exiting...");
                                    break;
                                },
                                _ => {}
                            }
                        },
                    }
                }

                info!("Disconnecting from peripheral...");
                peripheral.disconnect().await?;
            }
            Err(e) => { error!("Failed to connect: {}", e); }
        };
    }
}
