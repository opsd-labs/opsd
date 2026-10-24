use super::*;
use axum::{Json, extract::State as S};
use opsd::network::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct Link {
    pub channel: (String, mpsc::Sender<Frame>),
    pub seen: i64,
    pub connected_at: i64,
    pub rtt_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct NodeChannels {
    paths: HashMap<String, BTreeMap<PathKind, Link>>,
}

impl NodeChannels {
    pub fn get(&self, node: &str) -> Option<&(String, mpsc::Sender<Frame>)> {
        let paths = self.paths.get(node)?;
        let alive =
            |l: &&Link| l.error.is_none() && l.seen > now() - 45 && !l.channel.1.is_closed();
        let public = paths.get(&PathKind::Public).filter(alive);
        let overlay = paths.get(&PathKind::Overlay).filter(alive);
        if let Some(link) = public
            && (overlay.is_none() || now() - link.connected_at >= 15)
        {
            return Some(&link.channel);
        }
        overlay.or(public).map(|l| &l.channel)
    }
    pub fn contains_key(&self, node: &str) -> bool {
        self.get(node).is_some()
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.paths.keys().filter(|node| self.contains_key(node))
    }
    pub fn remove(&mut self, node: &str) {
        self.paths.remove(node);
    }
    pub fn insert(&mut self, node: String, path: PathKind, channel: (String, mpsc::Sender<Frame>)) {
        self.paths.entry(node).or_default().insert(
            path,
            Link {
                channel,
                seen: now(),
                connected_at: now(),
                rtt_ms: None,
                error: None,
            },
        );
    }
    pub fn current(&self, node: &str, path: PathKind, generation: &str) -> bool {
        self.paths
            .get(node)
            .and_then(|p| p.get(&path))
            .is_some_and(|link| link.channel.0 == generation)
    }
    pub fn touch(&mut self, node: &str, path: PathKind, generation: &str, rtt: Option<u64>) {
        if let Some(link) = self.paths.get_mut(node).and_then(|p| p.get_mut(&path))
            && link.channel.0 == generation
        {
            link.seen = now();
            if rtt.is_some() {
                link.rtt_ms = rtt;
            }
        }
    }
    pub fn disconnect(&mut self, node: &str, path: PathKind, generation: &str) {
        if self.current(node, path, generation) {
            self.paths
                .get_mut(node)
                .unwrap()
                .get_mut(&path)
                .unwrap()
                .error = Some("控制连接已断开".into());
        }
    }
    pub fn status(&self, node: &str) -> Value {
        let paths: Vec<_> = [PathKind::Public, PathKind::Overlay].into_iter().map(|path| {
            let link = self.paths.get(node).and_then(|p| p.get(&path));
            json!({"path":path,"connected":link.is_some_and(|l| l.seen > now()-45 && !l.channel.1.is_closed()),
                "last_seen":link.map(|l|l.seen),"connected_at":link.map(|l|l.connected_at),
                "rtt_ms":link.and_then(|l|l.rtt_ms),"error":link.and_then(|l|l.error.clone())})
        }).collect();
        let active = self.get(node).and_then(|channel| {
            self.paths
                .get(node)?
                .iter()
                .find(|(_, l)| l.channel.0 == channel.0)
                .map(|(p, _)| *p)
        });
        json!({"active_path":active,"paths":paths})
    }
}

pub async fn directory(s: &State) -> Result<Directory> {
    Ok(s.db.get("network", "directory").await?.unwrap_or_default())
}

pub async fn read(S(s): S<State>) -> ApiResult<Json<Value>> {
    let directory = directory(&s).await?;
    let nodes = s.db.list::<Node>("nodes").await?;
    let mut states = Vec::new();
    for node in nodes.iter().filter(|n| !n.revoked) {
        states.push(json!({"node_id":node.id,
            "connections":s.channels.read().await.status(&node.id),
            "facts":s.db.get::<NetworkFacts>("network_facts",&node.id).await?,
            "desired_revision":s.db.get::<Allowlist>("network_desired",&node.id).await?.map(|set|set.revision),
            "addresses":{"agent":s.db.get::<Value>("network_observations",&node.id).await?,"plugin":s.db.get::<Value>("network_plugin_observations",&node.id).await?,"retiring":s.db.get::<Vec<String>>("network_retiring",&node.id).await?.unwrap_or_default()},
            "applied":s.db.get::<AppliedNetwork>("network_applied",&node.id).await?}));
    }
    Ok(Json(json!({"directory":directory,"nodes":states})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerSelection {
    pub node_id: Option<String>,
}
pub async fn controller(
    S(s): S<State>,
    Json(input): Json<ControllerSelection>,
) -> ApiResult<Json<Directory>> {
    let guard = s.gate.lock().await;
    if let Some(id) = &input.node_id {
        check(
            s.db.get::<Node>("nodes", id)
                .await?
                .is_some_and(|n| !n.revoked),
            "控制机必须是已接入节点",
        )?;
    }
    let mut directory = directory(&s).await?;
    if directory.controller_node_id != input.node_id {
        directory.controller_node_id = input.node_id;
        directory.revision += 1;
        s.db.put("network", "directory", &directory).await?;
    }
    drop(guard);
    synchronize(&s).await?;
    Ok(Json(directory))
}

pub async fn update(
    S(s): S<State>,
    Json(mut input): Json<Directory>,
) -> ApiResult<Json<Directory>> {
    let _guard = s.gate.lock().await;
    let nodes = s.db.list::<Node>("nodes").await?;
    let ids: std::collections::HashSet<_> = nodes
        .iter()
        .filter(|n| !n.revoked)
        .map(|n| n.id.as_str())
        .collect();
    if let Some(controller) = &input.controller_node_id {
        check(
            ids.contains(controller.as_str()),
            "控制机必须是已接入的节点",
        )?;
    }
    let mut seen = std::collections::HashSet::new();
    for overlay in &input.overlays {
        check(
            ids.contains(overlay.node_id.as_str()) && seen.insert(&overlay.node_id),
            "ET 节点不存在或重复",
        )?;
        let address: std::net::IpAddr = overlay.address.parse().map_err(|_| bad("ET 地址无效"))?;
        ipnet::IpNet::new(address, overlay.prefix).map_err(|_| bad("ET 网段无效"))?;
        check(
            !overlay.interface.is_empty()
                && overlay.interface.len() <= 15
                && overlay
                    .interface
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b)),
            "ET 接口名无效",
        )?;
        check(overlay.provider == "et", "ET 地址必须来自 ET 插件")?;
        let facts =
            s.db.get::<NetworkFacts>("network_facts", &overlay.node_id)
                .await?
                .ok_or_else(|| bad("节点尚未上报本机地址"))?;
        check(
            facts
                .interfaces
                .iter()
                .any(|i| i.address == overlay.address && i.interface == overlay.interface),
            "ET 地址与节点实际接口不匹配，不能关联",
        )?;
    }
    let old = directory(&s).await?;
    check(
        input.revision == old.revision,
        "网络目录已变化，请重新读取后提交",
    )?;
    if old != input {
        input.revision += 1;
        s.db.put("network", "directory", &input).await?;
    }
    for mut node in nodes {
        let address = input
            .overlays
            .iter()
            .find(|m| m.node_id == node.id)
            .map(|m| m.address.clone());
        if node.overlay_address != address {
            node.overlay_address = address;
            s.db.put("nodes", &node.id, &node).await?;
        }
    }
    drop(_guard);
    synchronize(&s).await?;
    Ok(Json(input))
}

pub async fn observe(
    s: &State,
    node_id: &str,
    source: Option<std::net::IpAddr>,
    facts: Option<NetworkFacts>,
) -> Result<()> {
    let _guard = s.gate.lock().await;
    let Some(mut node) = s.db.get::<Node>("nodes", node_id).await? else {
        return Ok(());
    };
    anyhow::ensure!(!node.revoked, "节点已撤销");
    let previous_addresses = node.public_addresses.clone();
    if let Some(facts) = facts.filter(|f| f.error.is_none()) {
        anyhow::ensure!(facts.interfaces.len() <= 256, "网卡地址数量超过限制");
        for address in &facts.interfaces {
            let _: std::net::IpAddr = address.address.parse()?;
        }
        let local: Vec<_> = facts
            .interfaces
            .iter()
            .filter_map(|i| i.address.parse().ok())
            .filter(|a| public_ip(*a))
            .collect();
        // 本机事实更新不清除经认证连接观察到的 NAT 出口。
        let observed: Option<String> = s.db.get("network_sources", node_id).await?;
        node.public_addresses = local
            .into_iter()
            .map(|a| a.to_string())
            .chain(observed)
            .collect();
        s.db.put("network_facts", node_id, &facts).await?;
    }
    if let Some(source) = source.filter(|ip| public_ip(*ip)) {
        let address = source.to_string();
        if let Some(old) = s.db.get::<String>("network_sources", node_id).await? {
            node.public_addresses.retain(|a| a != &old);
            if old != address {
                s.db.put("network_retirement_ready", node_id, &true).await?;
            }
        }
        node.public_addresses.push(address.clone());
        s.db.put("network_sources", node_id, &address).await?;
        s.db.put("network_source_times", node_id, &now()).await?;
    }
    node.public_addresses.sort();
    node.public_addresses.dedup();
    let mut retiring =
        s.db.get::<Vec<String>>("network_retiring", node_id)
            .await?
            .unwrap_or_default();
    retiring.extend(
        previous_addresses
            .into_iter()
            .filter(|address| !node.public_addresses.contains(address)),
    );
    retiring.retain(|address| !node.public_addresses.contains(address));
    retiring.sort();
    retiring.dedup();
    s.db.put("network_retiring", node_id, &retiring).await?;
    let facts = s.db.get::<NetworkFacts>("network_facts", node_id).await?;
    let source = s.db.get::<String>("network_sources", node_id).await?;
    let source_at = s.db.get::<i64>("network_source_times", node_id).await?;
    let observations = json!({"local":facts.as_ref().map(|f|json!({"source":"agent_interface","at":f.collected_at,"addresses":f.interfaces})),
        "public_egress":source.map(|address|json!({"address":address,"source":"authenticated_connection","at":source_at,"inbound_verified":false}))});
    s.db.put("network_observations", node_id, &observations)
        .await?;
    s.db.put("nodes", node_id, &node).await?;
    Ok(())
}

pub async fn desired(s: &State, node_id: &str) -> Result<DesiredNetwork> {
    let _guard = s.network_gate.lock().await;
    let directory = directory(s).await?;
    let nodes = s.db.list::<Node>("nodes").await?;
    let mut public_addresses: Vec<_> = nodes
        .iter()
        .filter(|n| !n.revoked)
        .flat_map(|n| n.public_addresses.clone())
        .filter(|a| a.parse().is_ok_and(public_ip))
        .collect();
    for node in nodes.iter().filter(|n| !n.revoked) {
        public_addresses.extend(
            s.db.get::<Vec<String>>("network_retiring", &node.id)
                .await?
                .unwrap_or_default(),
        );
        public_addresses.extend(
            s.db.get::<Vec<String>>("network_plugin_addresses", &node.id)
                .await?
                .unwrap_or_default(),
        );
    }
    public_addresses.sort();
    public_addresses.dedup();
    let membership = directory.overlays.iter().find(|m| m.node_id == node_id);
    let overlay = membership
        .map(|m| {
            Ok::<_, anyhow::Error>(OverlayTrust {
                interface: m.interface.clone(),
                network: ipnet::IpNet::new(m.address.parse()?, m.prefix)?
                    .trunc()
                    .to_string(),
            })
        })
        .transpose()?;
    let mut allowlist = Allowlist {
        revision: 0,
        public_addresses,
        overlay,
    };
    let previous =
        s.db.get::<Allowlist>("network_desired", node_id)
            .await?
            .unwrap_or_default();
    allowlist.revision = previous.revision;
    if allowlist != previous {
        allowlist.revision += 1;
        s.db.put("network_desired", node_id, &allowlist).await?;
    }
    let mut targets = Vec::new();
    for node in nodes.iter().filter(|n| !n.revoked && n.id != node_id) {
        // NAT 出口只参与来源白名单，不当作该节点的入站服务地址。
        if let Some(facts) = s.db.get::<NetworkFacts>("network_facts", &node.id).await? {
            for address in facts
                .interfaces
                .iter()
                .map(|i| &i.address)
                .filter(|a| a.parse().is_ok_and(public_ip))
            {
                targets.push(ProbeTarget {
                    path: PathKind::Public,
                    node_id: node.id.clone(),
                    address: address.clone(),
                    port: None,
                });
            }
        }
        if let Some(m) = directory.overlays.iter().find(|m| m.node_id == node.id) {
            targets.push(ProbeTarget {
                path: PathKind::Overlay,
                node_id: node.id.clone(),
                address: m.address.clone(),
                port: None,
            });
        }
    }
    let hub_overlay_address = directory
        .controller_node_id
        .as_ref()
        .and_then(|id| directory.overlays.iter().find(|m| &m.node_id == id))
        .map(|m| m.address.clone());
    let previous =
        s.db.get::<DesiredNetwork>("network_frames", node_id)
            .await?;
    let mut desired = DesiredNetwork {
        revision: previous.as_ref().map(|p| p.revision).unwrap_or_default(),
        allowlist,
        hub_overlay_address,
        probes: ProbeTargetSet {
            version: previous.as_ref().map(|p| p.revision).unwrap_or_default() as i64,
            targets,
        },
    };
    if previous.as_ref() != Some(&desired) {
        desired.revision += 1;
        desired.probes.version = desired.revision as i64;
        s.db.put("network_frames", node_id, &desired).await?;
    }
    Ok(desired)
}

pub async fn synchronize(s: &State) -> Result<()> {
    let nodes = s.db.list::<Node>("nodes").await?;
    for node in nodes
        .iter()
        .filter(|n| !n.revoked && n.capabilities.as_ref().is_some_and(|c| c.dual_path))
    {
        let desired = desired(s, &node.id).await?;
        if let Some((_, tx)) = s.channels.read().await.get(&node.id) {
            let _ = tx.try_send(Frame::NetworkDesired { desired });
        }
    }
    Ok(())
}

pub async fn settle(s: &State) -> Result<()> {
    let _guard = s.gate.lock().await;
    let nodes = s.db.list::<Node>("nodes").await?;
    let mut expected = Vec::new();
    for node in nodes.iter().filter(|n| !n.revoked) {
        expected.extend(node.public_addresses.clone());
        expected.extend(
            s.db.get::<Vec<String>>("network_retiring", &node.id)
                .await?
                .unwrap_or_default(),
        );
        expected.extend(
            s.db.get::<Vec<String>>("network_plugin_addresses", &node.id)
                .await?
                .unwrap_or_default(),
        );
    }
    expected.sort();
    expected.dedup();
    let live: Vec<_> = {
        let channels = s.channels.read().await;
        nodes
            .iter()
            .filter(|n| {
                !n.revoked
                    && n.capabilities.as_ref().is_some_and(|c| c.dual_path)
                    && channels.contains_key(&n.id)
            })
            .collect()
    };
    if live.is_empty() {
        return Ok(());
    }
    // 新集合已在当前在线节点落地，才收敛由新认证公网连接确认替换的旧来源。
    for node in live {
        let Some(desired) = s.db.get::<Allowlist>("network_desired", &node.id).await? else {
            return Ok(());
        };
        let Some(applied) =
            s.db.get::<AppliedNetwork>("network_applied", &node.id)
                .await?
        else {
            return Ok(());
        };
        if desired.public_addresses != expected
            || applied.applied_revision != desired.revision
            || applied.error.is_some()
        {
            return Ok(());
        }
    }
    for node in nodes.iter().filter(|n| !n.revoked) {
        if s.db
            .get::<bool>("network_retirement_ready", &node.id)
            .await?
            == Some(true)
        {
            s.db.put("network_retiring", &node.id, &Vec::<String>::new())
                .await?;
            s.db.delete("network_retirement_ready", &node.id).await?;
        }
    }
    Ok(())
}

pub async fn plugin_observation(s: &State, provider: &str, input: Value) -> ApiResult<Value> {
    let guard = s.gate.lock().await;
    let node = input["node_id"]
        .as_str()
        .ok_or_else(|| bad("地址观测缺少节点身份"))?;
    let directory = directory(s).await?;
    check(
        directory.overlays.iter().any(|member| {
            member.node_id == node
                && member.provider == provider
                && json!(member.machine_id) == input["machine_id"]
                && json!(member.instance_id) == input["instance_id"]
        }),
        "地址观测不属于已关联实例",
    )?;
    check(
        s.db.get::<Node>("nodes", node)
            .await?
            .is_some_and(|n| !n.revoked),
        "节点已撤销",
    )?;
    let addresses = input["addresses"]
        .as_array()
        .ok_or_else(|| bad("地址观测格式无效"))?;
    check(addresses.len() <= 32, "地址观测过多")?;
    let mut addresses: Vec<String> = addresses
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|s| s.parse::<std::net::IpAddr>().ok())
        .filter(|ip| public_ip(*ip))
        .map(|ip| ip.to_string())
        .collect();
    addresses.sort();
    addresses.dedup();
    // STUN 暂未取得结果时保留上次观测，并通过时间说明新鲜度。
    if !addresses.is_empty() {
        let previous =
            s.db.get::<Vec<String>>("network_plugin_addresses", node)
                .await?
                .unwrap_or_default();
        let mut retiring =
            s.db.get::<Vec<String>>("network_retiring", node)
                .await?
                .unwrap_or_default();
        retiring.extend(
            previous
                .into_iter()
                .filter(|address| !addresses.contains(address)),
        );
        retiring.retain(|address| !addresses.contains(address));
        retiring.sort();
        retiring.dedup();
        s.db.put("network_retiring", node, &retiring).await?;
        // STUN 变化先保留旧来源；只有新公网认证连接确认替换后才允许收敛。
        s.db.put("network_plugin_addresses", node, &addresses)
            .await?;
        s.db.put("network_plugin_observations",node,&json!({"provider":provider,"kind":"stun","addresses":addresses,"at":now(),"source_at":input["source_at"],"inbound_verified":false})).await?;
    }
    drop(guard);
    synchronize(s).await?;
    Ok(json!({"observed":true}))
}
