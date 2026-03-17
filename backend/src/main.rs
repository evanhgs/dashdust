use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use sysinfo::{DiskKind, Disks, Networks, System};
use tokio::{sync::broadcast, time::interval};
use tower_http::{cors::CorsLayer, services::ServeDir};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemInfo {
    os: String,
    kernel: String,
    arch: String,
    hostname: String,
    uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CpuInfo {
    brand: String,
    cores: usize,
    threads: usize,
    usage_global: f32,
    usage_per_core: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryInfo {
    total: u64,
    used: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskInfo {
    name: String,
    kind: String,
    total: u64,
    used: u64,
    mount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NetworkInfo {
    name: String,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_speed: u64,
    tx_speed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Metrics {
    system: SystemInfo,
    cpu: CpuInfo,
    memory: MemoryInfo,
    disks: Vec<DiskInfo>,
    networks: Vec<NetworkInfo>,
}

const HISTORY_SIZE: usize = 60;

#[derive(Clone)]
struct AppState {
    tx: Arc<broadcast::Sender<String>>,
    history: Arc<Mutex<VecDeque<String>>>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();
    let (mut sender, _receiver) = socket.split();

    // Snapshot history (drop lock before any await)
    let snapshot: Vec<String> = state.history.lock().unwrap().iter().cloned().collect();
    for msg in snapshot {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            return;
        }
    }

    // Stream live updates
    while let Ok(msg) = rx.recv().await {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            break;
        }
    }
}

fn collect_metrics(
    sys: &mut System,
    networks: &mut Networks,
    disks: &mut Disks,
    prev_net: &mut HashMap<String, (u64, u64)>,
    prev_time: &mut Instant,
) -> Metrics {
    sys.refresh_all();
    networks.refresh();
    disks.refresh();

    let elapsed = prev_time.elapsed().as_secs_f64().max(0.001);
    *prev_time = Instant::now();

    let system_info = SystemInfo {
        os: System::name().unwrap_or_default(),
        kernel: System::kernel_version().unwrap_or_default(),
        arch: System::cpu_arch().unwrap_or_default(),
        hostname: System::host_name().unwrap_or_default(),
        uptime: System::uptime(),
    };

    let cpus = sys.cpus();
    let cpu_info = CpuInfo {
        brand: cpus.first().map(|c| c.brand().to_string()).unwrap_or_default(),
        cores: sys.physical_core_count().unwrap_or(0),
        threads: cpus.len(),
        usage_global: sys.global_cpu_usage(),
        usage_per_core: cpus.iter().map(|c| c.cpu_usage()).collect(),
    };

    let memory_info = MemoryInfo {
        total: sys.total_memory(),
        used: sys.used_memory(),
        available: sys.available_memory(),
        swap_total: sys.total_swap(),
        swap_used: sys.used_swap(),
    };

    let disk_list: Vec<DiskInfo> = disks
        .iter()
        .map(|d| DiskInfo {
            name: d.name().to_string_lossy().to_string(),
            kind: match d.kind() {
                DiskKind::SSD => "SSD".to_string(),
                DiskKind::HDD => "HDD".to_string(),
                _ => "Unknown".to_string(),
            },
            total: d.total_space(),
            used: d.total_space().saturating_sub(d.available_space()),
            mount: d.mount_point().to_string_lossy().to_string(),
        })
        .collect();

    let mut net_list = Vec::new();
    for (name, data) in networks.iter() {
        let rx = data.total_received();
        let tx = data.total_transmitted();
        let (prev_rx, prev_tx) = prev_net.get(name).copied().unwrap_or((rx, tx));
        let rx_speed = ((rx.saturating_sub(prev_rx)) as f64 / elapsed) as u64;
        let tx_speed = ((tx.saturating_sub(prev_tx)) as f64 / elapsed) as u64;
        prev_net.insert(name.clone(), (rx, tx));
        if rx > 0 || tx > 0 {
            net_list.push(NetworkInfo {
                name: name.clone(),
                rx_bytes: rx,
                tx_bytes: tx,
                rx_speed,
                tx_speed,
            });
        }
    }

    Metrics {
        system: system_info,
        cpu: cpu_info,
        memory: memory_info,
        disks: disk_list,
        networks: net_list,
    }
}

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel::<String>(16);
    let tx = Arc::new(tx);
    let history: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));

    let tx_clone = tx.clone();
    let history_clone = history.clone();

    tokio::spawn(async move {
        let mut sys = System::new_all();
        let mut networks = Networks::new_with_refreshed_list();
        let mut disks = Disks::new_with_refreshed_list();
        let mut prev_net: HashMap<String, (u64, u64)> = HashMap::new();
        let mut prev_time = Instant::now();
        let mut ticker = interval(Duration::from_secs(1));

        ticker.tick().await;
        collect_metrics(&mut sys, &mut networks, &mut disks, &mut prev_net, &mut prev_time);

        loop {
            ticker.tick().await;
            let metrics = collect_metrics(
                &mut sys,
                &mut networks,
                &mut disks,
                &mut prev_net,
                &mut prev_time,
            );
            if let Ok(json) = serde_json::to_string(&metrics) {
                {
                    let mut hist = history_clone.lock().unwrap();
                    hist.push_back(json.clone());
                    if hist.len() > HISTORY_SIZE {
                        hist.pop_front();
                    }
                }
                let _ = tx_clone.send(json);
            }
        }
    });

    let state = AppState { tx, history };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("DashDust running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
