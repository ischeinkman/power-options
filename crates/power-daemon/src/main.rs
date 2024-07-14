use std::{fs, io::Read, path::PathBuf};

use config::Config;
use profile::{Profile, ProfilesInfo};

mod config;
mod helpers;
mod profile;
mod systeminfo;

const CONFIG_FILE: &str = "/etc/power-daemon/config.toml";
const PROFILES_DIRECTORY: &str = "/etc/power-daemon/profiles";

static mut TEMPORARY_OVERRIDE: Option<String> = None;
static mut CONFIG: Option<Config> = None;
static mut PROFILES_INFO: Option<ProfilesInfo> = None;

fn main() {
    parse_config();
    parse_profiles();

    unsafe {
        let profile = PROFILES_INFO.as_ref().unwrap().get_active_profile();
        profile.cpu_settings.apply();
        profile.cpu_core_settings.apply();
    }
}

fn parse_config() {
    unsafe {
        CONFIG = Some(
            toml::from_str::<Config>(
                &fs::read_to_string(CONFIG_FILE).expect("Could not read config file"),
            )
            .expect("Could not parse config file")
            .into(),
        );
    }
}

fn parse_profiles() {
    let mut profiles = Vec::new();
    for profile_name in unsafe { CONFIG.as_ref().unwrap().profiles.iter() } {
        let path = PathBuf::from(format!("{PROFILES_DIRECTORY}/{profile_name}.toml"));
        let mut file = fs::File::open(&path).expect("Could not read file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("Could not read file");

        let mut profile: Profile = toml::from_str(&contents).expect("Could not parse profile");
        profile.profile_name = profile_name.clone();
        profiles.push(profile);
    }

    unsafe {
        // Order of priority for profile picking:
        // Config override > whatever profile corresponds to the power state
        let active_profile = if let Some(ref profile_override) =
            CONFIG.as_ref().unwrap().profile_override
        {
            profile::find_profile_index_by_name(&profiles, profile_override)
        } else if system_on_ac() {
            profile::find_profile_index_by_name(&profiles, &CONFIG.as_ref().unwrap().ac_profile)
        } else {
            profile::find_profile_index_by_name(&profiles, &CONFIG.as_ref().unwrap().bat_profile)
        };

        PROFILES_INFO = Some(
            ProfilesInfo {
                profiles,
                active_profile,
            }
            .into(),
        );
    }
}

fn update_profiles() {
    parse_profiles();
    unsafe {
        // Order of priority for profile picking:
        // Runtime override > [Config override > whatever profile corresponds to the power state] -> (already performed at parse_profiles)
        if let Some(ref temporary_override) = TEMPORARY_OVERRIDE {
            PROFILES_INFO.as_mut().unwrap().active_profile = profile::find_profile_index_by_name(
                &PROFILES_INFO.as_ref().unwrap().profiles,
                &temporary_override,
            );
        }
    }
}

fn system_on_ac() -> bool {
    let mut ac_online = false;

    if let Ok(entries) = fs::read_dir("/sys/class/power_supply/") {
        for entry in entries {
            if let Ok(entry) = entry {
                let entry_path = entry.path();
                if let Ok(type_path) = fs::read_to_string(entry_path.join("type")) {
                    let supply_type = type_path.trim();
                    if supply_type == "Mains" {
                        if let Ok(ac_status) = fs::read_to_string(entry_path.join("online")) {
                            ac_online = ac_status.trim() == "1";
                        }
                    }
                }
            }
        }
    }

    ac_online
}
