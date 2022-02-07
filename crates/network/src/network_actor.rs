use log::info;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use core::actor::{Actor, ActorContext, ActorId, Event};
use core::cast;
use core::sim::Simulation;

use crate::model::*;

pub const NETWORK_ID: &str = "net";

struct HostInfo {}

pub struct Network {
    network_model: Rc<RefCell<dyn NetworkModel>>,
    hosts: BTreeMap<String, HostInfo>,
    actor_hosts: HashMap<ActorId, String>,

    id_counter: AtomicUsize,
}

impl Network {
    pub fn new(network_model: Rc<RefCell<dyn NetworkModel>>) -> Self {
        Self {
            network_model,
            hosts: BTreeMap::new(),
            actor_hosts: HashMap::new(),
            id_counter: AtomicUsize::new(1),
        }
    }

    pub fn add_host(&mut self, host_id: &str) {
        self.hosts.insert(host_id.to_string(), HostInfo {});
    }

    pub fn set_actor_host(&mut self, actor_id: ActorId, host_id: &str) {
        self.actor_hosts.insert(actor_id, host_id.to_string());
    }

    pub fn get_actor_host(&self, actor_id: &ActorId) -> Option<&String> {
        self.actor_hosts.get(&actor_id)
    }

    pub fn check_same_host(&self, actor1_id: &ActorId, actor2_id: &ActorId) -> bool {
        let actor1_host = self.get_actor_host(&actor1_id);
        let actor2_host = self.get_actor_host(&actor2_id);
        actor1_host.is_some() && actor2_host.is_some() && actor1_host.unwrap() == actor2_host.unwrap()
    }

    pub fn get_hosts(&self) -> Vec<String> {
        self.hosts.keys().cloned().collect()
    }

    pub fn send_msg(&self, message: String, dest: ActorId, ctx: &mut ActorContext) -> usize {
        let msg_id = self.id_counter.fetch_add(1, Ordering::Relaxed);
        let msg = Message {
            id: msg_id,
            src: ctx.id.clone(),
            dest: dest.clone(),
            data: message,
        };
        ctx.emit_now(MessageSend { message: msg }, ActorId::from(NETWORK_ID));

        msg_id
    }

    pub fn send_event<T: Event>(&self, event: T, dest: ActorId, ctx: &mut ActorContext) {
        info!(
            "System time: {}, {} send Event to {}",
            ctx.time(),
            ctx.id.clone(),
            dest.clone()
        );

        if !self.check_same_host(&ctx.id, &dest) {
            ctx.emit(event, dest, self.network_model.borrow().latency());
        } else {
            ctx.emit(event, dest, 0.);
        }
    }

    pub fn send_msg_from_sim(&self, message: String, src: ActorId, dest: ActorId, sim: &mut Simulation) -> usize {
        let msg_id = self.id_counter.fetch_add(1, Ordering::Relaxed);
        let msg = Message {
            id: msg_id,
            src: src.clone(),
            dest: dest.clone(),
            data: message,
        };

        sim.add_event_now(MessageSend { message: msg }, src, ActorId::from(NETWORK_ID));

        msg_id
    }

    pub fn transfer_data(
        &self,
        src: ActorId,
        dest: ActorId,
        size: f64,
        notification_dest: ActorId,
        ctx: &mut ActorContext,
    ) -> usize {
        let data_id = self.id_counter.fetch_add(1, Ordering::Relaxed);

        let data = Data {
            id: data_id,
            src,
            dest,
            size,
            notification_dest,
        };

        ctx.emit_now(DataTransferRequest { data }, ActorId::from(NETWORK_ID));

        data_id
    }

    pub fn transfer_data_from_sim(
        &self,
        src: ActorId,
        dest: ActorId,
        size: f64,
        notification_dest: ActorId,
        sim: &mut Simulation,
    ) -> usize {
        let data_id = self.id_counter.fetch_add(1, Ordering::Relaxed);

        let data = Data {
            id: data_id,
            src,
            dest,
            size,
            notification_dest,
        };

        sim.add_event_now(
            DataTransferRequest { data },
            ActorId::from(NETWORK_ID),
            ActorId::from(NETWORK_ID),
        );

        data_id
    }
}

impl Actor for Network {
    fn on(&mut self, event: Box<dyn Event>, _from: ActorId, ctx: &mut ActorContext) {
        cast!(match event {
            MessageSend { message } => {
                info!(
                    "System time: {}, {} send Message '{}' to {}",
                    ctx.time(),
                    message.src,
                    message.data,
                    message.dest.clone()
                );
                ctx.emit(
                    MessageReceive {
                        message: message.clone(),
                    },
                    ActorId::from(NETWORK_ID),
                    self.network_model.borrow().latency(),
                );
            }
            MessageReceive { message } => {
                info!(
                    "System time: {}, {} received Message '{}' from {}",
                    ctx.time(),
                    message.dest,
                    message.data,
                    message.src
                );
                ctx.emit_now(
                    MessageDelivery {
                        message: message.clone(),
                    },
                    message.dest.clone(),
                );
            }
            DataTransferRequest { data } => {
                info!(
                    "System time: {}, Data ID: {}, From: {}, To {}, Size: {}",
                    ctx.time(),
                    data.id,
                    data.src,
                    data.dest,
                    data.size
                );
                ctx.emit(
                    StartDataTransfer { data: data.clone() },
                    ActorId::from(NETWORK_ID),
                    self.network_model.borrow().latency(),
                );
            }
            StartDataTransfer { data } => {
                self.network_model.borrow_mut().send_data(data.clone(), ctx);
            }
            DataReceive { data } => {
                info!(
                    "System time: {}, Data ID: {}, From: {}, To {}, Size: {}",
                    ctx.time(),
                    data.id,
                    data.src,
                    data.dest,
                    data.size
                );
                self.network_model.borrow_mut().receive_data(data.clone(), ctx);
                ctx.emit_now(
                    DataTransferCompleted { data: data.clone() },
                    data.notification_dest.clone(),
                );
            }
        })
    }

    fn is_active(&self) -> bool {
        true
    }
}
