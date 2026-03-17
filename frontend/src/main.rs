use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SystemInfo {
    os: String,
    kernel: String,
    arch: String,
    hostname: String,
    uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CpuInfo {
    brand: String,
    cores: usize,
    threads: usize,
    usage_global: f32,
    usage_per_core: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MemoryInfo {
    total: u64,
    used: u64,
    available: u64,
    swap_total: u64,
    swap_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DiskInfo {
    name: String,
    kind: String,
    total: u64,
    used: u64,
    mount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NetworkInfo {
    name: String,
    rx_bytes: u64,
    tx_bytes: u64,
    rx_speed: u64,
    tx_speed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Metrics {
    system: SystemInfo,
    cpu: CpuInfo,
    memory: MemoryInfo,
    disks: Vec<DiskInfo>,
    networks: Vec<NetworkInfo>,
}

const HISTORY: usize = 60;

fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1_073_741_824;
    const MIB: u64 = 1_048_576;
    const KIB: u64 = 1_024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m {}s", minutes, secs % 60)
    }
}

fn push_hist(h: &mut Vec<f32>, v: f32) {
    h.push(v);
    if h.len() > HISTORY {
        h.remove(0);
    }
}

// Returns (line_path, area_path)
fn svg_area(data: &[f32], max_val: f32, w: f32, h: f32) -> (String, String) {
    let n = data.len();
    if n < 2 {
        return (String::new(), String::new());
    }
    let pad = 2.0_f32;
    let mut line = String::new();
    let mut first_x = 0.0_f32;
    let mut last_x = 0.0_f32;

    for (i, &v) in data.iter().enumerate() {
        let slot = HISTORY - n + i;
        let x = slot as f32 / (HISTORY - 1) as f32 * w;
        let y = (h - pad) - (v / max_val).clamp(0.0, 1.0) * (h - pad * 2.0);
        if i == 0 {
            first_x = x;
            line.push_str(&format!("M{:.1} {:.1}", x, y));
        } else {
            line.push_str(&format!(" L{:.1} {:.1}", x, y));
        }
        if i == n - 1 {
            last_x = x;
        }
    }

    let area = format!(
        "{} L{:.1} {:.1} L{:.1} {:.1}Z",
        line, last_x, h, first_x, h
    );
    (line, area)
}

#[component]
fn App() -> impl IntoView {
    let (metrics, set_metrics) = signal(Metrics::default());
    let (connected, set_connected) = signal(false);

    let cpu_hist = RwSignal::new(Vec::<f32>::new());
    let mem_hist = RwSignal::new(Vec::<f32>::new());
    let rx_hist = RwSignal::new(Vec::<f32>::new());
    let tx_hist = RwSignal::new(Vec::<f32>::new());

    // Memoized paths — recompute once per tick, shared by line + area paths
    let cpu_paths = Memo::new(move |_| svg_area(&cpu_hist.get(), 100.0, 400.0, 80.0));

    let mem_paths = Memo::new(move |_| {
        let max = (metrics.get().memory.total as f32 / 1_073_741_824.0).max(1.0);
        svg_area(&mem_hist.get(), max, 400.0, 80.0)
    });

    let rx_max = Memo::new(move |_| {
        rx_hist.get().iter().copied().fold(1.0_f32, f32::max) * 1.25
    });
    let tx_max = Memo::new(move |_| {
        tx_hist.get().iter().copied().fold(1.0_f32, f32::max) * 1.25
    });
    let rx_paths = Memo::new(move |_| svg_area(&rx_hist.get(), rx_max.get(), 400.0, 70.0));
    let tx_paths = Memo::new(move |_| svg_area(&tx_hist.get(), tx_max.get(), 400.0, 70.0));

    Effect::new(move |_| {
        let window = web_sys::window().expect("no window");
        let location = window.location();
        let host = location
            .host()
            .unwrap_or_else(|_| "localhost:3000".to_string());
        let protocol = if location.protocol().unwrap_or_default() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let ws = WebSocket::new(&format!("{}://{}/ws", protocol, host))
            .expect("WebSocket failed");

        let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_connected.set(true);
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_connected.set(false);
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if let Ok(m) = serde_json::from_str::<Metrics>(&s) {
                    cpu_hist.update(|h| push_hist(h, m.cpu.usage_global));
                    mem_hist.update(|h| {
                        push_hist(h, m.memory.used as f32 / 1_073_741_824.0)
                    });
                    let rx: f32 = m
                        .networks
                        .iter()
                        .map(|n| n.rx_speed as f32 * 8.0 / 1_000_000.0)
                        .sum();
                    let tx: f32 = m
                        .networks
                        .iter()
                        .map(|n| n.tx_speed as f32 * 8.0 / 1_000_000.0)
                        .sum();
                    rx_hist.update(|h| push_hist(h, rx));
                    tx_hist.update(|h| push_hist(h, tx));
                    set_metrics.set(m);
                }
            }
        }) as Box<dyn FnMut(_)>);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
    });

    view! {
        <div class="dashboard">
            <header class="header">
                <div class="header-left">
                    <span class="logo">"◈ DashDust"</span>
                    <span class="hostname">{move || metrics.get().system.hostname.clone()}</span>
                </div>
                <div class="header-right">
                    <span class=move || {
                        if connected.get() { "status online" } else { "status offline" }
                    }>
                        {move || if connected.get() { "● Live" } else { "○ Offline" }}
                    </span>
                </div>
            </header>

            <main class="main">

                // ── System
                <div class="card card-system">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"System"</span>
                    </div>
                    <div class="card-body">
                        <div class="info-grid">
                            <div class="info-item">
                                <span class="info-label">"OS"</span>
                                <span class="info-value">{move || metrics.get().system.os.clone()}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Kernel"</span>
                                <span class="info-value">{move || metrics.get().system.kernel.clone()}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Arch"</span>
                                <span class="info-value">{move || metrics.get().system.arch.clone()}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Uptime"</span>
                                <span class="info-value">{move || format_uptime(metrics.get().system.uptime)}</span>
                            </div>
                        </div>
                    </div>
                </div>

                // ── CPU
                <div class="card card-cpu">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Processor"</span>
                        <span class="chart-val">
                            {move || format!("{:.1}%", metrics.get().cpu.usage_global)}
                        </span>
                    </div>
                    <div class="card-body">
                        <div class="cpu-brand">{move || metrics.get().cpu.brand.clone()}</div>
                        <div class="info-grid" style="margin-bottom:1rem">
                            <div class="info-item">
                                <span class="info-label">"Cores"</span>
                                <span class="info-value">{move || metrics.get().cpu.cores.to_string()}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Threads"</span>
                                <span class="info-value">{move || metrics.get().cpu.threads.to_string()}</span>
                            </div>
                        </div>
                        <div class="chart-wrap">
                            <svg viewBox="0 0 400 80" style="width:100%;height:80px" preserveAspectRatio="none">
                                <defs>
                                    <linearGradient id="g-cpu" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="0%" stop-color="#4a9eff" stop-opacity="0.28"/>
                                        <stop offset="100%" stop-color="#4a9eff" stop-opacity="0"/>
                                    </linearGradient>
                                </defs>
                                <path d=move || cpu_paths.get().1 fill="url(#g-cpu)"/>
                                <path d=move || cpu_paths.get().0 fill="none" stroke="#4a9eff" stroke-width="1.5"/>
                            </svg>
                            <div class="chart-axis">
                                <span>"100%"</span>
                                <span>"0"</span>
                            </div>
                        </div>
                    </div>
                </div>

                // ── Memory
                <div class="card card-memory">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Memory"</span>
                        <span class="chart-val">
                            {move || {
                                let m = metrics.get().memory;
                                let pct = if m.total > 0 {
                                    m.used as f64 / m.total as f64 * 100.0
                                } else { 0.0 };
                                format!("{:.1}%  {}", pct, format_bytes(m.used))
                            }}
                        </span>
                    </div>
                    <div class="card-body">
                        <div class="chart-wrap">
                            <svg viewBox="0 0 400 80" style="width:100%;height:80px" preserveAspectRatio="none">
                                <defs>
                                    <linearGradient id="g-mem" x1="0" y1="0" x2="0" y2="1">
                                        <stop offset="0%" stop-color="#2dd4bf" stop-opacity="0.28"/>
                                        <stop offset="100%" stop-color="#2dd4bf" stop-opacity="0"/>
                                    </linearGradient>
                                </defs>
                                <path d=move || mem_paths.get().1 fill="url(#g-mem)"/>
                                <path d=move || mem_paths.get().0 fill="none" stroke="#2dd4bf" stroke-width="1.5"/>
                            </svg>
                            <div class="chart-axis">
                                <span>{move || format!("{:.0} GiB", metrics.get().memory.total as f64 / 1_073_741_824.0)}</span>
                                <span>"0"</span>
                            </div>
                        </div>
                        <div class="info-grid" style="margin-top:1rem">
                            <div class="info-item">
                                <span class="info-label">"Used"</span>
                                <span class="info-value">{move || format_bytes(metrics.get().memory.used)}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Available"</span>
                                <span class="info-value">{move || format_bytes(metrics.get().memory.available)}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Total"</span>
                                <span class="info-value">{move || format_bytes(metrics.get().memory.total)}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Swap"</span>
                                <span class="info-value">
                                    {move || {
                                        let m = metrics.get().memory;
                                        format!("{} / {}", format_bytes(m.swap_used), format_bytes(m.swap_total))
                                    }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>

                // ── Storage
                <div class="card card-storage">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Storage"</span>
                    </div>
                    <div class="card-body">
                        <div class="donuts">
                            {move || {
                                metrics.get().disks.iter().map(|disk| {
                                    let pct = if disk.total > 0 {
                                        disk.used as f64 / disk.total as f64 * 100.0
                                    } else { 0.0 };
                                    let r = 34.0_f64;
                                    let circ = 2.0 * std::f64::consts::PI * r;
                                    let filled = (pct / 100.0 * circ).max(0.0);
                                    let gap = (circ - filled).max(0.0);
                                    let color = if pct >= 90.0 { "#f87171" }
                                        else if pct >= 70.0 { "#fb923c" }
                                        else { "#9b6dff" };
                                    let disk = disk.clone();
                                    view! {
                                        <div class="donut">
                                            <svg viewBox="0 0 100 100" style="width:110px;height:110px;flex-shrink:0">
                                                <circle cx="50" cy="50" r="34"
                                                    fill="none" stroke="#1a2236" stroke-width="10"/>
                                                <circle cx="50" cy="50" r="34"
                                                    fill="none"
                                                    stroke=color
                                                    stroke-width="10"
                                                    stroke-dasharray=format!("{:.2} {:.2}", filled, gap)
                                                    stroke-linecap="round"
                                                    transform="rotate(-90 50 50)"
                                                />
                                                <text x="50" y="47"
                                                    text-anchor="middle"
                                                    font-size="15"
                                                    font-weight="700"
                                                    fill="#e8eaf0">
                                                    {format!("{:.0}%", pct)}
                                                </text>
                                                <text x="50" y="62"
                                                    text-anchor="middle"
                                                    font-size="8"
                                                    fill="#6b7a99">
                                                    {disk.mount.clone()}
                                                </text>
                                            </svg>
                                            <div class="donut-meta">
                                                <span class="disk-name">{disk.name.clone()}</span>
                                                <span class="disk-badge">{disk.kind.clone()}</span>
                                                <span class="disk-size">
                                                    {format!("{} / {}", format_bytes(disk.used), format_bytes(disk.total))}
                                                </span>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()
                            }}
                        </div>
                    </div>
                </div>

                // ── Network
                <div class="card card-network">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Network"</span>
                    </div>
                    <div class="card-body">
                        <div class="net-charts">
                            <div class="net-chart">
                                <div class="net-chart-hdr">
                                    <span class="net-arrow net-down">"↓"</span>
                                    <span class="net-chart-lbl">"Download"</span>
                                    <span class="chart-val">
                                        {move || format!("{:.2} Mb/s", rx_hist.get().last().copied().unwrap_or(0.0))}
                                    </span>
                                </div>
                                <div class="chart-wrap">
                                    <svg viewBox="0 0 400 70" style="width:100%;height:70px" preserveAspectRatio="none">
                                        <defs>
                                            <linearGradient id="g-rx" x1="0" y1="0" x2="0" y2="1">
                                                <stop offset="0%" stop-color="#2dd4bf" stop-opacity="0.28"/>
                                                <stop offset="100%" stop-color="#2dd4bf" stop-opacity="0"/>
                                            </linearGradient>
                                        </defs>
                                        <path d=move || rx_paths.get().1 fill="url(#g-rx)"/>
                                        <path d=move || rx_paths.get().0 fill="none" stroke="#2dd4bf" stroke-width="1.5"/>
                                    </svg>
                                    <div class="chart-axis">
                                        <span>{move || format!("{:.1}", rx_max.get())}</span>
                                        <span>"0"</span>
                                    </div>
                                </div>
                            </div>

                            <div class="net-chart">
                                <div class="net-chart-hdr">
                                    <span class="net-arrow net-up">"↑"</span>
                                    <span class="net-chart-lbl">"Upload"</span>
                                    <span class="chart-val">
                                        {move || format!("{:.2} Mb/s", tx_hist.get().last().copied().unwrap_or(0.0))}
                                    </span>
                                </div>
                                <div class="chart-wrap">
                                    <svg viewBox="0 0 400 70" style="width:100%;height:70px" preserveAspectRatio="none">
                                        <defs>
                                            <linearGradient id="g-tx" x1="0" y1="0" x2="0" y2="1">
                                                <stop offset="0%" stop-color="#9b6dff" stop-opacity="0.28"/>
                                                <stop offset="100%" stop-color="#9b6dff" stop-opacity="0"/>
                                            </linearGradient>
                                        </defs>
                                        <path d=move || tx_paths.get().1 fill="url(#g-tx)"/>
                                        <path d=move || tx_paths.get().0 fill="none" stroke="#9b6dff" stroke-width="1.5"/>
                                    </svg>
                                    <div class="chart-axis">
                                        <span>{move || format!("{:.1}", tx_max.get())}</span>
                                        <span>"0"</span>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <div class="net-ifaces">
                            {move || {
                                metrics.get().networks.iter().map(|net| {
                                    let net = net.clone();
                                    view! {
                                        <div class="net-iface">
                                            <span class="net-iface-name">{net.name.clone()}</span>
                                            <span class="net-down">
                                                {format!("↓ {:.2} Mb/s", net.rx_speed as f64 * 8.0 / 1_000_000.0)}
                                            </span>
                                            <span class="net-up">
                                                {format!("↑ {:.2} Mb/s", net.tx_speed as f64 * 8.0 / 1_000_000.0)}
                                            </span>
                                        </div>
                                    }
                                }).collect_view()
                            }}
                        </div>
                    </div>
                </div>

            </main>
        </div>
    }
}

fn main() {
    leptos::mount::mount_to_body(App);
}
