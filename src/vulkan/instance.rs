use ash::{ext::debug_utils, vk, Entry, Instance};
use rwh_06::DisplayHandle;
use std::{error::Error, ffi, os::raw::c_char};

pub fn create_instance(
    entry: &Entry,
    display_handle: DisplayHandle,
) -> Result<Instance, Box<dyn Error>> {
    unsafe {
        let app_name = ffi::CStr::from_bytes_with_nul_unchecked(env!("CARGO_PKG_NAME").as_bytes());
        let appinfo = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(0)
            .engine_name(app_name)
            .engine_version(0)
            .api_version(vk::make_api_version(0, 1, 0, 0));
        let mut extension_names =
            ash_window::enumerate_required_extensions(display_handle.as_raw())
                .unwrap()
                .to_vec();
        extension_names.push(debug_utils::NAME.as_ptr());
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            // Enabling this extension is a requirement when using `VK_KHR_portability_subset`
            extension_names.push(ash::khr::get_physical_device_properties2::NAME.as_ptr());
        }
        #[cfg(debug_assertions)]
        let layer_names = [ffi::CStr::from_bytes_with_nul_unchecked(
            b"VK_LAYER_KHRONOS_validation\0",
        )];
        #[cfg(not(debug_assertions))]
        let layer_names: Vec<ffi::CString> = Vec::new();
        let layers_names_raw: Vec<*const c_char> = layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect();
        let create_flags = if cfg!(any(target_os = "macos", target_os = "ios")) {
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::default()
        };
        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&appinfo)
            .enabled_layer_names(&layers_names_raw)
            .enabled_extension_names(&extension_names)
            .flags(create_flags);
        let instance: Instance = entry
            .create_instance(&create_info, None)
            .expect("Instance creation error");

        Ok(instance)
    }
}
