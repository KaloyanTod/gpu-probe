//! Gather host + adapter metadata into one struct.
//!
//! We columnize the fields we care to query on, but ALSO keep the entire raw
//! wgpu AdapterInfo as JSON so no field is ever lost even if we didn't give it
//! its own column.

use sysinfo::System;

/// Everything about the machine + GPU that stamps a result row.
pub struct HardwareInfo {
    // GPU
    pub gpu_name: String,
    pub gpu_vendor_id: u32,
    pub gpu_device_id: u32,
    pub gpu_driver: String,
    pub gpu_driver_info: String,
    pub gpu_device_type: String, // Discrete/Integrated/Virtual/Cpu/Other
    pub backend: String,         // Vulkan/Metal/Dx12/Gl/...
    pub raw_adapter_json: String,

    // Software
    pub wgpu_version: String,

    // Host
    pub os_name: String,
    pub os_version: String,
    pub cpu_brand: String,
    pub cpu_cores: u32,
    pub total_ram_bytes: u64,
}

fn device_type_str(t: wgpu::DeviceType) -> &'static str {
    match t {
        wgpu::DeviceType::Other => "Other",
        wgpu::DeviceType::IntegratedGpu => "Integrated",
        wgpu::DeviceType::DiscreteGpu => "Discrete",
        wgpu::DeviceType::VirtualGpu => "Virtual",
        wgpu::DeviceType::Cpu => "Cpu",
    }
}

fn backend_str(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Noop => "Noop",
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Dx12 => "Dx12",
        wgpu::Backend::Gl => "Gl",
        wgpu::Backend::BrowserWebGpu => "BrowserWebGpu",
    }
}

/// Build the full stamp from the adapter info plus a fresh sysinfo scan.
pub fn collect(info: &wgpu::AdapterInfo) -> HardwareInfo {
    // Serialize the entire AdapterInfo so nothing is ever lost.
    let raw_adapter_json =
        serde_json::to_string(info).unwrap_or_else(|_| "{\"error\":\"serialize failed\"}".into());

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Physical core count (falls back to logical if unavailable).
    let cpu_cores = System::physical_core_count()
        .unwrap_or_else(|| sys.cpus().len()) as u32;

    HardwareInfo {
        gpu_name: info.name.clone(),
        gpu_vendor_id: info.vendor,
        gpu_device_id: info.device,
        gpu_driver: info.driver.clone(),
        gpu_driver_info: info.driver_info.clone(),
        gpu_device_type: device_type_str(info.device_type).to_string(),
        backend: backend_str(info.backend).to_string(),
        raw_adapter_json,

        // Captured from Cargo.toml at build time by build.rs — the exact
        // `=`-pinned wgpu version.
        wgpu_version: env!("WGPU_VERSION").to_string(),

        os_name: System::name().unwrap_or_else(|| "unknown".to_string()),
        os_version: System::long_os_version()
            .or_else(System::os_version)
            .unwrap_or_else(|| "unknown".to_string()),
        cpu_brand,
        cpu_cores,
        total_ram_bytes: sys.total_memory(),
    }
}
