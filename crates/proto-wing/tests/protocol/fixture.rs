//! Loopback OSC desk shared by protocol feature checks.
use super::*;

pub(crate) struct Desk {
    pub(crate) addr: String,
    pub(crate) values: Arc<Mutex<HashMap<String, O>>>,
    pub(crate) writes: Arc<Mutex<Vec<(String, O)>>>,
    pub(crate) ignored_writes: Arc<Mutex<HashSet<String>>>,
    pub(crate) definitions: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub(crate) reroute_on_definition: Arc<Mutex<Option<(String, O)>>>,
    task: JoinHandle<()>,
}
impl Drop for Desk {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Desk {
    pub(crate) async fn new() -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap().to_string();
        let values = Arc::new(Mutex::new(HashMap::<String, O>::from([
            (
                "/?".into(),
                O::String("WING,127.0.0.1,My WING,wing-compact,TEST,3.0.5".into()),
            ),
            ("/ch/1/fdr".into(), O::Float(-2.)),
            ("/ch/1/mute".into(), O::Int(0)),
            ("/ch/1/eq/mdl".into(), O::String("STD".into())),
            ("/ch/1/eq/1g".into(), O::Float(3.)),
            ("/ch/1/in/set/altsrc".into(), O::Int(0)),
            ("/ch/1/in/set/srcauto".into(), O::Int(0)),
            ("/ch/1/in/conn/grp".into(), O::String("LCL".into())),
            ("/ch/1/in/conn/in".into(), O::Int(3)),
            ("/ch/1/in/conn/altgrp".into(), O::String("USB".into())),
            ("/ch/1/in/conn/altin".into(), O::Int(12)),
            ("/io/in/LCL/3/g".into(), O::Float(12.)),
            ("/io/in/LCL/3/vph".into(), O::Int(0)),
            ("/fx/1/mdl".into(), O::String("HALL".into())),
            ("/fx/1/dcy".into(), O::Float(1.)),
        ])));
        let writes = Arc::new(Mutex::new(Vec::new()));
        let state = values.clone();
        let changes = writes.clone();
        let ignored_writes = Arc::new(Mutex::new(HashSet::<String>::new()));
        let ignored = ignored_writes.clone();
        let definitions = Arc::new(Mutex::new(HashMap::<String, Vec<u8>>::new()));
        let defs = definitions.clone();
        let reroute_on_definition = Arc::new(Mutex::new(None::<(String, O)>));
        let reroute = reroute_on_definition.clone();
        let task = tokio::spawn(async move {
            let mut data = [0u8; 4096];
            loop {
                let (n, peer) = socket.recv_from(&mut data).await.unwrap();
                let Ok(packet) = osc::decode_packet(&data[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    if msg.args == vec![O::Blob(vec![0xdd])] {
                        if let Some((path, value)) = reroute.lock().unwrap().take() {
                            state.lock().unwrap().insert(path, value);
                        }
                        let data = defs
                            .lock()
                            .unwrap()
                            .get(&msg.addr)
                            .cloned()
                            .unwrap_or_else(|| vec![0xde]);
                        let bytes = osc::encode_message(&msg.addr, vec![O::Blob(data)]).unwrap();
                        socket.send_to(&bytes, peer).await.unwrap();
                        continue;
                    }
                    if msg.args.is_empty() {
                        let value = state.lock().unwrap().get(&msg.addr).cloned();
                        let Some(value) = value else { continue };
                        let args = match value {
                            O::Float(v) => {
                                vec![O::String(v.to_string()), O::Float(0.123), O::Float(v)]
                            }
                            O::Int(v) => vec![O::String(v.to_string()), O::Float(0.456), O::Int(v)],
                            v => vec![v],
                        };
                        let bytes = osc::encode_message(&msg.addr, args).unwrap();
                        socket.send_to(&bytes, peer).await.unwrap();
                    } else if let [value] = msg.args.as_slice() {
                        changes
                            .lock()
                            .unwrap()
                            .push((msg.addr.clone(), value.clone()));
                        if ignored.lock().unwrap().contains(&msg.addr) {
                            continue;
                        }
                        if msg.addr.ends_with("/in/conn") {
                            let O::String(node) = value else {
                                panic!("node must be a string")
                            };
                            let mut state = state.lock().unwrap();
                            for field in node.split(',') {
                                let (key, value) = field.split_once('=').unwrap();
                                let value = if key.ends_with("grp") {
                                    O::String(value.into())
                                } else {
                                    O::Int(value.parse().unwrap())
                                };
                                state.insert(format!("{}/{key}", msg.addr), value);
                            }
                        } else {
                            state
                                .lock()
                                .unwrap()
                                .insert(msg.addr.clone(), value.clone());
                        }
                        // WING does not echo leaf writes; a read is needed.
                    }
                }
            }
        });
        Self {
            addr,
            values,
            writes,
            ignored_writes,
            definitions,
            reroute_on_definition,
            task,
        }
    }
}
