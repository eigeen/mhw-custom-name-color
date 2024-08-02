use std::{
    ffi::{c_void, CStr},
    ptr::addr_of_mut,
    str::FromStr,
    sync::Once,
};

use anyhow::Context;
use log::{error, info};
use mhw_toolkit::game::{
    address::{self, AddressRepository},
    resources::Player,
};
use strum::EnumString;
use windows::Win32::{
    Foundation::{BOOL, TRUE},
    System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH},
};

mod logger {
    use log::LevelFilter;
    use mhw_toolkit::logger::MHWLogger;
    use once_cell::sync::Lazy;

    static LOGGER: Lazy<MHWLogger> = Lazy::new(|| MHWLogger::new("UnlimitedInsurance"));

    pub fn init_log() {
        log::set_logger(&*LOGGER).unwrap();
        log::set_max_level(LevelFilter::Debug);
    }
}

static MAIN_THREAD_ONCE: Once = Once::new();
static mut ORIGINAL_FUNCTION: *mut c_void = std::ptr::null_mut();
static mut CONFIG_COLOR: Color = Color::Default;
type PlayerDeathFunc = extern "C" fn(*const c_void, *const c_void) -> i64;

#[repr(i8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, EnumString)]
#[strum(serialize_all = "snake_case")]
enum Color {
    #[default]
    Default = -1, // 不改变
    White = 0,
    Green = 1,
    Orange = 2,
    Blue = 3,
    Purple = 4,
    Yellow = 5,
}

impl Color {
    pub fn from_i32(i: i32) -> Option<Self> {
        match i {
            0 => Some(Color::White),
            1 => Some(Color::Green),
            2 => Some(Color::Orange),
            3 => Some(Color::Blue),
            4 => Some(Color::Purple),
            5 => Some(Color::Yellow),
            _ => None,
        }
    }
}

extern "C" fn hooked_display(a1: *const c_void, a2: *const c_void) -> i64 {
    unsafe {
        // 调用原始函数
        let original: PlayerDeathFunc = std::mem::transmute(ORIGINAL_FUNCTION);
        let result = original(a1, a2);

        // 处理
        let name_ptr = a1.byte_add(0x49) as *const i8;
        let Ok(name) = CStr::from_ptr(name_ptr).to_str() else {
            error!("获取玩家名称失败");
            return result;
        };
        let Some(current_player) = Player::current_player() else {
            return result;
        };
        let Some(player_info) = current_player.info() else {
            return result;
        };
        if name != player_info.name() {
            return result;
        }
        let Some(color) = (a2.byte_add(0x7F) as *mut i8).as_mut() else {
            error!("获取玩家名称颜色失败");
            return result;
        };
        if CONFIG_COLOR == Color::Default {
            return result;
        }
        *color = CONFIG_COLOR as i8;

        result
    }
}

fn hook_display() -> anyhow::Result<()> {
    unsafe {
        // 获取目标函数地址
        let func_addr = AddressRepository::get_instance()
            .lock()
            .unwrap()
            .get_address(address::player::ClonePlayerShortInfo)
            .map_err(|e| anyhow::anyhow!(e))?;
        let target_func: *mut c_void = func_addr as *mut c_void;

        // 创建钩子
        let create_hook_status = minhook_sys::MH_CreateHook(
            target_func,
            hooked_display as *mut c_void,
            addr_of_mut!(ORIGINAL_FUNCTION),
        );
        if create_hook_status == minhook_sys::MH_OK {
            // 启用钩子
            minhook_sys::MH_EnableHook(target_func);
        } else {
            return Err(anyhow::anyhow!("创建Hook失败"));
        }
        Ok(())
    }
}

fn load_global_config() -> anyhow::Result<()> {
    let config_str = std::fs::read_to_string("nativePC/plugins/custom_name_color_config.txt")?;
    let config_str = config_str.trim();
    info!("Read config: `{}`", config_str);
    unsafe {
        if let Ok(config_int) = config_str.parse::<i32>() {
            CONFIG_COLOR = Color::from_i32(config_int).unwrap_or_default();
        } else {
            CONFIG_COLOR = Color::from_str(config_str).unwrap_or_default();
        }
        info!("Config loaded: {:?}", CONFIG_COLOR);
    }

    Ok(())
}

fn main_entry() -> anyhow::Result<()> {
    logger::init_log();

    // load config
    load_global_config()
        .context("Failed to load config at 'nativePC/plugins/custom_name_color_config.txt'")?;

    mhw_toolkit::game::hooks::init_mh();
    hook_display()?;

    Ok(())
}

#[no_mangle]
#[allow(non_snake_case)]
extern "system" fn DllMain(_: usize, call_reason: u32, _: usize) -> BOOL {
    match call_reason {
        DLL_PROCESS_ATTACH => {
            MAIN_THREAD_ONCE.call_once(|| {
                if let Err(e) = main_entry() {
                    error!("{}", e);
                }
            });
        }
        DLL_PROCESS_DETACH => (),
        _ => (),
    }
    TRUE
}
