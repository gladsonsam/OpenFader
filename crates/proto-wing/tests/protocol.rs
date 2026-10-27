//! Behavioral protocol checks based on the manual's wire examples. No hardware
//! assumptions are promoted to hardware verification by these loopback checks.
use openfader_core::{MixerDriver, ParamValue, StripId};
use openfader_proto_wing::{
    fx,
    mapping::{self, Mapping, WireKind},
    WingDriver,
};
use openfader_transport::{discovery::parse_info_reply, osc};
use rosc::OscType as O;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UdpSocket, task::JoinHandle};

#[path = "protocol/fixture.rs"]
mod fixture;
use fixture::Desk;
#[path = "protocol/basics.rs"]
mod basics;
#[path = "protocol/filters.rs"]
mod filters;
#[path = "protocol/fx.rs"]
mod fx_checks;
#[path = "protocol/groups.rs"]
mod groups;
#[path = "protocol/routing.rs"]
mod routing;
#[path = "protocol/sends.rs"]
mod sends;
#[path = "protocol/source.rs"]
mod source;
