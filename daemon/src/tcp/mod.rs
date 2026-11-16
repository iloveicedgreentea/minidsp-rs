//! TCP server compatible with the official mobile and desktop application
use std::{
    net::{Ipv4Addr, ToSocketAddrs},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use futures::{pin_mut, SinkExt, StreamExt};
use minidsp::{
    transport::{
        net::{discovery, Codec},
        Transport,
    },
    MiniDSPError,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    select,
};
use tokio_util::codec::Framed;

use crate::device_manager::Device;

use super::config;

/// This lets multiple users talk to the same device simultaneously, which depending on the
/// user could be problematic.
async fn forward<T>(tcp: T, device: Transport) -> Result<()>
where
    T: AsyncRead + AsyncWrite + 'static,
{
    // Truncate each HID frame after its ending
    let mut device = device.map(|frame| {
        let frame = frame?;
        if frame.is_empty() {
            return Err(MiniDSPError::MalformedResponse(
                "Received an empty frame".to_string(),
            ));
        }

        let len = frame[0] as usize;
        if frame.len() < len {
            return Err(MiniDSPError::MalformedResponse(format!(
                "Expected frame of length {}, got {}",
                len,
                frame.len()
            )));
        }

        Ok::<_, MiniDSPError>(frame.slice(0..len))
    });

    // Apply framing to the TCP stream
    let remote = Framed::new(tcp, Codec::new_server());
    pin_mut!(remote);

    loop {
        select! {
            frame = device.next() => {
                match frame {
                    Some(frame) => remote.send(frame?).await.context("remote.send failed")?,
                    None => {
                        return Err(MiniDSPError::TransportClosed.into());
                    }
                }
            },
            frame = remote.next() => {
                let frame = frame.ok_or(MiniDSPError::TransportClosed)?;
                device.send(frame.context("decoding frame")?).await.context("device_tx.send failed")?;
            },
        }
    }
}

pub fn start_advertise(cfg: Arc<config::TcpServer>) -> Result<(), anyhow::Error> {
    // Use the same server configuration for the advertised address and device identity.
    if let Some(ref advertise) = cfg.advertise {
        let ip_address = Ipv4Addr::from_str(&advertise.ip)?;
        let hostname: Arc<str> = Arc::from(advertise.name.clone());
        let cfg = cfg.clone();
        let packet_fn = move || -> Option<discovery::DiscoveryPacket> {
            // Find a suitable device to forward this client to
            let device = device_matches(&cfg).ok()?;
            let device_info = device.device_info()?;

            let mut packet = discovery::DiscoveryPacket {
                // The mac address is used to distinguish between devices on the *device list* page in the MiniDSP Device Console app
                mac_address: [0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00],
                ip_address,
                hwid: device_info.hw_id,
                dsp_id: device_info.dsp_version,
                fw_major: device_info.fw_major,
                fw_minor: device_info.fw_minor,
                sn: ((device_info.serial - 900000) & 0xFFFF) as u16,
                hostname: hostname.to_string(),
            };

            // Use a unique MAC by stitching the IP's bytes in the last 4 bytes
            packet.mac_address[2..].copy_from_slice(&packet.ip_address.octets());

            Some(packet)
        };

        let bind_addr = match &advertise.bind_address {
            None => None,
            Some(addr) => Some(addr.to_socket_addrs()?.next().ok_or_else(|| {
                anyhow::anyhow!("bind adddress didn't resolve to a usable address")
            })?),
        };

        let interval = Duration::from_secs(1);
        tokio::spawn(discovery::server::advertise_packet(
            bind_addr, packet_fn, interval,
        ));
    }
    Ok(())
}

pub async fn main(cfg: config::TcpServer) -> Result<(), MiniDSPError> {
    let cfg = Arc::new(cfg);

    let bind_address = cfg.bind_address.as_deref().unwrap_or("0.0.0.0:5333");

    if let Err(adv_err) = start_advertise(cfg.clone()) {
        log::error!("error launching advertisement task: {adv_err:?}");
    }

    let listener = TcpListener::bind(&bind_address).await?;
    log::info!("Listening on {}", &bind_address);
    loop {
        select! {
           result = listener.accept() => {
                let (stream, addr) = result?;
                log::info!("[{addr:?}] New connection");

                // Find a suitable device to forward this client to
                let device = {
                    let cfg = cfg.clone();
                    tokio::task::spawn_blocking(move || device_matches(&cfg).ok()).await.unwrap()
                };

                log::info!("[{:?}] New connection assiged to {}",
                    addr,
                    device.as_ref().map(|dev| dev.url.clone()).unwrap_or_else(|| "(no devices found)".to_string())
                );

                if let Some(hub) = device.and_then(|dev| dev.to_hub()) {
                    tokio::spawn(async move {
                        let result = forward(stream, Box::pin(hub)).await;

                        if let Err(e) = result {
                            log::info!("[{addr}] Connection closed: {e:?}");
                        }

                        log::info!("[{addr:?}] Closed");
                    });
                }
           },
        }
    }
}

fn device_matches(cfg: &config::TcpServer) -> Result<Arc<Device>> {
    let app = super::APP.get().unwrap();
    let app = app.blocking_read();
    let device = {
        let mut devices = app
            .device_manager
            .as_ref()
            .ok_or(MiniDSPError::TransportClosed)?
            .devices();

        if let Some(serial) = cfg.device_serial {
            devices.into_iter().find(|dev| {
                dev.device_info()
                    .map(|di| di.serial == serial)
                    .unwrap_or(false)
            })
        } else if let Some(device_index) = cfg.device_index {
            devices.into_iter().nth(device_index)
        } else {
            devices.sort_by_key(|dev| !dev.is_local());
            devices.into_iter().next()
        }
    };

    device.ok_or_else(|| anyhow::anyhow!("no matching devices found"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use tokio::{net::UdpSocket, time::Instant};

    use super::*;
    use crate::{device_manager::DeviceManager, discovery::Registry, App, Opts, APP};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn advertisements_keep_each_address_bound_to_its_own_device() -> Result<()> {
        // Receive real discovery packets, but send them from loopback and advertise
        // loopback addresses so this test cannot offer a usable bridge on the LAN.
        let receiver = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, discovery::DISCOVERY_PORT)).await?;
        let servers = vec![
            config::TcpServer {
                bind_address: Some("127.0.0.1:0".into()),
                device_serial: Some(911111),
                // An unadvertised local bridge must not start announcements for
                // the other servers either.
                ..Default::default()
            },
            config::TcpServer {
                bind_address: Some("127.0.0.1:0".into()),
                device_serial: Some(911111),
                advertise: Some(config::Advertise {
                    ip: "127.0.0.240".into(),
                    name: "minidsp-rs-test-first".into(),
                    bind_address: Some("127.0.0.1:0".into()),
                }),
                ..Default::default()
            },
            config::TcpServer {
                bind_address: Some("127.0.0.1:0".into()),
                device_serial: Some(922222),
                advertise: Some(config::Advertise {
                    ip: "127.0.0.241".into(),
                    name: "minidsp-rs-test-second".into(),
                    bind_address: Some("127.0.0.1:0".into()),
                }),
                ..Default::default()
            },
        ];
        APP.set(App::new(
            Opts::default(),
            config::Config {
                tcp_servers: servers.clone(),
                ignore_advertisements: true,
                ..Default::default()
            },
        ))
        .ok()
        .expect("test application should only be initialized once");
        {
            let mut app = APP.get().unwrap().write().await;
            let manager = DeviceManager::new(Registry::new(), HashSet::new(), true);
            manager.register_static("mock:?serial=911111&dsp_version=99");
            manager.register_static("mock:?serial=922222&dsp_version=100");
            app.device_manager = Some(manager);
        }
        // Start the real TCP server entry point, including its discovery tasks.
        let tasks: Vec<_> = servers
            .into_iter()
            .map(|cfg| tokio::spawn(main(cfg)))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(4);
        let mut counts = [0, 0];
        let mut buffer = [0; 1024];
        while let Ok(result) =
            tokio::time::timeout_at(deadline, receiver.recv_from(&mut buffer)).await
        {
            let (size, _) = result?;
            let packet = discovery::DiscoveryPacket::parse(&buffer[..size])?;
            if !packet.hostname.starts_with("minidsp-rs-test-") {
                continue;
            }
            let (index, serial, dsp_id, name) = match packet.ip_address.octets()[3] {
                240 => (0, 11111, 99, "minidsp-rs-test-first"),
                241 => (1, 22222, 100, "minidsp-rs-test-second"),
                _ => panic!("unexpected advertised address: {packet:?}"),
            };
            assert_eq!(packet.sn, serial, "wrong serial for {}", packet.ip_address);
            assert_eq!(packet.dsp_id, dsp_id, "wrong DSP for {}", packet.ip_address);
            assert_eq!(packet.hostname, name);
            assert_eq!(packet.mac_address[..2], [0xff, 0xff]);
            assert_eq!(packet.mac_address[2..], packet.ip_address.octets());
            counts[index] += 1;
        }
        assert!(
            counts.iter().all(|count| *count >= 2),
            "missing repeated announcements: {counts:?}"
        );
        for task in tasks {
            assert!(!task.is_finished(), "TCP server exited unexpectedly");
            task.abort();
        }
        Ok(())
    }
}
