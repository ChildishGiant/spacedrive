use std::{
	convert::Infallible,
	net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
	str::FromStr,
	sync::{Arc, PoisonError, RwLock},
};

use flume::{bounded, Receiver, Sender};
use libp2p::{
	core::muxing::StreamMuxerBox,
	futures::StreamExt,
	swarm::dial_opts::{DialOpts, PeerCondition},
	PeerId, Swarm, SwarmBuilder, Transport,
};
use stable_vec::StableVec;
use tokio::{
	net::TcpListener,
	sync::{mpsc, oneshot},
};
use tracing::warn;

use crate::{
	quic::libp2p::socketaddr_to_quic_multiaddr, ConnectionRequest, HookEvent, HookId, ListenerId,
	RemoteIdentity, UnicastStream, P2P,
};

/// [libp2p::PeerId] for debugging purposes only.
#[derive(Debug)]
pub struct Libp2pPeerId(libp2p::PeerId);

#[derive(Debug)]
enum InternalEvent {
	RegisterListener {
		id: ListenerId,
		ipv4: bool,
		addr: SocketAddr,
		result: oneshot::Sender<Result<(), String>>,
	},
	UnregisterListener {
		id: ListenerId,
		ipv4: bool,
		result: oneshot::Sender<Result<(), String>>,
	},
}

/// Transport using Quic to establish a connection between peers.
/// This uses `libp2p` internally.
#[derive(Debug)]
pub struct QuicTransport {
	id: ListenerId,
	p2p: Arc<P2P>,
	state: Arc<RwLock<State>>,
	internal_tx: Sender<InternalEvent>,
}

#[derive(Debug, Default)]
struct State {
	ipv4_addr: Option<Listener<SocketAddrV4>>,
	ipv6_addr: Option<Listener<SocketAddrV6>>,
}

#[derive(Debug)]
struct Listener<T> {
	addr: T,
	libp2p: Result<ListenerId, String>,
}

impl QuicTransport {
	/// Spawn the `QuicTransport` and register it with the P2P system.
	/// Be aware spawning this does nothing unless you call `Self::set_ipv4_enabled`/`Self::set_ipv6_enabled` to enable the listeners.
	// TODO: Error type here
	pub fn spawn(p2p: Arc<P2P>, todo_port: u16) -> Result<(Self, Libp2pPeerId), String> {
		// This is sketchy, but it makes the whole system a lot easier to work with
		// We are assuming the libp2p `Keypair`` is the same format as our `Identity` type.
		// This is *acktually* true but they reserve the right to change it at any point.
		let keypair = libp2p::identity::Keypair::generate_ed25519();
		// TODO: Derive this this not generate it
		// libp2p::identity::Keypair::ed25519_from_bytes(p2p.identity().to_bytes()).unwrap();
		let libp2p_peer_id = Libp2pPeerId(keypair.public().to_peer_id());

		let (tx, rx) = bounded(15);
		let (internal_tx, internal_rx) = bounded(15);
		let (connect_tx, connect_rx) = mpsc::channel(15);
		let id = p2p.register_listener("libp2p-quic", tx, move |listener_id, peer, _addrs| {
			// TODO: I don't love this always being registered. Really it should only show up if the other device is online (do a ping-type thing)???
			peer.listener_available(listener_id, connect_tx.clone());
		});

		// let application_name = format!("/{}/spacetime/1.0.0", p2p.app_name());
		let mut swarm = ok(ok(SwarmBuilder::with_existing_identity(keypair)
			.with_tokio()
			.with_other_transport(|keypair| {
				libp2p_quic::GenTransport::<libp2p_quic::tokio::Provider>::new(
					libp2p_quic::Config::new(keypair),
				)
				.map(|(p, c), _| (p, StreamMuxerBox::new(c)))
				.boxed()
			}))
		// .with_behaviour(|_| SpaceTime::new(p2p.clone(), id)))
		.with_behaviour(|_| libp2p::ping::Behaviour::default()))
		.with_swarm_config(|cfg| {
			cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
		})
		.build();

		swarm
			.listen_on(socketaddr_to_quic_multiaddr(&SocketAddr::from((
				Ipv4Addr::LOCALHOST,
				todo_port,
			))))
			.unwrap();

		let state: Arc<RwLock<State>> = Default::default();
		tokio::spawn(start(
			p2p.clone(),
			id,
			state.clone(),
			swarm,
			rx,
			internal_rx,
			connect_rx,
		));

		Ok((
			Self {
				id,
				p2p,
				state,
				internal_tx,
			},
			libp2p_peer_id,
		))
	}
}

fn ok<T>(v: Result<T, Infallible>) -> T {
	match v {
		Ok(v) => v,
		Err(_) => unreachable!(),
	}
}

async fn start(
	p2p: Arc<P2P>,
	id: ListenerId,
	state: Arc<RwLock<State>>,
	mut swarm: Swarm<libp2p::ping::Behaviour>, // TODO: SpaceTime
	rx: Receiver<HookEvent>,
	internal_rx: Receiver<InternalEvent>,
	mut connect_rx: mpsc::Receiver<ConnectionRequest>,
) {
	// let mut ipv4_listener = None;
	// let mut ipv6_listener = None;

	loop {
		println!("POLL");
		tokio::select! {
			Ok(event) = rx.recv_async() => match event {
				HookEvent::Shutdown => break,
				_ => {},
			},
			event = swarm.select_next_some() => match event {
				event => println!("libp2p event: {:?}", event),
			},
			Ok(event) = internal_rx.recv_async() => match event {
				// InternalEvent::RegisterListener { id, ipv4, addr, result } => {
				// 	match swarm.listen_on(socketaddr_to_quic_multiaddr(&addr)) {
				// 		Ok(libp2p_listener_id) => {
				// 			let this = match ipv4 {
				// 				true => &mut ipv4_listener,
				// 				false => &mut ipv6_listener,
				// 			};
				// 			// TODO: Diff the `addr` & if it's changed actually update it
				// 			if this.is_none() {
				// 				*this =  Some((libp2p_listener_id, addr));
				// 				p2p.register_listener_addr(id, addr);
				// 			}

				// 			let _ = result.send(Ok(()));
				// 		},
				// 		Err(e) => {
				// 			panic!("{:?}", e); // TODO
				// 			let _ = result.send(Err(e.to_string()));
				// 		},
				// 	}
				// },
				// InternalEvent::UnregisterListener { id, ipv4, result } => {
				// 	let this = match ipv4 {
				// 		true => &mut ipv4_listener,
				// 		false => &mut ipv6_listener,
				// 	};
				// 	if let Some((addr_id, addr)) = this.take() {
				// 		if swarm.remove_listener(addr_id) {
				// 			p2p.unregister_listener_addr(id, addr);
				// 		}
				// 	}
				// 	let _ = result.send(Ok(()));
				// },
				_ => {}, // TODO: Fix this
			},
			Some(req) = connect_rx.recv() => {
				println!("DIAL {:?}", req.addrs);
				let opts = DialOpts::unknown_peer_id().addresses(req.addrs.iter().map(socketaddr_to_quic_multiaddr).collect()).build();

				// println!("RESULT {:?}", swarm.dial(opts));

				// match swarm.dial(opts) {
				// 	Ok(_) => {
				// 		tokio::spawn(async move {
				// 			tokio::time::sleep(std::time::Duration::from_secs(99999)).await;
				// 			let _req = req;
				// 		});
				// 	},
				// 	Err(err) => {
				// 		// panic!("{:?}", e); // TODO

				// 		let _ = req.tx.send(Err(err.to_string()));
				// 	},
				// }

				let Err(err) = swarm.dial(opts) else {
					tokio::spawn(async move {
						tokio::time::sleep(std::time::Duration::from_secs(99999)).await;
						let _req = req;
					});

					continue;
				};

				let _ = req.tx.send(Err(err.to_string()));



				// let Err(err) = swarm.dial(opts) else {
				// 	// TODO

				// 	tokio::spawn(async move {
				// 		tokio::time::sleep(std::time::Duration::from_secs(99999)).await;
				// 		let _req = req;
				// 	});

				// 	return;
				// };

				// panic!("ERR {:?}", err);

				// let _ = req.tx.send(Err(err.to_string()));



				// println!("{:?}\n\n", req.addrs);

				// let bruh = req.addrs.iter().filter(|a| a.is_ipv4()).map(socketaddr_to_quic_multiaddr).collect::<Vec<_>>();
				// // println!("BRUH {bruh:?}");

				// let opts = DialOpts::unknown_peer_id()
				// 	// .addresses(bruh)
				// 	.address(socketaddr_to_quic_multiaddr(req.addrs.iter().next().unwrap()))
				// 	.build();
				// // let opts = DialOpts::peer_id(PeerId::from_str("12D3KooWQ7ei5eiMWos5gkXao9YaPBwi2bHgHnam4xiLnFGLAfKy").unwrap())
				// // 	.condition(PeerCondition::Disconnected)
				// //    .addresses(req.addrs.iter().map(socketaddr_to_quic_multiaddr).collect())
				// //    .build();


				// let id = opts.connection_id();
				// let Err(err) = swarm.dial(opts) else {
				// 	// println!("QQQ"); // TODO
				// 	// swarm.behaviour_mut().state.establishing_outbound.lock().unwrap_or_else(PoisonError::into_inner).insert(id, req);

				// 	// let y = swarm.behaviour_mut().state.clone();
				// 	// tokio::spawn(async move {
				// 	// 	// TODO: Timeout and remove from the map sending an error
				// 	// 	loop {
				// 	// 		println!("{:?}", y.establishing_outbound);
				// 	// 		tokio::time::sleep(std::time::Duration::from_secs(100)).await;
				// 	// 	}
				// 	// });

				// 	tokio::spawn(async move {
				// 		tokio::time::sleep(std::time::Duration::from_secs(99999)).await;
				// 		let _req = req;
				// 	});

				// 	return;
				// };

				// println!("EEE"); // TODO

				// warn!(
				// 	"error dialing peer '{}' with addresses '{:?}': {}",
				// 	req.to, req.addrs, err
				// );
				// println!("EMIT ERROR {:?}", err.to_string());
				// let _ = req.tx.send(Err(err.to_string()));

				// println!("DONE"); // TODO
			}
		}
	}
}
