use anyhow::{bail, Context, Result};
use evdev_rs::{
    enums::{EventCode, EventType, EV_KEY as KeyCode},
    Device, DeviceWrapper,
};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub name: String,
    pub path: PathBuf,
    pub phys: String,
    /// True if the device looks like a keyboard (has EV_KEY + KEY_A).
    pub is_keyboard: bool,
}

impl DeviceInfo {
    pub fn with_path(path: PathBuf) -> Result<Self> {
        let f = std::fs::File::open(&path).context(format!("opening {}", path.display()))?;
        let input = Device::new_from_file(f)
            .with_context(|| format!("failed to create new Device from file {}", path.display()))?;

        Ok(Self {
            name: input.name().unwrap_or("").to_string(),
            phys: input.phys().unwrap_or("").to_string(),
            is_keyboard: device_is_keyboard(&input),
            path,
        })
    }

    pub fn with_name(name: &str, phys: Option<&str>) -> Result<Self> {
        let mut devices = Self::obtain_device_list()?;

        if let Some(phys) = phys {
            match devices.iter().position(|item| item.phys == phys) {
                Some(idx) => return Ok(devices.remove(idx)),
                None => {
                    bail!(
                        "Requested device `{}` with phys=`{}` was not found",
                        name,
                        phys
                    );
                }
            }
        }

        let mut devices_with_name: Vec<_> = devices
            .into_iter()
            .filter(|item| item.name == name)
            .collect();

        if devices_with_name.is_empty() {
            bail!("No device found with name `{}`", name);
        }

        if devices_with_name.len() > 1 {
            log::warn!("The following devices match name `{}`:", name);
            for dev in &devices_with_name {
                log::warn!("{:?}", dev);
            }
            log::warn!(
                "evremap will use the first entry. If you want to \
                       use one of the others, add the corresponding phys \
                       value to your configuration, for example, \
                       `phys = \"{}\"` for the second entry in the list.",
                devices_with_name[1].phys
            );
        }

        Ok(devices_with_name.remove(0))
    }

    /// Returns true if the device at `path` looks like a keyboard.
    /// Used during hot-plug when we only have a path and haven't yet built
    /// a full `DeviceInfo`.  Avoids a double-open for devices already in the
    /// list — use `DeviceInfo::is_keyboard` on an existing entry instead.
    pub fn path_is_keyboard(path: &Path) -> bool {
        let Ok(f) = std::fs::File::open(path) else {
            return false;
        };
        let Ok(dev) = Device::new_from_file(f) else {
            return false;
        };
        device_is_keyboard(&dev)
    }

    /// Return all currently present keyboard devices.
    /// No extra opens: the keyboard check is done on the `Device` already
    /// opened by `with_path` inside `obtain_device_list`.
    pub fn all_keyboards() -> Result<Vec<DeviceInfo>> {
        let all = Self::obtain_device_list()?;
        Ok(all.into_iter().filter(|d| d.is_keyboard).collect())
    }

    fn obtain_device_list() -> Result<Vec<DeviceInfo>> {
        let mut devices = vec![];
        for entry in std::fs::read_dir("/dev/input")? {
            let entry = entry?;

            if !entry
                .file_name()
                .to_str()
                .unwrap_or("")
                .starts_with("event")
            {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            match DeviceInfo::with_path(path) {
                Ok(item) => devices.push(item),
                Err(err) => log::error!("{:#}", err),
            }
        }

        // Order by name, but when multiple devices have the same name,
        // order by the event device unit number
        devices.sort_by(|a, b| match a.name.cmp(&b.name) {
            Ordering::Equal => {
                event_number_from_path(&a.path).cmp(&event_number_from_path(&b.path))
            }
            different => different,
        });
        Ok(devices)
    }
}

/// Keyboard heuristic: has EV_KEY, reports KEY_A, and has a non-empty phys
/// string.  Real hardware devices always have a phys set by the kernel driver.
/// uinput virtual devices (including evremap's own output devices) have an
/// empty phys, so this prevents evremap from grabbing its own virtual devices
/// and entering a feedback loop.
fn device_is_keyboard(dev: &Device) -> bool {
    !dev.phys().unwrap_or("").is_empty()
        && dev.has(EventType::EV_KEY)
        && dev.has_event_code(&EventCode::EV_KEY(KeyCode::KEY_A))
}

fn event_number_from_path(path: &PathBuf) -> u32 {
    match path.to_str() {
        Some(s) => match s.rfind("event") {
            Some(idx) => s[idx + 5..].parse().unwrap_or(0),
            None => 0,
        },
        None => 0,
    }
}

pub fn list_devices() -> Result<()> {
    let devices = DeviceInfo::obtain_device_list()?;
    for item in &devices {
        println!("Name: {}", item.name);
        println!("Path: {}", item.path.display());
        println!("Phys: {}", item.phys);
        println!();
    }
    Ok(())
}
