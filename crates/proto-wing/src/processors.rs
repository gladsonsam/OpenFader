//! WING scene processor rosters, from the pinned libwing 3.0.5 schema.
//! Live DD remains authoritative; mismatched firmware fails closed.
use crate::{
    console_metadata::{encode, readback_agrees, value_of},
    mapping::{root, TOPOLOGY},
    native::{Definition, Kind},
    WingDriver, QUERY_TIMEOUT,
};
use anyhow::{ensure, Context};
use openfader_core::{
    native_library::Check,
    native_processor::{
        self, NativeProcessorBlock as Block, NativeProcessorDomain as Domain,
        NativeProcessorLeaf as Leaf, NativeProcessorSnapshot as Snapshot,
        NativeProcessorValue as PV,
    },
    StripId,
};
use openfader_transport::OscSession;
use rosc::OscType;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{atomic::Ordering, Arc, OnceLock},
};

fn schemas(version: u32) -> anyhow::Result<&'static Value> {
    static V1: OnceLock<Value> = OnceLock::new();
    static V2: OnceLock<Value> = OnceLock::new();
    match version {
        1 => Ok(V1.get_or_init(|| {
            serde_json::from_str(include_str!("schema/processors.json"))
                .expect("embedded WING 3.0 schema")
        })),
        2 => Ok(V2.get_or_init(|| {
            serde_json::from_str(include_str!("schema/processors_31.json"))
                .expect("embedded WING 3.1 schema")
        })),
        _ => anyhow::bail!("unsupported WING processor schema version"),
    }
}
fn domain_name(d: Domain) -> &'static str {
    match d {
        Domain::Gate => "gate",
        Domain::Dynamics => "dyn",
        Domain::Eq => "eq",
    }
}
fn family(key: &str) -> anyhow::Result<(&'static str, StripId)> {
    let i = openfader_core::strip_index_for_key(TOPOLOGY, key).context("unknown WING strip")?;
    let id = StripId(u16::try_from(i)?);
    let f = TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .context("unknown strip family")?;
    ensure!(f.key != "dca", "DCA has no processor");
    Ok((f.key, id))
}
fn rosters(
    version: u32,
    family: &str,
    domain: Domain,
    model: &str,
) -> anyhow::Result<Vec<&'static Vec<Value>>> {
    let entry = schemas(version)?
        .get(format!("{family}/{}", domain_name(domain)))
        .and_then(|v| v.get(model))
        .context("unsupported WING processor model or family")?;
    if let Some(rows) = entry.as_array() {
        return Ok(vec![rows]);
    }
    let variants = entry
        .get("variants")
        .and_then(Value::as_array)
        .context("invalid registered processor variants")?;
    variants
        .iter()
        .map(|v| v.as_array().context("invalid registered processor roster"))
        .collect()
}
fn matching_roster(
    variants: &[&'static Vec<Value>],
    names: &BTreeSet<&str>,
) -> anyhow::Result<&'static Vec<Value>> {
    let mut matches = variants.iter().copied().filter(|rows| {
        rows.iter()
            .filter_map(|v| v["name"].as_str())
            .collect::<BTreeSet<_>>()
            == *names
    });
    let rows = matches.next().context("processor roster not registered")?;
    ensure!(
        matches.next().is_none(),
        "ambiguous registered processor roster"
    );
    Ok(rows)
}
fn static_valid(row: &Value, v: &PV) -> bool {
    let kind = row["type"].as_str().unwrap_or("");
    let number = match v {
        PV::Number(n) => Some(*n),
        PV::Integer(i) => Some(*i as f64),
        PV::Bool(b) => Some(if *b { 1. } else { 0. }),
        _ => None,
    };
    match kind {
        "integer" => {
            (if row["minint"] == 0 && row["maxint"] == 1 {
                matches!(v, PV::Bool(_))
            } else {
                matches!(v, PV::Integer(_))
            }) && number.is_some_and(|n| {
                n.fract() == 0.
                    && row["minint"].as_f64().is_some_and(|x| n >= x)
                    && row["maxint"].as_f64().is_some_and(|x| n <= x)
            })
        }
        "linear float" | "log float" => {
            matches!(v, PV::Number(_))
                && number.is_some_and(|n| {
                    n.is_finite()
                        && row["minfloat"]
                            .as_f64()
                            .zip(row["maxfloat"].as_f64())
                            .is_some_and(|(a, b)| {
                                // Documented decimal endpoints may round to
                                // adjacent float32 values in native DD.
                                let lo = a.min(b);
                                let hi = a.max(b);
                                let low_allowance = 2. * f64::from(f32::EPSILON) * lo.abs().max(1.);
                                let high_allowance =
                                    4. * f64::from(f32::EPSILON) * hi.abs().max(1.);
                                n >= lo - low_allowance && n <= hi + high_allowance
                            })
                })
        }
        "string enum" => {
            matches!(v,PV::Enum(s) if row["items"].as_array().is_some_and(|items|items.iter().any(|i|i["item"].as_str()==Some(s))))
                || matches!(v,PV::Number(n) if row["float_items"].as_array().is_some_and(|items|items.iter().any(|i|i.as_f64().is_some_and(|choice| choice as f32 == *n as f32))))
        }
        "float enum" => {
            matches!(v,PV::Number(n) if row["items"].as_array().is_some_and(|items|items.iter().any(|i|i["item"].as_f64().is_some_and(|x|(x-n).abs()<0.0001))))
        }
        _ => false,
    }
}
fn scalar(v: &PV) -> Value {
    match v {
        PV::Bool(x) => json!(x),
        PV::Integer(x) => json!(x),
        PV::Number(x) => json!(x),
        PV::Text(x) | PV::Enum(x) => json!(x),
    }
}
fn typed(def: &Definition, value: Value) -> anyhow::Result<PV> {
    Ok(match &def.kind {
        Kind::Integer { min: 0, max: 1 } => PV::Bool(value.as_bool().context("invalid bool")?),
        Kind::Integer { .. } => PV::Integer(value.as_i64().context("invalid integer")?),
        Kind::Linear { .. } | Kind::Log { .. } | Kind::FloatEnum(_) | Kind::Fader => {
            PV::Number(value.as_f64().context("invalid number")?)
        }
        Kind::Enum(_) => PV::Enum(value.as_str().context("invalid enum")?.into()),
        Kind::Text { .. } => PV::Text(value.as_str().context("invalid text")?.into()),
        Kind::Node => anyhow::bail!("unexpected processor node"),
    })
}
/// Validate portable WING processor blocks without connecting to a mixer.
pub fn validate_native_processor_snapshot(snapshot: &Snapshot) -> anyhow::Result<()> {
    native_processor::validate_structure(snapshot)?;
    ensure!(
        snapshot.provider == "wing" && matches!(snapshot.version, 1 | 2),
        "unsupported WING processor snapshot"
    );
    for b in &snapshot.blocks {
        let (family, _) = family(&b.strip_key)?;
        let variants = rosters(snapshot.version, family, b.domain, &b.model)?;
        let values: BTreeMap<&str, &PV> =
            b.leaves.iter().map(|v| (v.id.as_str(), &v.value)).collect();
        let names: BTreeSet<&str> = values.keys().copied().collect();
        let expected = matching_roster(&variants, &names)?;
        ensure!(
            values.len() == expected.len(),
            "incomplete WING processor roster"
        );
        for row in expected {
            let name = row["name"].as_str().context("invalid schema")?;
            let v = values.get(name).context("missing WING processor leaf")?;
            ensure!(
                static_valid(row, v),
                "invalid WING processor value for {name}: value {v:?}, registered {row}"
            );
        }
    }
    Ok(())
}
/// Offline archive/capture validation: no applicable strip/domain may be omitted.
pub fn validate_complete_native_processor_snapshot(snapshot: &Snapshot) -> anyhow::Result<()> {
    validate_native_processor_snapshot(snapshot)?;
    let mut required = BTreeSet::new();
    for f in TOPOLOGY {
        let domains: &[Domain] = match f.key {
            "ch" => &[Domain::Gate, Domain::Dynamics, Domain::Eq],
            "aux" if snapshot.version == 2 => &[Domain::Dynamics, Domain::Eq],
            "aux" => &[Domain::Eq],
            "bus" | "main" | "mtx" => &[Domain::Dynamics, Domain::Eq],
            _ => &[],
        };
        for i in f.first..f.first + f.count {
            let key = openfader_core::strip_key_for_index(TOPOLOGY, i)
                .context("unknown WING strip key")?;
            for &domain in domains {
                required.insert((key.clone(), domain));
            }
        }
    }
    let actual: BTreeSet<_> = snapshot
        .blocks
        .iter()
        .map(|b| (b.strip_key.clone(), b.domain))
        .collect();
    ensure!(actual == required, "incomplete WING processor snapshot");
    Ok(())
}

impl WingDriver {
    fn processor_schema_version(&self) -> anyhow::Result<u32> {
        let link = self.link.read().unwrap();
        let firmware = &link.as_ref().context("WING not connected")?.info.firmware;
        let matches_series = |prefix: &str| {
            firmware
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(['.', '-', '+', ' ']))
        };
        if matches_series("3.0") {
            Ok(1)
        } else if matches_series("3.1") {
            Ok(2)
        } else {
            anyhow::bail!("WING processor schema unavailable for firmware {firmware}")
        }
    }
    async fn model_on(&self, session: &Arc<OscSession>, base: &str) -> anyhow::Result<String> {
        let m = session
            .query_timeout(&format!("{base}/mdl"), QUERY_TIMEOUT)
            .await?;
        self.ensure_current(session)?;
        let [OscType::String(s)] = m.args.as_slice() else {
            anyhow::bail!("invalid WING processor model")
        };
        Ok(s.clone())
    }
    async fn defs_for(
        &self,
        session: &Arc<OscSession>,
        base: &str,
        version: u32,
        family: &str,
        d: Domain,
        model: &str,
    ) -> anyhow::Result<Vec<Definition>> {
        let defs = self.native_defs(session, base).await?;
        let variants = rosters(version, family, d, model)?;
        let mut actual_names = BTreeSet::new();
        for def in &defs {
            let name = def.address_name()?;
            if !def.read_only
                && !name.starts_with('$')
                && name != "mdl"
                && !matches!(def.kind, Kind::Node)
            {
                actual_names.insert(name);
            }
        }
        let actual: BTreeSet<&str> = actual_names.iter().map(String::as_str).collect();
        let expected = matching_roster(&variants, &actual).map_err(|error| anyhow::anyhow!(
            "WING: {base} model {model} live roster differs from registered schema: {error:#}; live definitions {:?}",
            defs.iter().map(|def| (&def.name, &def.kind, def.read_only)).collect::<Vec<_>>()
        ))?;
        if version == 2 && model == "WARM" {
            // 3.1 distinguishes the processor's position-5 MIX from the
            // global mix. Only accept the independently corroborated native
            // identity and its documented domain; never alias by name alone.
            let mix = defs
                .iter()
                .find(|def| def.address_name().is_ok_and(|name| name == "wmix"))
                .context("WARM native mix unavailable")?;
            ensure!(
                mix.index == 5
                    && mix.label == "MIX"
                    && mix.unit == 2
                    && !mix.read_only
                    && matches!(mix.kind, Kind::Linear { min, max, steps } if min == 0. && max == 100. && steps == 101),
                "WING: {base}/wmix does not match registered WARM mix identity"
            );
            static IDS: OnceLock<Value> = OnceLock::new();
            let ids = IDS.get_or_init(|| {
                serde_json::from_str(include_str!("schema/warm_position5_ids.json"))
                    .expect("embedded WING position identities")
            });
            if let Some(expected_hash) = ids.get(base).and_then(Value::as_i64) {
                ensure!(
                    i64::from(mix.hash) == expected_hash,
                    "WING: {base}/wmix native identity changed"
                );
            } else {
                // Aux dynamics did not exist in the 3.0.5 source: its live
                // model-scoped index, label and domain are the only available
                // identity facts, with no fabricated family hash.
                ensure!(
                    family == "aux" && d == Domain::Dynamics,
                    "WING: no registered position identity for {base}"
                );
            }
        }
        for row in expected
            .iter()
            .filter(|row| row.get("float_items").is_some())
        {
            let name = row["name"].as_str().context("invalid registered choice")?;
            let def = defs
                .iter()
                .find(|def| def.address_name().is_ok_and(|child| child == name))
                .context("missing live choice")?;
            let compatible = match &def.kind {
                Kind::FloatEnum(live) => {
                    let options = row["float_items"]
                        .as_array()
                        .context("invalid registered numeric choices")?;
                    live.len() == options.len()
                        && live.iter().all(|(actual, _)| {
                            options
                                .iter()
                                .any(|v| v.as_f64().is_some_and(|v| v as f32 == *actual))
                        })
                }
                Kind::Enum(live) => {
                    let options = row["items"]
                        .as_array()
                        .context("invalid registered string choices")?;
                    live.len() == options.len()
                        && live.iter().all(|(actual, _)| {
                            options.iter().any(|v| v["item"].as_str() == Some(actual))
                        })
                }
                _ => false,
            };
            ensure!(
                compatible,
                "WING: {base}/{name} live choice type or options differ from registered schema"
            );
        }
        let wanted: BTreeSet<&str> = expected.iter().filter_map(|v| v["name"].as_str()).collect();
        Ok(defs
            .into_iter()
            .filter(|d| {
                d.address_name()
                    .is_ok_and(|name| wanted.contains(name.as_str()))
            })
            .collect())
    }
    async fn read_block(
        &self,
        session: &Arc<OscSession>,
        stamp: (u64, u64, u32),
        key: &str,
        d: Domain,
        check: Check<'_>,
    ) -> anyhow::Result<Block> {
        let (generation, native_revision, version) = stamp;
        let (fam, id) = family(key)?;
        let base = format!("{}/{}", root(id)?.0, domain_name(d));
        let model = self.model_on(session, &base).await?;
        let defs = self
            .defs_for(session, &base, version, fam, d, &model)
            .await?;
        let mut leaves = Vec::new();
        for def in &defs {
            ensure!(check(), "scene session or access changed");
            let name = def.address_name()?;
            let path = format!("{base}/{name}");
            let msg = session.query_timeout(&path, QUERY_TIMEOUT).await?;
            let value = typed(def, value_of(def, &msg)?)?;
            leaves.push(Leaf { id: name, value });
        }
        ensure!(
            self.model_on(session, &base).await? == model,
            "processor model changed during capture"
        );
        ensure!(
            self.defs_for(session, &base, version, fam, d, &model)
                .await?
                == defs,
            "processor definitions changed during capture"
        );
        ensure!(
            check()
                && self.generation.load(Ordering::Acquire) == generation
                && self.native_revision.load(Ordering::Acquire) == native_revision,
            "session or native state changed during capture"
        );
        let block = Block {
            strip_key: key.into(),
            domain: d,
            model,
            leaves,
        };
        validate_native_processor_snapshot(&Snapshot {
            provider: "wing".into(),
            version,
            blocks: vec![block.clone()],
        })?;
        Ok(block)
    }
    pub(crate) async fn capture_processors(&self, check: Check<'_>) -> anyhow::Result<Snapshot> {
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let rev = self.native_revision.load(Ordering::Acquire);
        let version = self.processor_schema_version()?;
        let mut blocks = Vec::new();
        for (i, f) in TOPOLOGY
            .iter()
            .flat_map(|f| (f.first..f.first + f.count).map(move |i| (i, f)))
        {
            let key =
                openfader_core::strip_key_for_index(TOPOLOGY, i).context("unknown strip key")?;
            let domains: &[Domain] = match f.key {
                "ch" => &[Domain::Gate, Domain::Dynamics, Domain::Eq],
                "aux" if version == 2 => &[Domain::Dynamics, Domain::Eq],
                "aux" => &[Domain::Eq],
                "bus" | "main" | "mtx" => &[Domain::Dynamics, Domain::Eq],
                _ => &[],
            };
            for &d in domains {
                blocks.push(
                    self.read_block(&session, (generation, rev, version), &key, d, check)
                        .await?,
                );
            }
        }
        let snapshot = Snapshot {
            provider: "wing".into(),
            version,
            blocks,
        };
        validate_complete_native_processor_snapshot(&snapshot)?;
        Ok(snapshot)
    }
    pub(crate) async fn revalidate_processors(
        &self,
        snapshot: &Snapshot,
        check: Check<'_>,
    ) -> anyhow::Result<()> {
        validate_native_processor_snapshot(snapshot)?;
        ensure!(
            self.processor_schema_version()? == snapshot.version,
            "WING processor schema version differs from connected firmware"
        );
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let rev = self.native_revision.load(Ordering::Acquire);
        for expected in &snapshot.blocks {
            let actual = self
                .read_block(
                    &session,
                    (generation, rev, snapshot.version),
                    &expected.strip_key,
                    expected.domain,
                    check,
                )
                .await?;
            let by_id = |b: &Block| {
                b.leaves
                    .iter()
                    .map(|l| (l.id.clone(), l.value.clone()))
                    .collect::<BTreeMap<_, _>>()
            };
            ensure!(
                actual.model == expected.model && by_id(&actual) == by_id(expected),
                "WING processor changed during scene capture"
            );
        }
        Ok(())
    }
    pub(crate) async fn preflight_processors(
        &self,
        snapshot: &Snapshot,
        check: Check<'_>,
    ) -> anyhow::Result<()> {
        validate_native_processor_snapshot(snapshot)?;
        ensure!(
            self.processor_schema_version()? == snapshot.version,
            "WING processor schema version differs from connected firmware"
        );
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        for b in &snapshot.blocks {
            ensure!(check(), "scene session or access changed");
            let (fam, id) = family(&b.strip_key)?;
            let base = format!("{}/{}", root(id)?.0, domain_name(b.domain));
            let model = self.model_on(&session, &base).await?;
            // Selector DD proves the target model is offered, even when its
            // model-specific definitions are not exposed until after selection.
            let defs = self.native_defs(&session, &base).await?;
            let sel = defs
                .iter()
                .find(|d| d.address_name().is_ok_and(|child| child == "mdl"))
                .context("processor selector unavailable")?;
            ensure!(
                !sel.read_only
                    && matches!(&sel.kind,Kind::Enum(options) if options.iter().any(|(v,_)|v==&b.model)),
                "WING processor model unavailable"
            );
            if model == b.model {
                let defs = self
                    .defs_for(&session, &base, snapshot.version, fam, b.domain, &model)
                    .await?;
                for leaf in &b.leaves {
                    let def = defs
                        .iter()
                        .find(|d| d.address_name().is_ok_and(|child| child == leaf.id))
                        .context("missing live processor leaf")?;
                    encode(def, &scalar(&leaf.value))?;
                }
            }
            ensure!(
                self.model_on(&session, &base).await? == model,
                "processor model changed during preflight"
            );
            ensure!(
                self.generation.load(Ordering::Acquire) == generation,
                "WING session changed during preflight"
            );
        }
        Ok(())
    }
    pub(crate) async fn recall_processors(
        &self,
        snapshot: &Snapshot,
        check: Check<'_>,
    ) -> anyhow::Result<()> {
        self.preflight_processors(snapshot, check).await?;
        let session = self.session()?;
        let generation = self.generation.load(Ordering::Acquire);
        let _guard = self.writes.lock().await;
        let mut attempted = false;
        let result = async {
            for b in &snapshot.blocks {
                ensure!(check(), "scene session or access changed");
                self.ensure_current(&session)?;
                let (fam, id) = family(&b.strip_key)?;
                let base = format!("{}/{}", root(id)?.0, domain_name(b.domain));
                if self.model_on(&session, &base).await? != b.model {
                    if !attempted {
                        let mut cache = self.color_domains.lock().unwrap();
                        self.native_revision.fetch_add(1, Ordering::AcqRel);
                        cache.clear();
                    }
                    ensure!(
                        check() && self.generation.load(Ordering::Acquire) == generation,
                        "scene session or access changed before processor model write"
                    );
                    self.ensure_current(&session)?;
                    attempted = true;
                    session
                        .send(
                            &format!("{base}/mdl"),
                            vec![OscType::String(b.model.clone())],
                        )
                        .await?;
                    ensure!(
                        self.model_on(&session, &base).await? == b.model,
                        "processor model readback mismatch"
                    );
                }
                let defs = self
                    .defs_for(&session, &base, snapshot.version, fam, b.domain, &b.model)
                    .await?;
                // Validate every leaf against the selected model's live DD before
                // sending any of this block's parameter writes.
                let mut writes = Vec::new();
                for leaf in &b.leaves {
                    let def = defs
                        .iter()
                        .find(|d| d.address_name().is_ok_and(|child| child == leaf.id))
                        .context("missing live processor leaf")?;
                    writes.push((def, encode(def, &scalar(&leaf.value))?, scalar(&leaf.value)));
                }
                for (def, arg, expected) in writes {
                    ensure!(
                        check() && self.generation.load(Ordering::Acquire) == generation,
                        "scene session or access changed"
                    );
                    ensure!(
                        self.model_on(&session, &base).await? == b.model,
                        "processor model changed"
                    );
                    if !attempted {
                        let mut cache = self.color_domains.lock().unwrap();
                        self.native_revision.fetch_add(1, Ordering::AcqRel);
                        cache.clear();
                    }
                    let path = format!("{base}/{}", def.address_name()?);
                    ensure!(
                        check() && self.generation.load(Ordering::Acquire) == generation,
                        "scene session or access changed before processor parameter write"
                    );
                    self.ensure_current(&session)?;
                    attempted = true;
                    session.send(&path, vec![arg]).await?;
                    let msg = session.query_timeout(&path, QUERY_TIMEOUT).await?;
                    let actual = value_of(def, &msg)?;
                    ensure!(
                        check() && self.generation.load(Ordering::Acquire) == generation,
                        "scene session or access changed during processor readback"
                    );
                    ensure!(
                        self.model_on(&session, &base).await? == b.model,
                        "processor model changed during readback"
                    );
                    let (same, _) = readback_agrees(def, &expected, &actual)?;
                    ensure!(same, "WING processor readback mismatch at {path}");
                }
            }
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if attempted {
            let mut cache = self.color_domains.lock().unwrap();
            self.native_revision.fetch_add(1, Ordering::AcqRel);
            cache.clear();
        }
        result.map_err(|e| {
            if attempted {
                anyhow::anyhow!("WING processor recall may be partial: {e:#}")
            } else {
                e
            }
        })
    }
}
