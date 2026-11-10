//! Turn engine output into actionable findings while retaining the original log.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub fn exit_hint(code: Option<i32>) -> Option<&'static str> {
    match code? as u32 {
        0xc0000135 => Some(
            "Windows 找不到引擎依赖 DLL。请完整解压离线包；计算驱动仍需由显卡厂商提供。 / A required engine DLL is missing. Extract the complete offline package and check the vendor compute driver.",
        ),
        0xc000007b => Some(
            "Windows 检测到错误架构或损坏的程序/DLL。请使用匹配系统架构的完整包。 / An executable or DLL has the wrong architecture or is corrupt.",
        ),
        0xc0000142 => Some(
            "引擎依赖初始化失败，请检查显卡驱动并导出诊断信息。 / An engine dependency failed to initialize; check the compute driver.",
        ),
        0xc000001d => Some(
            "处理器不支持引擎使用的指令。请使用兼容的引擎构建。 / The CPU does not support an instruction used by this engine.",
        ),
        0xc0000005 => Some(
            "引擎或驱动发生访问冲突，请保留任务日志并检查计算驱动。 / The engine or driver crashed with an access violation.",
        ),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub vendor: String,
    pub pci: String,
    pub driver: String,
    pub memory: String,
    pub physical_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Issue {
    pub code: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub engine: PathBuf,
    pub checked_at: u64,
    pub devices: Vec<Device>,
    pub issues: Vec<Issue>,
    pub raw: String,
}

pub fn parse(engine: PathBuf, raw: String) -> Report {
    let mut devices: Vec<Device> = Vec::new();
    let mut backend = String::new();
    let mut current: Option<Device> = None;
    for line in raw.lines().map(str::trim) {
        if ["CUDA Info:", "HIP Info:", "Metal Info:", "OpenCL Info:"].contains(&line) {
            if let Some(device) = current.take() {
                devices.push(device);
            }
            backend = line.trim_end_matches(" Info:").to_owned();
        }
        if let Some(tail) = line.strip_prefix("Backend Device ID #") {
            if let Some(device) = current.take() {
                devices.push(device);
            }
            if let Ok(id) = tail
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
            {
                current = Some(Device {
                    id,
                    backend: backend.clone(),
                    ..Device::default()
                });
            }
        } else if let Some(device) = current.as_mut() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim_end_matches('.') {
                "Name" => device.name = value,
                "Type" => device.device_type = value,
                "Vendor" => device.vendor = value,
                "PCI.Addr.BDF" => device.pci = value,
                "Driver.Version" => device.driver = value,
                "Memory.Total" => device.memory = value,
                _ => {}
            }
        }
    }
    if let Some(device) = current {
        devices.push(device);
    }
    for d in &mut devices {
        d.physical_key = if d.device_type.eq_ignore_ascii_case("CPU") {
            "cpu".into()
        } else if !d.pci.is_empty() {
            format!("pci:{}", d.pci.to_lowercase().trim_start_matches("0000:"))
        } else if !d.name.is_empty() {
            // Conservative when PCI identity is unavailable: identical names share
            // a reservation rather than accidentally competing on one physical GPU.
            format!(
                "name:{}",
                d.name.to_lowercase().split_whitespace().collect::<String>()
            )
        } else {
            "unknown-device".into()
        };
    }
    let mut issues = Vec::new();
    let mut add = |code: &str, severity: &str, title: &str, detail: &str| {
        issues.push(Issue {
            code: code.into(),
            severity: severity.into(),
            title: title.into(),
            detail: detail.into(),
        })
    };
    if raw.contains("Failed to initialize NVIDIA RTC")
        || raw.contains("CUDA SDK Toolkit not installed")
    {
        add(
            "cuda_rtc",
            "warning",
            "CUDA 编译组件未就绪",
            "已检测到 NVIDIA 驱动，但缺少或无法加载 CUDA Toolkit 的 NVRTC。OpenCL 回退仍可计算；请匹配驱动与 Toolkit 版本后重新检测，再对照测速。",
        );
    }
    if raw.contains("Failed to initialize HIP RTC") || raw.contains("HIP SDK not installed") {
        add(
            "hip_rtc",
            "warning",
            "HIP 编译组件未就绪",
            "AMD HIP 编译组件未就绪。请检查对应系统的 HIP SDK 或 ROCm；存在 OpenCL 回退时仍可计算。",
        );
    }
    if raw.contains("CUDA_ERROR_UNSUPPORTED_PTX_VERSION")
        || raw.contains("unsupported display driver")
    {
        add(
            "driver_mismatch",
            "error",
            "驱动与编译组件不兼容",
            "当前驱动无法处理编译组件生成的内核。请使用相互兼容的官方驱动与工具包版本。",
        );
    }
    if raw.contains("CL_PLATFORM_NOT_FOUND_KHR")
        || raw.contains("No OpenCL-compatible")
        || raw.contains("No devices found")
    {
        add(
            "no_backend",
            "error",
            "未找到可用计算后端",
            "安装显卡厂商的计算驱动；CPU 也需要 OpenCL 运行时。安装后重新检测设备。",
        );
    }
    if raw.contains("Permission denied") {
        add(
            "permission",
            "error",
            "计算资源访问被拒绝",
            "检查设备节点、工作区及引擎目录的访问权限；Linux 还需检查 render 或 video 用户组。",
        );
    }
    if raw.contains("CL_MEM_OBJECT_ALLOCATION_FAILURE")
        || raw.contains("Not enough allocatable device memory")
        || raw.contains("CUDA_ERROR_OUT_OF_MEMORY")
    {
        add(
            "memory",
            "error",
            "设备内存不足",
            "结束占用同一设备的其他任务，先恢复自动内核参数；减少批次或设备虚拟实例数后重试。",
        );
    }
    if !devices.is_empty()
        && devices
            .iter()
            .all(|d| d.device_type.eq_ignore_ascii_case("CPU"))
    {
        add(
            "cpu_only",
            "warning",
            "当前仅检测到 CPU",
            "任务可以使用 CPU 运行；如预期使用显卡，请检查显卡计算驱动和运行时。",
        );
    }
    if devices.is_empty() && issues.is_empty() {
        issues.push(Issue {
            code: "no_inventory".into(),
            severity: "warning".into(),
            title: "引擎未返回设备清单".into(),
            detail: "查看原始输出确认引擎版本与运行时。缺少设备清单时，队列会采用独占方式运行。"
                .into(),
        });
    }
    Report {
        engine,
        checked_at: now(),
        devices,
        issues,
        raw,
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_loader_failures_are_actionable() {
        assert!(
            exit_hint(Some(0xc0000135_u32 as i32))
                .unwrap()
                .contains("DLL")
        );
        assert!(
            exit_hint(Some(0xc000007b_u32 as i32))
                .unwrap()
                .contains("架构")
        );
        assert!(exit_hint(Some(0)).is_none());
        assert!(exit_hint(Some(1)).is_none());
    }
    #[test]
    fn aliases_reserve_one_physical_gpu_and_fan_warning_is_not_fatal() {
        let text = "CUDA Info:\nBackend Device ID #1\nName...........: NVIDIA RTX\nType...........: GPU\nPCI.Addr.BDF...: 0000:01:00.0\nOpenCL Info:\nBackend Device ID #02\nName...........: NVIDIA RTX\nType...........: GPU\nPCI.Addr.BDF...: 01:00.0\nnvmlDeviceGetFanSpeed(): Not Supported";
        let report = parse("hashcat".into(), text.into());
        assert_eq!(report.devices.len(), 2);
        assert_eq!(
            report.devices[0].physical_key,
            report.devices[1].physical_key
        );
        assert!(report.issues.is_empty());
    }
    #[test]
    fn missing_cuda_compiler_explains_opencl_fallback() {
        let report = parse(
            "hashcat".into(),
            "Failed to initialize NVIDIA RTC library.\nFalling back to OpenCL runtime.".into(),
        );
        assert_eq!(report.issues[0].code, "cuda_rtc");
        assert_eq!(report.issues[0].severity, "warning");
    }
}
