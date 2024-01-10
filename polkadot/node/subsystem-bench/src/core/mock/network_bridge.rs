// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.
//!
//! Mocked `network-bridge` subsystems that uses a `NetworkInterface` to access
//! the emulated network.
use futures::{
	channel::{
		mpsc::{UnboundedReceiver, UnboundedSender},
		oneshot,
	},
	Future,
};
use overseer::AllMessages;
use parity_scale_codec::Encode;
use polkadot_node_subsystem_types::{
	messages::{BitfieldDistributionMessage, NetworkBridgeEvent},
	OverseerSignal,
};
use std::{collections::HashMap, f32::consts::E, pin::Pin};

use futures::{FutureExt, Stream, StreamExt};

use polkadot_primitives::CandidateHash;
use sc_network::{
	network_state::Peer,
	request_responses::{IncomingRequest, ProtocolConfig},
	OutboundFailure, PeerId, RequestFailure,
};

use polkadot_node_subsystem::{
	messages::NetworkBridgeTxMessage, overseer, SpawnedSubsystem, SubsystemError,
};

use polkadot_node_network_protocol::{
	request_response::{ v1::ChunkResponse, Recipient, Requests, ResponseSender},
	Versioned,
};
use polkadot_primitives::AuthorityDiscoveryId;

use crate::core::network::{NetworkEmulatorHandle, NetworkInterfaceReceiver, NetworkMessage};

const LOG_TARGET: &str = "subsystem-bench::network-bridge";

/// A mock of the network bridge tx subsystem.
pub struct MockNetworkBridgeTx {
	/// A network emulator handle
	network: NetworkEmulatorHandle,
	/// A channel to the network interface,
	to_network_interface: UnboundedSender<NetworkMessage>,
}

/// A mock of the network bridge tx subsystem.
pub struct MockNetworkBridgeRx {
	/// A network interface receiver
	network_receiver: NetworkInterfaceReceiver,
	/// Chunk request sender
	chunk_request_sender: Option<ProtocolConfig>,
}

impl MockNetworkBridgeTx {
	pub fn new(
		network: NetworkEmulatorHandle,
		to_network_interface: UnboundedSender<NetworkMessage>,
	) -> MockNetworkBridgeTx {
		Self { network, to_network_interface }
	}
}

impl MockNetworkBridgeRx {
	pub fn new(
		network_receiver: NetworkInterfaceReceiver,
		chunk_request_sender: Option<ProtocolConfig>,
	) -> MockNetworkBridgeRx {
		Self { network_receiver, chunk_request_sender }
	}
}

#[overseer::subsystem(NetworkBridgeTx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeTx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "network-bridge-tx", future }
	}
}

#[overseer::subsystem(NetworkBridgeRx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeRx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "network-bridge-rx", future }
	}
}

// Helper trait for `Requests`.
trait RequestExt {
	fn authority_id(&self) -> Option<&AuthorityDiscoveryId>;
	fn into_response_sender(self) -> ResponseSender;
}

impl RequestExt for Requests {
	fn authority_id(&self) -> Option<&AuthorityDiscoveryId> {
		match self {
			Requests::ChunkFetchingV1(request) => {
				if let Recipient::Authority(authority_id) = &request.peer {
					Some(authority_id)
				} else {
					None
				}
			},
			request => {
				unimplemented!("RequestAuthority not implemented for {:?}", request)
			},
		}
	}

	fn into_response_sender(self) -> ResponseSender {
		match self {
			Requests::ChunkFetchingV1(outgoing_request) => {
				outgoing_request.pending_response
			},
			Requests::AvailableDataFetchingV1(outgoing_request) => {
				outgoing_request.pending_response
			}
			_ => unimplemented!("unsupported request type")
		}
	}
}

#[overseer::contextbounds(NetworkBridgeTx, prefix = self::overseer)]
impl MockNetworkBridgeTx {
	async fn run<Context>(self, mut ctx: Context) {
		// Main subsystem loop.
		loop {
			let subsystem_message = ctx.recv().await.expect("Overseer never fails us");
			match subsystem_message {
				orchestra::FromOrchestra::Signal(signal) => match signal {
					OverseerSignal::Conclude => return,
					_ => {},
				},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					NetworkBridgeTxMessage::SendRequests(requests, _if_disconnected) => {
						for request in requests {
							gum::debug!(target: LOG_TARGET, request = ?request, "Processing request");
							let peer_id =
								request.authority_id().expect("all nodes are authorities").clone();

							if !self.network.is_peer_connected(&peer_id) {
								// Attempting to send a request to a disconnected peer.
								let _ = request.into_response_sender().send(Err(RequestFailure::NotConnected)).expect("send never fails");
								continue
							}
							
							let peer_message =
								NetworkMessage::RequestFromNode(peer_id.clone(), request);
								
							let _ = self.to_network_interface.unbounded_send(peer_message);
						}
					},
					NetworkBridgeTxMessage::ReportPeer(_) => {
						// ingore rep changes
					},
					_ => {
						unimplemented!("Unexpected network bridge message")
					},
				},
			}
		}
	}
}

#[overseer::contextbounds(NetworkBridgeRx, prefix = self::overseer)]
impl MockNetworkBridgeRx {
	async fn run<Context>(mut self, mut ctx: Context) {
		// Main subsystem loop.
		let mut from_network_interface = self.network_receiver.0;
		loop {
			futures::select! {
				maybe_peer_message = from_network_interface.next() => {
					if let Some(message) = maybe_peer_message {
						match message {
							NetworkMessage::MessageFromPeer(message) => match message {
								Versioned::V2(
									polkadot_node_network_protocol::v2::ValidationProtocol::BitfieldDistribution(
										bitfield,
									),
								) => {
									ctx.send_message(
										BitfieldDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(PeerId::random(), polkadot_node_network_protocol::Versioned::V2(bitfield)))
									).await;
								},
								_ => {
									unimplemented!("We only talk v2 network protocol")
								},
							},
							NetworkMessage::RequestFromPeer(request) => {
								if let Some(protocol) = self.chunk_request_sender.as_mut() {
									if let Some(inbound_queue) = protocol.inbound_queue.as_ref() {
										let _ = inbound_queue
											.send(request)
											.await
											.expect("Forwarding requests to subsystem never fails");
									}
								}
							},
							_ => {
								panic!("NetworkMessage::RequestFromNode is not expected to be received from a peer")
							}
						}
					}
				},
				subsystem_message = ctx.recv().fuse() => {
					match subsystem_message.expect("Overseer never fails us") {
						orchestra::FromOrchestra::Signal(signal) => match signal {
							OverseerSignal::Conclude => return,
							_ => {},
						},
						_ => {
							unimplemented!("Unexpected network bridge rx message")
						},
					}
				}
			}
		}
	}
}
