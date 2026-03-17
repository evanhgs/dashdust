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

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    const KB: u64 = 1_024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
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

fn format_speed(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

fn usage_color(pct: f32) -> &'static str {
    if pct >= 90.0 {
        "#f87171"
    } else if pct >= 70.0 {
        "#fb923c"
    } else {
        "#4a9eff"
    }
}

#[component]
fn App() -> impl IntoView {
    let (metrics, set_metrics) = signal(Metrics::default());
    let (connected, set_connected) = signal(false);

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
        let url = format!("{}://{}/ws", protocol, host);

        let ws = WebSocket::new(&url).expect("WebSocket failed");

        let set_conn = set_connected;
        let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_conn.set(true);
        }) as Box<dyn FnMut(_)>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let set_conn2 = set_connected;
        let onclose = Closure::wrap(Box::new(move |_: web_sys::Event| {
            set_conn2.set(false);
        }) as Box<dyn FnMut(_)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let set_m = set_metrics;
        let onmessage = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if let Ok(m) = serde_json::from_str::<Metrics>(&s) {
                    set_m.set(m);
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

                <div class="card card-cpu">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Processor"</span>
                    </div>
                    <div class="card-body">
                        <div class="cpu-brand">{move || metrics.get().cpu.brand.clone()}</div>
                        <div class="info-grid" style="margin-bottom: 1rem;">
                            <div class="info-item">
                                <span class="info-label">"Cores"</span>
                                <span class="info-value">{move || metrics.get().cpu.cores.to_string()}</span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Threads"</span>
                                <span class="info-value">{move || metrics.get().cpu.threads.to_string()}</span>
                            </div>
                        </div>
                        <div class="usage-bar-container">
                            <div class="usage-label">
                                <span>"Global Usage"</span>
                                <span class="usage-pct">
                                    {move || format!("{:.1}%", metrics.get().cpu.usage_global)}
                                </span>
                            </div>
                            <div class="usage-bar">
                                <div
                                    class="usage-fill"
                                    style=move || {
                                        let u = metrics.get().cpu.usage_global.min(100.0);
                                        format!(
                                            "width:{:.1}%; background: linear-gradient(90deg, #4a9eff, #9b6dff);",
                                            u
                                        )
                                    }
                                ></div>
                            </div>
                        </div>
                        <div class="cores-grid">
                            {move || {
                                metrics
                                    .get()
                                    .cpu
                                    .usage_per_core
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &usage)| {
                                        let color = usage_color(usage);
                                        view! {
                                            <div class="core-bar">
                                                <span class="core-label">{format!("C{}", i)}</span>
                                                <div class="mini-bar">
                                                    <div
                                                        class="mini-fill"
                                                        style=format!(
                                                            "width:{:.0}%; background:{};",
                                                            usage.min(100.0),
                                                            color,
                                                        )
                                                    ></div>
                                                </div>
                                                <span class="core-pct">{format!("{:.0}%", usage)}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    </div>
                </div>

                <div class="card card-memory">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Memory"</span>
                    </div>
                    <div class="card-body">
                        <div class="usage-bar-container">
                            <div class="usage-label">
                                <span>"RAM"</span>
                                <span class="usage-pct">
                                    {move || {
                                        let m = metrics.get().memory;
                                        format!("{} / {}", format_bytes(m.used), format_bytes(m.total))
                                    }}
                                </span>
                            </div>
                            <div class="usage-bar">
                                <div
                                    class="usage-fill"
                                    style=move || {
                                        let m = metrics.get().memory;
                                        let pct = if m.total > 0 {
                                            m.used as f64 / m.total as f64 * 100.0
                                        } else {
                                            0.0
                                        };
                                        let color = if pct >= 90.0 {
                                            "#f87171"
                                        } else if pct >= 70.0 {
                                            "#fb923c"
                                        } else {
                                            "#2dd4bf"
                                        };
                                        format!("width:{:.1}%; background:{};", pct, color)
                                    }
                                ></div>
                            </div>
                        </div>
                        <div class="usage-bar-container">
                            <div class="usage-label">
                                <span>"Swap"</span>
                                <span class="usage-pct">
                                    {move || {
                                        let m = metrics.get().memory;
                                        format!(
                                            "{} / {}",
                                            format_bytes(m.swap_used),
                                            format_bytes(m.swap_total),
                                        )
                                    }}
                                </span>
                            </div>
                            <div class="usage-bar">
                                <div
                                    class="usage-fill"
                                    style=move || {
                                        let m = metrics.get().memory;
                                        let pct = if m.swap_total > 0 {
                                            m.swap_used as f64 / m.swap_total as f64 * 100.0
                                        } else {
                                            0.0
                                        };
                                        format!(
                                            "width:{:.1}%; background: linear-gradient(90deg, #fb923c, #f87171);",
                                            pct,
                                        )
                                    }
                                ></div>
                            </div>
                        </div>
                        <div class="info-grid">
                            <div class="info-item">
                                <span class="info-label">"Total"</span>
                                <span class="info-value">
                                    {move || format_bytes(metrics.get().memory.total)}
                                </span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Used"</span>
                                <span class="info-value">
                                    {move || format_bytes(metrics.get().memory.used)}
                                </span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Available"</span>
                                <span class="info-value">
                                    {move || format_bytes(metrics.get().memory.available)}
                                </span>
                            </div>
                            <div class="info-item">
                                <span class="info-label">"Swap Used"</span>
                                <span class="info-value">
                                    {move || format_bytes(metrics.get().memory.swap_used)}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>

                <div class="card card-storage">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Storage"</span>
                    </div>
                    <div class="card-body">
                        {move || {
                            metrics
                                .get()
                                .disks
                                .iter()
                                .map(|disk| {
                                    let pct = if disk.total > 0 {
                                        disk.used as f64 / disk.total as f64 * 100.0
                                    } else {
                                        0.0
                                    };
                                    let color = if pct >= 90.0 {
                                        "#f87171"
                                    } else if pct >= 70.0 {
                                        "#fb923c"
                                    } else {
                                        "#9b6dff"
                                    };
                                    let disk = disk.clone();
                                    view! {
                                        <div class="disk-item">
                                            <div class="disk-header">
                                                <span class="disk-name">{disk.name.clone()}</span>
                                                <span class="disk-badge">{disk.kind.clone()}</span>
                                                <span class="disk-mount">{disk.mount.clone()}</span>
                                            </div>
                                            <div class="usage-bar-container">
                                                <div class="usage-label">
                                                    <span></span>
                                                    <span class="usage-pct">
                                                        {format!(
                                                            "{} / {} ({:.1}%)",
                                                            format_bytes(disk.used),
                                                            format_bytes(disk.total),
                                                            pct,
                                                        )}
                                                    </span>
                                                </div>
                                                <div class="usage-bar">
                                                    <div
                                                        class="usage-fill"
                                                        style=format!(
                                                            "width:{:.1}%; background:{};",
                                                            pct,
                                                            color,
                                                        )
                                                    ></div>
                                                </div>
                                            </div>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                </div>

                <div class="card card-network">
                    <div class="card-header">
                        <span class="card-icon">"⬡"</span>
                        <span class="card-title">"Network"</span>
                    </div>
                    <div class="card-body">
                        <div class="net-grid">
                            {move || {
                                metrics
                                    .get()
                                    .networks
                                    .iter()
                                    .map(|net| {
                                        let net = net.clone();
                                        view! {
                                            <div class="net-item">
                                                <div class="net-name">{net.name.clone()}</div>
                                                <div class="net-stats">
                                                    <div class="net-stat">
                                                        <span class="net-arrow net-down">"↓"</span>
                                                        <div>
                                                            <div class="net-speed">
                                                                {format_speed(net.rx_speed)}
                                                            </div>
                                                            <div class="net-total">
                                                                {format!("total {}", format_bytes(net.rx_bytes))}
                                                            </div>
                                                        </div>
                                                    </div>
                                                    <div class="net-stat">
                                                        <span class="net-arrow net-up">"↑"</span>
                                                        <div>
                                                            <div class="net-speed">
                                                                {format_speed(net.tx_speed)}
                                                            </div>
                                                            <div class="net-total">
                                                                {format!("total {}", format_bytes(net.tx_bytes))}
                                                            </div>
                                                        </div>
                                                    </div>
                                                </div>
                                            </div>
                                        }
                                    })
                                    .collect_view()
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
