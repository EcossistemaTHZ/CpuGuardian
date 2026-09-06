use std::{fs, path::Path};

/// Retorna a maior temperatura plausível encontrada em hwmon/thermal_zone.
/// Ausência de sensor não é erro: alguns notebooks não expõem o coretemp.
pub fn read_cpu_celsius() -> Option<f32> {
    read_hwmon().or_else(read_thermal_zone)
}

fn read_hwmon() -> Option<f32> {
    let root = Path::new("/sys/class/hwmon");
    let mut values = Vec::new();
    for hwmon in fs::read_dir(root).ok()?.flatten() {
        let name = fs::read_to_string(hwmon.path().join("name")).unwrap_or_default();
        if !["coretemp", "k10temp", "zenpower"]
            .iter()
            .any(|x| name.trim().contains(x))
        {
            continue;
        }
        if let Ok(entries) = fs::read_dir(hwmon.path()) {
            for entry in entries.flatten() {
                let file = entry.file_name();
                let file = file.to_string_lossy();
                if file.starts_with("temp") && file.ends_with("_input") {
                    if let Some(value) = parse_millidegrees(&entry.path()) {
                        values.push(value);
                    }
                }
            }
        }
    }
    values.into_iter().max_by(f32::total_cmp)
}

fn read_thermal_zone() -> Option<f32> {
    let root = Path::new("/sys/class/thermal");
    let mut values = Vec::new();
    for zone in fs::read_dir(root).ok()?.flatten() {
        if zone
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            if let Some(value) = parse_millidegrees(&zone.path().join("temp")) {
                values.push(value);
            }
        }
    }
    values.into_iter().max_by(f32::total_cmp)
}

fn parse_millidegrees(path: &Path) -> Option<f32> {
    let raw: f32 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
    let celsius = if raw > 1000.0 { raw / 1000.0 } else { raw };
    (celsius >= 10.0 && celsius <= 125.0).then_some(celsius)
}
